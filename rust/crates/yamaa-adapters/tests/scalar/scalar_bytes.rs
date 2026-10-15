use yamaa_adapters::scalar_bytes::{decode, encode, exact_float, operate, MAX_SCALAR_BYTES};
use yamaa_core::value::Value;

/// Every i64 endpoint remains distinct from missing and every raw float bit survives.
#[test]
fn lossless_primitive_carriers() {
    for n in [i64::MIN, i64::MAX, 9007199254740993, -2147483648, 0] {
        assert_eq!(decode(1, n.to_string().as_bytes()), Ok(Value::Int(n)));
        let bytes = encode(Value::Int(n)).unwrap();
        assert_eq!(bytes.tag, 1);
        assert_eq!(bytes.payload, n.to_string().as_bytes());
    }
    for bits in [
        0_u64,
        1,
        0x8000000000000000,
        0x7fefffffffffffff,
        0x3ff0000000000000,
    ] {
        let scalar = decode(2, &bits.to_le_bytes()).unwrap();
        assert_eq!(encode(scalar).unwrap().payload, bits.to_le_bytes());
    }
    for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(decode(2, &f.to_le_bytes()), Ok(Value::Missing));
    }
    for text in ["", "a\0b", "\u{96ea}\u{1f980}", "e\u{301}", "\u{e9}"] {
        let bytes = encode(Value::Str(text.into())).unwrap();
        assert_eq!(bytes.tag, 3);
        assert_eq!(decode(3, &bytes.payload), Ok(Value::Str(text.into())));
    }
    assert_eq!(decode(0, &[]), Ok(Value::Missing));
    assert_eq!(decode(4, &[0]), Ok(Value::Bool(false)));
    assert_eq!(decode(4, &[1]), Ok(Value::Bool(true)));
}

/// No malformed type, alternate integer spelling or invalid Unicode reaches core.
#[test]
fn reject_invalid_payloads() {
    for text in [
        "",
        "01",
        "+1",
        "-0",
        " 1",
        "1.0",
        "9223372036854775808",
        "-9223372036854775809",
    ] {
        assert!(decode(1, text.as_bytes()).is_err());
    }
    for (tag, bytes) in [
        (0, vec![0]),
        (4, vec![2]),
        (4, vec![]),
        (2, vec![0; 7]),
        (2, vec![0; 9]),
        (5, vec![]),
        (3, vec![0xff]),
        (3, vec![0xc0, 0x80]),
        (3, vec![0xed, 0xa0, 0x80]),
        (3, vec![0xf4, 0x90, 0x80, 0x80]),
    ] {
        assert!(decode(tag, &bytes).is_err());
    }
    assert_eq!(
        decode(3, &vec![b'x'; MAX_SCALAR_BYTES + 1]),
        Err("scalar exceeds byte limit")
    );
}

/// Scalar operations compose the same exact arithmetic/comparison primitives.
#[test]
fn checked_operations_and_comparisons() {
    assert_eq!(
        operate(1, &Value::Int(9007199254740993), Some(&Value::Int(1))),
        Ok(Value::Int(9007199254740994))
    );
    assert!(operate(1, &Value::Int(i64::MAX), Some(&Value::Int(1)))
        .unwrap_err()
        .contains("9223372036854775808"));
    assert!(operate(7, &Value::Int(i64::MIN), None)
        .unwrap_err()
        .contains("9223372036854775808"));
    assert_eq!(
        operate(
            12,
            &Value::Int(9007199254740993),
            Some(&Value::float(9007199254740992.0))
        ),
        Ok(Value::Bool(true))
    );
    assert_eq!(
        operate(1, &Value::Int(1), Some(&Value::float(0.5))),
        Ok(Value::float(1.5))
    );
    assert_eq!(
        operate(4, &Value::Int(3), Some(&Value::Int(2))),
        Ok(Value::float(1.5))
    );
    assert!(operate(4, &Value::Int(3), Some(&Value::Int(0)))
        .unwrap_err()
        .contains("division_by_zero"));
    assert_eq!(
        operate(
            8,
            &Value::Str("a\0b".into()),
            Some(&Value::Str("a\0b".into()))
        ),
        Ok(Value::Bool(true))
    );
    assert_eq!(
        operate(8, &Value::Missing, Some(&Value::Int(1))),
        Ok(Value::Missing)
    );
    assert!(operate(1, &Value::Int(1), Some(&Value::Bool(true))).is_err());
    assert!(operate(8, &Value::Int(1), Some(&Value::Str("1".into()))).is_err());
    assert!(operate(99, &Value::Int(1), Some(&Value::Int(1))).is_err());
    assert!(operate(6, &Value::Int(1), Some(&Value::Int(1))).is_err());
    assert!(operate(1, &Value::Int(1), None).is_err());
}

/// Exact conversion at both i64 ends avoids host decimal-parser rounding.
#[test]
fn exact_float_conversion_uses_bits() {
    assert_eq!(
        exact_float(&Value::Int(i64::MIN)),
        Ok(Value::float(-9223372036854775808.0))
    );
    assert!(exact_float(&Value::Int(i64::MAX)).is_err());
    assert!(exact_float(&Value::Int(9007199254740993)).is_err());
    assert_eq!(
        exact_float(&Value::Int(9007199254740992)),
        Ok(Value::float(9007199254740992.0))
    );
    assert!(exact_float(&Value::Missing).is_err());
}

/// Host epochs cover the entire civil range and drop precision at this boundary.
#[test]
fn temporal_host_epochs_have_exact_values() {
    use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
    for (year, month, day, days) in [
        (1, 1, 1, -719162.0_f64),
        (1970, 1, 1, 0.0),
        (2000, 2, 29, 11016.0),
        (9999, 12, 31, 2932896.0),
    ] {
        let date = Date::new(year, month, day, DatePrecision::Year).unwrap();
        let bytes = encode(Value::Date(date)).unwrap();
        assert_eq!(bytes.tag, 5);
        assert_eq!(bytes.payload, days.to_le_bytes());
        let Value::Date(decoded) = decode(5, &bytes.payload).unwrap() else {
            panic!("date")
        };
        assert_eq!(decoded.collected_precision(), DatePrecision::Day);
        assert_eq!(
            decode(5, &bytes.payload),
            Ok(Value::Date(
                Date::new(year, month, day, DatePrecision::Day).unwrap()
            ))
        );
    }
    for (year, month, day, hour, minute, second, epoch) in [
        (1, 1, 1, 0, 0, 0, -62135596800.0_f64),
        (1969, 12, 31, 23, 59, 59, -1.0),
        (1970, 1, 1, 0, 0, 0, 0.0),
        (9999, 12, 31, 23, 59, 59, 253402300799.0),
    ] {
        let date = Date::new(year, month, day, DatePrecision::Day).unwrap();
        let value = DateTime::new(date, hour, minute, second, DateTimePrecision::Day).unwrap();
        let bytes = encode(Value::DateTime(value)).unwrap();
        assert_eq!(bytes.tag, 6);
        assert_eq!(bytes.payload, epoch.to_le_bytes());
        let Value::DateTime(decoded) = decode(6, &bytes.payload).unwrap() else {
            panic!("datetime")
        };
        assert_eq!(decoded.collected_precision(), DateTimePrecision::Second);
        assert_eq!(
            decode(6, &bytes.payload),
            Ok(Value::DateTime(
                DateTime::new(date, hour, minute, second, DateTimePrecision::Second).unwrap()
            ))
        );
    }
}

/// Fractional, nonfinite and out-of-calendar epochs never saturate into values.
#[test]
fn temporal_host_epochs_reject_invalid_values() {
    for tag in [5, 6] {
        for epoch in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.5, -0.5, 1e13] {
            assert!(decode(tag, &epoch.to_le_bytes()).is_err());
        }
        assert!(decode(tag, &[0; 7]).is_err());
        assert!(decode(tag, &[0; 9]).is_err());
    }
    for epoch in [-719163.0_f64, 2932897.0] {
        assert!(decode(5, &epoch.to_le_bytes()).is_err());
    }
    for epoch in [-62135596801.0_f64, 253402300800.0] {
        assert!(decode(6, &epoch.to_le_bytes()).is_err());
    }
}
