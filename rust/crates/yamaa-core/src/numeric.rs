//! Basic arithmetic over already evaluated, typed operands (REQ-0420–0434).
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
    pub fn float(value: f64) -> Self {
        FiniteFloat::new(value).map_or(Self::Missing, Self::Float)
    }

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
    pub fn phase(&self) -> &'static str {
        "derivation"
    }

    pub fn condition(&self) -> &'static str {
        match self.kind {
            ArithmeticErrorKind::IntegerOverflow { .. } => "integer_overflow",
            ArithmeticErrorKind::DivisionByZero => "division_by_zero",
        }
    }

    pub fn requirement(&self) -> &'static str {
        match self.kind {
            ArithmeticErrorKind::IntegerOverflow { .. } => "REQ-0434",
            ArithmeticErrorKind::DivisionByZero => "REQ-0430",
        }
    }
}

fn checked_integer(value: i128, expression: &str) -> Result<Number, ArithmeticError> {
    i64::try_from(value)
        .map(Number::Int)
        .map_err(|_| ArithmeticError {
            expression: expression.to_string(),
            kind: ArithmeticErrorKind::IntegerOverflow { value },
        })
}

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
