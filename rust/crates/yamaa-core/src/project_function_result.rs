//! Compare temporary conformance copies without changing the runtime result.
//! REQ-0692 uses exact binary64 decimal quantization, not ROUND's tie interval.
use crate::value::Value;
use num_bigint::BigUint;

fn decimal_copy(value: f64, decimals: u32) -> (bool, BigUint) {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, shift) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), exponent - 1075)
    };
    let mut numerator = BigUint::from(mantissa);
    let mut denominator = BigUint::from(1u32);
    if shift >= 0 {
        numerator <<= shift as usize;
    } else {
        denominator <<= (-shift) as usize;
    }
    numerator *= BigUint::from(10u32).pow(decimals);
    let remainder = &numerator % &denominator;
    let mut integer = &numerator / &denominator;
    if (remainder << 1usize) >= denominator {
        integer += 1u32;
    }
    let negative = bits >> 63 != 0 && integer != BigUint::from(0u32);
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
            decimal_copy(a.get(), decimals as u32) == decimal_copy(b.get(), decimals as u32)
        }
        _ => actual == expected,
    }
}
