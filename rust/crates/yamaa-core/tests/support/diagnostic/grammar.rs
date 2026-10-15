//! Independent vocabulary/context truth from actual shared parser failures.
#![allow(unused_imports)]
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::json::Value as V;
pub(crate) use yamaa_core::{
    aggregate_parser::{parse_aggregate, ParseError as A},
    diagnostic::{ConditionCode, Context, Diagnostic},
    numeric_parser::{parse_numeric, ParseError as N},
};

pub(crate) fn text(v: &str) -> V {
    V::String(v.into())
}
pub(crate) fn check(
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
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
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
                (
                    "argument_count",
                    V::Number(("2").parse().expect("canonical integer")),
                ),
            ],
        ),
    ] {
        let Err(N::Grammar { failure, .. }) = parse_numeric(expression) else {
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
        let Err(A::Grammar { failure, .. }) = parse_aggregate(expression) else {
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

pub(crate) fn predicate_reached(
) -> std::collections::BTreeSet<yamaa_core::diagnostic::ConditionCode> {
    pub(crate) use yamaa_core::json::Value as V;
    pub(crate) use yamaa_core::{
        diagnostic::ConditionCode as C,
        predicate_parser::{parse_predicate, ParseError},
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
        }) = parse_predicate(text)
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
        assert_eq!(d.context["predicate"], V::String(text.into()));
        assert_eq!(d.context["position"], V::from(position.character));
        assert_eq!(d.source_span, None);
        assert_eq!(d.operand_route, None);
        reached.insert(d.code);
    }
    let expression = "A = 9223372036854775808";
    let Err(ParseError::Grammar {
        position, failure, ..
    }) = parse_predicate(expression)
    else {
        panic!("overflowing integer literal must fail parsing")
    };
    let d = failure.original_diagnostic("filter", expression, position.character);
    assert_eq!(d.code, C::PredicateIntegerOverflow);
    assert_eq!(d.definition().phase, "parse");
    assert_eq!(d.definition().condition, "integer_overflow");
    assert_eq!(d.definition().requirement, Some("REQ-0014"));
    assert_eq!(d.spec_paths, ["filter"]);
    assert_eq!(
        d.context,
        [
            ("predicate".into(), V::String(expression.into())),
            (
                "position".into(),
                V::Number(("4").parse().expect("canonical integer"))
            ),
        ]
        .into()
    );
    assert_eq!(d.source_span, None);
    assert_eq!(d.operand_route, None);
    reached.insert(d.code);
    reached
}
