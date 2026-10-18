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
