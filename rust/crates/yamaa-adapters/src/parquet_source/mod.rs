//! Held Parquet source bytes to an owned engine table under the closed profile.
//!
//! Fixed source and table boundaries are logical admission checks, not process
//! memory guarantees. This codec performs no filesystem
//! access, resource lookup, host callbacks, semantic fallback, or publication.
#[cfg(feature = "parquet")]
mod compact;
#[cfg(feature = "parquet")]
mod compression;
#[cfg(feature = "parquet")]
mod delta;
#[cfg(feature = "parquet")]
mod framing;
#[cfg(feature = "parquet")]
mod metadata;
#[cfg(feature = "parquet")]
mod payload;
#[cfg(feature = "parquet")]
mod physical;
#[cfg(feature = "parquet")]
mod profile;
#[cfg(feature = "parquet")]
mod reader;
#[cfg(feature = "parquet")]
mod rle;
#[cfg(feature = "parquet")]
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
use crate::arrow_table::UTF8_OFFSETS;
use crate::arrow_table::{ArrowTable, TableError};

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
pub fn parse(content: &[u8]) -> Result<ArrowTable, Error> {
    convert(reader::read(content)?)
}

/// Admit the fixed ODM schema before decoding values, then expose its eleven
/// canonical text fields. Physical vendor chunks are validated and discarded.
#[cfg(feature = "parquet")]
pub fn parse_odm(content: &[u8]) -> Result<ArrowTable, Error> {
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
/// geometry or geography annotation, which the pinned Parquet release does
/// not know: admission removes it before the footer is decoded, so it is
/// reported absent, and [`parse`] refuses its column (PARQUET-7). Each type
/// is spelled as the Parquet crate names it, which is stable within one
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

#[cfg(feature = "parquet")]
fn convert(decoded: reader::Decoded) -> Result<ArrowTable, Error> {
    table::convert(decoded).map_err(|error| match error {
        table::Error::Invalid => Error::Malformed,
        table::Error::Representation => Error::Representation {
            representation: UTF8_OFFSETS,
        },
        table::Error::Table(error) => Error::Table(error),
    })
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
pub fn parse(_: &[u8]) -> Result<ArrowTable, Error> {
    Err(Error::ReaderUnavailable)
}
#[cfg(not(feature = "parquet"))]
pub fn parse_odm(_: &[u8]) -> Result<ArrowTable, Error> {
    Err(Error::ReaderUnavailable)
}
#[cfg(not(feature = "parquet"))]
pub fn signature(_: &[u8]) -> Result<Vec<ColumnSignature>, Error> {
    Err(Error::ReaderUnavailable)
}
