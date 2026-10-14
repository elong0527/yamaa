use yamaa_core::{
    dataset::{ElapsedUnit as U, Temporal, TemporalKind as K, TemporalStep},
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::Value,
};

fn operation(kind: K) -> Temporal {
    Temporal::new(
        kind,
        kind.fields().iter().map(|name| (*name).into()).collect(),
        "columns.RESULT.derivation.operation".into(),
    )
    .unwrap()
}
fn result(kind: K, values: &[Value]) -> Value {
    match operation(kind).apply_values(values).unwrap().unwrap() {
        TemporalStep::Value(value) => value,
        TemporalStep::Invalid => panic!("unexpected recovery"),
    }
}
fn date(text: &str) -> Value {
    Value::Date(text.parse().unwrap())
}
fn datetime(text: &str) -> Value {
    Value::DateTime(text.parse().unwrap())
}
fn text(text: &str) -> Value {
    Value::Str(text.into())
}

#[test]
fn date_shift_crosses_calendar_boundaries_and_preserves_imputation_precision() {
    for (source, days, expected) in [
        ("2030-01-31", 28, "2030-02-28"),
        ("2024-02-28", 1, "2024-02-29"),
        ("2024-03-01", -1, "2024-02-29"),
        ("2024-12-31", 1, "2025-01-01"),
        ("0001-01-01", 3_652_058, "9999-12-31"),
        ("9999-12-31", -3_652_058, "0001-01-01"),
    ] {
        assert_eq!(
            result(K::DateShift, &[date(source), Value::Int(days)]),
            date(expected)
        );
    }
    let source = Value::Date(Date::new(2024, 2, 1, DatePrecision::Month).unwrap());
    let Value::Date(shifted) = result(K::DateShift, &[source, Value::Int(28)]) else {
        panic!()
    };
    assert_eq!(shifted.to_string(), "2024-02-29");
    assert_eq!(shifted.collected_precision(), DatePrecision::Month);
}

#[test]
fn datetime_diff_matches_pk_relative_times_and_civil_dst_boundaries() {
    for (sample, expected) in [
        ("2030-01-01T07:45:00", -0.25),
        ("2030-01-01T10:00:00", 2.0),
        ("2030-01-02T09:30:00", 25.5),
    ] {
        assert_eq!(
            result(
                K::DateTimeDifference { unit: U::Hour },
                &[datetime("2030-01-01T08:00:00"), datetime(sample)]
            ),
            Value::float(expected)
        );
    }
    for (start, end, expected) in [
        ("2024-02-28T23:59:59", "2024-03-01T00:00:00", 86401),
        ("2024-12-31T23:59:59", "2025-01-01T00:00:00", 1),
        ("2025-03-09T01:30:00", "2025-03-09T03:30:00", 7200),
        ("0001-01-01T00:00:00", "9999-12-31T23:59:59", 315537897599),
    ] {
        assert_eq!(
            result(
                K::DateTimeDifference { unit: U::Second },
                &[datetime(start), datetime(end)]
            ),
            Value::Int(expected)
        );
        assert_eq!(
            result(
                K::DateTimeDifference { unit: U::Second },
                &[datetime(end), datetime(start)]
            ),
            Value::Int(-expected)
        );
    }
    for (unit, divisor) in [(U::Minute, 60.0), (U::Hour, 3600.0), (U::Day, 86400.0)] {
        assert_eq!(
            result(
                K::DateTimeDifference { unit },
                &[
                    datetime("2030-01-01T00:00:00"),
                    datetime("2030-01-01T00:00:01")
                ]
            ),
            Value::float(1.0 / divisor)
        );
    }
}

#[test]
fn datetime_shift_scales_exact_integers_and_rounds_result_ties_even() {
    for (source, amount, unit, expected) in [
        (
            "2024-02-28T23:59:59",
            Value::Int(1),
            U::Second,
            "2024-02-29T00:00:00",
        ),
        (
            "2024-12-31T23:30:00",
            Value::float(0.5),
            U::Hour,
            "2025-01-01T00:00:00",
        ),
        (
            "2030-01-01T08:00:00",
            Value::Int(24),
            U::Hour,
            "2030-01-02T08:00:00",
        ),
        (
            "2030-01-01T08:00:00",
            Value::Int(4),
            U::Hour,
            "2030-01-01T12:00:00",
        ),
        (
            "2030-01-01T00:00:00",
            Value::float(0.025),
            U::Minute,
            "2030-01-01T00:00:02",
        ),
        (
            "2030-01-01T00:00:01",
            Value::float(0.025),
            U::Minute,
            "2030-01-01T00:00:02",
        ),
        (
            "2030-01-01T00:00:00",
            Value::float(-0.025),
            U::Minute,
            "2029-12-31T23:59:58",
        ),
        (
            "9999-12-31T00:00:01",
            Value::float(0.025),
            U::Minute,
            "9999-12-31T00:00:02",
        ),
        (
            "0001-01-01T00:00:00",
            Value::Int(3652058),
            U::Day,
            "9999-12-31T00:00:00",
        ),
    ] {
        assert_eq!(
            result(K::DateTimeShift { unit }, &[datetime(source), amount]),
            datetime(expected)
        );
    }
    let date: Date = "2024-02-28".parse().unwrap();
    let source = DateTime::new(date, 0, 0, 0, DateTimePrecision::Day).unwrap();
    let Value::DateTime(shifted) = result(
        K::DateTimeShift { unit: U::Day },
        &[Value::DateTime(source), Value::Int(1)],
    ) else {
        panic!()
    };
    assert_eq!(shifted.collected_precision(), DateTimePrecision::Day);
}

#[test]
fn duration_value_reads_signed_fractional_and_week_forms_exactly() {
    for (source, unit, expected) in [
        ("-PT15M", U::Hour, Value::float(-0.25)),
        ("P1DT2H3M", U::Second, Value::Int(93780)),
        ("P2W", U::Day, Value::float(14.0)),
        ("PT0.5H", U::Second, Value::Int(1800)),
        ("PT0,5H", U::Minute, Value::float(30.0)),
        ("P0.5D", U::Hour, Value::float(12.0)),
        ("PT0.1S", U::Second, Value::float(0.1)),
        ("-PT0S", U::Second, Value::Int(0)),
        ("PT9223372036854775807S", U::Second, Value::Int(i64::MAX)),
        ("-PT9223372036854775808S", U::Second, Value::Int(i64::MIN)),
    ] {
        assert_eq!(result(K::DurationValue { unit }, &[text(source)]), expected);
    }
}

#[test]
fn duration_text_writes_canonical_days_and_exact_integer_boundaries() {
    for (source, unit, expected) in [
        (Value::Int(93780), U::Second, "P1DT2H3M"),
        (Value::float(2.5), U::Hour, "PT2H30M"),
        (Value::Int(26), U::Hour, "P1DT2H"),
        (Value::Int(7), U::Day, "P7D"),
        (Value::float(-0.25), U::Hour, "-PT15M"),
        (Value::float(-0.0), U::Second, "PT0S"),
        (Value::float(0.1), U::Second, "PT0.1S"),
        (Value::float(0.0000001), U::Second, "PT0.0000001S"),
        (Value::Int(0), U::Day, "PT0S"),
    ] {
        assert_eq!(result(K::DurationText { unit }, &[source]), text(expected));
    }
    for source in [
        i64::MIN,
        -9007199254740993,
        -1,
        0,
        1,
        9007199254740993,
        i64::MAX,
    ] {
        let written = result(K::DurationText { unit: U::Second }, &[Value::Int(source)]);
        assert_eq!(
            result(K::DurationValue { unit: U::Second }, &[written]),
            Value::Int(source)
        );
    }
}

#[test]
fn invalid_duration_text_recovers_only_when_declared() {
    for source in [
        "",
        "P",
        "PT",
        "P1DT",
        "P1M",
        "P1Y",
        "P1W2D",
        "P1DT2W",
        "PT1M1H",
        "PT1H1H",
        "PT1.0H2M",
        "P0.5DT1H",
        "+PT1S",
        " PT1S",
        "PT1S ",
        "PT-1S",
        "PT.5S",
        "PT1.S",
        "PT1e3S",
        "pt1s",
        "PT9223372036854775808S",
    ] {
        let operation = operation(K::DurationValue { unit: U::Second });
        let error = operation
            .apply_values(&[text(source)])
            .unwrap()
            .unwrap_err();
        assert_eq!(
            error.definition().condition,
            "invalid_duration_text",
            "{source}"
        );
        assert_eq!(
            operation
                .with_invalid(Value::Missing)
                .unwrap()
                .apply_values(&[text(source)]),
            Some(Ok(TemporalStep::Invalid)),
            "{source}"
        );
    }
}

#[test]
fn arithmetic_overflow_is_fatal_and_missingness_does_not_hide_wrong_types() {
    for (kind, values, requirement) in [
        (
            K::DateShift,
            vec![date("0001-01-01"), Value::Int(-1)],
            "REQ-1298",
        ),
        (
            K::DateShift,
            vec![date("9999-12-31"), Value::Int(1)],
            "REQ-1298",
        ),
        (
            K::DateShift,
            vec![date("2030-01-01"), Value::Int(i64::MAX)],
            "REQ-1298",
        ),
        (
            K::DateTimeShift { unit: U::Second },
            vec![datetime("9999-12-31T23:59:59"), Value::Int(1)],
            "REQ-1300",
        ),
        (
            K::DateTimeShift { unit: U::Day },
            vec![datetime("2030-01-01T00:00:00"), Value::float(f64::MAX)],
            "REQ-1300",
        ),
        (
            K::DurationText { unit: U::Day },
            vec![Value::float(f64::MAX)],
            "REQ-1302",
        ),
    ] {
        let error = operation(kind).apply_values(&values).unwrap().unwrap_err();
        assert_eq!(error.definition().condition, "conversion_failed");
        assert_eq!(error.definition().requirement, Some(requirement));
    }
    for (kind, values) in [
        (K::DateShift, vec![Value::Missing, Value::float(1.0)]),
        (
            K::DateTimeDifference { unit: U::Hour },
            vec![Value::Missing, date("2030-01-01")],
        ),
        (
            K::DateTimeShift { unit: U::Second },
            vec![Value::Missing, Value::float(1.0)],
        ),
        (K::DurationText { unit: U::Second }, vec![text("1")]),
        (K::DurationValue { unit: U::Second }, vec![Value::Int(1)]),
    ] {
        assert_eq!(
            operation(kind)
                .apply_values(&values)
                .unwrap()
                .unwrap_err()
                .definition()
                .condition,
            "incompatible_input_type"
        );
    }
    for kind in [
        K::DateShift,
        K::DateTimeDifference { unit: U::Hour },
        K::DateTimeShift { unit: U::Second },
        K::DurationValue { unit: U::Second },
        K::DurationText { unit: U::Second },
    ] {
        assert_eq!(
            result(kind, &vec![Value::Missing; kind.fields().len()]),
            Value::Missing
        );
        assert_eq!(operation(kind).apply_values(&[]), None);
    }
}

#[test]
fn duration_handler_metadata_and_operand_indices_remain_bounded() {
    use yamaa_core::{dataset::InvalidTemporal, diagnostic::MAX_PROJECTION_BYTES};
    let duration = operation(K::DurationValue { unit: U::Hour });
    assert_eq!(duration.validate_operand(1, &Value::Missing), None);
    assert_eq!(duration.with_invalid(Value::Str("x".repeat(MAX_PROJECTION_BYTES / 6))), Err(InvalidTemporal::Limit));
    assert_eq!(operation(K::DateShift).with_invalid(Value::Missing), Err(InvalidTemporal::Shape));
}
