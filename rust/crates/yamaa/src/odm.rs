//! CDISC ODM XML clinical items read into one fixed-schema [`Table`].
//!
//! [`read_odm`] reads a plain XML file, or a plain or gzip TAR archive holding
//! one XML member, and returns every clinical item in source order with its
//! study, subject, event, form and item-group context. The thirty columns are
//! [`ODM_COLUMNS`]: `SourceOrdinal` is an integer counting the items from one,
//! `ValuePresent` and `IsNull` hold the text `true` or `false`, and every
//! other column is nullable text, so the table is an ordinary engine input.
//! Refusals arrive as [`Error::Odm`] with a stable code; a build without the
//! `odm` feature returns [`Error::Unavailable`].
//!
//! The source is captured once through a resolver, as every other read is:
//! [`read_odm`] reads a local caller path through the local file resolver,
//! whose parent it selects and whose file must be a regular file, never a
//! link (FILE-7, FILE-8). [`Engine::read_odm`] reads through the engine's
//! resolver.

use std::path::Path;

use crate::engine::Engine;
use crate::error::{Error, OdmError, TableError};
use crate::table::Table;

/// The clinical-item columns in table order.
pub const ODM_COLUMNS: [&str; 30] = yamaa_core::odm_items::COLUMNS;

/// Bounds and archive selection for [`read_odm`]. Every limit is inclusive.
/// The struct is `#[non_exhaustive]`: start from [`OdmOptions::new`] and the
/// `with_*` methods.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OdmOptions {
    /// The exact TAR member name to read. Without it, a TAR input must hold
    /// exactly one `.xml` member outside macOS metadata.
    pub archive_member: Option<String>,
    /// Members a TAR input may contain, counting every member: at least 1,
    /// 10,000 by default.
    pub max_archive_members: usize,
    /// Bytes a plain XML file or the selected member may hold, and bytes a
    /// gzip archive may expand to across all members: at least 1,
    /// 268,435,456 (256 MiB) by default.
    pub max_expanded_bytes: u64,
    /// Element nesting depth, counting every element: at least 8, 64 by
    /// default, and never deeper than 256.
    pub max_xml_depth: usize,
}

impl Default for OdmOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl OdmOptions {
    /// The default bounds, selecting the only XML member of a TAR input.
    pub fn new() -> Self {
        let defaults = yamaa_core::odm_items::Options::default();
        Self {
            archive_member: defaults.archive_member,
            max_archive_members: defaults.max_archive_members,
            max_expanded_bytes: defaults.max_expanded_bytes,
            max_xml_depth: defaults.max_xml_depth,
        }
    }

    /// Read the TAR member with exactly this name.
    pub fn with_archive_member(mut self, member: impl Into<String>) -> Self {
        self.archive_member = Some(member.into());
        self
    }

    /// Bound the members a TAR input may contain.
    pub fn with_max_archive_members(mut self, limit: usize) -> Self {
        self.max_archive_members = limit;
        self
    }

    /// Bound the XML bytes and the expansion of a compressed archive.
    pub fn with_max_expanded_bytes(mut self, limit: u64) -> Self {
        self.max_expanded_bytes = limit;
        self
    }

    /// Bound the element nesting depth.
    pub fn with_max_xml_depth(mut self, limit: usize) -> Self {
        self.max_xml_depth = limit;
        self
    }

    fn to_core(&self) -> yamaa_core::odm_items::Options {
        yamaa_core::odm_items::Options {
            archive_member: self.archive_member.clone(),
            max_archive_members: self.max_archive_members,
            max_expanded_bytes: self.max_expanded_bytes,
            max_xml_depth: self.max_xml_depth,
        }
    }
}

/// Read every clinical item of one ODM 1.3 or 2.0 file into a table. The
/// file is captured once through the local file resolver.
///
/// # Example
///
/// ```
/// use yamaa::{read_odm, Error, OdmOptions, ODM_COLUMNS};
///
/// let path = std::env::temp_dir().join(format!("yamaa-read-odm-{}.xml", std::process::id()));
/// std::fs::write(
///     &path,
///     r#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">
///   <ClinicalData StudyOID="S" MetaDataVersionOID="M">
///     <SubjectData SubjectKey="01"><StudyEventData StudyEventOID="E">
///       <FormData FormOID="F"><ItemGroupData ItemGroupOID="G">
///         <ItemData ItemOID="AGE" Value="42"/>
///       </ItemGroupData></FormData>
///     </StudyEventData></SubjectData>
///   </ClinicalData>
/// </ODM>"#,
/// )
/// .unwrap();
/// let result = read_odm(&path, &OdmOptions::new());
/// std::fs::remove_file(&path).unwrap();
/// if cfg!(feature = "odm") {
///     let table = result.unwrap();
///     assert_eq!(table.rows(), 1);
///     assert_eq!(table.columns().len(), ODM_COLUMNS.len());
/// } else {
///     assert!(matches!(result, Err(Error::Unavailable { feature: "odm" })));
/// }
/// ```
pub fn read_odm(path: &Path, options: &OdmOptions) -> Result<Table, Error> {
    table(yamaa_adapters::odm_xml::read(path, &options.to_core()))
}

impl Engine {
    /// [`read_odm`] through this engine's resolver, which serves `source`
    /// under that name: one capture, as an input. Without a resolver,
    /// `source` is a local path read exactly as [`read_odm`] reads it.
    ///
    /// A resolver's source is held whole before it is read as XML or TAR,
    /// so the per-file source bound of 10 GiB applies first: a larger source
    /// fails with the code `limit` and the message `ODM source exceeds
    /// 10737418240 bytes`, and a plain XML source over `max_expanded_bytes`
    /// is refused only once it is held. [`read_odm`] streams a local file
    /// instead: it never holds an archive whole and refuses a plain XML file
    /// over `max_expanded_bytes` before reading it.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use yamaa::{Engine, EngineOptions, Error, HostLanguage, MemoryResolver, OdmOptions};
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "study/odm.xml", "odm:1", Arc::from(
    ///     &br#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">
    ///   <ClinicalData StudyOID="S" MetaDataVersionOID="M">
    ///     <SubjectData SubjectKey="01"><StudyEventData StudyEventOID="E">
    ///       <FormData FormOID="F"><ItemGroupData ItemGroupOID="G">
    ///         <ItemData ItemOID="AGE" Value="42"/>
    ///       </ItemGroupData></FormData>
    ///     </StudyEventData></SubjectData>
    ///   </ClinicalData>
    /// </ODM>"#[..],
    /// ));
    /// let engine = Engine::new(
    ///     EngineOptions::new(HostLanguage::Python, "doctest".into(), "0".into())
    ///         .with_resolver(resolver),
    /// );
    /// let result = engine.read_odm("study/odm.xml", &OdmOptions::new());
    /// if cfg!(feature = "odm") {
    ///     assert_eq!(result.unwrap().rows(), 1);
    /// } else {
    ///     assert!(matches!(result, Err(Error::Unavailable { feature: "odm" })));
    /// }
    /// ```
    pub fn read_odm(&self, source: &str, options: &OdmOptions) -> Result<Table, Error> {
        match crate::internal::resolver_bridge(self.shared()) {
            Some(resolver) => table(yamaa_adapters::odm_xml::read_with_resolver(
                resolver,
                source,
                &options.to_core(),
            )),
            None => read_odm(Path::new(source), options),
        }
    }
}

fn table(
    result: Result<yamaa_adapters::public_table::PublicTable, yamaa_adapters::odm_xml::Error>,
) -> Result<Table, Error> {
    use yamaa_adapters::odm_xml;
    match result {
        Ok(table) => Ok(Table::from_adapter(&table)),
        Err(odm_xml::Error::Unavailable) => Err(Error::Unavailable { feature: "odm" }),
        Err(odm_xml::Error::Read(error)) => Err(Error::Odm(OdmError::from_core(&error))),
        Err(odm_xml::Error::Table(error)) => Err(Error::Table(TableError::from_ipc(error))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_build_from_the_core_defaults() {
        let options = OdmOptions::new();
        assert_eq!(options, OdmOptions::default());
        assert_eq!(options.to_core(), yamaa_core::odm_items::Options::default());
        let custom = OdmOptions::new()
            .with_archive_member("study/odm.xml")
            .with_max_archive_members(3)
            .with_max_expanded_bytes(1024)
            .with_max_xml_depth(8);
        assert_eq!(custom.archive_member.as_deref(), Some("study/odm.xml"));
        assert_eq!(
            (
                custom.max_archive_members,
                custom.max_expanded_bytes,
                custom.max_xml_depth
            ),
            (3, 1024, 8)
        );
        assert_eq!(ODM_COLUMNS[29], "SourceOrdinal");
    }
}
