//! Exact CSV display rounding under REQ-0745/0747/0748, after execution gates.
use alloc::string::String;
use num_bigint::BigUint;

use crate::{decimal_rounding::binary64_fraction, value::FiniteFloat};

/// Trusted output budget exhaustion, never an invalid language precision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limit;

/// Format the exact binary64 fraction, with genuine ties away from zero.
/// This does not use the numeric function's near-tie interval or round back to
/// binary64. Every finite binary64 has at most 1,074 exact fractional decimal
/// places; larger declarations append zeros, bounding integer intermediates
/// independently of the declared width. Charge width before constructing powers
/// and charge the complete spelling before allocating the final text.
pub fn fixed_point(value: FiniteFloat, digits: usize, maximum: usize) -> Result<String, Limit> {
    let minimum = if digits == 0 {
        1
    } else {
        digits.checked_add(2).ok_or(Limit)?
    };
    if minimum > maximum {
        return Err(Limit);
    }
    let exact_digits = digits.min(1074);
    let (mut numerator, denominator) = binary64_fraction(value.get());
    numerator *= BigUint::from(10u32).pow(exact_digits as u32);
    let remainder = &numerator % &denominator;
    let mut rounded = numerator / &denominator;
    if (remainder << 1usize) >= denominator {
        rounded += 1u32;
    }
    let negative = value.get().is_sign_negative() && rounded != BigUint::from(0u32);
    // This bounded intermediate has at most 1,383 decimal digits.
    let decimal = rounded.to_str_radix(10);
    let whole = decimal.len().saturating_sub(exact_digits).max(1);
    let length = whole
        .checked_add(usize::from(negative))
        .and_then(|n| n.checked_add(if digits == 0 { 0 } else { digits + 1 }))
        .filter(|&n| n <= maximum)
        .ok_or(Limit)?;
    let mut result = String::with_capacity(length);
    if negative {
        result.push('-');
    }
    if digits == 0 {
        result.push_str(&decimal);
    } else {
        if decimal.len() > exact_digits {
            let split = decimal.len() - exact_digits;
            result.push_str(&decimal[..split]);
            result.push('.');
            result.push_str(&decimal[split..]);
        } else {
            result.push_str("0.");
            result.extend(core::iter::repeat_n('0', exact_digits - decimal.len()));
            result.push_str(&decimal);
        }
        result.extend(core::iter::repeat_n('0', digits - exact_digits));
    }
    Ok(result)
}
