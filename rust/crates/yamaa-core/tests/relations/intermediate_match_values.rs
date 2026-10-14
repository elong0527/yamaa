//! Independent normalized-document admission, with no engine or host effects.
use yamaa_core::{
    dataset::{MatchRead, MatchValue},
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
    specification::{BindError, CompilationLimits, PrepareError, PreparedSpecification},
    table::{Column, TableSchema, ValueRef},
    value::{compare_present_refs, ColumnType as T, Value},
};
enum Tree<'a> {
    Text(&'a str),
    Scalar(N),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Text(value) => N::Text(value.into()),
            Self::Scalar(node) => node,
            Self::List(items) => N::Sequence(items.into_iter().map(|v| v.append(nodes)).collect()),
            Self::Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(k, v)| (Self::Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
fn doc(key: Tree<'_>) -> SpecificationDocument {
    use Tree::*;
    let column = |name, kind, variable| {
        Map(vec![
            ("name", Text(name)),
            ("type", Text(kind)),
            (
                "derivation",
                Map(vec![(
                    "value",
                    Map(vec![("source", Map(vec![("variable", Text(variable))]))]),
                )]),
            ),
        ])
    };
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("OUT")),
        ("base", Text("SRC")),
        (
            "input",
            Map(vec![
                ("SRC", Map(vec![("path", Text("src.csv"))])),
                ("TAB", Map(vec![("path", Text("tab.csv"))])),
            ]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "intermediates",
            List(vec![Map(vec![
                ("id", Text("LOOK")),
                ("dataset", Text("TAB")),
                ("key", key),
                ("no_match", Scalar(N::Null)),
            ])]),
        ),
        (
            "columns",
            List(vec![
                column("ID", "str", "SRC.ID"),
                column("N", "int", "SRC.N"),
                column("V", "str", "LOOK.VAL"),
            ]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("out.csv")),
                ("columns", List(vec![Text("ID"), Text("N"), Text("V")])),
            ]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, DocumentLimits::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
fn schemas() -> (TableSchema, TableSchema) {
    let schema = |fields: &[(&str, T)]| {
        TableSchema::new(
            fields
                .iter()
                .map(|(name, kind)| Column {
                    name: (*name).into(),
                    kind: *kind,
                })
                .collect(),
        )
        .unwrap()
    };
    (
        schema(&[("ID", T::Str), ("N", T::Int)]),
        schema(&[
            ("K", T::Str),
            ("KINT", T::Int),
            ("KNUM", T::Float),
            ("VAL", T::Str),
        ]),
    )
}
fn pad<'a>(source: &'a str, width: &'a str) -> Tree<'a> {
    Tree::Map(vec![(
        "str_pad",
        Tree::Map(vec![
            ("source", Tree::Text(source)),
            ("width", Tree::Scalar(N::Integer(width.into()))),
        ]),
    )])
}
fn finding_ids(key: Tree<'_>) -> Vec<(String, Vec<String>)> {
    let prepared = PreparedSpecification::prepare(&doc(key)).unwrap();
    let (src, tab) = schemas();
    let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&src, &tab]) else {
        panic!("invalid")
    };
    findings
        .iter()
        .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
        .map(|d| {
            (
                d.code.definition().requirement.unwrap().to_string(),
                d.spec_paths,
            )
        })
        .collect()
}
#[test]
fn padding_and_source_values_have_no_destination_and_preserve_dependency_scope() {
    for source in ["N", "SRC.N"] {
        let prepared =
            PreparedSpecification::prepare(&doc(Tree::Map(vec![("K", pad(source, "8"))]))).unwrap();
        let (src, tab) = schemas();
        let plan = prepared.bind_sources(&[&src, &tab]).unwrap();
        let keys = &plan.intermediates()[0].keys;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].source_column, 0);
        assert_eq!(keys[0].path, "intermediates[0].key.K");
        let MatchValue::StrPad {
            read,
            operation: Some(op),
        } = &keys[0].value
        else {
            panic!("owned padding")
        };
        assert_eq!(op.source(), source);
        assert_eq!(
            matches!(read, MatchRead::Output { column: 1, .. }),
            source == "N"
        );
        assert_eq!(plan.output().columns().len(), 3);
    }
    let prepared = PreparedSpecification::prepare(&doc(Tree::Map(vec![(
        "KNUM",
        Tree::Map(vec![("source", Tree::Text("SRC.N"))]),
    )])))
    .unwrap();
    let (src, tab) = schemas();
    let plan = prepared.bind_sources(&[&src, &tab]).unwrap();
    assert!(matches!(
        &plan.intermediates()[0].keys[0].value,
        MatchValue::Source(MatchRead::Record { column: 1, .. })
    ));
}
#[test]
fn authored_type_donor_width_reference_then_dependent_read_failure_order() {
    for (field, first) in [
        ("K", None),
        ("KINT", Some("REQ-0323")),
        ("ABSENT", Some("REQ-0116")),
    ] {
        let actual = finding_ids(Tree::Map(vec![(field, pad("SRC.ABSENT", "0"))]));
        let mut expected = Vec::new();
        if let Some(id) = first {
            expected.push((id.into(), vec!["intermediates[0].key".into()]));
        }
        expected.extend([
            (
                "REQ-1261".into(),
                vec![format!("intermediates[0].key.{field}.str_pad.width")],
            ),
            (
                "REQ-0117".into(),
                vec![format!("intermediates[0].key.{field}")],
            ),
            (
                "REQ-0103".into(),
                vec!["columns.V.derivation.source".into()],
            ),
        ]);
        assert_eq!(actual, expected);
    }
    assert_eq!(
        finding_ids(Tree::Map(vec![("K", pad("SRC.N", "0"))])),
        [(
            "REQ-1261".into(),
            vec!["intermediates[0].key.K.str_pad.width".into()]
        )]
    );
}
#[test]
fn literal_text_is_not_a_reference_and_boolean_family_is_not_coerced() {
    let prepared = PreparedSpecification::prepare(&doc(Tree::Map(vec![(
        "K",
        Tree::Map(vec![("literal", Tree::Text("SRC.ABSENT"))]),
    )])))
    .unwrap();
    let (src, tab) = schemas();
    let plan = prepared.bind_sources(&[&src, &tab]).unwrap();
    assert_eq!(
        plan.intermediates()[0].keys[0].value,
        MatchValue::Literal(Value::Str("SRC.ABSENT".into()))
    );
    assert_eq!(
        finding_ids(Tree::Map(vec![(
            "KINT",
            Tree::Map(vec![("literal", Tree::Scalar(N::Boolean(true)))])
        )]))[0]
            .0,
        "REQ-0323"
    );
}
#[test]
fn aggregate_key_count_refuses_before_deferred_width_ownership_or_findings() {
    let document = doc(Tree::Map(vec![
        ("K", pad("SRC.ABSENT", "0")),
        (
            "KINT",
            Tree::Map(vec![("literal", Tree::Scalar(N::Integer("1".into())))]),
        ),
    ]));
    let limits = CompilationLimits {
        keys: 1,
        ..Default::default()
    };
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(&document, limits),
        Err(PrepareError::Limit("intermediate_keys"))
    ));
}
#[test]
fn unsupported_operations_keep_the_authored_gate_path() {
    let (key, operation, path) = (
        Tree::Map(vec![(
            "K",
            Tree::Map(vec![(
                "compute",
                Tree::Map(vec![("expr", Tree::Text("1 / 0"))]),
            )]),
        )]),
        "compute",
        "intermediates[0].key.K.compute",
    );
    let Err(PrepareError::Unsupported(features)) = PreparedSpecification::prepare(&doc(key)) else {
        panic!("explicit unsupported gate")
    };
    assert!(features
        .iter()
        .any(|f| f.operation == operation && f.path == path));
}
#[test]
fn borrowed_equality_preserves_complete_integer_precision_and_missingness() {
    use core::cmp::Ordering::*;
    for (left, right, result) in [
        (Value::Int(7), Value::float(7.), Equal),
        (
            Value::Int(9007199254740993),
            Value::float(9007199254740992.),
            Greater,
        ),
        (
            Value::Int(i64::MAX),
            Value::float(9223372036854775808.),
            Less,
        ),
        (
            Value::Int(i64::MIN),
            Value::float(-9223372036854775808.),
            Equal,
        ),
        (Value::float(-0.), Value::float(0.), Equal),
    ] {
        assert_eq!(
            compare_present_refs(ValueRef::from(&left), ValueRef::from(&right)),
            Ok(result)
        );
    }
    assert!(compare_present_refs(ValueRef::Missing, ValueRef::Int(0)).is_err());
}
