use crate::support::document::Tree;
use crate::support::roots::{Codelist, CodelistType, Item, Source};
use yamaa_core::{
    project_environment::{Draft, Environment, ExecutionEnvironment},
    project_function::Language,
    schema::{Document, DocumentNode as N},
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};

fn t(value: &str) -> Tree {
    Tree::Scalar(N::Text(value.into()))
}
fn m(fields: Vec<(&'static str, Tree)>) -> Tree {
    Tree::map(fields)
}

/// A 1.0 specification (D4) whose `CODE` column binds the codelist
/// `identifier`. It declares its model beside that submission metadata
/// (SUB-3, D25): `ADaM`, which admits the `Assigned` origin as written
/// (SUB-15).
pub fn spec(
    identifier: &str,
    kind: &str,
    allowed: Option<&str>,
    rows: bool,
    extra: Option<&str>,
) -> Document {
    let read = |name: &str| m(vec![("value", t(name))]);
    let origin = m(vec![("type", t("Assigned"))]);
    let submission = vec![
        ("codelist", t(identifier)),
        ("origins", Tree::List(vec![origin])),
    ];
    let mut code = vec![
        ("name", t("CODE")),
        ("type", t(kind)),
        ("derivation", read("SRC.CODE")),
        ("submission", m(submission)),
    ];
    if extra == Some("metadata") {
        code.push(("metadata", m(vec![("codelist", t(identifier))])));
    }
    if let Some(value) = allowed {
        code.push((
            "verifications",
            Tree::List(vec![m(vec![(
                "allowed_values",
                m(vec![("values", Tree::List(vec![t(value)]))]),
            )])]),
        ));
    }
    let mut fields = vec![
        ("yamaa", t("1.0")),
        ("type", t("spec")),
        ("model", t("ADaM")),
    ];
    fields.extend([
        ("domain", t("TEST")),
        ("input", m(vec![("SRC", m(vec![("path", t("input.csv"))]))])),
        ("keys", Tree::List(vec![t("ID")])),
        (
            "columns",
            Tree::List(vec![
                m(vec![
                    ("name", t("ID")),
                    ("type", t("int")),
                    ("derivation", read("SRC.ID")),
                ]),
                m(code),
                m(vec![
                    ("name", t("LATE")),
                    ("type", t("int")),
                    ("derivation", read("SRC.LATE")),
                ]),
            ]),
        ),
        (
            "output",
            m(vec![
                ("path", t("output.csv")),
                ("columns", Tree::List(vec![t("ID"), t("CODE"), t("LATE")])),
            ]),
        ),
    ]);
    if rows {
        fields.push((
            "rows",
            Tree::List(vec![m(vec![
                ("id", t("all")),
                ("derivations", m(vec![("ID", read("SRC.ID"))])),
            ])]),
        ));
    }
    if extra == Some("root_metadata") {
        fields.push(("metadata", m(vec![("has_no_data", t("true"))])));
    }
    m(fields).document()
}
/// The admitted metadata of a 1.0 project with one `SEX` codelist file
/// holding the value `F`.
pub fn environment(extensible: bool) -> ExecutionEnvironment {
    Environment::admit(
        Some(Language::Python),
        Draft {
            language: Some(Language::Python),
            lock: None,
            functions: None,
            codelists: vec![Source {
                standard: None,
                codelists: vec![Codelist {
                    id: "SEX".into(),
                    name: "Sex".into(),
                    data_type: CodelistType::Text,
                    extensible,
                    alias: None,
                    format_name: None,
                    items: Some(vec![Item {
                        value: Value::Str("F".into()),
                        decode: None,
                        rank: None,
                        alias: None,
                        extended: false,
                    }]),
                    external: None,
                }],
            }
            .root()],
        },
    )
    .unwrap()
    .into_execution()
}
pub fn schema() -> TableSchema {
    TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "CODE".into(),
            kind: ColumnType::Str,
        },
        Column {
            name: "LATE".into(),
            kind: ColumnType::Int,
        },
    ])
    .unwrap()
}
