//! YAML 1.2 core scalar resolution, preserving arbitrary-width integer identity.

use num_bigint::BigInt;
use yamaa_core::schema::DocumentNode as N;

use super::DecodeFailure;

fn digits(text: &str, radix: u32) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_digit(radix))
}

fn decimal_float(text: &str) -> bool {
    let text = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = text
        .split_once(['e', 'E'])
        .map_or((text, None), |(a, b)| (a, Some(b)));
    if exponent.is_some_and(|value| !digits(value.strip_prefix(['+', '-']).unwrap_or(value), 10)) {
        return false;
    }
    if let Some((whole, fraction)) = mantissa.split_once('.') {
        (digits(whole, 10) && (fraction.is_empty() || digits(fraction, 10)))
            || (whole.is_empty() && digits(fraction, 10))
    } else {
        digits(mantissa, 10)
    }
}

pub(super) fn decode(text: String, limit: usize) -> Result<N, DecodeFailure> {
    match text.as_str() {
        "" | "~" | "null" | "Null" | "NULL" => return Ok(N::Null),
        "true" | "True" | "TRUE" => return Ok(N::Boolean(true)),
        "false" | "False" | "FALSE" => return Ok(N::Boolean(false)),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" | "-.inf" | "-.Inf" | "-.INF"
        | ".nan" | ".NaN" | ".NAN" => return Ok(N::Null),
        _ => {}
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(&text);
    if digits(unsigned, 10) {
        if unsigned.len() > limit {
            return Err(DecodeFailure::Limit("numeric_digits"));
        }
        let value = unsigned.trim_start_matches('0');
        let canonical = if value.is_empty() {
            "0".into()
        } else if text.starts_with('-') {
            format!("-{value}")
        } else {
            value.into()
        };
        return Ok(N::Integer(canonical));
    }
    for (prefix, radix) in [("0x", 16), ("0o", 8)] {
        if let Some(value) = text
            .strip_prefix(prefix)
            .filter(|value| digits(value, radix))
        {
            if value.len() > limit {
                return Err(DecodeFailure::Limit("numeric_digits"));
            }
            let integer =
                BigInt::parse_bytes(value.as_bytes(), radix).ok_or(DecodeFailure::Internal)?;
            return Ok(N::Integer(integer.to_string()));
        }
    }
    if decimal_float(&text) {
        if text.bytes().filter(u8::is_ascii_digit).count() > limit {
            return Err(DecodeFailure::Limit("numeric_digits"));
        }
        let value: f64 = text.parse().map_err(|_| DecodeFailure::Internal)?;
        return Ok(if value.is_finite() {
            N::Float(value)
        } else {
            N::Null
        });
    }
    Ok(N::Text(text))
}
