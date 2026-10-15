//! Closed scalar arithmetic and comparison; host storage never selects semantics.
use crate::{
    numeric::{self, BinaryOperator, Number, UnaryOperator},
    value::{compare_present, Value},
};
use alloc::{format, string::String};

/// Closed infix/unary spelling admission before host operands are converted.
pub fn operator_code(name: &str, unary: bool) -> Option<i32> {
    Some(match (name, unary) {
        ("+", true) => 6,
        ("-", true) => 7,
        ("+", false) => 1,
        ("-", false) => 2,
        ("*", false) => 3,
        ("/", false) => 4,
        ("==", false) => 8,
        ("!=", false) => 9,
        ("<", false) => 10,
        ("<=", false) => 11,
        (">", false) => 12,
        (">=", false) => 13,
        _ => return None,
    })
}

/// Convert an integer to binary64 only when the numeric value is exactly retained.
/// Never delegate canonical integer text parsing to a host's decimal parser.
pub fn exact_float(value: &Value) -> Result<Value, &'static str> {
    let Value::Int(number) = value else {
        return Err("expected an integer scalar");
    };
    let result = Value::float(*number as f64);
    if compare_present(value, &result) != Ok(core::cmp::Ordering::Equal) {
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
            "scalar operator",
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
        use core::cmp::Ordering::*;
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
    numeric::binary(op, left, right, "scalar operator")
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
