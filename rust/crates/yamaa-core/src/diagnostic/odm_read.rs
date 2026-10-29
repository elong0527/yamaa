//! The refusals of an ODM source (ODM-1) as the issues they report.
//!
//! Every refusal of the source itself is a registered `source_odm_*`
//! condition in the `ingest` phase, naming the source in `context.path`
//! (#3030 D73, D158). An option below its minimum is refused before the
//! source is read, as `value_not_permitted` at the option's name, and a
//! source the host cannot read is the opaque resource refusal every
//! unreadable file reports. A path that reaches no regular file is refused
//! before the reader runs, with FILE-7's and FILE-8's conditions.
use super::{ConditionCode as C, Context, Diagnostic};
use crate::application_issue::{rejected, Fault, Step};
use crate::json::Value as V;
use crate::odm_items::Error;

/// One refusal of an ODM source with what names the source: its path as the
/// caller gave it to `read_odm`, or as an input declares it, with that
/// input's dataset. An input's refusal is reported at `input.<dataset>.path`
/// and names the dataset too; a `read_odm` refusal has no spec path.
#[derive(Clone, Copy, Debug)]
pub struct OdmReadFinding<'a> {
    pub refusal: &'a Error,
    /// The source as given or as declared, never a resolved path.
    pub path: &'a str,
    /// The input dataset that reads the source; `None` for `read_odm`.
    pub dataset: Option<&'a str>,
}

fn text(value: &str) -> V {
    V::String(value.into())
}

/// An ODM limit that cannot be admitted, reported at its argument.
pub fn invalid_odm_limit(option: &str, minimum: u64, value: V) -> Diagnostic {
    Diagnostic {
        code: C::OdmSourceOptionInvalid,
        spec_paths: vec![option.into()],
        context: Context::from([
            ("minimum".into(), V::from(minimum)),
            ("value".into(), value),
        ]),
        source_span: None,
        operand_route: None,
    }
}

impl OdmReadFinding<'_> {
    /// The registered cause of the refusal, or `None` for the opaque
    /// refusal of a source the host cannot read.
    pub fn code(self) -> Option<C> {
        Some(match self.refusal {
            Error::Io { .. } => return None,
            Error::Option { .. } => C::OdmSourceOptionInvalid,
            Error::Archive { .. } => C::OdmSourceArchiveInvalid,
            Error::Compression { .. } => C::OdmSourceCompressionUnsupported,
            Error::Member(_) => C::OdmSourceArchiveMember,
            Error::Limit(_) => C::OdmSourceLimitExceeded,
            Error::Malformed { .. } => C::OdmSourceXmlMalformed,
            Error::Doctype | Error::Encoding { .. } => C::OdmSourceXmlUnsupported,
            Error::Document(_) => C::OdmSourceDocumentUnsupported,
            Error::Layout(_) => C::OdmSourceLayoutUnsupported,
            Error::MissingAttribute { .. } => C::OdmSourceAttributeMissing,
            Error::AmbiguousAttribute { .. } => C::OdmSourceAttributeAmbiguous,
            Error::InvalidAttribute { .. } => C::OdmSourceAttributeInvalid,
            Error::MetadataConflict { .. } => C::OdmSourceMetadataConflict,
            Error::Value { .. } => C::OdmSourceValueUnsupported,
            Error::Transactional { .. } => C::OdmSourceTransactional,
            Error::NoClinicalData => C::OdmSourceNoClinicalData,
        })
    }

    /// The issue the refusal reports: its cause, where it is reported and
    /// the facts its message names.
    pub fn diagnostic(self) -> Diagnostic {
        let Some(code) = self.code() else {
            // A host read failure is no fact about the document (D107).
            return rejected(Step::Ingest, Fault::ResourceUnreadable);
        };
        if let Error::Option {
            option,
            minimum,
            value,
        } = self.refusal
        {
            return invalid_odm_limit(option, *minimum, V::from(*value));
        }
        let mut context = Context::from([("path".into(), text(self.path))]);
        let mut fact = |key: &str, value: V| {
            context.insert(key.into(), value);
        };
        match self.refusal {
            Error::Archive { reason } => fact("reason", text(reason)),
            Error::Compression { format } => fact("format", text(format)),
            Error::Member(member) => fact("reason", text(&member.to_string())),
            Error::Limit(limit) => fact("reason", text(&limit.to_string())),
            Error::Malformed { position, reason } => {
                fact("position", V::from(*position));
                fact("reason", text(reason));
            }
            refusal @ (Error::Doctype | Error::Encoding { .. }) => {
                fact("reason", text(&refusal.to_string()));
            }
            Error::Document(document) => fact("reason", text(&document.to_string())),
            Error::Layout(layout) => fact("reason", text(&layout.to_string())),
            Error::MissingAttribute { element, attribute } => {
                fact("element", text(element));
                fact("attribute", text(attribute));
            }
            Error::AmbiguousAttribute { name } => fact("attribute", text(name)),
            Error::InvalidAttribute {
                element,
                attribute,
                value,
            }
            | Error::Transactional {
                element,
                attribute,
                value,
            } => {
                fact("element", text(element));
                fact("attribute", text(attribute));
                fact("value", text(value));
            }
            Error::MetadataConflict {
                kind,
                study,
                version,
                oid,
            } => {
                fact("definition", text(kind.as_str()));
                fact("study", text(study));
                fact("metadata_version", text(version));
                fact("oid", text(oid));
            }
            Error::Value { item, refusal } => {
                fact("item", text(item));
                fact("reason", text(&refusal.to_string()));
            }
            Error::NoClinicalData => {}
            Error::Io { .. } | Error::Option { .. } => unreachable!("handled above"),
        }
        let spec_paths = match self.dataset {
            Some(dataset) => {
                context.insert("dataset".into(), text(dataset));
                vec![format!("input.{dataset}.path")]
            }
            None => Vec::new(),
        };
        Diagnostic {
            code,
            spec_paths,
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
