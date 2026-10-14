use yamaa_core::numeric::{sqrt, ArithmeticErrorKind, Number};

/// Missing normalizes before domain checking; negative present values retain diagnostics.
#[test]
fn square_root_domain_conditions_and_normalization() {
    for missing in [
        Number::Missing,
        Number::float(f64::NAN),
        Number::float(f64::NEG_INFINITY),
    ] {
        assert_eq!(sqrt(missing, "SQRT(A)"), Ok(Number::Missing));
    }
    for negative in [Number::Int(i64::MIN), Number::float(-f64::from_bits(1))] {
        let error = sqrt(negative, "SQRT(A)").unwrap_err();
        assert_eq!(error.kind, ArithmeticErrorKind::SqrtOfNegative);
        assert_eq!(error.expression, "SQRT(A)");
        assert_eq!(error.phase(), "derivation");
        assert_eq!(error.condition(), "sqrt_of_negative");
        assert_eq!(error.requirement(), "REQ-0431");
    }
}

/// Compare square roots bit-for-bit, including sign and integer promotion boundaries.
#[test]
fn square_root_preserves_zero_sign_and_promotes_integers() {
    for value in [-0.0, 0.0, f64::from_bits(1), f64::MIN_POSITIVE, f64::MAX] {
        let Number::Float(actual) = sqrt(Number::float(value), "SQRT(A)").unwrap() else {
            panic!("expected float");
        };
        assert_eq!(actual.get().to_bits(), value.sqrt().to_bits());
    }
    for value in [0, 1, (1_i64 << 53) - 1, (1_i64 << 53) + 1, i64::MAX] {
        let Number::Float(actual) = sqrt(Number::Int(value), "SQRT(A)").unwrap() else {
            panic!("expected float");
        };
        assert_eq!(actual.get().to_bits(), (value as f64).sqrt().to_bits());
    }
}
