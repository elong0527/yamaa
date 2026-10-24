//! The operations on a 1.0 specification, which every part of the move that
//! reads documents shares (#3691). A case's `document` is the specification
//! written as a JSON object, read as the document a YAML source decodes to,
//! its members in written order; each operation prepares it as the 1.0 front
//! end lowers it (D76). An operation a single table needs belongs in its
//! part's module, not here.
use super::{column_type, File};
use crate::support::{diagnostic::json::Json, document::Tree, specification::prepare};
use yamaa_core::{
    diagnostic::Diagnostic,
    schema::DocumentNode,
    specification::{BindError, PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
};

pub(super) const FILES: &[File] = &[fixture!("result_types.json")];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "bind" => bind(input),
        "check" => check(input),
        "output" => output(input),
        "prepare" => prepared_findings(input),
        _ => return None,
    })
}

/// The tree a YAML source decodes `json` to: a number with a fraction or an
/// exponent is a float, another an integer of that spelling.
pub(crate) fn tree(json: &Json) -> Tree {
    match json {
        Json::Null => Tree::Scalar(DocumentNode::Null),
        Json::Bool(value) => Tree::boolean(*value),
        Json::Number(token) if token.contains(['.', 'e', 'E']) => {
            Tree::float(token.parse().expect("a finite float"))
        }
        Json::Number(token) => Tree::integer(token),
        Json::String(text) => Tree::text(text),
        Json::Array(items) => Tree::list(items.iter().map(tree).collect()),
        Json::Object(members) => Tree::Map(
            members
                .iter()
                .map(|(name, value)| (name.clone(), tree(value)))
                .collect(),
        ),
    }
}

/// The findings preparation refuses `document` with, `{document}`.
fn prepared_findings(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document"]);
    match prepare(&tree(input.field("document")).document()) {
        Ok(_) => Vec::new(),
        Err(PrepareError::Invalid(findings)) => findings
            .iter()
            .map(|finding| finding.diagnostic())
            .collect(),
        Err(other) => panic!("preparation refuses with no language finding: {other:?}"),
    }
}

/// What `check` reports of a prepared `document` before any record is read,
/// its output declarations' findings last, `{document}`. It admits, and
/// does not read, the `sources` that `bind` reads, so one case can name both
/// operations when they report the same issues.
fn check(input: &Json) -> Vec<Diagnostic> {
    prepared(input.closed(&["document", "sources"])).check_declaration_diagnostics()
}

/// The findings binding a prepared `document` to its inputs' schemas
/// refuses with, `{document, sources}`: `sources` gives each input of the
/// document, in its written order, as `[column, type]` pairs.
fn bind(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "sources"]);
    let prepared = prepared(input);
    let schemas = schemas(input);
    match prepared.bind_sources(&schemas.iter().collect::<Vec<_>>()) {
        Ok(_) => Vec::new(),
        Err(BindError::Invalid(findings)) => findings
            .iter()
            .flat_map(|finding| {
                finding
                    .diagnostics(prepared.source())
                    .expect("a finding with its diagnostics")
            })
            .collect(),
        Err(other) => panic!("binding refuses with no language finding: {other:?}"),
    }
}

/// The schemas of `sources`, one for each input of `document` in its
/// written order.
pub(super) fn schemas(input: &Json) -> Vec<TableSchema> {
    let sources = input.field("sources").members();
    let declared = input
        .field("document")
        .field("input")
        .members()
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        sources
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        declared,
        "the sources are the document's inputs, in their written order"
    );
    sources
        .iter()
        .map(|(_, columns)| {
            TableSchema::new(
                columns
                    .items()
                    .iter()
                    .map(|column| match column.items() {
                        [name, kind] => Column {
                            name: name.text().into(),
                            kind: column_type(kind),
                        },
                        other => panic!("a column is a [name, type] pair: {other:?}"),
                    })
                    .collect(),
            )
            .expect("an admitted schema")
        })
        .collect()
}

/// The findings of a prepared `document`'s output declarations, `{document}`.
fn output(input: &Json) -> Vec<Diagnostic> {
    prepared(input.closed(&["document"]))
        .output_findings()
        .iter()
        .map(|finding| finding.diagnostic())
        .collect()
}

/// The prepared `document` of `input`, which must prepare.
pub(super) fn prepared(input: &Json) -> PreparedSpecification {
    prepare(&tree(input.field("document")).document())
        .unwrap_or_else(|error| panic!("the document prepares: {error:?}"))
}
