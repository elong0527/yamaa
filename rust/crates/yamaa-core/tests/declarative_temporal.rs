use yamaa_core::{
    dataset::{CalendarUnit as U, DateBounds as B, Temporal, TemporalKind as K},
    table::ValueRef,
    temporal::Date,
    value::Value,
};
fn date(text: &str) -> Date {
    text.parse().unwrap()
}
fn difference(start: &str, end: &str, unit: U, bounds: B) -> i64 {
    let operation = Temporal::new(
        K::Difference { unit, bounds },
        vec!["START".into(), "END".into()],
        "diff".into(),
    )
    .unwrap();
    match operation
        .apply(&[Some(date(start)), Some(date(end))])
        .unwrap()
    {
        Value::Int(value) => value,
        other => panic!("{other:?}"),
    }
}
#[test]
fn whole_calendar_units_preserve_clamped_anniversaries_sign_and_endpoint_rules() {
    for (start, end, unit, bounds, expected) in [
        ("2020-02-29", "2021-02-28", U::Year, B::Exclusive, 1),
        ("2020-02-29", "2021-02-27", U::Year, B::Exclusive, 0),
        ("2023-01-31", "2023-02-28", U::Month, B::Exclusive, 1),
        ("2024-01-31", "2024-02-28", U::Month, B::Exclusive, 0),
        ("2024-01-31", "2024-02-29", U::Month, B::Exclusive, 1),
        ("2023-01-31", "2023-03-30", U::Month, B::Exclusive, 1),
        ("2023-01-31", "2023-03-31", U::Month, B::Exclusive, 2),
        ("2024-02-28", "2024-03-07", U::Week, B::Exclusive, 1),
        ("2023-02-28", "2023-03-06", U::Week, B::Exclusive, 0),
        ("2024-02-28", "2024-03-01", U::Day, B::Exclusive, 2),
        ("2024-02-28", "2024-03-01", U::Day, B::Inclusive, 3),
        ("2024-02-28", "2024-03-01", U::Day, B::Between, 1),
        ("0001-01-01", "9999-12-31", U::Day, B::Exclusive, 3_652_058),
    ] {
        assert_eq!(difference(start, end, unit, bounds), expected);
        assert_eq!(difference(end, start, unit, bounds), -expected);
    }
    assert_eq!(
        difference("2024-03-01", "2024-03-01", U::Day, B::Between),
        0
    );
    assert_eq!(
        difference("2024-03-01", "2024-03-01", U::Day, B::Inclusive),
        1
    );
}
#[test]
fn study_day_has_no_zero_and_epoch_counts_use_complete_civil_dates() {
    let study = Temporal::new(
        K::StudyDay,
        vec!["DATE".into(), "REF".into()],
        "study".into(),
    )
    .unwrap();
    for (day, expected) in [("2023-12-31", -1), ("2024-01-01", 1), ("2024-01-02", 2)] {
        assert_eq!(
            study.apply(&[Some(date(day)), Some(date("2024-01-01"))]),
            Some(Value::Int(expected))
        );
    }
    let epoch = Temporal::new(K::EpochDay, vec!["DATE".into()], "epoch".into()).unwrap();
    for (day, expected) in [
        ("1970-01-01", 0),
        ("1969-12-31", -1),
        ("2000-01-01", 10_957),
    ] {
        assert_eq!(epoch.apply(&[Some(date(day))]), Some(Value::Int(expected)));
    }
    assert_eq!(epoch.apply(&[None]), Some(Value::Missing));
    assert_eq!(epoch.apply(&[]), None);
    assert_eq!(
        study.apply(&[None, Some(date("2024-01-01"))]),
        Some(Value::Missing)
    );
}
#[test]
fn immutable_declarations_reject_invalid_arity_and_bounds_and_retain_typed_operand_facts() {
    assert!(Temporal::new(K::StudyDay, vec!["DATE".into()], "study".into()).is_err());
    assert!(Temporal::new(
        K::Difference {
            unit: U::Year,
            bounds: B::Inclusive
        },
        vec!["S".into(), "E".into()],
        "diff".into()
    )
    .is_err());
    let operation = Temporal::new(
        K::StudyDay,
        vec!["DTHDTM".into(), "REF".into()],
        "columns.DTHDY.derivation.study_day".into(),
    )
    .unwrap();
    assert_eq!(operation.operand(0, ValueRef::Missing), Some(Ok(None)));
    let diagnostic = operation
        .operand(0, ValueRef::Str("2024-01-01"))
        .unwrap()
        .unwrap_err();
    assert_eq!(diagnostic.definition().phase, "validation");
    assert_eq!(diagnostic.definition().condition, "incompatible_input_type");
    assert_eq!(diagnostic.definition().requirement, Some("REQ-0606"));
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.DTHDY.derivation.study_day.date"]
    );
    assert_eq!(diagnostic.context.len(), 3);
    assert_eq!(diagnostic.source_span, None);
    assert_eq!(diagnostic.operand_route, None);
}

#[test]
fn oversized_temporal_metadata_is_refused_before_diagnostic_projection() {
    use yamaa_core::{dataset::InvalidTemporal, diagnostic::MAX_PROJECTION_BYTES};
    let huge = "x".repeat(MAX_PROJECTION_BYTES / 6);
    assert_eq!(
        Temporal::new(K::EpochDay, vec![huge], "epoch".into()),
        Err(InvalidTemporal::Limit)
    );
    let valid = Temporal::new(K::EpochDay, vec!["DATE".into()], "epoch".into()).unwrap();
    assert_eq!(valid.operand(1, ValueRef::Missing), None);
}
