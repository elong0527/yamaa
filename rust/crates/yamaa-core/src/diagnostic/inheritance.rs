//! Pure inheritance traversal and composition causes with original
//! declaring-file provenance.
use super::{ConditionCode as C, Diagnostic, ProjectionError};
use crate::inheritance::Named;
use crate::json::Value as JsonValue;

/// Borrow facts retained by the traversal or the composition. This grants no
/// source authority and leaves opaque host errors, schema-arena findings and
/// policy failures separate.
pub enum InheritanceFinding<'a> {
    InvalidParent {
        declaring: &'a str,
        path: &'a str,
    },
    ParentUnavailable {
        declaring: &'a str,
        path: &'a str,
    },
    /// The layer `source` lies on the network share `parent_root` and writes
    /// `path` relative to itself, while the entry lies below `entry_root`: no
    /// written path names that location from the entry (LAYER-10, FILE-4).
    /// The resolver port names both roots; only it knows how an identity is
    /// rooted.
    ParentOnAnotherShare {
        source: &'a str,
        path: &'a str,
        parent_root: &'a str,
        entry_root: &'a str,
    },
    /// The field `named`, a `parent` or an `include` entry of the layer
    /// before the last in `path`, reaches a layer still being composed
    /// (LAYER-3).
    Cycle {
        path: &'a [String],
        returns_to_entry: bool,
        named: Named,
    },
    /// The `include` entry at `index` of `declaring`, written `path`, reaches
    /// `layer`, which the composition already reached (LAYER-3).
    Repeated {
        declaring: &'a str,
        index: usize,
        path: &'a str,
        layer: &'a str,
    },
    /// The fragment `source` declares `field`, `parent` or `include`, which a
    /// layer an `include` entry names does not (LAYER-4): reported at that
    /// field of the fragment.
    Fragment {
        source: &'a str,
        entry: &'a str,
        field: &'a str,
    },
    /// A caller named a parent, `value` as the caller gave it, for an entry
    /// that declares `field`, `parent` or `include`, so the entry is no
    /// fragment to check on it (LAYER-4, #3694): reported at `parent`, the
    /// argument, which no file holds.
    NotFragment {
        field: &'a str,
        value: &'a str,
    },
    /// A layer declares a language version the engine does not read (SCHEMA-2,
    /// LAYER-4): reported at the entry's `yamaa`, or at the `parent` or
    /// `include` entry naming it, which the layer `declaring` writes.
    LanguageVersion {
        source: &'a str,
        entry: &'a str,
        version: &'a str,
        reads: &'a [String],
        named: Named,
        declaring: Option<&'a str>,
    },
    /// A layer declares a type other than `expected`, the one every layer of
    /// its chain declares (SCHEMA-2, LAYER-4, PROJECT-16): reported at the
    /// entry's `type`, or at the `parent` or `include` entry naming it, which
    /// the layer `declaring` writes.
    FileType {
        source: &'a str,
        entry: &'a str,
        file_type: &'a str,
        expected: crate::language::FileType,
        named: Named,
        declaring: Option<&'a str>,
    },
}

fn text(value: &str) -> JsonValue {
    JsonValue::String(value.into())
}

/// The `parent` or `include[i]` that names a refused layer, with the layer
/// that writes it recorded as `declaring_source`, which locates the issue in
/// that layer (P-ISSUE-1); none for the entry, which no field names. A field
/// without the layer that writes it, or the entry with one, has no semantic
/// projection.
fn naming_field(
    context: &mut super::Context,
    named: Named,
    declaring: Option<&str>,
) -> Result<Option<String>, ProjectionError> {
    match (named.field(), declaring) {
        (None, None) => Ok(None),
        (Some(field), Some(declaring)) => {
            context.insert("declaring_source".into(), text(declaring));
            Ok(Some(field))
        }
        _ => Err(ProjectionError::InvalidContext),
    }
}

impl InheritanceFinding<'_> {
    /// Preserve the existing phase, authored path and source/entry/parent
    /// context. A contradictory closed-loop carrier has no semantic projection.
    pub fn diagnostic(self) -> Result<Diagnostic, ProjectionError> {
        let mut context = super::Context::new();
        let parent = || String::from("parent");
        let (code, path) = match self {
            Self::InvalidParent { declaring, path } => {
                context.insert("reason".into(), text("remote_reference"));
                context.insert("source".into(), text(declaring));
                context.insert("parent".into(), text(path));
                (C::InheritanceInvalidParent, parent())
            }
            Self::ParentUnavailable { declaring, path } => {
                context.insert("path".into(), text(path));
                context.insert("source".into(), text(declaring));
                (C::InheritanceParentUnavailable, parent())
            }
            Self::ParentOnAnotherShare {
                source,
                path,
                parent_root,
                entry_root,
            } => {
                context.insert("source".into(), text(source));
                context.insert("path".into(), text(path));
                context.insert("parent_root".into(), text(parent_root));
                context.insert("entry_root".into(), text(entry_root));
                (C::InheritanceParentOnAnotherShare, parent())
            }
            Self::Cycle {
                path,
                returns_to_entry,
                named,
            } => {
                // A parent a caller names that is the entry itself closes
                // the cycle at the argument that names it.
                let field = match named {
                    Named::CallerParent => String::from("parent"),
                    named => named.field().ok_or(ProjectionError::InvalidContext)?,
                };
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
                (C::InheritanceCycle, field)
            }
            Self::Repeated {
                declaring,
                index,
                path,
                layer,
            } => {
                context.insert("layer".into(), text(layer));
                context.insert("path".into(), text(path));
                context.insert("source".into(), text(declaring));
                let field = Named::Include(index).field().expect("an include entry");
                (C::InheritanceRepeatedLayer, field)
            }
            Self::Fragment {
                source,
                entry,
                field,
            } => {
                context.insert("construct".into(), text(field));
                context.insert("entry".into(), text(entry));
                context.insert("source".into(), text(source));
                (C::InheritanceFragmentField, field.into())
            }
            Self::NotFragment { field, value } => {
                context.insert("construct".into(), text(field));
                context.insert("value".into(), text(value));
                (C::InheritanceNotFragment, String::from("parent"))
            }
            Self::LanguageVersion {
                source,
                entry,
                version,
                reads,
                named,
                declaring,
            } => {
                context.insert("source".into(), text(source));
                context.insert("entry".into(), text(entry));
                crate::language::version_context(&mut context, version, reads);
                match naming_field(&mut context, named, declaring)? {
                    None => (C::HeaderLanguageVersion, "yamaa".into()),
                    Some(field) => (C::InheritanceParentLanguageVersion, field),
                }
            }
            Self::FileType {
                source,
                entry,
                file_type,
                expected,
                named,
                declaring,
            } => {
                context.insert("source".into(), text(source));
                context.insert("entry".into(), text(entry));
                crate::language::file_type_context(&mut context, file_type, expected);
                match naming_field(&mut context, named, declaring)? {
                    None => (C::HeaderFileType, "type".into()),
                    Some(field) => (C::InheritanceParentFileType, field),
                }
            }
        };
        Ok(Diagnostic {
            code,
            spec_paths: vec![path],
            context,
            source_span: None,
            operand_route: None,
        })
    }
}
