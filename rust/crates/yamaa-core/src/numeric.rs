//! Basic arithmetic over already evaluated, typed operands (FORMULA-12 to FORMULA-15, VALUE-5 and VALUE-10).
//!
//! Parsing, operand evaluation order, callbacks and handler application belong
//! to later layers. These functions do not skip evaluating an expression tree.

use crate::value::{compare_present, FiniteFloat, Value, ValueType};

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

    /// VALUE-8: an `int` widened to the nearest binary64 `float`.
    pub fn widened(self) -> Self {
        match self {
            Self::Int(value) => Self::float(value as f64),
            other => other,
        }
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
    IntegerOverflow {
        value: i128,
    },
    DivisionByZero,
    SqrtOfNegative,
    LnOfNonpositive,
    /// Promoted finite arguments retained as exact bits for future diagnostic transport.
    InvalidPower {
        base_bits: u64,
        exponent_bits: u64,
    },
}

/// Portable condition data; a future evaluator adds source paths/handler traces.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("numeric: expression={expression:?}, kind={kind:?}")]
pub struct ArithmeticError {
    pub expression: String,
    pub kind: ArithmeticErrorKind,
}

impl ArithmeticError {
    /// Return the registry phase owning this arithmetic failure.
    pub fn phase(&self) -> &'static str {
        self.kind.diagnostic_code().definition().phase
    }

    /// Return the portable condition vocabulary entry for this arithmetic failure.
    pub fn condition(&self) -> &'static str {
        self.kind.diagnostic_code().definition().condition
    }

    /// Return the normative arithmetic rule, matching the Python reference.
    pub fn rule(&self) -> &'static str {
        self.kind
            .diagnostic_code()
            .definition()
            .rule
            .expect("numeric and conversion causes have a normative rule")
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

/// Return missing for equal arguments. A mixed pair compares by exact value,
/// like every other comparison; zero signs are equal. A present unmatched left
/// operand has the promoted pair type. A missing right operand has no value
/// to promote by; the bound site widens the result when its declared type is
/// `float` (EXPR-14).
pub fn null_if(left: Number, right: Number) -> Number {
    if left == Number::Missing {
        return Number::Missing;
    }
    if right == Number::Missing {
        return left;
    }
    if compare_present(&left.into(), &right.into()) == Ok(std::cmp::Ordering::Equal) {
        return Number::Missing;
    }
    match (left, right) {
        (Number::Int(_), Number::Int(_)) => left,
        _ => Number::float(left.as_float().expect("present numeric left operand")),
    }
}

/// Integral-valued functions that always return float (VALUE-10).
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

/// Return the binary64 square root after numeric promotion (FORMULA-15 and VALUE-10).
/// Missing propagates; negative present input reports a domain condition. Negative
/// zero is valid and retains its sign, matching the Python reference.
pub fn sqrt(number: Number, expression: &str) -> Result<Number, ArithmeticError> {
    let Some(value) = number.as_float() else {
        return Ok(Number::Missing);
    };
    if value < 0.0 {
        return Err(ArithmeticError {
            expression: expression.into(),
            kind: ArithmeticErrorKind::SqrtOfNegative,
        });
    }
    Ok(Number::float(libm::sqrt(value)))
}

/// Portable unary math from pinned libm; this is not platform-Python bit parity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathFunction {
    Exp,
    Ln,
}

/// Apply the explicitly selected portable math policy (FORMULA-14, FORMULA-15 and VALUE-10).
/// Promote before math, propagate missing, and normalize overflow to missing.
pub fn math(
    function: MathFunction,
    number: Number,
    expression: &str,
) -> Result<Number, ArithmeticError> {
    let Some(value) = number.as_float() else {
        return Ok(Number::Missing);
    };
    if function == MathFunction::Ln && value <= 0.0 {
        return Err(ArithmeticError {
            expression: expression.into(),
            kind: ArithmeticErrorKind::LnOfNonpositive,
        });
    }
    Ok(Number::float(match function {
        MathFunction::Exp => libm::exp(value),
        MathFunction::Ln => libm::log(value),
    }))
}

/// Apply portable POWER after both operands evaluate; missing precedes domain checks.
/// Domain tests use promoted binary64 arguments, as the reference does. Diagnostic
/// bits preserve signed zero and full finite values without changing error equality.
pub fn power(left: Number, right: Number, expression: &str) -> Result<Number, ArithmeticError> {
    let (Some(base), Some(exponent)) = (left.as_float(), right.as_float()) else {
        return Ok(Number::Missing);
    };
    if (base == 0.0 && exponent < 0.0) || (base < 0.0 && libm::trunc(exponent) != exponent) {
        return Err(ArithmeticError {
            expression: expression.into(),
            kind: ArithmeticErrorKind::InvalidPower {
                base_bits: base.to_bits(),
                exponent_bits: exponent.to_bits(),
            },
        });
    }
    Ok(Number::float(libm::pow(base, exponent)))
}

/// Round to decimal places under FORMULA-16, returning float or normalized missing.
/// The integer digits declaration is admitted before any value is read.
pub(crate) fn round_integer_places(value: Number, digits: i64) -> Number {
    value.as_float().map_or(Number::Missing, |value| {
        Number::float(crate::decimal_rounding::round_finite(value, digits))
    })
}
