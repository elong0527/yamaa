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
