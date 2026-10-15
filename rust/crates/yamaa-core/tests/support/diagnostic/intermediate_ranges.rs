//! Original declarations reach range and projection causes with complete context.
use crate::support::document::Tree;
use crate::support::table_schema as schema;
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    schema::{Document, SpecificationDocument},
    specification::{BindError, PrepareError, PreparedSpecification},
    value::ColumnType as T,
};

fn model(between: Option<Tree>, columns: Option<Tree>) -> SpecificationDocument {
    let mut item = vec![
        ("id", Tree::text("P")),
        ("dataset", Tree::text("D")),
        ("key", Tree::List(vec![Tree::text("ID")])),
    ];
    if let Some(between) = between {
        item.push(("between", between));
    }
    if let Some(columns) = columns {
        item.push(("columns", columns));
    }
    let mut nodes = vec![];
    let root = Tree::map(vec![
        ("schema_version", Tree::text("1.0")),
        ("domain", Tree::text("TEST")),
        ("base", Tree::text("SRC")),
        (
            "input",
            Tree::map(vec![
                ("SRC", Tree::map(vec![("path", Tree::text("src.csv"))])),
                ("D", Tree::map(vec![("path", Tree::text("donor.csv"))])),
            ]),
        ),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        ("intermediates", Tree::List(vec![Tree::map(item)])),
        (
            "columns",
            Tree::List(vec![Tree::map(vec![
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
            ])]),
        ),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("out.csv")),
                ("columns", Tree::List(vec![Tree::text("ID")])),
            ]),
        ),
    ])
    .append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap()
}
fn text(value: &str) -> JsonValue {
    JsonValue::String(value.into())
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
fn bind(between: Option<Tree>, columns: Option<Tree>, lower: T) -> Vec<Diagnostic> {
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
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(&model(
        Some(Tree::map(vec![("value", Tree::text("ID"))])),
        None,
    )) else {
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
        Some(Tree::map(vec![
            ("value", Tree::text("UNKNOWN")),
            ("lower", Tree::text("ABSENT_LOW")),
            ("upper", Tree::text("ABSENT_HIGH")),
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
        Some(Tree::map(vec![
            ("value", Tree::text("UNKNOWN")),
            ("lower", Tree::text("LOW")),
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
        Some(Tree::map(vec![
            ("value", Tree::text("ID")),
            ("lower", Tree::text("LOW")),
            ("upper", Tree::text("HIGH")),
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
    let mut findings = bind(None, Some(Tree::List(vec![Tree::text("ABSENT")])), T::Int);
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
