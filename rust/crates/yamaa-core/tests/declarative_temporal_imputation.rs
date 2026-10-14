use yamaa_core::{
    dataset::{
        date_policy, DayDeclaration as D, Imputation, ImputationKind as K, ImputationStep as S,
        TimeRule,
    },
    table::ValueRef,
    temporal::{Date, DatePrecision as P, DateTime, DateTimePrecision as TP},
    value::Value,
};
fn operation(kind: K, bound: bool, missing: Option<Value>, invalid: Option<Value>) -> Imputation {
    Imputation::new(
        kind,
        "SRC.TEXT".into(),
        bound.then(|| "REF".into()),
        "columns.DATE.derivation.impute".into(),
        missing,
        invalid,
    )
    .unwrap()
}
fn date_policy_kind(minimum: P, day: D<'_>) -> K {
    date_policy((minimum == P::Year).then_some("6"), day, minimum, "date").unwrap()
}
#[test]
fn independent_partial_dates_preserve_precision_and_calendar_bound_intervals() {
    let first = operation(
        date_policy_kind(P::Year, D::Position("first")),
        false,
        None,
        None,
    );
    let last = operation(
        date_policy_kind(P::Year, D::Position("last")),
        false,
        None,
        None,
    );
    for (input, early, late, precision) in [
        ("1975", "1975-06-01", "1975-06-30", P::Year),
        ("1980-02", "1980-02-01", "1980-02-29", P::Month),
        ("1900-02", "1900-02-01", "1900-02-28", P::Month),
        ("1990-06-15", "1990-06-15", "1990-06-15", P::Day),
    ] {
        for (op, expected) in [(&first, early), (&last, late)] {
            let S::Value(Value::Date(date)) = op.source_value(ValueRef::Str(input)).unwrap() else {
                panic!("date")
            };
            assert_eq!(date.to_string(), expected);
            assert_eq!(date.collected_precision(), precision);
            let flag = operation(K::DatePrecision, false, None, None)
                .source_value(ValueRef::Date(date))
                .unwrap();
            assert_eq!(
                flag,
                S::Value(Value::Str(
                    match precision {
                        P::Year => "Y",
                        P::Month => "M",
                        P::Day => "D",
                    }
                    .into()
                ))
            );
        }
    }
    let bounded = operation(
        date_policy_kind(P::Year, D::Position("first")),
        true,
        None,
        None,
    );
    for (text, lower, expected) in [
        ("2024", "2024-07-20", Some("2024-07-20")),
        ("2024-02", "2024-02-29", Some("2024-02-29")),
        ("2024-02", "2024-03-01", None),
        ("2024", "2025-01-01", None),
    ] {
        let S::Bound(completed) = bounded.source_value(ValueRef::Str(text)).unwrap() else {
            panic!("bound")
        };
        let result = bounded
            .apply_bound(completed, ValueRef::Date(lower.parse().unwrap()))
            .unwrap();
        assert_eq!(
            result,
            expected.map_or(Value::Missing, |text| Value::Date(text.parse().unwrap()))
        );
        if let Value::Date(date) = result {
            assert_eq!(date.collected_precision(), completed.collected_precision());
        }
    }
    assert!(matches!(
        bounded.source_value(ValueRef::Str("2024-02-29")),
        Ok(S::Value(_))
    ));
    let month = operation(
        date_policy_kind(P::Month, D::Position("first")),
        true,
        Some(Value::Int(1)),
        Some(Value::Int(2)),
    );
    assert_eq!(
        month.source_value(ValueRef::Str("2024")),
        Ok(S::Value(Value::Missing))
    );
    assert_eq!(
        month.source_value(ValueRef::Missing),
        Ok(S::Recovered { handler: "missing" })
    );
    assert_eq!(
        month.source_value(ValueRef::Str("2024-00")),
        Ok(S::Recovered { handler: "invalid" })
    );
}
#[test]
fn datetime_edges_precision_and_explicit_date_conversion_are_field_copies() {
    for (rule, expected) in [
        (TimeRule::First, "2024-02-29T00:00:00"),
        (TimeRule::Last, "2024-02-29T23:59:59"),
    ] {
        let op = operation(K::DateTime { time: rule }, false, None, None);
        let S::Value(Value::DateTime(value)) =
            op.source_value(ValueRef::Str("2024-02-29")).unwrap()
        else {
            panic!("datetime")
        };
        assert_eq!(value.to_string(), expected);
        assert_eq!(value.collected_precision(), TP::Day);
        assert_eq!(
            operation(K::DateTimePrecision, false, None, None)
                .source_value(ValueRef::DateTime(value)),
            Ok(S::Value(Value::Str("D".into())))
        );
        let S::Value(Value::Date(date)) = operation(K::ToDate, false, None, None)
            .source_value(ValueRef::DateTime(value))
            .unwrap()
        else {
            panic!("date")
        };
        assert_eq!(date.to_string(), "2024-02-29");
        assert_eq!(date.collected_precision(), P::Day);
    }
    let date: Date = "2024-02-29".parse().unwrap();
    let datetime: DateTime = "2024-02-29T12:13".parse().unwrap();
    let op = operation(
        K::DateTime {
            time: TimeRule::First,
        },
        false,
        None,
        None,
    );
    assert!(op.source_value(ValueRef::Date(date)).is_err());
    assert_eq!(
        op.source_value(ValueRef::DateTime(datetime)),
        Ok(S::Value(Value::DateTime(datetime)))
    );
    assert_eq!(
        operation(K::DateTimePrecision, false, None, None)
            .source_value(ValueRef::Str("2024-02-29T12:13")),
        Ok(S::Value(Value::Str("S".into())))
    );
    for invalid in [
        "0000",
        "2024-13",
        "2023-02-29",
        "2024-02-",
        "2024-02-29Z",
        "2024-02-29T00:00:00+00:00",
        "\u{0662}024",
        "2024\u{2010}02",
    ] {
        assert!(
            operation(K::DatePrecision, false, None, None)
                .source_value(ValueRef::Str(invalid))
                .is_err(),
            "{invalid}"
        );
    }
}
#[path = "diagnostic/temporal_imputation.rs"]
mod diagnostic;
#[test]
fn independent_diagnostic_truth_reaches_every_imputation_cause() {
    assert_eq!(diagnostic::reached().len(), 16);
}

#[test]
fn calendar_policy_preserves_wide_integers_and_unreachable_month_precedence() {
    use yamaa_core::dataset::DatePolicyError;
    use yamaa_core::diagnostic::{ConditionCode as C, ContextValue};
    let wide = "9".repeat(5000);
    let DatePolicyError::Diagnostic(error) =
        date_policy(Some(&wide), D::Number("0"), P::Month, "date").unwrap_err()
    else {
        panic!("admitted semantic failure")
    };
    assert_eq!(error.code, C::ImputationMonthNotPermitted);
    assert_eq!(error.context["month"], ContextValue::Integer(wide.clone()));
    let DatePolicyError::Diagnostic(error) =
        date_policy(Some(&wide), D::Number("1"), P::Year, "date").unwrap_err()
    else {
        panic!("admitted semantic failure")
    };
    assert_eq!(error.code, C::ImputationMonthRange);
    assert_eq!(error.context["month"], ContextValue::Integer(wide.clone()));
    let DatePolicyError::Diagnostic(error) =
        date_policy(Some("6"), D::Number(&wide), P::Year, "date").unwrap_err()
    else {
        panic!("admitted semantic failure")
    };
    assert_eq!(error.code, C::ImputationDayRange);
    assert_eq!(error.context["value"], ContextValue::Integer(wide));
}

#[test]
fn public_policy_admits_whole_context_before_semantic_copies_or_integer_scans() {
    use yamaa_core::dataset::{DatePolicyError as E, MAX_IMPUTATION_POLICY_BYTES};
    let oversized = "9".repeat(MAX_IMPUTATION_POLICY_BYTES);
    assert_eq!(
        date_policy(Some(&oversized), D::Number("0"), P::Month, "date"),
        Err(E::Limit)
    );
    assert_eq!(
        date_policy(Some("6"), D::Position(&oversized), P::Year, "date"),
        Err(E::Limit)
    );
    assert_eq!(
        date_policy(Some("6"), D::Number("1"), P::Year, &oversized),
        Err(E::Limit)
    );
    // A formatted member path and its retained diagnostic path can coexist.
    assert_eq!(
        date_policy(
            Some("6"),
            D::Number("1"),
            P::Year,
            &oversized[..MAX_IMPUTATION_POLICY_BYTES / 2]
        ),
        Err(E::Limit)
    );
    for number in ["", "01", "+1", "-0", "1.0", "bad"] {
        assert_eq!(
            date_policy(Some(number), D::Number("1"), P::Year, "date"),
            Err(E::Shape)
        );
        assert_eq!(
            date_policy(Some("6"), D::Number(number), P::Year, "date"),
            Err(E::Shape)
        );
    }
    assert_eq!(
        date_policy(None, D::Number("1"), P::Day, "date"),
        Err(E::Shape)
    );
    assert_eq!(
        date_policy(Some("6"), D::Number("1"), P::Year, ""),
        Err(E::Shape)
    );
}

#[test]
fn constructor_refuses_whole_owned_metadata_and_invalid_shapes_before_later_copies() {
    use yamaa_core::dataset::MAX_IMPUTATION_POLICY_BYTES;
    let oversized = "x".repeat(MAX_IMPUTATION_POLICY_BYTES / 2);
    for field in ["source", "bound", "path", "missing", "invalid"] {
        assert!(
            Imputation::new(
                date_policy_kind(P::Year, D::Position("first")),
                if field == "source" {
                    oversized.clone()
                } else {
                    "SRC.D".into()
                },
                Some(if field == "bound" {
                    oversized.clone()
                } else {
                    "SRC.B".into()
                }),
                if field == "path" {
                    oversized.clone()
                } else {
                    "columns.D".into()
                },
                (field == "missing").then(|| Value::Str(oversized.clone())),
                (field == "invalid").then(|| Value::Str(oversized.clone())),
            )
            .is_none(),
            "{field} metadata must be refused"
        );
    }
    assert!(Imputation::new(
        K::DatePrecision,
        oversized[..oversized.len() / 2].to_owned(),
        None,
        oversized[..oversized.len() / 2].to_owned(),
        None,
        None,
    )
    .is_none());
    assert!(Imputation::new(
        K::DatePrecision,
        String::new(),
        None,
        "path".into(),
        None,
        None
    )
    .is_none());
    assert!(Imputation::new(
        K::DatePrecision,
        "SRC.D".into(),
        None,
        String::new(),
        None,
        None
    )
    .is_none());
    assert!(Imputation::new(
        K::DatePrecision,
        "SRC.D".into(),
        Some("SRC.B".into()),
        "path".into(),
        None,
        None
    )
    .is_none());
    assert!(Imputation::new(
        K::ToDate,
        "SRC.D".into(),
        None,
        "path".into(),
        Some(Value::Missing),
        None
    )
    .is_none());
}
