//! The one decimal rounding: exact, bounded and independent of platform math.

use num_bigint::BigUint;

/// Round the exact magnitude of a finite binary64 to `digits` decimal places
/// under FORMULA-16: |value| × 10^digits becomes an integer, where a fraction of
/// one half, or within 2^-26 below one half, rounds away from zero. So does a
/// normal value that is the binary64 a tie of at most fifteen significant
/// digits reads as, the way such a tie written in decimal is stored. This is
/// the language's only rounding interval. The rounding operation, CSV display
/// precision, project-function result comparison, float `shift` amounts of a
/// datetime and `duration_text` seconds all select their integer here. Callers
/// bound `digits`, which bounds the intermediates; negative digits round left
/// of the decimal point.
pub(crate) fn rounded_magnitude(value: f64, digits: i64) -> BigUint {
    let (mut numerator, mut spacing, mut denominator) = binary64_fraction(value);
    let power = BigUint::from(10u32).pow(digits.unsigned_abs() as u32);
    if digits >= 0 {
        numerator *= &power;
        spacing = spacing.map(|spacing| spacing * &power);
    } else {
        denominator *= &power;
    }
    let twice_remainder = (&numerator % &denominator) << 1usize;
    let mut lower = numerator / &denominator;
    let even = value.to_bits() & 1 == 0;
    if rounds_as_tie(
        &lower,
        &twice_remainder,
        spacing.as_ref(),
        even,
        &denominator,
    ) {
        lower += 1u32;
    }
    lower
}

/// Whether the scaled magnitude `lower + twice_remainder / (2 × denominator)`
/// rounds away from zero. Its distance below the tie `lower + 1/2` is
/// `gap / (2 × denominator)`, with `gap = denominator - twice_remainder`. The
/// fixed window is 2^-26. The second window is half the value's own scaled
/// spacing `spacing / denominator`, so that the value is the binary64 the tie
/// reads as: the nearest, and at exactly half a spacing only an `even`
/// significand, as round-to-nearest-even reads a midpoint. It applies only to a normal value (`spacing` present) whose tie
/// `10 × lower + 5` has at most fifteen significant digits, where one binary64
/// is nearest each such decimal and no candidate shares it.
fn rounds_as_tie(
    lower: &BigUint,
    twice_remainder: &BigUint,
    spacing: Option<&BigUint>,
    even: bool,
    denominator: &BigUint,
) -> bool {
    if twice_remainder >= denominator {
        return true;
    }
    let gap = denominator - twice_remainder;
    if (&gap << 25usize) <= *denominator {
        return true;
    }
    spacing.is_some_and(|spacing| {
        *lower < BigUint::from(10u64.pow(14)) && (gap < *spacing || (gap == *spacing && even))
    })
}

/// Apply the rounding to a finite promoted binary64 and return binary64.
/// Extreme digits return before constructing powers, bounding intermediates to
/// at most 2,155 bits and the selected decimal integer to at most 649 digits.
/// A result of zero is positive zero.
pub(crate) fn round_finite(value: f64, digits: i64) -> f64 {
    if digits > 340 {
        return if value == 0.0 { 0.0 } else { value };
    }
    if digits < -309 {
        return 0.0;
    }
    let lower = rounded_magnitude(value, digits);
    // Parse only our bounded decimal integer/exponent spelling. This final
    // conversion rounds once to binary64; overflow becomes infinity for the
    // numeric boundary to normalize. The spelling cannot be syntactically invalid.
    let result = format!("{}e{}", lower, -digits)
        .parse::<f64>()
        .expect("integer and bounded decimal exponent form a valid float");
    if result == 0.0 {
        0.0
    } else if value.is_sign_negative() {
        -result
    } else {
        result
    }
}

/// The magnitude of a finite binary64 and, for a normal value, its spacing
/// (the distance to the next binary64 of larger magnitude), as numerators over
/// one denominator of at most 2^1,074.
fn binary64_fraction(value: f64) -> (BigUint, Option<BigUint>, BigUint) {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, shift) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), exponent - 1075)
    };
    let mut numerator = BigUint::from(mantissa);
    let mut spacing = BigUint::from(1u32);
    let mut denominator = BigUint::from(1u32);
    if shift >= 0 {
        numerator <<= shift as usize;
        spacing <<= shift as usize;
    } else {
        denominator <<= (-shift) as usize;
    }
    (numerator, (exponent != 0).then_some(spacing), denominator)
}
