use std::collections::BTreeSet;
use yamaa_core::{
    dataset::{
        date_policy, DatePolicyError, DayDeclaration as D, Imputation, ImputationKind as K,
        TimeRule,
    },
    diagnostic::{ConditionCode, ContextValue as V, Diagnostic},
    table::ValueRef,
    temporal::DatePrecision as P,
    value::Value,
};
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
fn operation(kind: K) -> Imputation {
    Imputation::new(
        kind,
        "SRC.V".into(),
        None,
        "columns.V.derivation.impute".into(),
        None,
        None,
    )
    .unwrap()
}
pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let mut check = |diagnostic: Diagnostic,
                     phase: &str,
                     condition: &str,
                     requirement: &str,
                     path: &str,
                     fields: Vec<(&str, V)>| {
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            (phase, condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_paths, [path]);
        assert_eq!(
            diagnostic.context,
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect()
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(diagnostic.code);
    };
    for (month, day, minimum, condition, requirement, path, fields) in [
        (
            Some("6"),
            D::Number("1"),
            P::Month,
            "month_not_permitted",
            "REQ-0592",
            "impute.month",
            vec![("month", V::Scalar(Value::Int(6)))],
        ),
        (
            None,
            D::Number("1"),
            P::Year,
            "month_required",
            "REQ-0592",
            "impute.month",
            vec![("minimum_source_precision", text("year"))],
        ),
        (
            Some("15"),
            D::Number("1"),
            P::Year,
            "month_out_of_range",
            "REQ-0608",
            "impute.month",
            vec![("month", V::Scalar(Value::Int(15)))],
        ),
        (
            Some("6"),
            D::Number("32"),
            P::Year,
            "value_not_permitted",
            "REQ-0608",
            "impute",
            vec![
                ("field", text("day")),
                ("value", V::Scalar(Value::Int(32))),
                ("permitted", text("1 to 31")),
            ],
        ),
        (
            Some("6"),
            D::Position("mid"),
            P::Year,
            "value_not_permitted",
            "REQ-0609",
            "impute.day",
            vec![
                ("value", text("mid")),
                ("permitted", V::Sequence(vec![text("first"), text("last")])),
            ],
        ),
    ] {
        let DatePolicyError::Diagnostic(diagnostic) =
            date_policy(month, day, minimum, "impute").unwrap_err()
        else {
            panic!("admitted semantic policy failure")
        };
        check(
            diagnostic,
            "validation",
            condition,
            requirement,
            path,
            fields,
        );
    }
    let date = date_policy(Some("12"), D::Number("31"), P::Year, "impute").unwrap();
    for (kind, requirement, condition) in [
        (date, "REQ-0588", "invalid_date_text"),
        (
            K::DateTime {
                time: TimeRule::First,
            },
            "REQ-1182",
            "invalid_datetime_text",
        ),
        (K::DateTimePrecision, "REQ-1183", "invalid_datetime_text"),
    ] {
        let op = operation(kind);
        check(
            op.source_value(ValueRef::Missing).unwrap_err(),
            "impute",
            "missing_input",
            requirement,
            "columns.V.derivation.impute",
            vec![
                ("operation", text(op.operation())),
                ("source", text("SRC.V")),
            ],
        );
        check(
            op.source_value(ValueRef::Str("UNK")).unwrap_err(),
            "impute",
            condition,
            requirement,
            "columns.V.derivation.impute",
            vec![
                ("operation", text(op.operation())),
                ("source", text("SRC.V")),
                ("value", text("UNK")),
            ],
        );
    }
    check(
        operation(date)
            .source_value(ValueRef::Str("2023-04"))
            .unwrap_err(),
        "impute",
        "invalid_calendar_date",
        "REQ-0608",
        "columns.V.derivation.impute",
        vec![
            ("value", text("2023-04")),
            ("completed", text("2023-04-31")),
        ],
    );
    for (kind, requirement, expected) in [
        (
            K::DateTime {
                time: TimeRule::First,
            },
            "REQ-1182",
            "str",
        ),
        (K::DateTimePrecision, "REQ-1183", "str or datetime"),
        (
            K::ToDate,
            "REQ-0607",
            "datetime, ISO date text, or ISO 8601 datetime text",
        ),
    ] {
        check(
            operation(kind).source_value(ValueRef::Int(1)).unwrap_err(),
            "validation",
            "incompatible_input_type",
            requirement,
            "columns.V.derivation.impute.source",
            vec![
                ("source", text("SRC.V")),
                ("expected", text(expected)),
                ("actual", text("int")),
            ],
        );
    }
    check(
        operation(K::ToDate)
            .source_value(ValueRef::Str("2024"))
            .unwrap_err(),
        "validation",
        "invalid_date_text",
        "REQ-0607",
        "columns.V.derivation.impute",
        vec![("operation", text("to_date")), ("value", text("2024"))],
    );
    reached
}
