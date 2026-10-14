use std::collections::BTreeSet;
use yamaa_core::{
    dataset::{ElapsedUnit as U, Temporal, TemporalKind as K},
    diagnostic::ConditionCode as C,
    value::Value,
};

pub(super) fn reached() -> BTreeSet<C> {
    let mut reached = BTreeSet::new();
    for (kind, values, code, phase, condition, requirement) in [
        (
            K::DateShift,
            vec![Value::Str("2030-01-01".into()), Value::Int(1)],
            C::DateShiftInput,
            "validation",
            "incompatible_input_type",
            "REQ-1298",
        ),
        (
            K::DateShift,
            vec![Value::Date("9999-12-31".parse().unwrap()), Value::Int(1)],
            C::DateShiftRange,
            "convert",
            "conversion_failed",
            "REQ-1298",
        ),
        (
            K::DateTimeDifference { unit: U::Hour },
            vec![Value::Date("2030-01-01".parse().unwrap()), Value::Missing],
            C::DatetimeDifferenceInput,
            "validation",
            "incompatible_input_type",
            "REQ-1299",
        ),
        (
            K::DateTimeShift { unit: U::Second },
            vec![Value::Missing, Value::float(1.0)],
            C::DatetimeShiftInput,
            "validation",
            "incompatible_input_type",
            "REQ-1300",
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
            "REQ-1300",
        ),
        (
            K::DurationValue { unit: U::Second },
            vec![Value::Int(1)],
            C::DurationValueInput,
            "validation",
            "incompatible_input_type",
            "REQ-1301",
        ),
        (
            K::DurationText { unit: U::Second },
            vec![Value::Str("PT1S".into())],
            C::DurationTextInput,
            "validation",
            "incompatible_input_type",
            "REQ-1302",
        ),
        (
            K::DurationValue { unit: U::Hour },
            vec![Value::Str("P1M".into())],
            C::DurationInvalid,
            "impute",
            "invalid_duration_text",
            "REQ-1301",
        ),
        (
            K::DurationText { unit: U::Day },
            vec![Value::float(f64::MAX)],
            C::DurationTextRange,
            "convert",
            "conversion_failed",
            "REQ-1302",
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
                diagnostic.definition().requirement
            ),
            (phase, condition, Some(requirement))
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(diagnostic.code);
    }
    reached
}
