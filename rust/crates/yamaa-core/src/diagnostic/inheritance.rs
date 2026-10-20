//! Pure inheritance traversal causes with original declaring-file provenance.
use super::{ConditionCode as C, Diagnostic, ProjectionError};
use crate::json::Value as JsonValue;

/// Borrow facts retained by the traversal. This grants no source authority and
/// leaves opaque host errors, schema-arena findings and policy failures separate.
pub enum InheritanceFinding<'a> {
    InvalidParent {
        declaring: &'a str,
        path: &'a str,
    },
    ParentUnavailable {
        declaring: &'a str,
        path: &'a str,
    },
    Cycle {
        path: &'a [String],
        returns_to_entry: bool,
    },
    Version {
        source: &'a str,
        entry: &'a str,
        expected: &'a str,
        actual: &'a str,
        is_entry: bool,
    },
    /// A layer declares a language version the engine does not read (SCHEMA-2,
    /// LAYER-4): reported at the entry's `yamaa`, or at the `parent` naming it.
    LanguageVersion {
        source: &'a str,
        entry: &'a str,
        version: &'a str,
        reads: &'a [String],
        is_entry: bool,
    },
    /// A layer declares a type other than `spec` (SCHEMA-2, LAYER-4): reported
    /// at the entry's `type`, or at the `parent` naming it.
    FileType {
        source: &'a str,
        entry: &'a str,
        file_type: &'a str,
        is_entry: bool,
    },
}

fn text(value: &str) -> JsonValue {
    JsonValue::String(value.into())
}

impl InheritanceFinding<'_> {
    /// Preserve the existing phase, authored path and source/entry/parent
    /// context. A contradictory closed-loop carrier has no semantic projection.
    pub fn diagnostic(self) -> Result<Diagnostic, ProjectionError> {
        let mut context = super::Context::new();
        let (code, path) = match self {
            Self::InvalidParent { declaring, path } => {
                context.insert("reason".into(), text("remote_reference"));
                context.insert("source".into(), text(declaring));
                context.insert("parent".into(), text(path));
                (C::InheritanceInvalidParent, "parent")
            }
            Self::ParentUnavailable { declaring, path } => {
                context.insert("path".into(), text(path));
                context.insert("source".into(), text(declaring));
                (C::InheritanceParentUnavailable, "parent")
            }
            Self::Cycle {
                path,
                returns_to_entry,
            } => {
                if path.len() < 2 || path.first() != path.last() {
                    return Err(ProjectionError::InvalidContext);
                }
                context.insert(
                    "reason".into(),
                    text(if returns_to_entry {
                        "parent_chain_returns_to_entry"
                    } else {
                        "parent_chain_returns_to_active_layer"
                    }),
                );
                context.insert(
                    "cycle".into(),
                    JsonValue::Array(path.iter().map(|s| text(s)).collect()),
                );
                context.insert("source".into(), text(&path[path.len() - 2]));
                (C::InheritanceCycle, "parent")
            }
            Self::Version {
                source,
                entry,
                expected,
                actual,
                is_entry,
            } => {
                context.insert("source".into(), text(source));
                context.insert("entry".into(), text(entry));
                if is_entry {
                    context.insert("expected".into(), text(expected));
                    context.insert("actual".into(), text(actual));
                    (C::InheritanceEntryVersion, "schema_version")
                } else {
                    context.insert("entry_version".into(), text(expected));
                    context.insert("parent_version".into(), text(actual));
                    (C::InheritanceParentVersion, "parent")
                }
            }
            Self::LanguageVersion {
                source,
                entry,
                version,
                reads,
                is_entry,
            } => {
                context.insert("source".into(), text(source));
                context.insert("entry".into(), text(entry));
                crate::language::version_context(&mut context, version, reads);
                if is_entry {
                    (C::HeaderLanguageVersion, "yamaa")
                } else {
                    (C::InheritanceParentLanguageVersion, "parent")
                }
            }
            Self::FileType {
                source,
                entry,
                file_type,
                is_entry,
            } => {
                context.insert("source".into(), text(source));
                context.insert("entry".into(), text(entry));
                crate::language::file_type_context(
                    &mut context,
                    file_type,
                    crate::language::FileType::Spec,
                );
                if is_entry {
                    (C::HeaderFileType, "type")
                } else {
                    (C::InheritanceParentFileType, "parent")
                }
            }
        };
        Ok(Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        })
    }
}
