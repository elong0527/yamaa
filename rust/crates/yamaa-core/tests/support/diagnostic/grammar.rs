//! Independent vocabulary/context truth from actual shared expression failures.
#![allow(unused_imports)]
use super::rendered;
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::json::Value as V;
pub(crate) use yamaa_core::{
    diagnostic::{ConditionCode, Context, Diagnostic},
    expression::{compile, CompileError, Context as Field},
};

pub(crate) use super::text;
#[track_caller]
pub(crate) fn check(
    finding: Diagnostic,
    expression: &str,
    condition: &str,
    rule: &str,
    mut fields: Vec<(&str, V)>,
) -> ConditionCode {
    fields.push(("expr", text(expression)));
    super::expect(
        &finding,
        "validation",
        condition,
        Some(rule),
        &["columns.VALUE.derivation.expr"],
        fields,
    )
}
/// One syntax condition, one refused-construct cause and one refused-function
/// cause serve compute and aggregate alike.
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let path = "columns.VALUE.derivation.expr";
    let mut reached = BTreeSet::new();
    for (field, expression, condition, rule, extra) in [
        (
            Field::Compute,
            "1 +",
            "invalid_expression",
            "FORMULA-1",
            vec![],
        ),
        (
            Field::Compute,
            "1 < 2",
            "prohibited_construct",
            "FORMULA-27",
            vec![("construct", text("comparison"))],
        ),
        (
            Field::Compute,
            "abs(1, 2)",
            "prohibited_function",
            "FORMULA-13",
            vec![
                ("function", text("abs")),
                (
                    "argument_count",
                    V::Number(("2").parse().expect("canonical integer")),
                ),
            ],
        ),
        (
            Field::Aggregate,
            "SUM(SRC.ID) +",
            "invalid_expression",
            "FORMULA-1",
            vec![],
        ),
        (
            Field::Aggregate,
            "SUM(SRC.ID) < 2",
            "prohibited_construct",
            "FORMULA-27",
            vec![("construct", text("comparison"))],
        ),
        (
            Field::Aggregate,
            "sumx(SRC.ID)",
            "prohibited_function",
            "FORMULA-13",
            vec![("function", text("sumx"))],
        ),
        (
            Field::Aggregate,
            "SUM(SUM(SRC.ID))",
            "nested_reduction",
            "FORMULA-21",
            vec![("outer", text("SUM")), ("inner", text("SUM"))],
        ),
    ] {
        let Err(CompileError::Invalid {
            position, failure, ..
        }) = compile(expression, field, path)
        else {
            panic!("expression failure")
        };
        reached.insert(check(
            failure.diagnostic(field, path, expression, position.character),
            expression,
            condition,
            rule,
            extra,
        ));
    }
    reached
}

pub(crate) fn predicate_reached(
) -> std::collections::BTreeSet<yamaa_core::diagnostic::ConditionCode> {
    pub(crate) use yamaa_core::diagnostic::ConditionCode as C;
    let mut reached = std::collections::BTreeSet::new();
    for (text, code, rule) in [
        ("A >", C::ExpressionInvalid, "FORMULA-1"),
        (
            "A LIKE 'a' ESCAPE 'ab'",
            C::ExpressionInvalidEscape,
            "FORMULA-7",
        ),
        (
            "STR_CONTAINS(A, '(')",
            C::ExpressionInvalidRegex,
            "FORMULA-8",
        ),
        ("A = DATE '2026-02-30'", C::ExpressionInvalid, "FORMULA-1"),
    ] {
        let Err(CompileError::Invalid {
            position, failure, ..
        }) = compile(text, Field::Predicate, "columns.VALUE.window.filter")
        else {
            panic!("predicate must fail")
        };
        let d = failure.diagnostic(
            Field::Predicate,
            "columns.VALUE.window.filter",
            text,
            position.character,
        );
        assert_eq!(d.code, code);
        assert_eq!(d.definition().phase, "validation");
        assert_eq!(d.definition().condition, "invalid_expression");
        assert_eq!(d.definition().rule, Some(rule));
        assert_eq!(failure.condition(), "invalid_expression");
        assert_eq!(failure.rule(), rule);
        assert_eq!(d.spec_paths, ["columns.VALUE.window.filter"]);
        assert_eq!(d.context["predicate"], V::String(text.into()));
        assert_eq!(d.context["position"], V::from(position.character));
        assert_eq!(d.source_span, None);
        assert_eq!(d.operand_route, None);
        reached.insert(rendered(&d));
    }
    let expression = "A = 9223372036854775808";
    let Err(CompileError::Invalid {
        position, failure, ..
    }) = compile(expression, Field::Predicate, "filter")
    else {
        panic!("an overflowing integer literal fails before data")
    };
    let d = failure.diagnostic(Field::Predicate, "filter", expression, position.character);
    assert_eq!(d.code, C::LiteralIntegerOverflow);
    assert_eq!(d.definition().phase, "validation");
    assert_eq!(d.definition().condition, "integer_overflow");
    assert_eq!(d.definition().rule, Some("VALUE-5"));
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
    reached.insert(rendered(&d));
    reached
}
