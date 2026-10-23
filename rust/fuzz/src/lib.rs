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

use std::convert::Infallible;

use yamaa_adapters::{arrow_table::ArrowTable, decoder::Decoder, specification_run::Error};
use yamaa_core::{
    decoder::Decoder as _,
    specification::{SourceDeclaration, SourceInterpretation, SourceProfile},
    table::TableAccess,
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
) -> Result<ArrowTable, Error> {
    Decoder::<Infallible>::default()
        .table(source, bytes, interpretation, None)
        .map(|snapshot| snapshot.into_table())
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

fn refuse_internal(result: &Result<ArrowTable, Error>) {
    if let Err(Error::SasSource(yamaa_adapters::sas_source::Error::Internal)) = result {
        panic!("the SAS reader returned an internal error for input bytes");
    }
}
