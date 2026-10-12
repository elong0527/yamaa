//! Original declarations reach range and projection causes with complete context.
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    schema::{Document, DocumentNode as N, SpecificationDocument, ValidationBudget},
    specification::{BindError, PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
    value::{ColumnType as T, Value},
};
enum Tree<'a> {
    Text(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
use Tree::*;
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Text(v) => N::Text(v.into()),
            List(items) => N::Sequence(items.into_iter().map(|v| v.append(nodes)).collect()),
            Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
fn model(between: Option<Tree<'_>>, columns: Option<Tree<'_>>) -> SpecificationDocument {
    let mut item = vec![
        ("id", Text("P")),
        ("dataset", Text("D")),
        ("key", Text("ID")),
    ];
    if let Some(between) = between {
        item.push(("between", between));
    }
    if let Some(columns) = columns {
        item.push(("columns", columns));
    }
    let mut nodes = vec![];
    let root = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("base", Text("SRC")),
        (
            "input",
            Map(vec![
                ("SRC", Map(vec![("path", Text("src.csv"))])),
                ("D", Map(vec![("path", Text("donor.csv"))])),
            ]),
        ),
        ("keys", List(vec![Text("ID")])),
        ("intermediates", List(vec![Map(item)])),
        (
            "columns",
            List(vec![Map(vec![
                ("name", Text("ID")),
                ("type", Text("int")),
                (
                    "derivation",
                    Map(vec![(
                        "value",
                        Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
                    )]),
                ),
            ])]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("out.csv")),
                ("columns", List(vec![Text("ID")])),
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
fn schema(fields: &[(&str, T)]) -> TableSchema {
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
}
fn text(value: &str) -> ContextValue {
    ContextValue::Scalar(Value::Str(value.into()))
}
fn check(
    d: Diagnostic,
    condition: &str,
    requirement: &str,
    path: &str,
    context: &[(&str, &str)],
) -> ConditionCode {
    assert_eq!(
        (
            d.definition().phase,
            d.definition().condition,
            d.definition().requirement
        ),
        ("validation", condition, Some(requirement))
    );
    assert_eq!(d.spec_paths, [path]);
    assert_eq!(
        d.context,
        context
            .iter()
            .map(|(name, value)| ((*name).into(), text(value)))
            .collect()
    );
    assert_eq!(d.source_span, None);
    assert_eq!(d.operand_route, None);
    d.code
}
fn bind(between: Option<Tree<'_>>, columns: Option<Tree<'_>>, lower: T) -> Vec<Diagnostic> {
    let prepared = PreparedSpecification::prepare(&model(between, columns)).unwrap();
    let src = schema(&[("ID", T::Int)]);
    let donor = schema(&[
        ("ID", T::Int),
        ("LOW", lower),
        ("HIGH", T::Float),
        ("V", T::Str),
    ]);
    let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&src, &donor]) else {
        panic!("authored binding must fail")
    };
    findings
        .iter()
        .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
        .collect()
}
pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let Err(PrepareError::Invalid(findings)) =
        PreparedSpecification::prepare(&model(Some(Map(vec![("value", Text("ID"))])), None))
    else {
        panic!("a range with neither bound must fail")
    };
    assert_eq!(findings.len(), 1);
    reached.insert(check(
        findings[0].diagnostic(),
        "invalid_field_type",
        "REQ-1049",
        "intermediates[0].between",
        &[
            ("intermediate", "P"),
            ("expected", "at least one of lower, upper"),
        ],
    ));
    let findings = bind(
        Some(Map(vec![
            ("value", Text("UNKNOWN")),
            ("lower", Text("ABSENT_LOW")),
            ("upper", Text("ABSENT_HIGH")),
        ])),
        None,
        T::Int,
    );
    assert_eq!(findings.len(), 2);
    for (finding, name) in findings.into_iter().zip(["ABSENT_LOW", "ABSENT_HIGH"]) {
        reached.insert(check(
            finding,
            "unknown_field",
            "REQ-0121",
            "intermediates[0].between",
            &[("intermediate", "P"), ("identifier", name)],
        ));
    }
    let mut findings = bind(
        Some(Map(vec![
            ("value", Text("UNKNOWN")),
            ("lower", Text("LOW")),
        ])),
        None,
        T::Int,
    );
    assert_eq!(findings.len(), 1);
    reached.insert(check(
        findings.remove(0),
        "unknown_field",
        "REQ-0121",
        "intermediates[0].between.value",
        &[("intermediate", "P"), ("identifier", "UNKNOWN")],
    ));
    let mut findings = bind(
        Some(Map(vec![
            ("value", Text("ID")),
            ("lower", Text("LOW")),
            ("upper", Text("HIGH")),
        ])),
        None,
        T::Str,
    );
    assert_eq!(findings.len(), 1);
    reached.insert(check(
        findings.remove(0),
        "incomparable_range_types",
        "REQ-0121",
        "intermediates[0].between",
        &[
            ("intermediate", "P"),
            ("value_type", "int"),
            ("lower_type", "str"),
            ("upper_type", "float"),
        ],
    ));
    let mut findings = bind(None, Some(List(vec![Text("ABSENT")])), T::Int);
    assert_eq!(findings.len(), 1);
    reached.insert(check(
        findings.remove(0),
        "unknown_field",
        "REQ-0122",
        "intermediates[0].columns",
        &[("intermediate", "P"), ("identifier", "D.ABSENT")],
    ));
    reached
}
