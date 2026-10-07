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

pub(super) fn predicate_reached(
) -> std::collections::BTreeSet<yamaa_core::diagnostic::ConditionCode> {
    use yamaa_core::{
        diagnostic::{ConditionCode as C, ContextValue as V},
        predicate_parser::{parse_predicate, ParseError},
        value::Value,
    };
    let mut reached = std::collections::BTreeSet::new();
    for (text, code, requirement) in [
        ("A >", C::PredicateInvalidExpression, "REQ-0188"),
        (
            "A LIKE 'a' ESCAPE 'ab'",
            C::PredicateInvalidEscape,
            "REQ-0191",
        ),
        ("STR_CONTAINS(A, '(')", C::PredicateInvalidRegex, "REQ-1244"),
        (
            "A = DATE '2026-02-30'",
            C::PredicateInvalidExpression,
            "REQ-0188",
        ),
    ] {
        let Err(ParseError::Grammar {
            position, failure, ..
        }) = parse_predicate(text, Default::default())
        else {
            panic!("predicate must fail")
        };
        let d = failure.diagnostic("columns.VALUE.window.filter", text, position.character);
        assert_eq!(d.code, code);
        assert_eq!(d.definition().phase, "validation");
        assert_eq!(d.definition().condition, "invalid_predicate");
        assert_eq!(d.definition().requirement, Some(requirement));
        assert_eq!(failure.condition(), "invalid_predicate");
        assert_eq!(failure.requirement(), requirement);
        assert_eq!(d.spec_paths, ["columns.VALUE.window.filter"]);
        assert_eq!(d.context["predicate"], V::Scalar(Value::Str(text.into())));
        assert_eq!(
            d.context["position"],
            V::Integer(position.character.to_string())
        );
        assert_eq!(d.source_span, None);
        assert_eq!(d.operand_route, None);
        reached.insert(d.code);
    }
    reached
}
