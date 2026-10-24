//! The inheritance refusals of a layered specification (LAYER-2, LAYER-3,
//! LAYER-4, LAYER-10) and its composed columns' unknown references
//! (COLUMN-5).
use super::text;
use crate::support::diagnostic::json::Json;
use yamaa_core::{
    diagnostic::{inheritance_dependency_diagnostics, Diagnostic, InheritanceFinding as Finding},
    inheritance::Named,
    resolution::Issue,
};

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "inheritance_finding" => vec![inheritance_finding(input)],
        "inherited_dependencies" => inherited_dependencies(input),
        _ => return None,
    })
}

/// A refusal of the layer chain, `{finding, ...}`: `invalid_parent` and
/// `parent_unavailable` with `declaring` and `path`,
/// `parent_on_another_share` with `source`, `path`, `parent_root` and
/// `entry_root`, `cycle` with `path`, `returns_to_entry` and `named`
/// (`"entry"`, `"parent"` or `{"include": <index>}`), `repeated` with
/// `declaring`, `index`, `path` and `layer`, `fragment` with `source`,
/// `entry` and `field`, and `not_fragment` with the `field` the entry
/// declares and the parent `value` a caller gave.
fn inheritance_finding(input: &Json) -> Diagnostic {
    let field = |key| text(input, key);
    let cycle;
    let finding = match field("finding") {
        "invalid_parent" => {
            input.closed(&["finding", "declaring", "path"]);
            Finding::InvalidParent {
                declaring: field("declaring"),
                path: field("path"),
            }
        }
        "parent_unavailable" => {
            input.closed(&["finding", "declaring", "path"]);
            Finding::ParentUnavailable {
                declaring: field("declaring"),
                path: field("path"),
            }
        }
        "parent_on_another_share" => {
            input.closed(&["finding", "source", "path", "parent_root", "entry_root"]);
            Finding::ParentOnAnotherShare {
                source: field("source"),
                path: field("path"),
                parent_root: field("parent_root"),
                entry_root: field("entry_root"),
            }
        }
        "cycle" => {
            input.closed(&["finding", "path", "returns_to_entry", "named"]);
            cycle = input
                .field("path")
                .items()
                .iter()
                .map(|layer| layer.text().to_owned())
                .collect::<Vec<_>>();
            Finding::Cycle {
                path: &cycle,
                returns_to_entry: input.field("returns_to_entry").boolean(),
                named: match input.field("named") {
                    Json::String(named) if named == "entry" => Named::Entry,
                    Json::String(named) if named == "parent" => Named::Parent,
                    named => match named.members() {
                        [(name, index)] if name == "include" => Named::Include(index.integer()),
                        other => panic!("no naming field {other:?}"),
                    },
                },
            }
        }
        "repeated" => {
            input.closed(&["finding", "declaring", "index", "path", "layer"]);
            Finding::Repeated {
                declaring: field("declaring"),
                index: input.field("index").integer(),
                path: field("path"),
                layer: field("layer"),
            }
        }
        "fragment" => {
            input.closed(&["finding", "source", "entry", "field"]);
            Finding::Fragment {
                source: field("source"),
                entry: field("entry"),
                field: field("field"),
            }
        }
        "not_fragment" => {
            input.closed(&["finding", "field", "value"]);
            Finding::NotFragment {
                field: field("field"),
                value: field("value"),
            }
        }
        other => panic!("no inheritance finding {other:?}"),
    };
    finding.diagnostic().expect("a complete context")
}

/// The findings of a composed specification's column dependencies, in the
/// resolver's order, `{issues}`: each issue is `{"unknown": {column,
/// dependency}}`.
fn inherited_dependencies(input: &Json) -> Vec<Diagnostic> {
    let issues = input
        .closed(&["issues"])
        .field("issues")
        .items()
        .iter()
        .map(|issue| match issue.members() {
            [(kind, facts)] if kind == "unknown" => {
                let facts = facts.closed(&["column", "dependency"]);
                Issue::Unknown {
                    column: text(facts, "column").into(),
                    dependency: text(facts, "dependency").into(),
                }
            }
            other => panic!("no dependency issue {other:?}"),
        })
        .collect::<Vec<_>>();
    inheritance_dependency_diagnostics(&issues)
}
