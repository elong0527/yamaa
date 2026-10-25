//! What preparation refuses (`preflight.json`) and what binding refuses
//! (`binding.json`). Most cases name the shared document operations; the
//! readers here serve the entry points below a document that some of those
//! records are reached through: a generated ordinal, one qualified read's
//! scope, the column dependency analysis, and the verification declarations
//! a prepared specification keeps.
use super::{column_type, document::prepared, text, File};
use crate::support::diagnostic::json::Json;
use yamaa_core::{
    column_dependencies::analyze,
    diagnostic::Diagnostic,
    reference_binding::{Catalog, Dataset, Field},
    reference_scope::{validate, Phase, Reach, Scope},
    source_ordinal::Ordinal,
    specification::{BindFinding, SourceDeclaration, SourceProfile},
    table::{Column, TableSchema},
};

pub(super) const FILES: &[File] = &[fixture!("binding.json"), fixture!("preflight.json")];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "column_dependencies" => column_dependencies(input),
        "ordinal" => ordinal(input),
        "qualified_reference" => qualified_reference(input),
        "verification_declarations" => verification_declarations(input),
        _ => return None,
    })
}

/// The findings of a prepared `document`'s verification declarations, its
/// columns' first and then the dataset's, `{document}`.
fn verification_declarations(input: &Json) -> Vec<Diagnostic> {
    prepared(input.closed(&["document"])).verification_declaration_diagnostics()
}

/// The refusal of the generated ordinal `field` of the input `dataset`,
/// whose stored `columns` are `[name, type]` pairs and which holds `rows`
/// records, `{dataset, columns, rows, field}`.
fn ordinal(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["dataset", "columns", "rows", "field"]);
    let schema = TableSchema::new(
        input
            .field("columns")
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
    .expect("an admitted schema");
    let field = text(input, "field");
    match Ordinal::admit(&schema, input.field("rows").integer(), field) {
        Ok(_) => Vec::new(),
        Err(error) => vec![error
            .finding(text(input, "dataset"), field)
            .unwrap_or_else(|| panic!("an ordinal refused with no language finding: {error:?}"))
            .diagnostic()],
    }
}

/// One qualified read's scope findings as binding projects them, `{source,
/// datasets, name, scope, path, row}`. `datasets` gives the stored datasets
/// of the catalog, each field a `[name, type]` pair. `scope` gives the read's
/// `drivers`, whether it reads the `current_driver`, its `reach` (`scalar`,
/// `record`, `relation` or `declared`), whether it is `joined`, and its
/// `phase`: `{"row": {"group_by": [...]}}` or `{"column": {"groups":
/// [[driver, [...]], ...]}}`. `path` and `row` (`null` for none) are the
/// finding's, and `source`, `{name, path}`, is the driver's input, declared as
/// a CSV input with no types or options. The read expects no type.
fn qualified_reference(input: &Json) -> Vec<Diagnostic> {
    fn names(json: &Json) -> Vec<&str> {
        json.items().iter().map(Json::text).collect()
    }
    let input = input.closed(&["source", "datasets", "name", "scope", "path", "row"]);
    let stored = input
        .field("datasets")
        .members()
        .iter()
        .map(|(name, fields)| {
            let fields = fields
                .items()
                .iter()
                .map(|field| match field.items() {
                    [name, kind] => Field {
                        name: name.text(),
                        column_type: Some(column_type(kind)),
                    },
                    other => panic!("a field is a [name, type] pair: {other:?}"),
                })
                .collect::<Vec<_>>();
            (name.as_str(), fields)
        })
        .collect::<Vec<_>>();
    let datasets = stored
        .iter()
        .map(|(name, fields)| Dataset { name, fields })
        .collect::<Vec<_>>();
    let catalog = Catalog::compile(&[], &datasets).expect("an admitted catalog");
    let scope =
        input
            .field("scope")
            .closed(&["drivers", "current_driver", "reach", "joined", "phase"]);
    let drivers = names(scope.field("drivers"));
    let reach = match text(scope, "reach") {
        "scalar" => Reach::Scalar,
        "record" => Reach::Record,
        "relation" => Reach::Relation,
        "declared" => Reach::Declared,
        other => panic!("no reach {other:?}"),
    };
    let [(phase, groups)] = scope.field("phase").members() else {
        panic!("a phase is one of row and column")
    };
    let group_by;
    let column_groups;
    let column_groups_by_driver;
    let phase = match phase.as_str() {
        "row" => {
            group_by = names(groups.closed(&["group_by"]).field("group_by"));
            Phase::Row {
                group_by: Some(&group_by),
            }
        }
        "column" => {
            column_groups = groups
                .closed(&["groups"])
                .field("groups")
                .items()
                .iter()
                .map(|pair| match pair.items() {
                    [driver, group] => (driver.text(), names(group)),
                    other => panic!("a group is a [driver, variables] pair: {other:?}"),
                })
                .collect::<Vec<_>>();
            column_groups_by_driver = column_groups
                .iter()
                .map(|(driver, group)| (*driver, &group[..]))
                .collect::<Vec<_>>();
            Phase::Column {
                groups: &column_groups_by_driver,
            }
        }
        other => panic!("no phase {other:?}"),
    };
    let source = input.field("source").closed(&["name", "path"]);
    let source = SourceDeclaration {
        name: text(source, "name").into(),
        path: text(source, "path").into(),
        types: Vec::new(),
        profile: SourceProfile::Csv,
        encoding: None,
        byte_order_mark: false,
        empty_string_present: false,
        ordinal: None,
    };
    let name = text(input, "name");
    let row = match input.field("row") {
        Json::Null => None,
        row => Some(row.text().to_owned()),
    };
    let scope = Scope {
        drivers: &drivers,
        current_driver: scope.field("current_driver").boolean(),
        reach,
        joined: scope.field("joined").boolean(),
        phase,
    };
    validate(&catalog, name, None, scope)
        .expect("an admitted qualified read")
        .into_iter()
        .flat_map(|finding| {
            BindFinding::QualifiedReference {
                path: text(input, "path").into(),
                name: name.into(),
                row: row.clone(),
                finding,
            }
            .diagnostics(&source)
            .expect("a finding binding projects")
        })
        .collect()
}

/// The column dependency analysis of the output `columns`, each derived at
/// its entry of `paths`, `{dependencies, keys, rows, columns, paths}`:
/// `dependencies` gives each column's output-column dependencies by index,
/// `null` for a column with no column-phase derivation, `keys` the key
/// columns by index, and `rows` whether row templates are declared.
fn column_dependencies(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["dependencies", "keys", "rows", "columns", "paths"]);
    let indices = |json: &Json| {
        json.items()
            .iter()
            .map(|index| index.integer())
            .collect::<Vec<usize>>()
    };
    let names = |key| {
        input
            .field(key)
            .items()
            .iter()
            .map(|name| name.text().to_owned())
            .collect::<Vec<_>>()
    };
    let dependencies = input
        .field("dependencies")
        .items()
        .iter()
        .map(|edges| match edges {
            Json::Null => None,
            edges => Some(indices(edges)),
        })
        .collect::<Vec<_>>();
    let (columns, paths) = (names("columns"), names("paths"));
    analyze(
        &dependencies,
        &indices(input.field("keys")),
        input.field("rows").boolean(),
    )
    .expect("admitted dependency metadata")
    .diagnostics
    .iter()
    .map(|finding| {
        finding
            .specification_diagnostic(&columns, &paths)
            .expect("a projection of admitted metadata")
    })
    .collect()
}
