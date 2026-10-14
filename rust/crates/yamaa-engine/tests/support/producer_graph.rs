use yamaa_core::{
    producer_admission::{Candidate, WrittenPath},
    producer_graph::SuppliedNode,
    project_environment::{Draft, Environment, ExecutionEnvironment, LockKind, LockReference},
    project_function::{Case, Definition, Language, Parameter},
    schema::{Document, DocumentNode as N, SpecificationDocument, ValidationBudget},
    value::{ColumnType, Value, ValueType},
};
enum Tree {
    Text(String),
    Map(Vec<(String, Tree)>),
    List(Vec<Tree>),
}
impl Tree {
    fn text(s: &str) -> Self {
        Self::Text(s.into())
    }
    fn map(items: Vec<(&str, Self)>) -> Self {
        Self::Map(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let n = match self {
            Self::Text(s) => N::Text(s),
            Self::Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (Self::text(&k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
            Self::List(items) => N::Sequence(items.into_iter().map(|v| v.append(nodes)).collect()),
        };
        let id = nodes.len();
        nodes.push(n);
        id
    }
}
pub fn environment(names: &[&str]) -> ExecutionEnvironment {
    let functions = names
        .iter()
        .map(|name| Definition {
            name: (*name).into(),
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
        })
        .collect();
    Environment::admit(
        Language::Python,
        Draft {
            language: Some(Language::Python),
            lock: Some(LockReference {
                written: "uv.lock".into(),
                kind: LockKind::Uv,
            }),
            functions: Some(functions),
            codelists: vec![],
            has_study: false,
            submissions: vec![],
        },
    )
    .unwrap()
    .into_execution()
}
pub fn document(inputs: &[(&str, Option<&str>)], call: Option<&str>) -> SpecificationDocument {
    use Tree as T;
    let first = inputs[0].0;
    let source = T::map(vec![(
        "source",
        T::map(vec![("variable", T::text(&format!("{first}.ID")))]),
    )]);
    let value = call.map_or_else(
        || {
            T::map(vec![(
                "source",
                T::map(vec![("variable", T::text(&format!("{first}.ID")))]),
            )])
        },
        |name| {
            T::map(vec![(
                "function",
                T::map(vec![
                    ("name", T::text(name)),
                    ("args", T::map(vec![("x", T::text("ID"))])),
                ]),
            )])
        },
    );
    let mut nodes = Vec::new();
    let root = T::map(vec![
        ("schema_version", T::text("1.0")),
        ("domain", T::text("TEST")),
        ("base", T::text(first)),
        ("keys", T::List(vec![T::text("ID")])),
        (
            "input",
            T::Map(
                inputs
                    .iter()
                    .map(|(name, schema)| {
                        let mut fields = vec![("path", T::text("produced.csv"))];
                        if let Some(schema) = schema {
                            fields.push(("schema", T::text(schema)));
                        }
                        ((*name).into(), T::map(fields))
                    })
                    .collect(),
            ),
        ),
        (
            "columns",
            T::List(vec![
                T::map(vec![
                    ("name", T::text("ID")),
                    ("type", T::text("int")),
                    ("label", T::text("Identifier")),
                    ("derivation", T::map(vec![("value", source)])),
                ]),
                T::map(vec![
                    ("name", T::text("VALUE")),
                    ("type", T::text("int")),
                    ("label", T::text("Value")),
                    ("derivation", T::map(vec![("value", value)])),
                ]),
            ]),
        ),
        (
            "output",
            T::map(vec![
                ("path", T::text("produced.csv")),
                ("columns", T::List(vec![T::text("ID"), T::text("VALUE")])),
            ]),
        ),
    ])
    .append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
pub fn candidate<'a>(
    consumer: &'a str,
    dataset: &'a str,
    identity: &'a str,
    document: &'a SpecificationDocument,
) -> Candidate<'a> {
    Candidate {
        dataset,
        schema_path: identity,
        schema_origin: WrittenPath {
            declaring_source: consumer,
            written: identity,
        },
        input_origin: WrittenPath {
            declaring_source: consumer,
            written: "produced.csv",
        },
        output_origin: WrittenPath {
            declaring_source: identity,
            written: "produced.csv",
        },
        schema_identity: identity,
        producer_identity: identity,
        source_identity: "/produced.csv",
        output_identity: "/produced.csv",
        document,
    }
}
pub fn node<'a>(
    identity: &'a str,
    document: &'a SpecificationDocument,
    producers: &'a [Candidate<'a>],
) -> SuppliedNode<'a> {
    SuppliedNode {
        identity,
        document,
        producers,
    }
}
