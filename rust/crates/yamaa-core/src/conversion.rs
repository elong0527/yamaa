//! Completed-result conversion under REQ-0009-0021 and REQ-0601.

use alloc::string::{String, ToString};

use crate::value::{ColumnType, FiniteFloat, Value, ValueType};

/// A failed conversion may contain an integer too large for a runtime value.
#[derive(Clone, Debug, PartialEq)]
pub enum ConversionValue {
    /// Ordinary input or a finite number parsed from numeric text.
    Runtime(Value),
    /// Canonical decimal integer diagnostic data, never a successful value.
    Integer(String),
}

/// The requirement owning a conversion failure, separate from arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionReason {
    IncompatibleOrInvalidText,
    IntegerRangeOrFraction,
    InvalidTemporalText,
}

/// Portable conversion failure data; callers decide whether to apply a handler.
#[derive(Clone, Debug, PartialEq)]
pub struct ConversionError {
    pub target: ColumnType,
    pub value: ConversionValue,
    pub reason: ConversionReason,
}

impl ConversionError {
    /// Return the conversion phase, distinct from arithmetic derivation errors.
    pub fn phase(&self) -> &'static str {
        "convert"
    }

    /// Return the closed portable condition vocabulary entry.
    pub fn condition(&self) -> &'static str {
        "conversion_failed"
    }

    /// Name the eligible handler without firing it or changing handler counts.
    pub fn applicable_handler(&self) -> &'static str {
        "unconvertible"
    }

    /// Return the requirement that owns this conversion failure.
    pub fn requirement(&self) -> &'static str {
        match self.reason {
            ConversionReason::IncompatibleOrInvalidText => "REQ-0013",
            ConversionReason::IntegerRangeOrFraction => "REQ-0021",
            ConversionReason::InvalidTemporalText => "REQ-0601",
        }
    }

    /// Preserve the parsed numeric source type used by the Python reference.
    pub fn source_type(&self) -> Option<ValueType> {
        match &self.value {
            ConversionValue::Runtime(value) => value.value_type(),
            ConversionValue::Integer(_) => Some(ValueType::Int),
        }
    }
}

/// Construct an owned failure without silently substituting missing.
fn failed(value: &Value, target: ColumnType, reason: ConversionReason) -> ConversionError {
    ConversionError {
        target,
        value: ConversionValue::Runtime(value.clone()),
        reason,
    }
}

/// Recognize only REQ-0015's non-finite spellings, including sign restrictions.
fn nonfinite_text(text: &str) -> bool {
    matches!(text, ".nan" | ".NaN" | ".NAN")
        || matches!(
            text.strip_prefix(['+', '-']).unwrap_or(text),
            ".inf" | ".Inf" | ".INF"
        )
}

/// Validate the closed ASCII number grammar and identify integer-only spelling.
/// A decimal point requires following digits; an exponent requires its digits.
fn integer_spelling(text: &str) -> Option<bool> {
    let bytes = text.as_bytes();
    let mut offset = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = offset;
    while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
        offset += 1;
    }
    if offset == start {
        return None;
    }
    let mut integer = true;
    if bytes.get(offset) == Some(&b'.') {
        integer = false;
        offset += 1;
        let start = offset;
        while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
            offset += 1;
        }
        if offset == start {
            return None;
        }
    }
    if matches!(bytes.get(offset), Some(b'e' | b'E')) {
        integer = false;
        offset += 1;
        if matches!(bytes.get(offset), Some(b'+' | b'-')) {
            offset += 1;
        }
        let start = offset;
        while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
            offset += 1;
        }
        if offset == start {
            return None;
        }
    }
    (offset == bytes.len()).then_some(integer)
}

/// Canonicalize validated integer text for lossless out-of-range diagnostics.
fn integer_diagnostic(text: &str) -> String {
    let digits = text
        .strip_prefix(['+', '-'])
        .unwrap_or(text)
        .trim_start_matches('0');
    if digits.is_empty() {
        return "0".into();
    }
    let mut result = String::new();
    if text.starts_with('-') {
        result.push('-');
    }
    result.push_str(digits);
    result
}

/// Convert a finite float only when exactly integral and within the i64 range.
fn integral(value: FiniteFloat) -> Result<Value, ConversionError> {
    let float = value.get();
    // MAX as f64 rounds to 2^63, so the upper bound must be exclusive.
    if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&float) {
        let integer = float as i64;
        if integer as f64 == float {
            return Ok(Value::Int(integer));
        }
    }
    Err(failed(
        &Value::Float(value),
        ColumnType::Int,
        ConversionReason::IntegerRangeOrFraction,
    ))
}

/// Parse a numeric destination after lexical validation, never coercing other types.
fn numeric_text(text: &str, target: ColumnType) -> Result<Value, ConversionError> {
    if nonfinite_text(text) {
        return Ok(Value::Missing);
    }
    let invalid = || ConversionError {
        target,
        value: ConversionValue::Runtime(Value::Str(text.into())),
        reason: ConversionReason::IncompatibleOrInvalidText,
    };
    let integer = integer_spelling(text).ok_or_else(invalid)?;
    if target == ColumnType::Int && integer {
        return text
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| ConversionError {
                target,
                value: ConversionValue::Integer(integer_diagnostic(text)),
                reason: ConversionReason::IntegerRangeOrFraction,
            });
    }
    let parsed = Value::float(text.parse::<f64>().map_err(|_| invalid())?);
    match (target, parsed) {
        (ColumnType::Int, Value::Float(value)) => integral(value),
        (_, value) => Ok(value),
    }
}

/// Render the shortest round-trip digits in positional notation (REQ-0018).
/// Ryu's tie-to-even digits match Python repr; Rust Display differs on exact ties.
/// Scientific notation is expanded without performing any floating-point math.
pub fn float_text(value: FiniteFloat) -> String {
    let mut buffer = ryu::Buffer::new();
    let text = buffer.format_finite(value.get());
    let Some((coefficient, exponent)) = text.split_once('e') else {
        return text.strip_suffix(".0").unwrap_or(text).into();
    };
    // Ryu emits a signed decimal exponent within binary64's bounded range.
    let power = exponent
        .parse::<i32>()
        .expect("Ryu emits a bounded decimal exponent");
    let negative = coefficient.starts_with('-');
    let coefficient = coefficient.strip_prefix('-').unwrap_or(coefficient);
    let point = coefficient.find('.').unwrap_or(coefficient.len()) as i32 + power;
    let digits: String = coefficient.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0');
    let mut result = String::new();
    if negative {
        result.push('-');
    }
    if point <= 0 {
        result.push_str("0.");
        for _ in 0..-point {
            result.push('0');
        }
        result.push_str(digits);
    } else if point as usize >= digits.len() {
        result.push_str(digits);
        for _ in digits.len()..point as usize {
            result.push('0');
        }
    } else {
        let point = point as usize;
        result.push_str(&digits[..point]);
        result.push('.');
        result.push_str(&digits[point..]);
    }
    result
}

/// Convert a completed derivation result through the closed column-type matrix.
/// Identity retains temporal precision; conversion to text intentionally drops it.
/// Missing succeeds before handler selection; bool fails for every column type.
pub fn convert(value: &Value, target: ColumnType) -> Result<Value, ConversionError> {
    use ColumnType as T;
    use ConversionReason as R;
    match (value, target) {
        (Value::Missing, _)
        | (Value::Str(_), T::Str)
        | (Value::Int(_), T::Int)
        | (Value::Float(_), T::Float)
        | (Value::Date(_), T::Date)
        | (Value::DateTime(_), T::DateTime) => Ok(value.clone()),
        (Value::Int(number), T::Str) => Ok(Value::Str(number.to_string())),
        (Value::Float(number), T::Str) => Ok(Value::Str(float_text(*number))),
        (Value::Date(date), T::Str) => Ok(Value::Str(date.to_string())),
        (Value::DateTime(date), T::Str) => Ok(Value::Str(date.to_string())),
        (Value::Str(text), T::Int | T::Float) => numeric_text(text, target),
        (Value::Float(number), T::Int) => integral(*number),
        (Value::Int(number), T::Float) => Ok(Value::float(*number as f64)),
        (Value::Str(text), T::Date) => text
            .parse()
            .map(Value::Date)
            .map_err(|_| failed(value, target, R::InvalidTemporalText)),
        (Value::Str(text), T::DateTime) => text
            .parse()
            .map(Value::DateTime)
            .map_err(|_| failed(value, target, R::InvalidTemporalText)),
        _ => Err(failed(value, target, R::IncompatibleOrInvalidText)),
    }
}
