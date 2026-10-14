use yamaa_core::{
    project_function_result::results_match,
    temporal::{Date, DatePrecision},
    value::Value,
};
#[test]
fn exact_decimal_ties_go_away_from_zero_without_round_interval_tolerance() {
    for (actual, expected, decimals, matches) in [
        (0.5, 1.0, 0, true),
        (-0.5, -1.0, 0, true),
        (0.49, -0.49, 0, true),
        (0.5, -0.5, 0, false),
        (1.23445, 1.2345, 4, true),
        (-1.23445, -1.2345, 4, true),
        (1.23444, 1.2345, 4, false),
        (2.675, 2.67, 2, true),
        (2.675, 2.68, 2, false),
        (2.685, 2.69, 2, true),
    ] {
        assert_eq!(
            results_match(&Value::float(actual), &Value::float(expected), decimals),
            matches,
            "{actual} {expected} {decimals}"
        );
    }
}
#[test]
fn minimum_subnormal_and_huge_precision_are_bounded_and_do_not_coalesce() {
    let smallest = Value::float(f64::from_bits(1));
    let next = Value::float(f64::from_bits(2));
    assert!(results_match(&smallest, &Value::float(0.0), 323));
    assert!(!results_match(&smallest, &Value::float(0.0), 324));
    for digits in [324, 340, i64::MAX] {
        assert!(results_match(&smallest, &smallest, digits));
        assert!(!results_match(&smallest, &next, digits));
        assert!(!results_match(
            &Value::float(f64::MAX),
            &Value::float(f64::from_bits(f64::MAX.to_bits() - 1)),
            digits
        ));
    }
}
#[test]
fn comparison_never_mutates_runtime_bits_or_normalizes_text() {
    let actual = Value::float(1.23445);
    let negative_zero = Value::float(-0.0);
    assert!(results_match(&actual, &Value::float(1.2345), 4));
    let Value::Float(value) = actual else {
        unreachable!()
    };
    assert_eq!(value.get().to_bits(), 1.23445f64.to_bits());
    assert!(results_match(&negative_zero, &Value::float(0.0), 4));
    let Value::Float(value) = negative_zero else {
        unreachable!()
    };
    assert_eq!(value.get().to_bits(), 0x8000_0000_0000_0000);
    assert!(!results_match(
        &Value::Str("e\u{301}".into()),
        &Value::Str("\u{e9}".into()),
        4
    ));
    assert!(results_match(
        &Value::Str("a\0b".into()),
        &Value::Str("a\0b".into()),
        4
    ));
}
#[test]
fn all_other_values_use_exact_type_and_missing_equality() {
    for digits in [0, 4, i64::MAX] {
        assert!(results_match(&Value::Missing, &Value::Missing, digits));
        assert!(!results_match(&Value::Missing, &Value::Int(0), digits));
        assert!(!results_match(&Value::Int(1), &Value::float(1.0), digits));
        assert!(results_match(
            &Value::Int(i64::MAX),
            &Value::Int(i64::MAX),
            digits
        ));
        let full = Value::Date(Date::new(2024, 1, 1, DatePrecision::Day).unwrap());
        let collected = Value::Date(Date::new(2024, 1, 1, DatePrecision::Year).unwrap());
        assert!(results_match(&full, &collected, digits));
    }
    assert!(!results_match(&Value::Int(1), &Value::Int(1), -1));
}
