use std::collections::BTreeMap;
use yamaa_core::{
    project_call_document::{decode, BindingFinding, Error, Input, Limits},
    project_function::{CallFinding, Case, Definition, Function, Language, Parameter},
    project_function_document::{Finding, Kind},
    schema::{Document, DocumentLimits, DocumentNode as N},
    value::{ColumnType, Value, ValueType},
};

enum Tree {
    Scalar(N),
    Map(Vec<(String, Tree)>),
}
impl Tree {
    fn text(value: &str) -> Self {
        Self::Scalar(N::Text(value.into()))
    }
    fn map(fields: Vec<(&str, Self)>) -> Self {
        Self::Map(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Scalar(node) => node,
            Self::Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(key, value)| {
                        let key = Self::text(&key).append(nodes);
                        (key, value.append(nodes))
                    })
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
fn call_tree(name: &str, args: Vec<(&str, Tree)>) -> Tree {
    Tree::map(vec![("name", Tree::text(name)), ("args", Tree::map(args))])
}
fn document(tree: Tree) -> Document {
    let mut nodes = vec![];
    let root = tree.append(&mut nodes);
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}
fn integer_function() -> Function {
    Function::admit(
        Language::Python,
        Definition {
            name: "id".into(),
            function: "project.identity".into(),
            description: "Return the integer.".into(),
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
                    id: "ordinary".into(),
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

#[test]
fn references_and_explicit_literals_retain_exact_kinds_order_and_origins() {
    let d = document(call_tree(
        "id",
        vec![
            ("reference", Tree::text("SRC.X")),
            (
                "text",
                Tree::map(vec![("literal", Tree::text("e\u{301}\0😀"))]),
            ),
            (
                "integer",
                Tree::Scalar(N::Integer("9223372036854775807".into())),
            ),
            ("zero", Tree::Scalar(N::Float(-0.0))),
            ("boolean", Tree::Scalar(N::Boolean(true))),
            ("missing", Tree::Scalar(N::Null)),
            ("date", Tree::map(vec![("date", Tree::text("2024-02-29"))])),
            (
                "datetime",
                Tree::map(vec![("datetime", Tree::text("2024-02-29T12:34:56"))]),
            ),
        ],
    ));
    let c = decode(&d, d.root(), "columns.VALUE.function", Limits::default()).unwrap();
    assert_eq!(c.name, "id");
    assert_eq!(c.name_node, d.field(d.root(), "name").unwrap());
    assert_eq!(c.node, d.root());
    assert_eq!(
        c.arguments
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        [
            "reference",
            "text",
            "integer",
            "zero",
            "boolean",
            "missing",
            "date",
            "datetime"
        ]
    );
    assert_eq!(c.arguments[0].input, Input::Reference("SRC.X".into()));
    assert_eq!(
        c.arguments[1].input,
        Input::Literal(Value::Str("e\u{301}\0😀".into()))
    );
    assert_eq!(c.arguments[2].input, Input::Literal(Value::Int(i64::MAX)));
    let Input::Literal(Value::Float(zero)) = c.arguments[3].input else {
        panic!("float kind")
    };
    assert_eq!(zero.get().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(c.arguments[4].input, Input::Literal(Value::Bool(true)));
    assert_eq!(c.arguments[5].input, Input::Literal(Value::Missing));
    assert_eq!(
        c.arguments[6].input,
        Input::Literal(Value::Date("2024-02-29".parse().unwrap()))
    );
    assert_eq!(
        c.arguments[7].input,
        Input::Literal(Value::DateTime("2024-02-29T12:34:56".parse().unwrap()))
    );
    let args = d.field(d.root(), "args").unwrap();
    for argument in c.arguments {
        assert_eq!(argument.node, d.field(args, &argument.name).unwrap());
    }
}

#[test]
fn independent_invalid_scalars_keep_all_authored_nodes_without_partial_calls() {
    let d = document(call_tree(
        "id",
        vec![
            (
                "wide",
                Tree::Scalar(N::Integer("9223372036854775808".into())),
            ),
            ("date", Tree::map(vec![("date", Tree::text("2023-02-29"))])),
            (
                "datetime",
                Tree::map(vec![("datetime", Tree::Scalar(N::Boolean(false)))]),
            ),
        ],
    ));
    let args = d.field(d.root(), "args").unwrap();
    let wide = d.field(args, "wide").unwrap();
    let date = d.field(args, "date").unwrap();
    let datetime = d.field(args, "datetime").unwrap();
    assert_eq!(
        decode(&d, d.root(), "call", Limits::default()),
        Err(Error::Findings(vec![
            Finding {
                path: "call.args.wide".into(),
                node: wide,
                kind: Kind::IntegerRange
            },
            Finding {
                path: "call.args.date".into(),
                node: date,
                kind: Kind::InvalidDate
            },
            Finding {
                path: "call.args.datetime".into(),
                node: d.field(datetime, "datetime").unwrap(),
                kind: Kind::NormalizedShape
            },
            Finding {
                path: "call.args.datetime".into(),
                node: datetime,
                kind: Kind::InvalidDateTime
            },
        ]))
    );
}

#[test]
fn metadata_checks_all_reference_types_after_explicit_missing() {
    let d = document(call_tree(
        "id",
        vec![
            ("x", Tree::Scalar(N::Null)),
            ("other", Tree::text("UNKNOWN")),
        ],
    ));
    let c = decode(&d, d.root(), "call", Limits::default()).unwrap();
    assert_eq!(
        c.validate(&integer_function(), &BTreeMap::new()),
        vec![
            BindingFinding::UnknownReference { argument: 1 },
            BindingFinding::Argument(CallFinding::UnknownArgument { argument: 1 }),
        ]
    );
    let d = document(call_tree("id", vec![("x", Tree::text("SRC.X"))]));
    let c = decode(&d, d.root(), "call", Limits::default()).unwrap();
    let mut refs = BTreeMap::from([("SRC.X".into(), ValueType::Float)]);
    assert_eq!(
        c.validate(&integer_function(), &refs),
        vec![BindingFinding::Argument(CallFinding::ArgumentType {
            argument: 0,
            expected: ValueType::Int,
            actual: ValueType::Float
        })]
    );
    refs.insert("SRC.X".into(), ValueType::Int);
    assert!(c.validate(&integer_function(), &refs).is_empty());
    let d = document(call_tree("other", vec![]));
    assert_eq!(
        decode(&d, d.root(), "call", Limits::default())
            .unwrap()
            .validate(&integer_function(), &refs),
        vec![BindingFinding::FunctionNameMismatch]
    );
}

#[test]
fn call_local_text_budget_ignores_unrelated_document_payload() {
    let d = document(Tree::map(vec![
        ("call", call_tree("id", vec![("x", Tree::text("X"))])),
        ("unrelated", Tree::text(&"q".repeat(100_000))),
    ]));
    let root = d.field(d.root(), "call").unwrap();
    assert!(decode(
        &d,
        root,
        "p",
        Limits {
            arguments: 1,
            text_bytes: 83
        }
    )
    .is_ok());
    assert_eq!(
        decode(
            &d,
            root,
            "p",
            Limits {
                arguments: 1,
                text_bytes: 82
            }
        ),
        Err(Error::Limit("text"))
    );
    assert_eq!(
        decode(
            &d,
            root,
            "p",
            Limits {
                arguments: 0,
                text_bytes: usize::MAX
            }
        ),
        Err(Error::Limit("arguments"))
    );
}

#[test]
fn repeated_long_diagnostic_paths_are_bounded_before_scalar_decoding() {
    let names = (0..32).map(|i| format!("arg{i}")).collect::<Vec<_>>();
    let args = names
        .iter()
        .map(|n| {
            (
                n.as_str(),
                Tree::Scalar(N::Integer("9223372036854775808".into())),
            )
        })
        .collect();
    let d = document(call_tree("id", args));
    assert_eq!(
        decode(&d, d.root(), &"p".repeat(10_000), Limits::default()),
        Err(Error::Limit("text"))
    );
}

#[test]
fn retired_call_fields_and_non_text_literal_wrappers_never_enter_the_model() {
    let d = document(Tree::map(vec![
        ("name", Tree::text("id")),
        ("contract_version", Tree::text("1")),
        (
            "args",
            Tree::map(vec![(
                "x",
                Tree::map(vec![("literal", Tree::Scalar(N::Integer("7".into())))]),
            )]),
        ),
    ]));
    let args = d.field(d.root(), "args").unwrap();
    let x = d.field(args, "x").unwrap();
    assert_eq!(
        decode(&d, d.root(), "call", Limits::default()),
        Err(Error::Findings(vec![
            Finding {
                path: "call".into(),
                node: d.root(),
                kind: Kind::NormalizedShape
            },
            Finding {
                path: "call.args.x".into(),
                node: d.field(x, "literal").unwrap(),
                kind: Kind::NormalizedShape
            },
        ]))
    );
    let d = document(call_tree(
        "id",
        vec![(
            "x",
            Tree::map(vec![
                ("date", Tree::text("2024-02-29")),
                ("datetime", Tree::text("2024-02-29T12:34:56")),
            ]),
        )],
    ));
    let x = d.field(d.field(d.root(), "args").unwrap(), "x").unwrap();
    assert_eq!(
        decode(&d, d.root(), "call", Limits::default()),
        Err(Error::Findings(vec![Finding {
            path: "call.args.x".into(),
            node: x,
            kind: Kind::NormalizedShape
        },]))
    );
}

#[test]
fn diagnostic_context_copies_of_a_long_function_name_are_preflighted() {
    let names = (0..32).map(|i| format!("arg{i}")).collect::<Vec<_>>();
    let args = names
        .iter()
        .map(|n| {
            (
                n.as_str(),
                Tree::Scalar(N::Integer("9223372036854775808".into())),
            )
        })
        .collect();
    let d = document(call_tree(&"f".repeat(16_384), args));
    assert_eq!(
        decode(&d, d.root(), "call", Limits::default()),
        Err(Error::Limit("text"))
    );
}
