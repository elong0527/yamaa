//! CDISC ODM XML clinical items read into the fixed clinical-item table,
//! enabled by the default `odm` feature.
//!
//! One call reads a plain XML file or a plain or gzip TAR archive holding
//! one XML member, and returns every clinical item in source order with its
//! projected study, subject, event, form and item-group context. The columns,
//! options and refusals belong to `yamaa_core::odm_items`.
//!
//! Admission:
//! - Input selection. A file whose first block is a TAR header (directly or
//!   after gzip decompression) is a TAR archive; a `.tar`, `.tar.gz` or
//!   `.tgz` name that is not one is refused. bzip2, xz and zstd archives are
//!   refused. A plain XML file refuses `archive_member` and must fit
//!   `max_expanded_bytes`. A specification's `.xml` input is a plain XML
//!   file by its extension: its captured bytes are read as XML under the
//!   default options, and nothing selects an archive (INPUT-1).
//! - TAR selection. Every member counts toward `max_archive_members`.
//!   Without `archive_member` the candidates are names ending in `.xml`
//!   (ASCII case-insensitive) outside `._*` files and `__MACOSX` folders, and
//!   exactly one must exist; `archive_member` selects only that exact name.
//!   The selected member needs a relative path without `..`, must be a
//!   regular (not sparse) file and must fit `max_expanded_bytes`. A gzip
//!   archive may expand to at most `max_expanded_bytes` across all members.
//! - XML. The document is UTF-8, or declared US-ASCII with only ASCII source
//!   bytes, and well formed under XML 1.0 and Namespaces
//!   1.0: one root, bound prefixes, valid characters and references, valid
//!   namespace URIs, no DOCTYPE and so no entities beyond the predefined five.
//!   Text nodes, attribute values, comments and processing instructions stay
//!   below 10,000,000 bytes, names within 50,000 bytes and nesting within
//!   `max_xml_depth` and 256 elements, the bounds libxml2 enforces by default.
//! - ODM. The root is `ODM` in the 1.3 or 2.0 namespace with an agreeing
//!   `ODMVersion`. Elements in other namespaces are ignored. Metadata names
//!   come from definitions outside `ClinicalData` that precede the item,
//!   keyed by exact (study OID, metadata version OID, OID); conflicting names
//!   are refused and namespaced name attributes on data elements override.
//!   An item needs `FormData` plus one `ItemGroupData`, or two nested item
//!   groups where the outer supplies the form columns. Nested containers,
//!   nested items, ODM 1.3 typed `ItemData*` elements in clinical data and
//!   ambiguous values are refused; `ItemData` outside `ClinicalData` is
//!   ignored. A container inside a foreign element is closed normally, even
//!   when that element's local name is `ItemData`.
//! - Snapshots (ODM-1). A `FileType` other than `Snapshot` on the root, or a
//!   `TransactionType` of `Remove` on any element read, is refused as not a
//!   snapshot, and a `FileType` or `TransactionType` value ODM does not
//!   define as an invalid attribute; `Insert`, `Update`, `Upsert` and
//!   `Context` read as the present value. `AuditRecord`, `Signature`,
//!   `Annotation`, `ReferenceData` and `AdminData` are skipped with all
//!   their content, which is checked only as XML and against the limits. A
//!   document without `ClinicalData` is refused once it has been read.
//!
//! Every refusal reports one issue ([`Error::issue`]): a `source_odm_*`
//! condition for the source, FILE-7's and FILE-8's conditions for a path
//! that reaches no regular file, `value_not_permitted` for an option below
//! its minimum, and the opaque resource refusal for a file the host cannot
//! read (#3030 D73, D158).
#[cfg(feature = "odm")]
mod archive;
#[cfg(feature = "odm")]
mod items;
#[cfg(feature = "odm")]
mod source;
#[cfg(feature = "odm")]
mod xml;

use crate::{
    column_table::ColumnTable,
    host_resources::Resolver,
    public_table::{PublicTable, TableError},
};
use std::{path::Path, sync::Arc};
use yamaa_core::{diagnostic::Diagnostic, odm_items::Options};

/// Adapter failures around the core-owned read refusals.
#[derive(Debug)]
pub enum Error {
    /// This build omits the optional ODM XML reader.
    Unavailable,
    /// The source path reaches no regular file, or the resolver cannot
    /// serve it, before the reader reads anything.
    Resource(crate::file_resources::Error),
    /// The source or an option is refused (ODM-1).
    Read(yamaa_core::odm_items::Error),
    /// The finished table could not be represented.
    Table(TableError),
}

impl From<yamaa_core::odm_items::Error> for Error {
    fn from(error: yamaa_core::odm_items::Error) -> Self {
        Self::Read(error)
    }
}

impl Error {
    /// Whether the document itself is refused, as one of ODM-1's
    /// `source_odm_*` conditions: not a build without the reader, a path or
    /// an option refused before the read, a source the host cannot read, or
    /// a table that cannot be represented.
    pub fn refuses_document(&self) -> bool {
        use yamaa_core::odm_items::Error as R;
        matches!(self, Self::Read(refusal) if !matches!(refusal, R::Option { .. } | R::Io { .. }))
    }

    /// The issue a refusal of the source `source`, named as the caller gave
    /// it, reports; `None` for a build without the reader and for a table
    /// that cannot be represented, which carry no issue. A path that reaches
    /// no regular file fails as any caller's own path does (FILE-7, FILE-8,
    /// FILE-11), and a refusal of the source or an option as ODM-1 says.
    pub fn issue(&self, source: &str) -> Option<Diagnostic> {
        match self {
            Self::Unavailable | Self::Table(_) => None,
            Self::Resource(error) => {
                Some(crate::file_resources::caller_read_failure(source, error))
            }
            Self::Read(refusal) => Some(
                yamaa_core::diagnostic::OdmReadFinding {
                    refusal,
                    path: source,
                    dataset: None,
                }
                .diagnostic(),
            ),
        }
    }
}

/// Read every clinical item of one ODM file into the clinical-item table.
/// The file is a caller path, captured once through the local file resolver:
/// its parent is selected and the file must be a regular file, never a link.
#[cfg(feature = "odm")]
pub fn read(path: &Path, options: &Options) -> Result<PublicTable, Error> {
    source::read(path, options)
}

/// Read every clinical item of the ODM source a caller's resolver serves
/// under `source`, captured once as an input.
#[cfg(feature = "odm")]
pub fn read_with_resolver(
    resolver: Arc<dyn Resolver>,
    source: &str,
    options: &Options,
) -> Result<PublicTable, Error> {
    source::read_with_resolver(resolver, source, options)
}

/// Read every clinical item of a specification input's captured `.xml`
/// file into the clinical-item table, under `read_odm`'s default options.
/// The bytes are the document itself: nothing is sniffed, so an archive is
/// read as XML and refused as such (INPUT-1, ODM-1).
#[cfg(feature = "odm")]
pub fn read_input(bytes: &[u8]) -> Result<ColumnTable, Error> {
    source::read_input(bytes)
}

/// A refusal raised inside a byte source while the XML tokenizer pulls from
/// it. It travels as an `io::Error` payload and maps back to the refusal.
#[cfg(feature = "odm")]
#[derive(Clone, Debug)]
enum Fault {
    Archive(&'static str),
    Limit(yamaa_core::odm_items::Limit),
    Character { position: u64, value: u32 },
    Ascii { position: u64, value: u8 },
    Encoding(&'static str),
}

#[cfg(feature = "odm")]
impl Fault {
    fn into_io(self) -> std::io::Error {
        std::io::Error::other(self)
    }

    fn into_error(self) -> yamaa_core::odm_items::Error {
        use yamaa_core::odm_items::Error;
        match self {
            Self::Archive(reason) => Error::Archive { reason },
            Self::Limit(limit) => Error::Limit(limit),
            Self::Character { position, value } => Error::Malformed {
                position,
                reason: format!("invalid XML character U+{value:04X}"),
            },
            Self::Ascii { position, value } => Error::Malformed {
                position,
                reason: format!("US-ASCII XML contains non-ASCII byte 0x{value:02X}"),
            },
            Self::Encoding(encoding) => Error::Encoding {
                encoding: encoding.into(),
            },
        }
    }

    /// Recover a refusal from an I/O error, or report the host's failure to
    /// read the source itself.
    fn from_io(error: &std::io::Error) -> yamaa_core::odm_items::Error {
        match error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<Self>())
        {
            Some(fault) => fault.clone().into_error(),
            None => yamaa_core::odm_items::Error::Io {
                reason: error.to_string(),
            },
        }
    }
}

#[cfg(feature = "odm")]
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.clone().into_error())
    }
}

#[cfg(feature = "odm")]
impl std::error::Error for Fault {}

/// ODM XML input is unavailable when the `odm` feature is disabled.
#[cfg(not(feature = "odm"))]
pub fn read(_path: &Path, _options: &Options) -> Result<PublicTable, Error> {
    Err(Error::Unavailable)
}

/// ODM XML input is unavailable when the `odm` feature is disabled; the
/// resolver is not asked for anything.
#[cfg(not(feature = "odm"))]
pub fn read_with_resolver(
    _resolver: Arc<dyn Resolver>,
    _source: &str,
    _options: &Options,
) -> Result<PublicTable, Error> {
    Err(Error::Unavailable)
}

/// A specification's `.xml` input is unavailable when the `odm` feature is
/// disabled, as every other format a build does not compile.
#[cfg(not(feature = "odm"))]
pub fn read_input(_bytes: &[u8]) -> Result<ColumnTable, Error> {
    Err(Error::Unavailable)
}
