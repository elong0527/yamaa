use yamaa_core::numeric::{integral, IntegralFunction, Number};

/// Compare exact result bits against independent standard-library operations,
/// canonicalizing zero to match the language's integer-intermediate semantics.
fn check(value: f64) {
    for (function, expected) in [
        (IntegralFunction::Ceil, value.ceil()),
        (IntegralFunction::Floor, value.floor()),
        (IntegralFunction::Trunc, value.trunc()),
    ] {
        let expected = if expected == 0.0 { 0.0 } else { expected };
        let Number::Float(actual) = integral(function, Number::float(value)) else {
            panic!("finite input must return float");
        };
        assert_eq!(
            actual.get().to_bits(),
            expected.to_bits(),
            "{function:?}({value:?})"
        );
    }
}

/// Cover every finite exponent and adjacent significand boundaries under both signs,
/// then sample deterministic binary64 patterns without a random-library dependency.
#[test]
fn integral_functions_match_standard_library_across_binary64() {
    for exponent in 0..2047_u64 {
        for fraction in [0, 1, (1_u64 << 51) - 1, 1_u64 << 51, (1_u64 << 52) - 1] {
            for sign in [0, 1_u64 << 63] {
                check(f64::from_bits(sign | (exponent << 52) | fraction));
            }
        }
    }
    let mut bits = 0x1234_5678_9abc_def0_u64;
    for _ in 0..100_000 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        let value = f64::from_bits(bits);
        if value.is_finite() {
            check(value);
        }
    }
}
