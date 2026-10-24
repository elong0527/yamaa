//! ISO 8601 fixed-length durations, read and written. Decimal components
//! combine exactly before the one conversion to binary64, and a written
//! duration splits the exact seconds its amount gives.
use super::ElapsedUnit;
use crate::conversion::float_text;
use crate::value::{FiniteFloat, Value};
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

/// The duration as a `float` in `unit` for every unit (EXPR-14): the exact
/// seconds rounded once to binary64, then divided by the unit's seconds.
pub(super) fn read(text: &str, unit: ElapsedUnit) -> Option<Value> {
    let (negative, numerator, places) = parse(text)?;
    let seconds = format!("{}{numerator}e-{places}", if negative { "-" } else { "" })
        .parse::<f64>()
        .ok()?;
    let value = seconds / f64::from(unit.seconds());
    value.is_finite().then(|| Value::float(value))
}

/// The one canonical text of `value`, a number of `unit`s (TIME-21): `P`,
/// the days, then `T` and the hours, minutes and seconds, each zero component
/// omitted and any fraction on the seconds alone; zero is `PT0S`, and a
/// negative amount takes one leading `-`. Weeks are never written, so one
/// amount has one text. An `int` is scaled to seconds exactly. A `float` is
/// multiplied by the unit's seconds once as binary64, and that number's
/// canonical text (VALUE-8) is the exact decimal the components split; `None`
/// when those seconds leave the binary64 range.
pub(super) fn write(value: &Value, unit: ElapsedUnit) -> Option<String> {
    let (negative, whole, fraction) = match value {
        Value::Int(amount) => {
            let seconds = i128::from(*amount) * i128::from(unit.seconds());
            (
                seconds < 0,
                BigUint::from(seconds.unsigned_abs()),
                String::new(),
            )
        }
        Value::Float(amount) => {
            let seconds = amount.get() * f64::from(unit.seconds());
            let text = float_text(FiniteFloat::new(seconds.abs())?);
            let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
            (
                seconds < 0.0,
                BigUint::parse_bytes(whole.as_bytes(), 10)?,
                fraction.to_owned(),
            )
        }
        _ => unreachable!("an admitted numeric amount"),
    };
    let day = BigUint::from(86_400u32);
    let days = &whole / &day;
    // Under a day, so at most one 32-bit digit.
    let rest = (&whole % &day)
        .to_u32_digits()
        .first()
        .copied()
        .unwrap_or(0);
    if days.bits() == 0 && rest == 0 && fraction.is_empty() {
        return Some("PT0S".into());
    }
    let mut text = String::from(if negative { "-P" } else { "P" });
    if days.bits() > 0 {
        text.push_str(&format!("{days}D"));
    }
    if rest > 0 || !fraction.is_empty() {
        text.push('T');
        for (count, designator) in [(rest / 3600, 'H'), (rest / 60 % 60, 'M')] {
            if count > 0 {
                text.push_str(&format!("{count}{designator}"));
            }
        }
        let seconds = rest % 60;
        if seconds > 0 || !fraction.is_empty() {
            text.push_str(&seconds.to_string());
            if !fraction.is_empty() {
                text.push('.');
                text.push_str(&fraction);
            }
            text.push('S');
        }
    }
    Some(text)
}
