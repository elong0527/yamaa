//! Held Parquet source bytes to an owned engine table under the closed profile.
//!
//! Fixed source and table boundaries are logical admission checks, not process
//! memory guarantees. This codec performs no filesystem
//! access, resource lookup, host callbacks, semantic fallback, or publication.
#[cfg(feature = "parquet")]
mod closed_profile;
#[cfg(feature = "parquet")]
mod columns;
#[cfg(feature = "parquet")]
mod compact;
#[cfg(feature = "parquet")]
mod compression;
#[cfg(feature = "parquet")]
mod conversion;
#[cfg(feature = "parquet")]
mod delta;
/// The differential harness against parquet 55 (#3878).
#[cfg(all(test, feature = "parquet"))]
mod differential;
#[cfg(feature = "parquet")]
mod footer;
#[cfg(feature = "parquet")]
pub(crate) mod format;
#[cfg(feature = "parquet")]
mod framing;
#[cfg(feature = "parquet")]
mod metadata;
#[cfg(feature = "parquet")]
mod payload;
/// parquet 55's column reader, which the differential harness reads by.
#[cfg(all(test, feature = "parquet"))]
mod physical;
#[cfg(feature = "parquet")]
mod profile;
#[cfg(feature = "parquet")]
mod reader;
#[cfg(feature = "parquet")]
mod rle;
/// parquet 55's metadata decoder, which the differential harness reads by.
#[cfg(all(test, feature = "parquet"))]
mod row_groups;
#[cfg(feature = "parquet")]
mod schema;
#[cfg(feature = "parquet")]
mod shape;
#[cfg(feature = "parquet")]
mod table;
/// The Parquet inputs that the integration tests write at test time.
#[cfg(all(test, feature = "parquet"))]
#[path = "../../tests/support/parquet_files.rs"]
mod test_files;
#[cfg(feature = "parquet")]
mod type_name;
#[cfg(feature = "parquet")]
mod wire;

#[cfg(feature = "parquet")]
use crate::column_table::UTF8_OFFSETS;
use crate::column_table::{ColumnTable, TableError};

/// Typed adapter failures; the consuming frontend supplies diagnostic location.
#[derive(Debug)]
pub enum Error {
    /// This build omits the optional Parquet reader and writer.
    ReaderUnavailable,
    OdmFields(Vec<yamaa_core::odm_source::SchemaError>),
    Malformed,
    Representation {
        representation: &'static str,
    },
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
    Unavailable {
        codec: i64,
    },
    EmptyName {
        field: usize,
    },
    DuplicateName {
        field: String,
    },
    Unsupported {
        field: String,
        stored_type: String,
    },
    Value {
        field: String,
        row: usize,
        value: i64,
    },
    Table(TableError),
}

impl Error {
    /// Borrow codec facts into the core-owned source finding without cloning errors.
    pub fn finding<'a>(
        &'a self,
        dataset: &'a str,
        path: &'a str,
    ) -> Option<yamaa_core::diagnostic::SourceFinding<'a>> {
        use yamaa_core::diagnostic::{ParquetFinding as P, SourceFinding};
        let finding = match self {
            Self::Malformed => P::Invalid,
            Self::EmptyName { field } => P::EmptyName { field: *field },
            Self::DuplicateName { field } => P::DuplicateName { field },
            Self::Unsupported { field, stored_type } => P::Unsupported { field, stored_type },
            Self::Value { field, row, value } => P::Value {
                field,
                row: *row,
                value: *value,
            },
            Self::ReaderUnavailable
            | Self::OdmFields(_)
            | Self::Representation { .. }
            | Self::ResourceLimit { .. }
            | Self::Unavailable { .. }
            | Self::Table(_) => return None,
        };
        Some(SourceFinding::Parquet {
            finding,
            dataset,
            path,
        })
    }
}

#[cfg(feature = "parquet")]
impl From<reader::Error> for Error {
    fn from(error: reader::Error) -> Self {
        match error {
            reader::Error::OdmFields(fields) => Self::OdmFields(fields),
            reader::Error::Representation => Self::Representation {
                representation: UTF8_OFFSETS,
            },
            reader::Error::Malformed | reader::Error::Profile(profile::Error::Invalid) => {
                Self::Malformed
            }
            reader::Error::ResourceLimit {
                resource,
                limit,
                actual,
            } => Self::ResourceLimit {
                resource,
                limit,
                actual,
            },
            reader::Error::Unavailable(codec) => Self::Unavailable { codec },
            reader::Error::Profile(profile::Error::EmptyName { field }) => {
                Self::EmptyName { field }
            }
            reader::Error::Profile(profile::Error::DuplicateName { field }) => {
                Self::DuplicateName { field }
            }
            reader::Error::Profile(profile::Error::Unsupported { field, stored_type }) => {
                Self::Unsupported { field, stored_type }
            }
            reader::Error::Profile(profile::Error::Value { field, row, value }) => {
                Self::Value { field, row, value }
            }
        }
    }
}

/// Decode every physical chunk before reporting closed-profile findings. The
/// resulting snapshot owns its values independently of `content`. Empty strings
/// remain present here; input declarations apply missing-string policy later.
#[cfg(feature = "parquet")]
pub fn parse(content: &[u8]) -> Result<ColumnTable, Error> {
    convert(reader::read(content)?)
}

/// Admit the fixed ODM schema before decoding values, then expose its eleven
/// canonical text fields. Physical vendor chunks are validated and discarded.
#[cfg(feature = "parquet")]
pub fn parse_odm(content: &[u8]) -> Result<ColumnTable, Error> {
    convert(reader::read_odm(content)?)
}

/// One stored column's schema, as the public conformance runner compares two
/// Parquet files (`parquet_signature`): its path, physical type, logical and
/// converted type annotations with a decimal's precision and scale, and its
/// maximum definition and repetition levels, which say whether it holds
/// missing values and whether it repeats. [`parse`] reads several of these
/// alike, INT32 and INT64 as one integer type and a column whatever its
/// nullability, so two equal tables can be stored differently. Annotations
/// are kept as stored: a legacy converted type alone differs from the logical
/// type that the runner's reader derives from it. The exception is a variant,
/// geometry or geography annotation, which the footer reads as none, so it
/// is reported absent, and [`parse`] refuses its column (PARQUET-7). Each
/// type is spelled as parquet 55.2.0 named it, which is stable within one
/// build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSignature {
    pub path: String,
    pub physical_type: String,
    pub logical_type: Option<String>,
    pub converted_type: String,
    pub precision: i32,
    pub scale: i32,
    pub max_definition_level: i16,
    pub max_repetition_level: i16,
}

/// Each stored column's [`ColumnSignature`], in schema order, read from the
/// footer alone through the same bounded admission as [`parse`].
#[cfg(feature = "parquet")]
pub fn signature(content: &[u8]) -> Result<Vec<ColumnSignature>, Error> {
    Ok(reader::signature(content)?)
}

/// A held file read by partition (#3792): what its scan admitted, which
/// decodes any range of its records again (#3830).
#[cfg(feature = "parquet")]
pub struct Scanned {
    /// Boxed: the held file's footer and pages are large beside a CSV
    /// reader's offsets.
    runs: Box<reader::Runs>,
}

/// A held file read by partition, which this build cannot read.
#[cfg(not(feature = "parquet"))]
pub struct Scanned {
    never: std::convert::Infallible,
}

/// Where a scan's table admission falls in the whole read's order: every
/// partition is admitted before any value converts.
#[cfg(feature = "parquet")]
const ADMIT: reader::Position = (17, 0, 0);
/// Where its conversion falls: each partition's values, in order.
#[cfg(feature = "parquet")]
const BUILD: reader::Position = (18, 0, 0);

/// Scan held bytes by partition (#3792), with `odm` as [`parse_odm`] does:
/// the row groups in partitions of at most `rows` records, a row group of
/// more records a partition of its own. Each partition is decoded once, in
/// order, given to `visit` with the position of its first record among the
/// file's records, and dropped (#3830): no two partitions are decoded at
/// once. Every finding [`parse`] reports is reported in the same order, a
/// value's row being its row in the whole file, except that each partition,
/// not the whole file, is admitted within the row and cell limits: the
/// whole read checks each stage over every partition before the next, so
/// the first finding is the one whose stage and column come first, then
/// whose partition does.
#[cfg(feature = "parquet")]
pub fn scan(
    content: &[u8],
    odm: bool,
    rows: std::num::NonZeroUsize,
    visit: &mut dyn FnMut(ColumnTable, usize),
) -> Result<Scanned, Error> {
    use yamaa_core::table::TableAccess;
    let runs = reader::Runs::open(content, odm, Some(rows), reader::NATIVE)?;
    let ready = runs.deferred().is_ok();
    let mut first: Option<(reader::Position, Error)> = None;
    let mut before = 0usize;
    for partition in 0..runs.len() {
        // The whole read decodes every partition's columns before any later
        // check, so the first partition that does not decode is its finding.
        let raw = runs.raw(partition)?;
        if ready {
            let found = match runs.decoded(raw, partition, before) {
                Err((position, error)) => Some((position, Error::from(error))),
                Ok(decoded) => match table::admit(&decoded) {
                    Err(error) => Some((ADMIT, table_error(error))),
                    Ok(rows) => match table::build(decoded, rows) {
                        Err(error) => Some((BUILD, table_error(error))),
                        Ok(table) => {
                            debug_assert_eq!(table.row_count(), runs.records(partition));
                            if first.is_none() {
                                visit(table, before);
                            }
                            None
                        }
                    },
                },
            };
            if let Some((position, error)) = found {
                if first.as_ref().is_none_or(|(at, _)| position < *at) {
                    first = Some((position, error));
                }
            }
        }
        before += runs.records(partition);
    }
    runs.deferred()?;
    match first {
        Some((_, error)) => Err(error),
        None => Ok(Scanned {
            runs: Box::new(runs),
        }),
    }
}

/// The table of records `range` of a file [`scan`] admitted (#3830),
/// decoded again from the row groups that hold them.
#[cfg(feature = "parquet")]
pub fn decode_range(
    scanned: &Scanned,
    range: std::ops::Range<usize>,
) -> Result<ColumnTable, Error> {
    use yamaa_core::table::TableAccess;
    let (decoded, offset) = scanned.runs.range(range.clone())?;
    let table = convert(decoded)?;
    if offset == 0 && table.row_count() == range.len() {
        return Ok(table);
    }
    table
        .rows(offset..offset + range.len())
        .map_err(Error::Table)
}

/// The table schema held bytes decode to, with `odm` as [`parse_odm`] does,
/// read from the footer alone (#3792); `None` when the footer does not read.
#[cfg(feature = "parquet")]
pub fn schema(content: &[u8], odm: bool) -> Option<yamaa_core::table::TableSchema> {
    reader::table_schema(content, odm, reader::NATIVE).ok()
}

#[cfg(feature = "parquet")]
fn convert(decoded: reader::Decoded) -> Result<ColumnTable, Error> {
    table::convert(decoded).map_err(table_error)
}

#[cfg(feature = "parquet")]
fn table_error(error: table::Error) -> Error {
    match error {
        table::Error::Invalid => Error::Malformed,
        table::Error::Representation => Error::Representation {
            representation: UTF8_OFFSETS,
        },
        table::Error::Table(error) => Error::Table(error),
    }
}

#[cfg(all(test, feature = "parquet"))]
mod tests {
    use super::*;
    use arrow_array::{ArrayRef, Int32Array, Int64Array, RecordBatch, StringArray};
    use arrow_schema::{DataType, Field, Schema};
    use parquet::arrow::ArrowWriter;
    use std::sync::Arc;
    use yamaa_core::table::TableAccess;

    /// A file of one column, `field`, holding `values`.
    fn written(field: Field, values: ArrayRef) -> Vec<u8> {
        let schema = Arc::new(Schema::new(vec![field]));
        let batch = RecordBatch::try_new(schema.clone(), vec![values]).unwrap();
        let mut bytes = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut bytes, schema, None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        bytes
    }

    /// The signature tells apart what the reader reads alike: an INT32 from
    /// an INT64 column, and a required column from an optional one.
    #[test]
    fn the_signature_holds_what_the_reader_reads_alike() {
        let wide = written(
            Field::new("AGE", DataType::Int64, true),
            Arc::new(Int64Array::from(vec![34, 51])),
        );
        let narrow = written(
            Field::new("AGE", DataType::Int32, true),
            Arc::new(Int32Array::from(vec![34, 51])),
        );
        let required = written(
            Field::new("AGE", DataType::Int64, false),
            Arc::new(Int64Array::from(vec![34, 51])),
        );
        let text = written(
            Field::new("NAME", DataType::Utf8, true),
            Arc::new(StringArray::from(vec![Some("ab"), None])),
        );
        for (left, right) in [(&wide, &narrow), (&wide, &required)] {
            let (left_table, right_table) = (parse(left).unwrap(), parse(right).unwrap());
            assert_eq!(left_table.schema(), right_table.schema());
            assert_eq!(left_table.row_count(), right_table.row_count());
            for row in 0..left_table.row_count() {
                assert_eq!(
                    left_table.cell(row, 0).unwrap(),
                    right_table.cell(row, 0).unwrap()
                );
            }
            assert_ne!(signature(left).unwrap(), signature(right).unwrap());
        }
        let [column] = signature(&wide).unwrap().try_into().unwrap();
        assert_eq!(column.path, "AGE");
        assert_eq!(column.physical_type, "INT64");
        assert_eq!(column.max_definition_level, 1);
        assert_eq!(column.max_repetition_level, 0);
        assert_eq!(signature(&narrow).unwrap()[0].physical_type, "INT32");
        assert_eq!(signature(&required).unwrap()[0].max_definition_level, 0);
        let [name] = signature(&text).unwrap().try_into().unwrap();
        assert_eq!(name.physical_type, "BYTE_ARRAY");
        assert!(name.logical_type.is_some());
        // A file signs alike however often it is read, and a file that does
        // not admit has no signature.
        assert_eq!(signature(&wide).unwrap(), signature(&wide).unwrap());
        assert!(signature(b"PAR1").is_err());
        assert!(signature(&wide[..wide.len() - 1]).is_err());
    }

    /// The pinned Parquet release does not know the geometry annotation, so
    /// the signature reports it absent, and the reader refuses the column.
    #[test]
    fn a_later_annotation_is_absent_from_the_signature_and_refused() {
        let geometry = &super::test_files::file("unsupported-geometry")[..];
        let [column] = signature(geometry).unwrap().try_into().unwrap();
        assert_eq!(column.path, "FIELD");
        assert_eq!(column.physical_type, "BYTE_ARRAY");
        assert_eq!(column.logical_type, None);
        assert!(matches!(
            parse(geometry),
            Err(Error::Unsupported { field, stored_type }) if field == "FIELD" && stored_type == "binary"
        ));
    }

    #[test]
    fn unsupported_text_representation_has_no_invalid_parquet_finding() {
        let error = Error::from(reader::Error::from(compact::Error::Representation));
        assert!(matches!(
            error,
            Error::Representation {
                representation: "Arrow Utf8 signed 32-bit offsets"
            }
        ));
        assert!(error.finding("RAW", "raw.parquet").is_none());
    }
}

/// Omitted codecs report a capability failure before inspecting source bytes.
#[cfg(not(feature = "parquet"))]
pub fn parse(_: &[u8]) -> Result<ColumnTable, Error> {
    Err(Error::ReaderUnavailable)
}
#[cfg(not(feature = "parquet"))]
pub fn parse_odm(_: &[u8]) -> Result<ColumnTable, Error> {
    Err(Error::ReaderUnavailable)
}
#[cfg(not(feature = "parquet"))]
pub fn signature(_: &[u8]) -> Result<Vec<ColumnSignature>, Error> {
    Err(Error::ReaderUnavailable)
}
#[cfg(not(feature = "parquet"))]
pub fn scan(
    _: &[u8],
    _: bool,
    _: std::num::NonZeroUsize,
    _: &mut dyn FnMut(ColumnTable, usize),
) -> Result<Scanned, Error> {
    Err(Error::ReaderUnavailable)
}
#[cfg(not(feature = "parquet"))]
pub fn decode_range(scanned: &Scanned, _: std::ops::Range<usize>) -> Result<ColumnTable, Error> {
    match scanned.never {}
}
#[cfg(not(feature = "parquet"))]
pub fn schema(_: &[u8], _: bool) -> Option<yamaa_core::table::TableSchema> {
    None
}
