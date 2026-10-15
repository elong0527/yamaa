//! Held Parquet source bytes to an owned engine table under the closed profile.
//!
//! Fixed source and table boundaries are logical admission checks, not process
//! memory guarantees. This codec performs no filesystem
//! access, resource lookup, host callbacks, semantic fallback, or publication.
mod compact;
mod compression;
mod delta;
mod framing;
mod metadata;
mod payload;
mod physical;
mod profile;
mod reader;
mod rle;
mod schema;
mod shape;
mod table;
mod type_name;

use crate::arrow_table::{ArrowTable, TableError, UTF8_OFFSETS};

/// Typed adapter failures; the consuming frontend supplies diagnostic location.
#[derive(Debug)]
pub enum Error {
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
            Self::OdmFields(_)
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
pub fn parse(content: &[u8]) -> Result<ArrowTable, Error> {
    convert(reader::read(content)?)
}

/// Admit the fixed ODM schema before decoding values, then expose its eleven
/// canonical text fields. Physical vendor chunks are validated and discarded.
pub fn parse_odm(content: &[u8]) -> Result<ArrowTable, Error> {
    convert(reader::read_odm(content)?)
}

fn convert(decoded: reader::Decoded) -> Result<ArrowTable, Error> {
    table::convert(decoded).map_err(|error| match error {
        table::Error::Invalid => Error::Malformed,
        table::Error::Representation => Error::Representation {
            representation: UTF8_OFFSETS,
        },
        table::Error::Table(error) => Error::Table(error),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_text_representation_has_no_invalid_parquet_finding() {
        let error = Error::from(reader::Error::Representation);
        assert!(matches!(
            error,
            Error::Representation {
                representation: "Arrow Utf8 signed 32-bit offsets"
            }
        ));
        assert!(error.finding("RAW", "raw.parquet").is_none());
    }
}
