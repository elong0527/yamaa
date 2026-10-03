use std::cmp::Ordering::{Equal, Greater, Less};
use yamaa_core::numeric::Number;
use yamaa_core::temporal::{Date, DatePrecision};
use yamaa_core::value::{
    compare_present, ComparisonError, FiniteFloat, Selection, Value, ValueType,
};

#[test]
fn boundary_normalization_preserves_finite_bits_and_rejects_every_nonfinite_kind() {
    for bits in [
        0,
        1,
        0x8000000000000000,
        0x7fefffffffffffff,
        0xffefffffffffffff,
    ] {
        let Value::Float(value) = Value::float(f64::from_bits(bits)) else {
            panic!("finite");
        };
        assert_eq!(value.get().to_bits(), bits);
    }
    for bits in [
        0x7ff0000000000000,
        0xfff0000000000000,
        0x7ff0000000000001,
        0x7ff8000000000000,
        0xffffffffffffffff,
    ] {
        let float = f64::from_bits(bits);
        assert_eq!(FiniteFloat::new(float), None);
        assert_eq!(Value::float(float), Value::Missing);
        assert_eq!(Number::float(float), Number::Missing);
    }
}

#[test]
fn missing_absence_empty_text_boolean_and_i64_are_distinct() {
    assert_ne!(Selection::Absent, Selection::Present(Value::Missing));
    assert_ne!(Value::Missing, Value::Str(String::new()));
    assert_eq!(Value::Str(".nan".into()).value_type(), Some(ValueType::Str));
    assert_eq!(Value::Bool(true).value_type(), Some(ValueType::Bool));
    assert_eq!(Number::try_from(&Value::Bool(true)), Err(ValueType::Bool));
    assert_eq!(
        Number::try_from(&Value::Str("42".into())),
        Err(ValueType::Str)
    );
    assert_eq!(Number::try_from(&Value::Missing), Ok(Number::Missing));
    for integer in [i64::MIN, i64::MAX, 9_007_199_254_740_993] {
        let number = Number::try_from(&Value::Int(integer)).unwrap();
        assert_eq!(Value::from(number), Value::Int(integer));
    }
}

#[test]
fn mixed_numeric_comparison_keeps_full_integer_precision() {
    for (integer, float, expected) in [
        (9_007_199_254_740_993, 9_007_199_254_740_992.0, Greater),
        (-9_007_199_254_740_993, -9_007_199_254_740_992.0, Less),
        (i64::MAX, 9_223_372_036_854_775_808.0, Less),
        (i64::MIN, -9_223_372_036_854_775_808.0, Equal),
        (i64::MIN + 1, -9_223_372_036_854_775_808.0, Greater),
        (i64::MAX, f64::MAX, Less),
        (i64::MIN, -f64::MAX, Greater),
        (0, -0.0, Equal),
        (0, 0.5, Less),
        (0, -0.5, Greater),
        (1, 1.5, Less),
        (-1, -1.5, Greater),
        (0, f64::from_bits(1), Less),
        (0, -f64::from_bits(1), Greater),
    ] {
        let (left, right) = (Value::Int(integer), Value::float(float));
        assert_eq!(compare_present(&left, &right), Ok(expected));
        assert_eq!(compare_present(&right, &left), Ok(expected.reverse()));
    }
    assert_eq!(
        compare_present(&Value::float(-0.0), &Value::float(0.0)),
        Ok(Equal)
    );
}

#[test]
fn ordered_comparison_does_not_coerce_or_choose_missing_placement() {
    assert_eq!(
        compare_present(&Value::Missing, &Value::Int(1)),
        Err(ComparisonError::MissingOperand)
    );
    assert_eq!(
        compare_present(&Value::Int(1), &Value::Str("1".into())),
        Err(ComparisonError::IncompatibleTypes)
    );
    assert_eq!(
        compare_present(&Value::Bool(false), &Value::Bool(true)),
        Err(ComparisonError::IncompatibleTypes)
    );
    assert_eq!(
        compare_present(
            &Value::Str("\u{e000}".into()),
            &Value::Str("\u{10000}".into())
        ),
        Ok(Less)
    );
    assert_eq!(
        compare_present(&Value::Str("é".into()), &Value::Str("e\u{301}".into())),
        Ok(Greater)
    );
    let imputed = Date::new(2024, 2, 29, DatePrecision::Year).unwrap();
    let collected: Date = "2024-02-29".parse().unwrap();
    assert_eq!(
        compare_present(&Value::Date(imputed), &Value::Date(collected)),
        Ok(Equal)
    );
    assert_eq!(
        Number::try_from(&Value::Date(collected)),
        Err(ValueType::Date)
    );
}
