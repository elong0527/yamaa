use crate::support::document::Tree;
use crate::support::roots::{Codelist, CodelistType, Item, Source};
use yamaa_core::{
    project_environment::{Draft, Environment, ExecutionEnvironment},
    project_function::Language,
    schema::{Document, DocumentNode as N, SpecificationDocument},
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};

fn t(value: &str) -> Tree {
    Tree::Scalar(N::Text(value.into()))
}
fn m(fields: Vec<(&'static str, Tree)>) -> Tree {
    Tree::map(fields)
}

fn read(name: &str) -> Tree {
    m(vec![(
        "value",
        m(vec![("source", m(vec![("variable", t(name))]))]),
    )])
}
pub fn spec(
    identifier: &str,
    kind: &str,
    allowed: Option<&str>,
    rows: bool,
    extra: Option<&str>,
) -> SpecificationDocument {
    let mut submission = vec![
        ("codelist", t(identifier)),
        ("origin", m(vec![("type", t("Assigned"))])),
    ];
    if extra == Some("role") {
        submission.push(("role", t("Result Qualifier")));
    }
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
        ("schema_version", t("1.0")),
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
    ];
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
    let mut nodes = vec![];
    let root = (m(fields)).append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap()
}
pub fn environment(extensible: bool) -> ExecutionEnvironment {
    Environment::admit(
        Language::Python,
        Draft {
            language: None,
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
