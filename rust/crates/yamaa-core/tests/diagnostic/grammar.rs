//! Independent vocabulary/context truth from actual shared parser failures.
use std::collections::BTreeSet;
use yamaa_core::{
    aggregate_parser::{parse_aggregate, ParseError as A},
    diagnostic::{ConditionCode, Context, ContextValue as V, Diagnostic},
    numeric_parser::{parse_numeric, ParseError as N},
    value::Value,
};

fn text(v: &str) -> V {
    V::Scalar(Value::Str(v.into()))
}
fn check(
    finding: Diagnostic,
    expression: &str,
    condition: &str,
    requirement: &str,
    fields: Vec<(&str, V)>,
) -> ConditionCode {
    let definition = finding.definition();
    assert_eq!(
        (
            definition.phase,
            definition.condition,
            definition.requirement
        ),
        ("validation", condition, Some(requirement))
    );
    assert_eq!(finding.spec_paths, ["columns.VALUE.derivation.expr"]);
    let mut expected: Context = fields.into_iter().map(|(k, v)| (k.into(), v)).collect();
    expected.insert("expr".into(), text(expression));
    assert_eq!(finding.context, expected);
    assert_eq!(finding.source_span, None);
    assert_eq!(finding.operand_route, None);
    finding.code
}
pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let path = "columns.VALUE.derivation.expr";
    let mut reached = BTreeSet::new();
    for (expression, condition, requirement, extra) in [
        ("1 +", "invalid_numeric_expression", "REQ-0439", vec![]),
        (
            "1 < 2",
            "prohibited_construct",
            "REQ-0441",
            vec![("construct", text("comparison"))],
        ),
        (
            "abs(1, 2)",
            "prohibited_function",
            "REQ-0440",
            vec![
                ("function", text("abs")),
                ("argument_count", V::Integer("2".into())),
            ],
        ),
    ] {
        let Err(N::Grammar { failure, .. }) = parse_numeric(expression, Default::default()) else {
            panic!("numeric grammar failure")
        };
        reached.insert(check(
            failure.diagnostic(path, expression).unwrap(),
            expression,
            condition,
            requirement,
            extra,
        ));
    }
    for (expression, condition, requirement, extra) in [
        (
            "SUM(SRC.ID) +",
            "invalid_aggregate_expression",
            "REQ-0499",
            vec![],
        ),
        (
            "SUM(SRC.ID) < 2",
            "prohibited_construct",
            "REQ-0512",
            vec![("construct", text("comparison"))],
        ),
        (
            "sumx(SRC.ID)",
            "prohibited_function",
            "REQ-0500",
            vec![("function", text("sumx"))],
        ),
        (
            "SUM(SUM(SRC.ID))",
            "nested_reduction",
            "REQ-0502",
            vec![("outer", text("SUM")), ("inner", text("SUM"))],
        ),
    ] {
        let Err(A::Grammar { failure, .. }) = parse_aggregate(expression, Default::default())
        else {
            panic!("aggregate grammar failure")
        };
        reached.insert(check(
            failure.diagnostic(path, expression).unwrap(),
            expression,
            condition,
            requirement,
            extra,
        ));
    }
    reached
}

#[test]
fn caller_supplied_function_span_cannot_slice_a_different_expression() {
    let Err(N::Grammar { failure, .. }) = parse_numeric("  foo(1)", Default::default()) else {
        panic!("prohibited function");
    };
    assert!(failure
        .diagnostic("columns.VALUE", "\u{03b2}\u{03b2}\u{03b2}")
        .is_none());
    let Err(A::Grammar { failure, .. }) = parse_aggregate("  sumx(SRC.ID)", Default::default())
    else {
        panic!("prohibited function");
    };
    assert!(failure.diagnostic("columns.VALUE", "").is_none());
}
