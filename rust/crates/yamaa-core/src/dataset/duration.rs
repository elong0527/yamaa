//! ISO 8601 fixed-length durations. Decimal components retain exact whole seconds.
use super::ElapsedUnit;
use crate::value::Value;
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
