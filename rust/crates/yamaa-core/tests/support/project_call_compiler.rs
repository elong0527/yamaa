use yamaa_core::{
    project_function::{Case, Definition, Function, Language, Parameter},
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
    value::{ColumnType, Value, ValueType},
};
pub enum Tree {
    Scalar(N),
    Map(Vec<(String, Tree)>),
    List(Vec<Tree>),
}
impl Tree {
    pub fn text(value: &str) -> Self {
        Self::Scalar(N::Text(value.into()))
    }
    pub fn map(fields: Vec<(&str, Self)>) -> Self {
        Self::Map(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Scalar(n) => n,
            Self::List(items) => {
                N::Sequence(items.into_iter().map(|item| item.append(nodes)).collect())
            }
            Self::Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(key, item)| (Self::text(&key).append(nodes), item.append(nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
pub fn function(name: &str) -> Function {
    Function::admit(
        Language::Python,
        Definition {
            name: name.into(),
            function: format!("project.{name}"),
            description: "Integer identity.".into(),
            params: vec![Parameter {
                name: "x".into(),
                kind: ValueType::Int,
                required: true,
                default: None,
                accepts_missing: false,
            }],
            returns: ColumnType::Int,
            may_return_missing: false,
            comparison_decimals: 4,
            tests: vec![
                Case {
                    id: "normal".into(),
                    covers: vec!["normal".into()],
                    args: vec![("x".into(), Value::Int(7))],
                    result: Value::Int(7),
                },
                Case {
                    id: "boundary".into(),
                    covers: vec!["boundary".into()],
                    args: vec![("x".into(), Value::Int(i64::MIN))],
                    result: Value::Int(i64::MIN),
                },
                Case {
                    id: "missing".into(),
                    covers: vec!["short-circuit-missing:x".into()],
                    args: vec![("x".into(), Value::Missing)],
                    result: Value::Missing,
                },
            ],
        },
    )
    .unwrap()
}
pub fn call(name: &str, args: Vec<(&str, Tree)>) -> Tree {
    Tree::map(vec![(
        "function",
        Tree::map(vec![("name", Tree::text(name)), ("args", Tree::map(args))]),
    )])
}
#[allow(dead_code)]
pub fn specification(columns: Vec<(&str, &str, Tree)>) -> SpecificationDocument {
    specification_with_rows(columns, Vec::new())
}
pub fn specification_with_rows(
    columns: Vec<(&str, &str, Tree)>,
    rows: Vec<Tree>,
) -> SpecificationDocument {
    let mut projection = vec![Tree::text("ID")];
    let mut declarations = vec![Tree::map(vec![
        ("name", Tree::text("ID")),
        ("type", Tree::text("int")),
        (
            "derivation",
            Tree::map(vec![(
                "value",
                Tree::map(vec![(
                    "source",
                    Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                )]),
            )]),
        ),
    ])];
    for (name, kind, value) in columns {
        projection.push(Tree::text(name));
        declarations.push(Tree::map(vec![
            ("name", Tree::text(name)),
            ("type", Tree::text(kind)),
            ("derivation", Tree::map(vec![("value", value)])),
        ]));
    }
    let mut fields = vec![
        ("schema_version", Tree::text("1.0")),
        ("domain", Tree::text("TEST")),
        (
            "input",
            Tree::map(vec![(
                "SRC",
                Tree::map(vec![("path", Tree::text("input.csv"))]),
            )]),
        ),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        ("columns", Tree::List(declarations)),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("output.csv")),
                ("columns", Tree::List(projection)),
            ]),
        ),
    ];
    if !rows.is_empty() {
        fields.push(("rows", Tree::List(rows)));
    }
    let mut nodes = vec![];
    let root = Tree::map(fields).append(&mut nodes);
    let d = Document::new(nodes, root, DocumentLimits::default()).unwrap();
    SpecificationDocument::admit(d, &mut ValidationBudget::new(Default::default()))
        .unwrap()
        .unwrap()
}
