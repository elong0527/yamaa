use std::collections::BTreeSet;
use yamaa_core::{
    dataset::{ElapsedUnit as U, Temporal, TemporalKind as K},
    diagnostic::ConditionCode as C,
    value::Value,
};

pub(crate) fn reached() -> BTreeSet<C> {
    let mut reached = BTreeSet::new();
    for (kind, values, code, phase, condition, rule) in [
        (
            K::DateShift,
            vec![Value::Str("2030-01-01".into()), Value::Int(1)],
            C::DateShiftInput,
            "validation",
            "incompatible_input_type",
            "TIME-14",
        ),
        (
            K::DateShift,
            vec![Value::Date("9999-12-31".parse().unwrap()), Value::Int(1)],
            C::DateShiftRange,
            "convert",
            "conversion_failed",
            "TIME-14",
        ),
        (
            K::DateTimeDifference { unit: U::Hour },
            vec![Value::Date("2030-01-01".parse().unwrap()), Value::Missing],
            C::DatetimeDifferenceInput,
            "validation",
            "incompatible_input_type",
            "TIME-4",
        ),
        (
            K::DateTimeShift { unit: U::Second },
            vec![Value::Missing, Value::float(1.0)],
            C::DatetimeShiftInput,
            "validation",
            "incompatible_input_type",
            "TIME-14",
        ),
        (
            K::DateTimeShift { unit: U::Second },
            vec![
                Value::DateTime("9999-12-31T23:59:59".parse().unwrap()),
                Value::Int(1),
            ],
            C::DatetimeShiftRange,
            "convert",
            "conversion_failed",
            "TIME-14",
        ),
        (
            K::DurationValue { unit: U::Second },
            vec![Value::Int(1)],
            C::DurationValueInput,
            "validation",
            "incompatible_input_type",
            "TIME-17",
        ),
        (
            K::DurationValue { unit: U::Hour },
            vec![Value::Str("P1M".into())],
            C::DurationInvalid,
            "impute",
            "invalid_duration_text",
            "TIME-17",
        ),
        (
            K::DateText { depth: 2 },
            vec![Value::Str("2030".into()), Value::float(1.0)],
            C::DateTextInput,
            "validation",
            "incompatible_input_type",
            "TIME-19",
        ),
        (
            K::DateText { depth: 2 },
            vec![Value::Str("2030".into()), Value::Str("13".into())],
            C::DateTextInvalid,
            "impute",
            "invalid_date_text",
            "TIME-19",
        ),
        (
            K::TimeOfDay,
            vec![Value::Str("08:30".into())],
            C::TimeOfDayInput,
            "validation",
            "incompatible_input_type",
            "TIME-20",
        ),
    ] {
        let operation = Temporal::new(
            kind,
            kind.fields().iter().map(|field| (*field).into()).collect(),
            "columns.RESULT.derivation".into(),
        )
        .unwrap();
        let diagnostic = operation.apply_values(&values).unwrap().unwrap_err();
        assert_eq!(diagnostic.code, code);
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().rule
            ),
            (phase, condition, Some(rule))
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(diagnostic.code);
    }
    reached
}
