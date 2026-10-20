//! Compare temporary conformance copies without changing the runtime result.
//! FUNC-13 quantizes both copies with the language's one rounding (FORMULA-16).
use crate::{decimal_rounding::rounded_magnitude, value::Value};
use num_bigint::BigUint;

fn decimal_copy(value: f64, decimals: i64) -> (bool, BigUint) {
    let integer = rounded_magnitude(value, decimals);
    let negative = value.is_sign_negative() && integer != BigUint::from(0u32);
    (negative, integer)
}

/// Invalid negative precision has no comparison success. At 324 or more places,
/// the decimal unit is smaller than half the minimum binary64 separation, so
/// quantization cannot merge distinct finite values. Huge requested precision
/// never allocates a huge power or string. Zero signs compare equally; originals
/// retain their exact bits for subsequent evaluation and output.
pub fn results_match(actual: &Value, expected: &Value, decimals: i64) -> bool {
    if decimals < 0 {
        return false;
    }
    match (actual, expected) {
        (Value::Float(a), Value::Float(b)) if decimals < 324 => {
            decimal_copy(a.get(), decimals) == decimal_copy(b.get(), decimals)
        }
        _ => actual == expected,
    }
}
