//! Held Parquet source bytes to an owned engine table under the closed profile.
//!
//! Resource policies are supplied by a trusted caller; they are not language
//! defaults or total-process memory guarantees. This codec performs no filesystem
//! access, resource lookup, host callbacks, semantic fallback, or publication.
mod compact;
mod compression;
mod delta;
mod expansion;
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

use crate::arrow_table::{ArrowTable, TableError, TableLimits};
pub use compression::Limits as CompressionLimits;
pub use framing::Limits as FramingLimits;
pub use metadata::Limits as MetadataLimits;
pub use reader::Limits;

/// Typed adapter failures; the consuming frontend supplies diagnostic location.
#[derive(Debug)]
pub enum Error {
    Malformed,
    Limit,
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
            Self::Limit | Self::Unavailable { .. } | Self::Table(_) => return None,
        };
        Some(SourceFinding::Parquet {
            finding,
            dataset,
            path,
        })
    }
    /// Unbounded owned compatibility. New callers use whole borrowed report admission;
    /// this existing Option signature and codec-boundary refusals stay unchanged.
    pub fn diagnostic(
        &self,
        dataset: &str,
        path: &str,
    ) -> Option<yamaa_core::diagnostic::Diagnostic> {
        self.finding(dataset, path)
            .map(yamaa_core::diagnostic::SourceFinding::owned_compatibility)
    }
}

impl From<reader::Error> for Error {
    fn from(error: reader::Error) -> Self {
        match error {
            reader::Error::Malformed | reader::Error::Profile(profile::Error::Invalid) => {
                Self::Malformed
            }
            reader::Error::Limit => Self::Limit,
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
/// Canonical table conversion shares the explicit retained-byte staging budget.
pub fn parse(
    content: &[u8],
    limits: Limits,
    table_limits: TableLimits,
) -> Result<ArrowTable, Error> {
    let decoded = reader::read(content, limits)?;
    table::convert(decoded, limits.retained_bytes, table_limits).map_err(|error| match error {
        table::Error::Limit => Error::Limit,
        table::Error::Invalid => Error::Malformed,
        table::Error::Table(error) => Error::Table(error),
    })
}
