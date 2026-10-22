use super::rendered;
use super::text;
use std::collections::BTreeSet;
use yamaa_core::json::Value as V;
use yamaa_core::{
    dataset::{
        completion_policy, DatePolicyError, DayDeclaration as D, FlagPart, Imputation,
        ImputationKind as K, ImputationStep, TemporalType as T, TimeRule,
    },
    diagnostic::{ConditionCode, Diagnostic},
    table::ValueRef,
    temporal::Precision as P,
};
fn operation(kind: K) -> Imputation {
    Imputation::new(
        kind,
        "SRC.V".into(),
        None,
        None,
        None,
        "columns.V.derivation.impute".into(),
        None,
    )
    .unwrap()
}
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let mut check = |diagnostic: Diagnostic,
                     phase: &str,
                     condition: &str,
                     rule: &str,
                     path: &str,
                     fields: Vec<(&str, V)>| {
        let definition = diagnostic.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            (phase, condition, Some(rule))
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
        reached.insert(rendered(&diagnostic));
    };
    for (month, day, minimum, condition, rule, path, fields) in [
        (
            Some("6"),
            Some(D::Number("1")),
            P::Month,
            "month_not_permitted",
            "TIME-10",
            "impute.month",
            vec![("month", V::from(6))],
        ),
        (
            None,
            Some(D::Number("1")),
            P::Year,
            "month_required",
            "TIME-10",
            "impute.month",
            vec![("minimum_source_precision", text("year"))],
        ),
        (
            Some("15"),
            Some(D::Number("1")),
            P::Year,
            "month_out_of_range",
            "TIME-10",
            "impute.month",
            vec![("month", V::from(15))],
        ),
        (
            None,
            None,
            P::Month,
            "day_required",
            "TIME-10",
            "impute.day",
            vec![("minimum_source_precision", text("month"))],
        ),
        (
            None,
            Some(D::Position("last")),
            P::Day,
            "day_not_permitted",
            "TIME-10",
            "impute.day",
            vec![("day", text("last"))],
        ),
        (
            Some("6"),
            Some(D::Number("32")),
            P::Year,
            "value_not_permitted",
            "TIME-10",
            "impute",
            vec![
                ("field", text("day")),
                ("value", V::from(32)),
                ("permitted", text("1 to 31")),
            ],
        ),
        (
            Some("6"),
            Some(D::Position("mid")),
            P::Year,
            "value_not_permitted",
            "TIME-10",
            "impute.day",
            vec![
                ("value", text("mid")),
                ("permitted", V::Array(vec![text("first"), text("last")])),
            ],
        ),
    ] {
        let DatePolicyError::Diagnostic(diagnostic) =
            completion_policy(T::Date, month, day, minimum, None, "impute").unwrap_err()
        else {
            panic!("admitted semantic policy failure")
        };
        check(diagnostic, "validation", condition, rule, path, fields);
    }
    let date = K::Impute(
        completion_policy(
            T::Date,
            Some("12"),
            Some(D::Number("31")),
            P::Year,
            None,
            "impute",
        )
        .unwrap(),
    );
    let datetime = K::Impute(
        completion_policy(
            T::DateTime,
            None,
            None,
            P::Day,
            Some(TimeRule::First),
            "impute",
        )
        .unwrap(),
    );
    // One grammar serves both types: a missing source is no failure, and text
    // that is not collected date text is invalid date text.
    for kind in [
        date,
        datetime,
        K::Precision { target: T::Date },
        K::Precision {
            target: T::DateTime,
        },
    ] {
        let op = operation(kind);
        assert_eq!(
            op.source_value(ValueRef::Missing),
            Ok(ImputationStep::Missing)
        );
        check(
            op.source_value(ValueRef::Str("UNK")).unwrap_err(),
            "impute",
            "invalid_date_text",
            "TIME-13",
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
        "TIME-10",
        "columns.V.derivation.impute",
        vec![
            ("value", text("2023-04")),
            ("completed", text("2023-04-31")),
        ],
    );
    for (kind, rule, expected) in [
        (date, "TIME-1", "str or date"),
        (datetime, "TIME-8", "str or datetime"),
        (
            K::Precision {
                target: T::DateTime,
            },
            "TIME-7",
            "str or datetime",
        ),
        (
            K::Flag {
                part: FlagPart::Date,
                seconds_collected: true,
            },
            "TIME-18",
            "date or datetime",
        ),
        (
            K::ToDate,
            "TIME-2",
            "datetime, ISO date text, or ISO 8601 datetime text",
        ),
    ] {
        check(
            operation(kind).source_value(ValueRef::Int(1)).unwrap_err(),
            "validation",
            "incompatible_input_type",
            rule,
            "columns.V.derivation.impute.source",
            vec![
                ("source", text("SRC.V")),
                ("expected", text(expected)),
                ("actual", text("int")),
            ],
        );
    }
    assert_eq!(
        operation(K::ToDate).source_value(ValueRef::Str("2024")),
        Ok(ImputationStep::Partial)
    );
    check(
        operation(K::ToDate)
            .source_value(ValueRef::Str("2024-13"))
            .unwrap_err(),
        "validation",
        "invalid_date_text",
        "TIME-2",
        "columns.V.derivation.impute",
        vec![("operation", text("to_date")), ("value", text("2024-13"))],
    );
    // An interval or duration names no single date, and its failure says so.
    check(
        operation(date)
            .source_value(ValueRef::Str("2024-01-01/2024-01-10"))
            .unwrap_err(),
        "impute",
        "interval_or_duration_text",
        "TIME-13",
        "columns.V.derivation.impute",
        vec![
            ("operation", text("impute")),
            ("source", text("SRC.V")),
            ("value", text("2024-01-01/2024-01-10")),
        ],
    );
    // `to_date` fails on one the same way, at the same stage.
    check(
        operation(K::ToDate)
            .source_value(ValueRef::Str("P3D"))
            .unwrap_err(),
        "impute",
        "interval_or_duration_text",
        "TIME-2",
        "columns.V.derivation.impute",
        vec![
            ("operation", text("to_date")),
            ("source", text("SRC.V")),
            ("value", text("P3D")),
        ],
    );
    reached
}
