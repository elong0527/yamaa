use yamaa_core::{
    scalar_language::replace_extreme,
    temporal::{Date, DatePrecision},
    value::Value,
};
#[test]
fn ties_preserve_first_type_and_precision_and_integer_comparison_is_exact() {
    for largest in [true, false] {
        assert!(!replace_extreme(&Value::Int(1), &Value::float(1.0), largest).unwrap());
        assert!(!replace_extreme(&Value::float(1.0), &Value::Int(1), largest).unwrap());
        let month = Value::Date(Date::new(2025, 1, 1, DatePrecision::Month).unwrap());
        let day = Value::Date(Date::new(2025, 1, 1, DatePrecision::Day).unwrap());
        assert!(!replace_extreme(&month, &day, largest).unwrap());
        assert!(!replace_extreme(&day, &month, largest).unwrap());
    }
    assert!(replace_extreme(
        &Value::float(9_007_199_254_740_992.0),
        &Value::Int(9_007_199_254_740_993),
        true
    )
    .unwrap());
    assert!(replace_extreme(
        &Value::Int(i64::MAX),
        &Value::float(9_223_372_036_854_775_808.0),
        true
    )
    .unwrap());
    assert!(!replace_extreme(
        &Value::Int(i64::MIN),
        &Value::float(-9_223_372_036_854_775_808.0),
        false
    )
    .unwrap());
    assert!(!replace_extreme(&Value::Missing, &Value::Missing, true).unwrap());
    assert!(replace_extreme(&Value::Missing, &Value::Int(2), true).unwrap());
    assert!(replace_extreme(&Value::Int(1), &Value::Str("2".into()), true).is_err());
}
