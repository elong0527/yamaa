//! Exact, bounded decimal rounding independent of platform math.

use alloc::format;
use num_bigint::BigUint;

/// Apply the exact REQ-0418 interval to a finite promoted binary64 input.
/// Extreme digits return before constructing powers, bounding intermediates to
/// at most 2,155 bits and the selected decimal integer to at most 649 digits.
pub(crate) fn round_finite(value: f64, digits: i64) -> f64 {
    if digits > 340 {
        return if value == 0.0 { 0.0 } else { value };
    }
    if digits < -309 {
        return 0.0;
    }
    let bits = value.to_bits();
    let (mut numerator, mut denominator) = binary64_fraction(value);
    let power = BigUint::from(10u32).pow(digits.unsigned_abs() as u32);
    if digits >= 0 {
        numerator *= &power;
    } else {
        denominator *= &power;
    }
    let remainder = &numerator % &denominator;
    let mut lower = &numerator / &denominator;
    if (remainder << 26usize) >= &denominator * ((1u32 << 25) - 1) {
        lower += 1u32;
    }
    // Parse only our bounded decimal integer/exponent spelling. This final
    // conversion rounds once to binary64; overflow becomes infinity for the
    // numeric boundary to normalize. The spelling cannot be syntactically invalid.
    let result = format!("{}e{}", lower, -digits)
        .parse::<f64>()
        .expect("integer and bounded decimal exponent form a valid float");
    if result == 0.0 {
        0.0
    } else if bits >> 63 != 0 {
        -result
    } else {
        result
    }
}

/// The magnitude of a finite binary64, with denominator at most 2^1,074.
/// Rounding policies choose their own interval over this exact fraction.
pub(crate) fn binary64_fraction(value: f64) -> (BigUint, BigUint) {
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
    (numerator, denominator)
}
