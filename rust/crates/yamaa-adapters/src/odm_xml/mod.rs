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
//!   `max_expanded_bytes`.
//! - TAR selection. Every member counts toward `max_archive_members`.
//!   Without `archive_member` the candidates are names ending in `.xml`
//!   (ASCII case-insensitive) outside `._*` files and `__MACOSX` folders, and
//!   exactly one must exist; `archive_member` selects only that exact name.
//!   The selected member needs a relative path without `..`, must be a
//!   regular (not sparse) file and must fit `max_expanded_bytes`. A gzip
//!   archive may expand to at most `max_expanded_bytes` across all members.
//! - XML. The document is UTF-8 and well formed under XML 1.0 and Namespaces
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
#[cfg(feature = "odm")]
mod archive;
#[cfg(feature = "odm")]
mod items;
#[cfg(feature = "odm")]
mod source;
#[cfg(feature = "odm")]
mod xml;

use crate::{ipc_table::TableError, public_table::PublicTable};
use std::path::Path;
use yamaa_core::odm_items::Options;

/// Adapter failures around the core-owned read refusals.
#[derive(Debug)]
pub enum Error {
    /// This build omits the optional ODM XML reader.
    Unavailable,
    /// The input cannot be represented by the clinical-item table.
    Read(yamaa_core::odm_items::Error),
    /// The finished table could not be represented.
    Table(TableError),
}

impl From<yamaa_core::odm_items::Error> for Error {
    fn from(error: yamaa_core::odm_items::Error) -> Self {
        Self::Read(error)
    }
}

/// Read every clinical item of one ODM file into the clinical-item table.
#[cfg(feature = "odm")]
pub fn read(path: &Path, options: &Options) -> Result<PublicTable, Error> {
    source::read(path, options)
}

/// A refusal raised inside a byte source while the XML tokenizer pulls from
/// it. It travels as an `io::Error` payload and maps back to the refusal.
#[cfg(feature = "odm")]
#[derive(Clone, Debug)]
enum Fault {
    Archive(&'static str),
    Limit(yamaa_core::odm_items::Limit),
    Character { position: u64, value: u32 },
    Encoding(&'static str),
}

#[cfg(feature = "odm")]
impl Fault {
    fn into_io(self) -> std::io::Error {
        std::io::Error::other(self)
    }

    fn into_error(self, path: &str) -> yamaa_core::odm_items::Error {
        use yamaa_core::odm_items::Error;
        match self {
            Self::Archive(reason) => Error::Archive {
                path: path.into(),
                reason,
            },
            Self::Limit(limit) => Error::Limit(limit),
            Self::Character { position, value } => Error::Malformed {
                position,
                reason: format!("invalid XML character U+{value:04X}"),
            },
            Self::Encoding(encoding) => Error::Encoding {
                encoding: encoding.into(),
            },
        }
    }

    /// Recover a refusal from an I/O error, or report the I/O failure itself.
    fn from_io(error: &std::io::Error, path: &str) -> yamaa_core::odm_items::Error {
        match error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<Self>())
        {
            Some(fault) => fault.clone().into_error(path),
            None => yamaa_core::odm_items::Error::Io {
                path: path.into(),
                reason: error.to_string(),
            },
        }
    }
}

#[cfg(feature = "odm")]
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.clone().into_error("ODM input"))
    }
}

#[cfg(feature = "odm")]
impl std::error::Error for Fault {}

/// ODM XML input is unavailable when the `odm` feature is disabled.
#[cfg(not(feature = "odm"))]
pub fn read(_path: &Path, _options: &Options) -> Result<PublicTable, Error> {
    Err(Error::Unavailable)
}
