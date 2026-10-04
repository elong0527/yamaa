//! Closed byte-carried host scalars; validate before constructing a Rust string.
use yamaa_core::{
    numeric::{self, BinaryOperator, Number, UnaryOperator},
    temporal::{DatePrecision, DateTimePrecision},
    value::{compare_present, Value},
};

pub const MAX_SCALAR_BYTES: usize = 1_048_576;
/// A logical scalar, not a vector: tag 0 missing, 1 int, 2 float, 3 str, 4 bool.
/// Integers use canonical ASCII decimal; float bits use eight little-endian bytes.
/// Tags 5/6 carry whole epoch days/seconds as exact binary64 little-endian bytes.
/// Host temporal encoding deliberately drops collected precision (REQ-0570).
#[derive(Debug, PartialEq, Eq)]
pub struct ScalarBytes {
    pub tag: i32,
    pub payload: Vec<u8>,
}

/// Admit a byte payload with no locale, R string pointer or sentinel assumption.
pub fn decode(tag: i32, bytes: &[u8]) -> Result<Value, &'static str> {
    if bytes.len() > MAX_SCALAR_BYTES {
        return Err("scalar exceeds byte limit");
    }
    Ok(match tag {
        0 if bytes.is_empty() => Value::Missing,
        1 => {
            let text = std::str::from_utf8(bytes).map_err(|_| "invalid integer scalar")?;
            let value = text.parse::<i64>().map_err(|_| "invalid integer scalar")?;
            if value.to_string() != text {
                return Err("invalid integer scalar");
            }
            Value::Int(value)
        }
        2 if bytes.len() == 8 => {
            Value::float(f64::from_le_bytes(bytes.try_into().expect("eight bytes")))
        }
        3 => Value::Str(
            std::str::from_utf8(bytes)
                .map_err(|_| "invalid UTF-8 scalar")?
                .into(),
        ),
        4 if bytes == [0] => Value::Bool(false),
        4 if bytes == [1] => Value::Bool(true),
        5 | 6 if bytes.len() == 8 => {
            let epoch = f64::from_le_bytes(bytes.try_into().expect("eight bytes"));
            // All valid civil epochs are far inside 2^53. Check before casting;
            // never saturate, round fractions or delegate to a host timezone.
            if !epoch.is_finite() || epoch.fract() != 0.0 || epoch.abs() > 1e12 {
                return Err("invalid temporal scalar");
            }
            if tag == 5 {
                Value::Date(
                    crate::arrow_temporal::date_from_days(epoch as i64, DatePrecision::Day)
                        .ok_or("invalid temporal scalar")?,
                )
            } else {
                Value::DateTime(
                    crate::arrow_temporal::datetime_from_seconds(
                        epoch as i64,
                        DateTimePrecision::Second,
                    )
                    .ok_or("invalid temporal scalar")?,
                )
            }
        }
        _ => return Err("invalid scalar payload"),
    })
}
/// Encode normalized host values; temporal precision is dropped only here.
pub fn encode(value: Value) -> Result<ScalarBytes, &'static str> {
    let (tag, payload) = match value {
        Value::Missing => (0, vec![]),
        Value::Int(n) => (1, n.to_string().into_bytes()),
        Value::Float(n) => (2, n.get().to_le_bytes().to_vec()),
        Value::Str(s) => (3, s.into_bytes()),
        Value::Bool(b) => (4, vec![u8::from(b)]),
        Value::Date(d) => (
            5,
            f64::from(crate::arrow_temporal::date_days(d))
                .to_le_bytes()
                .to_vec(),
        ),
        Value::DateTime(d) => (
            6,
            (crate::arrow_temporal::datetime_seconds(d) as f64)
                .to_le_bytes()
                .to_vec(),
        ),
    };
    if payload.len() > MAX_SCALAR_BYTES {
        return Err("scalar exceeds byte limit");
    }
    Ok(ScalarBytes { tag, payload })
}
/// Convert an integer to binary64 only when the numeric value is exactly retained.
/// Never delegate canonical integer text parsing to a host's decimal parser.
pub fn exact_float(value: &Value) -> Result<Value, &'static str> {
    let Value::Int(number) = value else {
        return Err("expected an integer scalar");
    };
    let result = Value::float(*number as f64);
    if compare_present(value, &result) != Ok(std::cmp::Ordering::Equal) {
        return Err("integer is not exactly representable as an R double");
    }
    Ok(result)
}
/// Apply a closed scalar operator using shared numeric and comparison semantics.
/// Codes: 1 +, 2 -, 3 *, 4 /, 5 MOD; 6 unary+, 7 unary-; 8 ==, 9 !=,
/// 10 <, 11 <=, 12 >, 13 >=. No vector recycling or implicit string conversion.
pub fn operate(code: i32, left: &Value, right: Option<&Value>) -> Result<Value, String> {
    if matches!(code, 6 | 7) {
        if right.is_some() {
            return Err("unary operator has an extra operand".into());
        }
        let number = Number::try_from(left).map_err(|_| "incompatible scalar operand")?;
        return numeric::unary(
            if code == 6 {
                UnaryOperator::Plus
            } else {
                UnaryOperator::Negate
            },
            number,
            "R scalar operator",
        )
        .map(Value::from)
        .map_err(arithmetic_error);
    }
    let right = right.ok_or("binary operator requires two operands")?;
    if (8..=13).contains(&code) {
        if matches!(left, Value::Missing) || matches!(right, Value::Missing) {
            return Ok(Value::Missing);
        }
        let order = compare_present(left, right).map_err(|_| "incompatible scalar operands")?;
        use std::cmp::Ordering::*;
        return Ok(Value::Bool(match code {
            8 => order == Equal,
            9 => order != Equal,
            10 => order == Less,
            11 => order != Greater,
            12 => order == Greater,
            _ => order != Less,
        }));
    }
    let op = match code {
        1 => BinaryOperator::Add,
        2 => BinaryOperator::Subtract,
        3 => BinaryOperator::Multiply,
        4 => BinaryOperator::Divide,
        5 => BinaryOperator::Modulo,
        _ => return Err("unsupported scalar operator".into()),
    };
    let left = Number::try_from(left).map_err(|_| "incompatible scalar operand")?;
    let right = Number::try_from(right).map_err(|_| "incompatible scalar operand")?;
    numeric::binary(op, left, right, "R scalar operator")
        .map(Value::from)
        .map_err(arithmetic_error)
}
/// Preserve the exact overflow integer and normative condition in R host errors.
fn arithmetic_error(error: numeric::ArithmeticError) -> String {
    match error.kind {
        numeric::ArithmeticErrorKind::IntegerOverflow { value } => {
            format!("{}: {value} ({})", error.condition(), error.requirement())
        }
        _ => format!("{} ({})", error.condition(), error.requirement()),
    }
}
