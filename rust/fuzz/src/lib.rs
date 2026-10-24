//! Shared harness for the fuzz targets: source declarations and the shared
//! source decode both host routes call.
//!
//! Every target feeds its input to an entry point the hosts use, and that
//! entry point applies the fixed `Limits` itself, so no target sets a bound of
//! its own. A crash, a sanitizer report, a panic or a broken target invariant
//! aborts the input, which fails the run. libFuzzer's panic hook aborts on
//! every panic it sees, and the Parquet reader keeps the panics it contains in
//! the third-party decoder from reaching that hook: they stay malformed
//! sources, as the hosts report them.

use std::{convert::Infallible, num::NonZeroUsize};

use yamaa_adapters::{column_table::ColumnTable, decoder::Decoder, specification_run::Error};
use yamaa_core::{
    decoder::Decoder as _,
    specification::{SourceDeclaration, SourceInterpretation, SourceProfile},
    table::{TableAccess, ValueRef},
    value::ColumnType,
};

/// One input declaration, as a specification's `input` entry would declare it.
/// Empty strings stay missing, the default convention.
pub fn source(
    profile: SourceProfile,
    types: Vec<(String, ColumnType)>,
    encoding: Option<&str>,
) -> SourceDeclaration {
    let path = match profile {
        SourceProfile::Csv => "raw.csv",
        SourceProfile::Parquet => "raw.parquet",
        SourceProfile::Xport => "raw.xpt",
        SourceProfile::Sas7bdat => "raw.sas7bdat",
        SourceProfile::OdmXml => "raw.xml",
    };
    SourceDeclaration {
        name: "RAW".into(),
        path: path.into(),
        types,
        profile,
        encoding: encoding.map(Into::into),
        byte_order_mark: false,
        empty_string_present: false,
        ordinal: None,
    }
}

/// Decode held bytes through the source decode that both host routes call:
/// the profile selects the CSV, Parquet, ReadStat or ODM XML reader, and the
/// declared types and interpretation apply as they do for a specification
/// input.
pub fn decode(
    source: &SourceDeclaration,
    bytes: &[u8],
    interpretation: SourceInterpretation,
) -> Result<ColumnTable, Error> {
    Decoder::<Infallible>::default()
        .table(source, bytes, interpretation, None)
        .map(|snapshot| snapshot.into_table().expect("an input read whole"))
}

/// Decode held bytes by partition too (#3792), in partitions of each of a
/// few sizes, and require the whole read's outcome: the same table, cell by
/// cell, or the same finding. The one exception is a whole read refused for
/// its table's size, which each partition may stay within.
pub fn partitioned_reads_match_the_whole_read(
    source: &SourceDeclaration,
    bytes: &[u8],
    interpretation: SourceInterpretation,
) {
    let mut decoder = Decoder::<Infallible>::default();
    let whole = decoder.table(source, bytes, interpretation, None);
    if whole.as_ref().err().is_some_and(table_size) {
        return;
    }
    for rows in [1, 2, 3, 7] {
        let rows = NonZeroUsize::new(rows).expect("a positive partition size");
        let divided = decoder.partitioned(source, bytes, interpretation, None, rows);
        match (&whole, &divided) {
            (Ok(whole), Ok(divided)) => {
                assert_eq!(whole.schema(), divided.schema());
                assert_eq!(cells(whole), cells(divided));
            }
            (Err(whole), Err(divided)) => {
                assert_eq!(format!("{whole:?}"), format!("{divided:?}"));
            }
            (whole, divided) => panic!(
                "the whole read {} where partitions of {rows} {}",
                if whole.is_ok() { "admits" } else { "refuses" },
                if divided.is_ok() { "admit" } else { "refuse" },
            ),
        }
    }
}

/// Whether a read was refused for its table's size: its rows, its cells or
/// its text's offsets, which bound a partition of a read by partition.
fn table_size(error: &Error) -> bool {
    use yamaa_adapters::column_table::TableError;
    use yamaa_adapters::csv_source::{self, TextTableError};
    use yamaa_adapters::parquet_source;
    let size = |resource: &str| matches!(resource, "table_rows" | "table_cells");
    match error {
        Error::Source(TextTableError::Csv(csv_source::Error::Limit { resource, .. }))
        | Error::Source(TextTableError::Table(TableError::Limit { resource, .. }))
        | Error::TypedSource(yamaa_adapters::typed_csv::Error::Table(TableError::Limit {
            resource,
            ..
        }))
        | Error::ParquetSource(parquet_source::Error::ResourceLimit { resource, .. })
        | Error::ParquetSource(parquet_source::Error::Table(TableError::Limit {
            resource, ..
        })) => size(resource),
        Error::ResourceLimit(refusal) => size(refusal.resource),
        Error::Source(TextTableError::Table(TableError::Representation { .. }))
        | Error::TypedSource(yamaa_adapters::typed_csv::Error::Table(
            TableError::Representation { .. },
        ))
        | Error::ParquetSource(parquet_source::Error::Representation { .. }) => true,
        _ => false,
    }
}

/// Every cell of a decoded table, in order.
fn cells(table: &impl TableAccess<Error = Infallible>) -> Vec<Vec<ValueRef<'_>>> {
    (0..table.row_count())
        .map(|row| {
            (0..table.schema().columns().len())
                .map(|column| table.cell(row, column).expect("a decoded cell"))
                .collect()
        })
        .collect()
}

/// Read one SAS file under the shared decode: undeclared in windows-1252, the
/// fixed ODM fields in UTF-8, then every numeric field declared as each
/// narrower type in turn. Input bytes must be read or refused as a source
/// finding, never as an internal error. A panic in a Rust callback already
/// aborts under libFuzzer's hook; the reader also returns an internal error
/// when ReadStat reports an allocation failure, and under libFuzzer's
/// allocation limit that can only be an empty allocation ReadStat refuses.
pub fn sas(profile: SourceProfile, bytes: &[u8]) {
    let undeclared = decode(
        &source(profile, Vec::new(), Some("windows-1252")),
        bytes,
        SourceInterpretation::Ordinary,
    );
    refuse_internal(&undeclared);
    refuse_internal(&decode(
        &source(profile, Vec::new(), Some("utf-8")),
        bytes,
        SourceInterpretation::Odm,
    ));
    let Ok(table) = undeclared else {
        return;
    };
    let numeric: Vec<String> = table
        .schema()
        .columns()
        .iter()
        .filter(|column| column.kind == ColumnType::Float)
        .map(|column| column.name.clone())
        .collect();
    if numeric.is_empty() {
        return;
    }
    for kind in [ColumnType::Int, ColumnType::Date, ColumnType::DateTime] {
        let types = numeric.iter().map(|name| (name.clone(), kind)).collect();
        refuse_internal(&decode(
            &source(profile, types, Some("windows-1252")),
            bytes,
            SourceInterpretation::Ordinary,
        ));
    }
}

fn refuse_internal(result: &Result<ColumnTable, Error>) {
    if let Err(Error::SasSource(yamaa_adapters::sas_source::Error::Internal)) = result {
        panic!("the SAS reader returned an internal error for input bytes");
    }
}
