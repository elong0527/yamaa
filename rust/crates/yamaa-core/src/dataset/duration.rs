//! ISO 8601 fixed-length durations. Decimal components retain exact whole seconds.
use super::ElapsedUnit;
use crate::value::Value;
use alloc::{
    format,
    string::{String, ToString},
};
use num_bigint::BigUint;

fn decimal(text: &str) -> Option<(BigUint, usize)> {
    let (integer, fraction) = text
        .split_once('.')
        .or_else(|| text.split_once(','))
        .unwrap_or((text, ""));
    if integer.is_empty()
        || !integer.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_empty() && (text.contains('.') || text.contains(','))
    {
        return None;
    }
    let fraction = fraction.trim_end_matches('0');
    let digits = format!("{integer}{fraction}");
    Some((BigUint::parse_bytes(digits.as_bytes(), 10)?, fraction.len()))
}

fn parse(text: &str) -> Option<(bool, BigUint, usize)> {
    let (negative, text) = text
        .strip_prefix('-')
        .map_or((false, text), |text| (true, text));
    let body = text.strip_prefix('P')?;
    if let Some(weeks) = body.strip_suffix('W') {
        let (number, places) = decimal(weeks)?;
        return Some((negative, number * 604800u32, places));
    }
    let bytes = body.as_bytes();
    let mut index = 0;
    let mut time = false;
    let mut time_components = 0;
    let mut last_rank = 0;
    let mut total = BigUint::from(0u32);
    let mut places = 0;
    let mut components = 0;
    while index < bytes.len() {
        if bytes[index] == b'T' {
            if time {
                return None;
            }
            time = true;
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_digit() || matches!(bytes[index], b'.' | b','))
        {
            index += 1;
        }
        let designator = *bytes.get(index)?;
        let (rank, seconds) = match (time, designator) {
            (false, b'D') => (1, 86400u32),
            (true, b'H') => (2, 3600u32),
            (true, b'M') => (3, 60u32),
            (true, b'S') => (4, 1u32),
            _ => return None,
        };
        if rank <= last_rank {
            return None;
        }
        let component = &body[start..index];
        let (number, scale) = decimal(component)?;
        index += 1;
        if (component.contains('.') || component.contains(',')) && index != bytes.len() {
            return None;
        }
        if scale > 0 {
            places = scale;
            total *= BigUint::from(10u32).pow(u32::try_from(places).ok()?);
        }
        total += number * seconds;
        last_rank = rank;
        components += 1;
        if time {
            time_components += 1;
        }
    }
    (components > 0 && (!time || time_components > 0)).then_some((negative, total, places))
}

pub(super) fn read(text: &str, unit: ElapsedUnit) -> Option<Value> {
    let (negative, numerator, places) = parse(text)?;
    let denominator = BigUint::from(10u32).pow(u32::try_from(places).ok()?);
    if unit == ElapsedUnit::Second && &numerator % &denominator == BigUint::from(0u32) {
        let magnitude = &numerator / &denominator;
        let text = if negative {
            format!("-{magnitude}")
        } else {
            magnitude.to_string()
        };
        return text.parse::<i64>().ok().map(Value::Int);
    }
    let seconds = format!("{}{numerator}e-{places}", if negative { "-" } else { "" })
        .parse::<f64>()
        .ok()?;
    let value = seconds / f64::from(unit.seconds());
    value.is_finite().then(|| Value::float(value))
}

fn float_decimal(value: f64) -> Option<(BigUint, usize)> {
    let mut buffer = ryu::Buffer::new();
    let text = buffer.format_finite(value);
    let (coefficient, exponent) = text
        .split_once('e')
        .map_or((text, 0), |(c, e)| (c, e.parse::<i32>().unwrap()));
    let (mut number, places) = decimal(coefficient)?;
    let scale = places as i32 - exponent;
    if scale < 0 {
        number *= BigUint::from(10u32).pow((-scale) as u32);
        Some((number, 0))
    } else {
        Some((number, scale as usize))
    }
}

pub(super) fn write(value: &Value, unit: ElapsedUnit) -> Option<String> {
    let (negative, mut remaining, places) = match value {
        Value::Int(number) => (
            *number < 0,
            BigUint::from(number.unsigned_abs()) * unit.seconds(),
            0,
        ),
        Value::Float(number) => {
            let seconds = number.get() * f64::from(unit.seconds());
            if !seconds.is_finite() {
                return None;
            }
            let magnitude = libm::fabs(seconds);
            let (number, places) = float_decimal(magnitude)?;
            (seconds < 0.0, number, places)
        }
        _ => return None,
    };
    if remaining == BigUint::from(0u32) {
        return Some("PT0S".into());
    }
    let denominator = BigUint::from(10u32).pow(places as u32);
    let mut result = if negative {
        String::from("-P")
    } else {
        String::from("P")
    };
    let mut has_time = false;
    for (seconds, designator) in [(86400u32, 'D'), (3600, 'H'), (60, 'M')] {
        let divisor = &denominator * seconds;
        let count = &remaining / &divisor;
        remaining %= &divisor;
        if count != BigUint::from(0u32) {
            if designator != 'D' && !has_time {
                result.push('T');
                has_time = true;
            }
            result.push_str(&count.to_string());
            result.push(designator);
        }
    }
    if remaining != BigUint::from(0u32) {
        if !has_time {
            result.push('T');
        }
        let integer = &remaining / &denominator;
        result.push_str(&integer.to_string());
        let fraction = remaining % &denominator;
        if fraction != BigUint::from(0u32) {
            let digits = fraction.to_string();
            result.push('.');
            for _ in digits.len()..places {
                result.push('0');
            }
            result.push_str(digits.trim_end_matches('0'));
        }
        result.push('S');
    }
    Some(result)
}
