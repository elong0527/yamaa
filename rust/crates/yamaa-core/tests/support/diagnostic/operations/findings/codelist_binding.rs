//! Binding a specification's codelists to the project's catalogue (CT-1,
//! CT-8, CT-9).
use super::{column_type, text, value};
use crate::support::{
    diagnostic::json::Json,
    roots::{Codelist, CodelistType, Item, Source},
};
use yamaa_core::{
    diagnostic::Diagnostic,
    project_codelist_binding::{bind, Binding, Path},
    project_terminology::Catalogue,
};

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "codelist_binding" => codelist_binding(input),
        _ => return None,
    })
}

/// The bindings of `bindings` against a catalogue of one codelist source
/// that lists `codelists`, `{codelists, bindings}`: each codelist is `{id,
/// name, data_type, extensible, items}`, its items the values it lists, and
/// each binding `{column, name, kind, path, codelist, allowed_values}`, the
/// column's position, name and type, the authored path of its binding, the
/// codelist it names and the paths of its `allowed_values` verifications.
fn codelist_binding(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["codelists", "bindings"]);
    let codelists = input
        .field("codelists")
        .items()
        .iter()
        .map(|codelist| {
            let codelist = codelist.closed(&["id", "name", "data_type", "extensible", "items"]);
            Codelist {
                id: text(codelist, "id").into(),
                name: text(codelist, "name").into(),
                data_type: match text(codelist, "data_type") {
                    "text" => CodelistType::Text,
                    "integer" => CodelistType::Integer,
                    "float" => CodelistType::Float,
                    other => panic!("no codelist data type {other:?}"),
                },
                extensible: codelist.field("extensible").boolean(),
                alias: None,
                format_name: None,
                items: Some(
                    codelist
                        .field("items")
                        .items()
                        .iter()
                        .map(|item| Item {
                            value: value(item),
                            decode: None,
                            rank: None,
                            alias: None,
                            extended: false,
                        })
                        .collect(),
                ),
                external: None,
            }
        })
        .collect();
    let catalogue = Catalogue::admit(vec![Source {
        standard: None,
        codelists,
    }
    .root()])
    .expect("an admitted catalogue");
    let written = input.field("bindings").items();
    let allowed = written
        .iter()
        .map(|binding| {
            binding
                .field("allowed_values")
                .items()
                .iter()
                .map(Json::text)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let bindings = written
        .iter()
        .zip(&allowed)
        .map(|(binding, allowed_values)| {
            let binding = binding.closed(&[
                "column",
                "name",
                "kind",
                "path",
                "codelist",
                "allowed_values",
            ]);
            Binding {
                column: binding.field("column").integer(),
                name: text(binding, "name"),
                kind: column_type(binding.field("kind")),
                path: Path::Authored(text(binding, "path")),
                codelist: text(binding, "codelist"),
                allowed_values,
            }
        })
        .collect::<Vec<_>>();
    match bind(&catalogue, &bindings) {
        Ok(_) => Vec::new(),
        Err(findings) => findings
            .iter()
            .map(|finding| finding.diagnostic())
            .collect(),
    }
}
