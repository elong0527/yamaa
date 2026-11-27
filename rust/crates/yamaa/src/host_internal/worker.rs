//! The spelling of the standalone command line's worker protocol (#5100):
//! its text, its values and its declared result types, which the binary
//! writes and reads and the R package's compiled conversion writes for the
//! R worker, so a result the R worker sends is spelled by the one
//! implementation that reads it. `rust/crates/yamaa-cli/src/worker.rs`
//! describes the protocol.

use std::fmt::Write as _;

use crate::host::functions::{ColumnType, FunctionError};
use crate::scalar::{Date, DateTime, Scalar, ScalarRef, Time};

/// Percent-encode `text`: a byte of `A-Za-z0-9-._~` as itself, any other as
/// `%` and two uppercase hexadecimal digits.
pub fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// Decode what [`encode`] made, refusing anything else as a protocol
/// failure.
pub fn decode(text: &str) -> Result<String, FunctionError> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            let digits = bytes
                .get(index + 1..index + 3)
                .and_then(|digits| std::str::from_utf8(digits).ok())
                .and_then(|digits| u8::from_str_radix(digits, 16).ok())
                .ok_or(FunctionError::Protocol)?;
            decoded.push(digits);
            index += 3;
        } else {
            decoded.push(byte);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| FunctionError::Protocol)
}

/// A declared result type as the protocol spells it, the function file's
/// spelling (VALUE-1).
pub fn column_type_name(kind: ColumnType) -> &'static str {
    use yamaa_core::value::ColumnType as Core;
    Core::ALL
        .into_iter()
        .find(|core| ColumnType::from_core(*core) == kind)
        .map_or("str", Core::name)
}

/// The declared result type the protocol's `text` names, or none.
pub fn column_type_named(text: &str) -> Option<ColumnType> {
    yamaa_core::value::ColumnType::from_name(text).map(ColumnType::from_core)
}

/// One value as the protocol spells it.
pub fn encode_value(value: ScalarRef<'_>) -> String {
    match value {
        ScalarRef::Missing => "-".to_owned(),
        ScalarRef::Str(text) => format!("s:{}", encode(text)),
        ScalarRef::Int(value) => format!("i:{value}"),
        ScalarRef::Float(value) => format!("f:{}", float_text(value)),
        ScalarRef::Bool(value) => format!("b:{value}"),
        ScalarRef::Date(date) => {
            let (year, month, day) = date.fields();
            format!("d:{year:04}-{month:02}-{day:02}")
        }
        ScalarRef::DateTime(datetime) => {
            let (year, month, day, hour, minute, second) = datetime.fields();
            format!("t:{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
        }
        ScalarRef::Time(time) => {
            let (hour, minute, second) = time.fields();
            format!("h:{hour:02}:{minute:02}:{second:02}")
        }
    }
}

/// A float as text that reads back as the same float: Rust's shortest
/// round-trip form, with `inf`, `-inf` and `nan` for the values that have no
/// decimal form.
fn float_text(value: f64) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "inf" } else { "-inf" }.to_owned()
    } else {
        format!("{value:?}")
    }
}

/// The value a reply spells, or why it is no value of the protocol, which
/// is a rejected result.
pub fn decode_value(field: &str) -> Result<Scalar, FunctionError> {
    let rejected = |reason: &str| FunctionError::Rejected {
        reason: format!("{reason}: {field}"),
        returned: None,
    };
    if field == "-" {
        return Ok(Scalar::Missing);
    }
    let Some((kind, text)) = field.split_once(':') else {
        return Err(rejected("a result has no type"));
    };
    match kind {
        "s" => decode(text)
            .map(Scalar::Str)
            .map_err(|_| rejected("a str result is not percent-encoded UTF-8")),
        "i" => text
            .parse()
            .map(Scalar::Int)
            .map_err(|_| rejected("an int result is no 64-bit integer")),
        "f" => text
            .parse()
            .map(Scalar::float)
            .map_err(|_| rejected("a float result is no float")),
        "b" => match text {
            "true" => Ok(Scalar::Bool(true)),
            "false" => Ok(Scalar::Bool(false)),
            _ => Err(rejected("a bool result is neither true nor false")),
        },
        "d" => parse_date(text)
            .map(Scalar::Date)
            .ok_or_else(|| rejected("a date result names no day")),
        "t" => parse_datetime(text)
            .map(Scalar::DateTime)
            .ok_or_else(|| rejected("a datetime result names no second")),
        "h" => parse_time(text)
            .map(Scalar::Time)
            .ok_or_else(|| rejected("a time result names no second")),
        _ => Err(rejected("a result has a type the protocol does not spell")),
    }
}

fn parse_date(text: &str) -> Option<Date> {
    let mut parts = text.splitn(3, '-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    Date::new(year, month, day)
}

fn parse_datetime(text: &str) -> Option<DateTime> {
    let (date, time) = text.split_once('T')?;
    let (year, month, day) = parse_date(date)?.fields();
    let (hour, minute, second) = parse_time(time)?.fields();
    DateTime::new(year, month, day, hour, minute, second)
}

fn parse_time(text: &str) -> Option<Time> {
    let mut parts = text.splitn(3, ':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next()?.parse().ok()?;
    let second = parts.next()?.parse().ok()?;
    Time::new(hour, minute, second)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The codec is the seam the workers' own tests cannot reach from Rust:
    /// each value spells as the protocol says and reads back as itself.
    #[test]
    fn every_value_spells_as_the_protocol_says_and_reads_back() {
        let cases = [
            (Scalar::Missing, "-"),
            (
                Scalar::Str("a b\tc/%\u{e9}".into()),
                "s:a%20b%09c%2F%25%C3%A9",
            ),
            (Scalar::Int(-42), "i:-42"),
            (Scalar::Float(0.1), "f:0.1"),
            (Scalar::Float(1e300), "f:1e300"),
            (Scalar::Bool(true), "b:true"),
            (
                Scalar::Date(Date::new(2024, 2, 29).unwrap()),
                "d:2024-02-29",
            ),
            (
                Scalar::DateTime(DateTime::new(999, 12, 31, 23, 59, 58).unwrap()),
                "t:0999-12-31T23:59:58",
            ),
            (Scalar::Time(Time::new(8, 5, 9).unwrap()), "h:08:05:09"),
        ];
        for (value, spelled) in cases {
            assert_eq!(
                encode_value(ScalarRef::from_owned(&value)),
                spelled,
                "{value:?}"
            );
            assert_eq!(decode_value(spelled).unwrap(), value, "{spelled}");
        }
        assert_eq!(encode_value(ScalarRef::Float(f64::INFINITY)), "f:inf");
        assert_eq!(encode_value(ScalarRef::Float(f64::NEG_INFINITY)), "f:-inf");
        assert_eq!(encode_value(ScalarRef::Float(f64::NAN)), "f:nan");
        // The engine turns a float that is not finite into a missing value.
        for spelled in ["f:inf", "f:-inf", "f:nan"] {
            assert_eq!(decode_value(spelled).unwrap(), Scalar::Missing, "{spelled}");
        }
    }

    #[test]
    fn a_reply_that_spells_no_value_is_a_rejected_result() {
        for (spelled, reason) in [
            ("x", "a result has no type: x"),
            ("i:1.5", "an int result is no 64-bit integer: i:1.5"),
            ("d:2024-02-30", "a date result names no day: d:2024-02-30"),
            (
                "t:2024-02-29",
                "a datetime result names no second: t:2024-02-29",
            ),
            ("h:08:30", "a time result names no second: h:08:30"),
            ("b:yes", "a bool result is neither true nor false: b:yes"),
            ("s:%ZZ", "a str result is not percent-encoded UTF-8: s:%ZZ"),
            (
                "q:1",
                "a result has a type the protocol does not spell: q:1",
            ),
        ] {
            match decode_value(spelled) {
                Err(FunctionError::Rejected {
                    reason: actual,
                    returned: None,
                }) => {
                    assert_eq!(actual, reason, "{spelled}")
                }
                other => panic!("{spelled}: {other:?}"),
            }
        }
    }

    #[test]
    fn text_percent_encodes_every_byte_outside_the_unreserved_set() {
        assert_eq!(encode("yamaa_benchmarks.bmi"), "yamaa_benchmarks.bmi");
        assert_eq!(encode("base::signif"), "base%3A%3Asignif");
        assert_eq!(decode("base%3a%3Asignif").unwrap(), "base::signif");
        assert!(matches!(decode("%4"), Err(FunctionError::Protocol)));
        assert!(matches!(decode("%FF"), Err(FunctionError::Protocol)));
    }

    /// Each declared result type spells as VALUE-1 names it and reads back
    /// as itself; a spelling outside the vocabulary names none.
    #[test]
    fn each_declared_result_type_spells_as_value_1_names_it() {
        for (kind, name) in [
            (ColumnType::Str, "str"),
            (ColumnType::Int, "int"),
            (ColumnType::Float, "float"),
            (ColumnType::Date, "date"),
            (ColumnType::DateTime, "datetime"),
            (ColumnType::Time, "time"),
        ] {
            assert_eq!(column_type_name(kind), name, "{kind:?}");
            assert_eq!(column_type_named(name), Some(kind), "{name}");
        }
        assert_eq!(column_type_named("bool"), None);
    }
}
