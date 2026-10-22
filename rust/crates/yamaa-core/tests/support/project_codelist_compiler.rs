use crate::support::document::Tree;
use crate::support::roots::{Codelist, CodelistType, Item, Source};
use yamaa_core::{
    ast::Surface,
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
    tree(identifier, kind, allowed, rows, extra, false).document()
}
/// The specification tree; `legacy` writes it in 0.5: its header, one
/// `origin` and the 0.5 spelling of a read.
fn tree(
    identifier: &str,
    kind: &str,
    allowed: Option<&str>,
    rows: bool,
    extra: Option<&str>,
    legacy: bool,
) -> Tree {
    let read = |name: &str| {
        let value = if legacy {
            m(vec![("source", m(vec![("variable", t(name))]))])
        } else {
            t(name)
        };
        m(vec![("value", value)])
    };
    let origin = m(vec![("type", t("Assigned"))]);
    let mut submission = vec![("codelist", t(identifier))];
    if legacy {
        submission.push(("origin", origin));
    } else {
        submission.push(("origins", Tree::List(vec![origin])));
    }
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
    let mut fields = if legacy {
        vec![("schema_version", t("1.0"))]
    } else {
        vec![
            ("yamaa", t("1.0")),
            ("type", t("spec")),
            ("model", t("ADaM")),
        ]
    };
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
    m(fields)
}
/// The admitted metadata of a 1.0 project with one `SEX` codelist file
/// holding the value `F`.
pub fn environment(extensible: bool) -> ExecutionEnvironment {
    admit(Surface::V1 { minor: 0 }, Some(Language::Python), extensible)
}
fn admit(surface: Surface, language: Option<Language>, extensible: bool) -> ExecutionEnvironment {
    Environment::admit(
        Some(Language::Python),
        Draft {
            surface,
            language,
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
/// The 0.5 specification and project of the families that keep them until
/// the 0.5 path is deleted (#3447, #3448).
#[allow(dead_code)]
pub mod v0_5 {
    use yamaa_core::{
        ast::Surface, project_environment::ExecutionEnvironment, schema::SpecificationDocument,
    };

    pub fn spec(
        identifier: &str,
        kind: &str,
        allowed: Option<&str>,
        rows: bool,
        extra: Option<&str>,
    ) -> SpecificationDocument {
        let tree = super::tree(identifier, kind, allowed, rows, extra, true);
        SpecificationDocument::admit(tree.document())
            .unwrap()
            .unwrap()
    }
    /// A 0.5 project that links no environment file and holds the codelist.
    pub fn environment(extensible: bool) -> ExecutionEnvironment {
        super::admit(Surface::V0_5, None, extensible)
    }
}
