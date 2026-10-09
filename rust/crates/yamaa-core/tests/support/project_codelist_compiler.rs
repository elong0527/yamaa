use yamaa_core::{
    project_environment::{Draft, Environment, ExecutionEnvironment},
    project_function::Language,
    project_terminology::{Codelist, DataType, Item, Source},
    schema::{Document, DocumentNode as N, SpecificationDocument, ValidationBudget},
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};
enum Tree {
    Scalar(N),
    Map(Vec<(&'static str, Tree)>),
    List(Vec<Tree>),
}
fn t(value: &str) -> Tree {
    Tree::Scalar(N::Text(value.into()))
}
fn m(fields: Vec<(&'static str, Tree)>) -> Tree {
    Tree::Map(fields)
}
fn append(tree: Tree, nodes: &mut Vec<N>) -> usize {
    let node = match tree {
        Tree::Scalar(node) => node,
        Tree::List(items) => {
            N::Sequence(items.into_iter().map(|item| append(item, nodes)).collect())
        }
        Tree::Map(items) => N::Mapping(
            items
                .into_iter()
                .map(|(key, item)| (append(t(key), nodes), append(item, nodes)))
                .collect(),
        ),
    };
    let id = nodes.len();
    nodes.push(node);
    id
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
    let mut submission = vec![("codelist", t(identifier))];
    if extra == Some("role") {
        submission.push(("role", t("Result")));
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
    let mut nodes = vec![];
    let root = append(m(fields), &mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
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
            has_study: false,
            submissions: vec![],
            codelists: vec![Source {
                standard: None,
                codelists: vec![Codelist {
                    id: "SEX".into(),
                    name: "Sex".into(),
                    data_type: DataType::Text,
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
            }],
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
