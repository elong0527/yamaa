//! Basic arithmetic over already evaluated, typed operands (REQ-0420-0434).
//!
//! Parsing, operand evaluation order, callbacks and handler application belong
//! to later layers. These functions do not skip evaluating an expression tree.

use alloc::string::{String, ToString};

use crate::value::{FiniteFloat, Value, ValueType};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    Missing,
    Int(i64),
    Float(FiniteFloat),
}

impl Number {
    /// Normalize binary64 input or an operator result before another numeric consumer.
    pub fn float(value: f64) -> Self {
        FiniteFloat::new(value).map_or(Self::Missing, Self::Float)
    }

    /// Promote a present number to binary64, retaining missing as None.
    fn as_float(self) -> Option<f64> {
        match self {
            Self::Int(value) => Some(value as f64),
            Self::Float(value) => Some(value.get()),
            Self::Missing => None,
        }
    }
}

impl TryFrom<&Value> for Number {
    type Error = ValueType;

    /// Accept only numeric or missing values; return the incompatible type otherwise.
    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::Missing => Ok(Self::Missing),
            Value::Int(number) => Ok(Self::Int(*number)),
            Value::Float(number) => Ok(Self::Float(*number)),
            Value::Str(_) => Err(ValueType::Str),
            Value::Bool(_) => Err(ValueType::Bool),
            Value::Date(_) => Err(ValueType::Date),
            Value::DateTime(_) => Err(ValueType::DateTime),
        }
    }
}

impl From<Number> for Value {
    /// Preserve the numeric variant when embedding it in the runtime value model.
    fn from(number: Number) -> Self {
        match number {
            Number::Missing => Self::Missing,
            Number::Int(value) => Self::Int(value),
            Number::Float(value) => Self::Float(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOperator {
    Plus,
    Negate,
    Abs,
}

/// The wider integer is diagnostic data, never a successful runtime value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithmeticErrorKind {
    IntegerOverflow { value: i128 },
    DivisionByZero,
}

/// Portable condition data; a future evaluator adds source paths/handler traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArithmeticError {
    pub expression: String,
    pub kind: ArithmeticErrorKind,
}

impl ArithmeticError {
    /// Return the derivation phase owned by arithmetic evaluation failures.
    pub fn phase(&self) -> &'static str {
        "derivation"
    }

    /// Return the portable condition vocabulary entry for this arithmetic failure.
    pub fn condition(&self) -> &'static str {
        match self.kind {
            ArithmeticErrorKind::IntegerOverflow { .. } => "integer_overflow",
            ArithmeticErrorKind::DivisionByZero => "division_by_zero",
        }
    }

    /// Return the normative arithmetic requirement, matching the Python reference.
    pub fn requirement(&self) -> &'static str {
        match self.kind {
            ArithmeticErrorKind::IntegerOverflow { .. } => "REQ-0434",
            ArithmeticErrorKind::DivisionByZero => "REQ-0430",
        }
    }
}

/// Narrow an exact intermediate to i64 or retain its full overflow diagnostic value.
fn checked_integer(value: i128, expression: &str) -> Result<Number, ArithmeticError> {
    i64::try_from(value)
        .map(Number::Int)
        .map_err(|_| ArithmeticError {
            expression: expression.to_string(),
            kind: ArithmeticErrorKind::IntegerOverflow { value },
        })
}

/// Apply a binary primitive to evaluated operands; propagate missing before arithmetic
/// and return structured overflow or zero-divisor failures with expression context.
pub fn binary(
    operator: BinaryOperator,
    left: Number,
    right: Number,
    expression: &str,
) -> Result<Number, ArithmeticError> {
    let (Some(a), Some(b)) = (left.as_float(), right.as_float()) else {
        return Ok(Number::Missing);
    };
    if matches!(operator, BinaryOperator::Divide | BinaryOperator::Modulo) && b == 0.0 {
        return Err(ArithmeticError {
            expression: expression.to_string(),
            kind: ArithmeticErrorKind::DivisionByZero,
        });
    }
    if let (Number::Int(a), Number::Int(b)) = (left, right) {
        // Every binary i64 result fits i128, including MIN * MIN. The
        // wider calculation also retains exact overflow diagnostic text.
        let (a, b) = (i128::from(a), i128::from(b));
        let value = match operator {
            BinaryOperator::Add => Some(a + b),
            BinaryOperator::Subtract => Some(a - b),
            BinaryOperator::Multiply => Some(a * b),
            BinaryOperator::Modulo => Some(a % b),
            BinaryOperator::Divide => None,
        };
        if let Some(value) = value {
            return checked_integer(value, expression);
        }
    }
    let result = match operator {
        BinaryOperator::Add => a + b,
        BinaryOperator::Subtract => a - b,
        BinaryOperator::Multiply => a * b,
        BinaryOperator::Divide => a / b,
        BinaryOperator::Modulo => a % b,
    };
    Ok(Number::float(result))
}

/// Apply a unary primitive, preserving numeric type and missing; reject i64 overflow.
pub fn unary(
    operator: UnaryOperator,
    value: Number,
    expression: &str,
) -> Result<Number, ArithmeticError> {
    match value {
        Number::Missing => Ok(Number::Missing),
        Number::Int(value) => {
            let value = i128::from(value);
            checked_integer(
                match operator {
                    UnaryOperator::Plus => value,
                    UnaryOperator::Negate => -value,
                    UnaryOperator::Abs => value.abs(),
                },
                expression,
            )
        }
        Number::Float(value) => Ok(Number::float(match operator {
            UnaryOperator::Plus => value.get(),
            UnaryOperator::Negate => -value.get(),
            UnaryOperator::Abs => value.get().abs(),
        })),
    }
}

/// Numeric functions selecting among already-evaluated arguments (REQ-0424).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionFunction {
    Greatest,
    Least,
    Coalesce,
}

/// Select a present value, then promote if any present argument is float.
/// Extremes compare before promotion and retain the first equal value (including
/// signed zero); COALESCE selects the first present value. Missing arguments do
/// not influence promotion. Empty primitive input is missing; the grammar layer
/// separately enforces each function's minimum arity.
pub fn select(function: SelectionFunction, arguments: &[Number]) -> Number {
    use core::cmp::Ordering;
    let mut selected = Number::Missing;
    let mut has_float = false;
    for &argument in arguments {
        if argument == Number::Missing {
            continue;
        }
        has_float |= matches!(argument, Number::Float(_));
        if selected == Number::Missing {
            selected = argument;
        } else if function != SelectionFunction::Coalesce {
            let order = crate::value::compare_present(&argument.into(), &selected.into())
                .expect("present numeric arguments are comparable");
            if (function == SelectionFunction::Greatest && order == Ordering::Greater)
                || (function == SelectionFunction::Least && order == Ordering::Less)
            {
                selected = argument;
            }
        }
    }
    match (selected, has_float) {
        (Number::Int(value), true) => Number::float(value as f64),
        _ => selected,
    }
}

/// Return missing for equal arguments, comparing mixed types after promotion.
/// Unlike extreme selection, the reference NULLIF compares a mixed pair as two
/// binary64 values. A present unmatched left operand has the promoted pair type.
pub fn null_if(left: Number, right: Number) -> Number {
    match (left, right) {
        (Number::Missing, _) => Number::Missing,
        (_, Number::Missing) => left,
        (Number::Int(a), Number::Int(b)) => {
            if a == b {
                Number::Missing
            } else {
                left
            }
        }
        _ => {
            let a = left.as_float().expect("present numeric left operand");
            let b = right.as_float().expect("present numeric right operand");
            if a == b {
                Number::Missing
            } else {
                Number::float(a)
            }
        }
    }
}

/// Integral-valued functions that always return float (REQ-0424).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegralFunction {
    Ceil,
    Floor,
    Trunc,
}

/// Round a finite number to an integral binary64 value, propagating missing.
/// Clear fractional significand bits directly so no host math library or bounded
/// integer conversion is needed. Zero results are positive, matching the Python
/// reference's integer intermediate, including for a negative-zero input.
pub fn integral(function: IntegralFunction, number: Number) -> Number {
    let Some(value) = number.as_float() else {
        return Number::Missing;
    };
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    if exponent >= 52 {
        // All finite binary64 values this large are already integral.
        return Number::float(value);
    }
    let truncated = if exponent < 0 {
        0.0
    } else {
        let fractional_mask = (1_u64 << (52 - exponent)) - 1;
        f64::from_bits(bits & !fractional_mask)
    };
    let result = match function {
        IntegralFunction::Ceil if value > truncated => truncated + 1.0,
        IntegralFunction::Floor if value < truncated => truncated - 1.0,
        _ => truncated,
    };
    Number::float(result)
}
