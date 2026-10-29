//! CDISC ODM XML clinical items read into one fixed-schema [`Table`].
//!
//! [`Engine::read_odm`] reads a plain XML file, or a plain or gzip TAR archive
//! holding one XML member, and returns every clinical item in source order
//! with its study, subject, event, form and item-group context. The thirty columns are
//! [`ODM_COLUMNS`]: `SourceOrdinal` is an integer counting the items from one,
//! `ValuePresent` and `IsNull` hold the text `true` or `false`, and every
//! other column is nullable text, so the table is an ordinary engine input.
//! Only a snapshot reads (ODM-1): a transactional document and one without
//! `ClinicalData` are refused, and audit records, signatures, annotations,
//! reference data and administrative data are skipped.
//!
//! A refusal is a [`FailureKind::Rejected`] failure with one issue, so a
//! host's error carries it (#3030 D73, D158): a `source_odm_*` condition for
//! the source, naming it as given in `context.path`; FILE-7's and FILE-8's
//! conditions for a path that reaches no regular file; `value_not_permitted`
//! at an option below its minimum; and `engine_rejected` for a file the host
//! cannot read. A build without the `odm` feature fails with
//! [`FailureKind::Unavailable`].
//!
//! The source is captured once through a resolver, as every other read is:
//! the engine's, or else the local file resolver, which reads a local caller
//! path from its parent and admits only a regular file, never a link (FILE-7,
//! FILE-8).

use std::path::Path;

use crate::diagnostic::{context_value_to_core, ContextValue, Report};
use crate::engine::Engine;
use crate::error::{Failure, FailureKind, TableError};
use crate::table::Table;
use crate::text::Action;

/// The clinical-item columns in table order.
pub const ODM_COLUMNS: [&str; 30] = yamaa_core::odm_items::COLUMNS;

/// Bounds and archive selection for [`Engine::read_odm`]. Every limit is
/// inclusive. The struct is `#[non_exhaustive]`: start from
/// [`OdmOptions::new`] and the `with_*` methods.
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
    /// Admit host limit values before reading `source`. Whole numbers within
    /// each limit's range are accepted; Boolean, missing, fractional, negative
    /// and unrepresentable values refuse as `value_not_permitted` at the
    /// argument, with the original value in its diagnostic context. Validation
    /// follows the reader's order: bytes, members, then depth.
    pub fn from_host_limits(
        source: &str,
        max_archive_members: ContextValue,
        max_expanded_bytes: ContextValue,
        max_xml_depth: ContextValue,
    ) -> Result<Self, Failure> {
        fn admit(
            source: &str,
            name: &str,
            minimum: u64,
            maximum: u64,
            value: ContextValue,
        ) -> Result<u64, Failure> {
            // R's ordinary numeric literals are doubles. A whole value has
            // the same diagnostic spelling as the integer Python supplied.
            let value = match value {
                ContextValue::Float(value) if value.is_finite() && value.fract() == 0.0 => {
                    if value >= i64::MIN as f64 && value < i64::MAX as f64 {
                        ContextValue::Int(value as i64)
                    } else if value >= 0.0 && value < u64::MAX as f64 {
                        ContextValue::Wide((value as u64).to_string())
                    } else {
                        ContextValue::Float(value)
                    }
                }
                value => value,
            };
            let number = match &value {
                ContextValue::Int(value) => u64::try_from(*value).ok(),
                ContextValue::Wide(value) => value.parse::<u64>().ok(),
                ContextValue::Float(value)
                    if value.is_finite()
                        && value.fract() == 0.0
                        && *value >= 0.0
                        && *value < u64::MAX as f64 =>
                {
                    Some(*value as u64)
                }
                _ => None,
            };
            if let Some(number) = number.filter(|number| (minimum..=maximum).contains(number)) {
                return Ok(number);
            }
            let issue = yamaa_core::diagnostic::invalid_odm_limit(
                name,
                minimum,
                context_value_to_core(&value),
            );
            Err(Failure::from_issues(Action::Read(source), &[issue]))
        }
        let bytes = admit(
            source,
            "max_expanded_bytes",
            yamaa_core::odm_items::Options::MIN_EXPANDED_BYTES,
            u64::MAX,
            max_expanded_bytes,
        )?;
        let members = admit(
            source,
            "max_archive_members",
            yamaa_core::odm_items::Options::MIN_ARCHIVE_MEMBERS,
            usize::MAX as u64,
            max_archive_members,
        )?;
        let depth = admit(
            source,
            "max_xml_depth",
            yamaa_core::odm_items::Options::MIN_XML_DEPTH as u64,
            usize::MAX as u64,
            max_xml_depth,
        )?;
        Ok(Self::new()
            .with_max_expanded_bytes(bytes)
            .with_max_archive_members(members as usize)
            .with_max_xml_depth(depth as usize))
    }

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

/// Every clinical item of the local file `source`, captured once through
/// the local file resolver; a refusal names the path as given.
fn read_local(source: &str, options: &OdmOptions) -> Result<Table, Failure> {
    table(
        yamaa_adapters::odm_xml::read(Path::new(source), &options.to_core()),
        source,
    )
}

impl Engine {
    /// Read every clinical item of one ODM 1.3 or 2.0 snapshot into a table,
    /// through this engine's resolver, which serves `source` under that name:
    /// one capture, as an input. Without a resolver, `source` is a local path
    /// that the local file resolver reads. A refusal is `cannot read
    /// <source>`, naming `source` as given, with one issue.
    /// [`FailureKind::Unavailable`] is the failure of a build without the
    /// `odm` feature.
    ///
    /// A resolver's source is held whole before it is read as XML or TAR,
    /// so the per-file source bound of 10 GiB applies first: a larger source
    /// fails as `source_odm_limit_exceeded` with the reason `ODM source
    /// exceeds 10737418240 bytes`, and a plain XML source over
    /// `max_expanded_bytes` is refused only once it is held. A local file is
    /// read in place instead: an archive streams and is never held whole,
    /// and a plain XML file over `max_expanded_bytes` is refused before it is
    /// read; one within that bound is held whole.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use yamaa::{Engine, EngineOptions, FailureKind, HostLanguage, MemoryResolver, OdmOptions};
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
    ///     let failure = result.unwrap_err();
    ///     assert_eq!(failure.kind, FailureKind::Unavailable { feature: "odm" });
    ///     assert_eq!(
    ///         failure.message,
    ///         "cannot read study/odm.xml: format feature not compiled in: odm"
    ///     );
    /// }
    /// ```
    pub fn read_odm(&self, source: &str, options: &OdmOptions) -> Result<Table, Failure> {
        match crate::internal::resolver_bridge(self.shared()) {
            Some(resolver) => table(
                yamaa_adapters::odm_xml::read_with_resolver(resolver, source, &options.to_core()),
                source,
            ),
            None => read_local(source, options),
        }
    }
}

/// The table, or the refusal of reading `source`, named as given: one issue,
/// or a capability or representation failure, which carries none.
fn table(
    result: Result<yamaa_adapters::public_table::PublicTable, yamaa_adapters::odm_xml::Error>,
    source: &str,
) -> Result<Table, Failure> {
    use yamaa_adapters::odm_xml;
    let action = Action::Read(source);
    match result {
        Ok(table) => Ok(Table::from_adapter(&table)),
        Err(odm_xml::Error::Unavailable) => Err(Failure::of(
            action,
            FailureKind::Unavailable { feature: "odm" },
        )),
        Err(odm_xml::Error::Table(error)) => Err(Failure::of(
            action,
            FailureKind::Table(Box::new(TableError::from_public(error))),
        )),
        Err(error) => {
            let issues = error.issue(source).into_iter().collect::<Vec<_>>();
            Err(Failure::rejected(action, &Report::from_issues(&issues)))
        }
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
