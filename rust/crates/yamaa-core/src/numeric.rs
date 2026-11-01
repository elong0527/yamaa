//! Basic arithmetic over already evaluated, typed operands (FORMULA-12 to FORMULA-15, VALUE-5 and VALUE-10).
//!
//! Parsing, operand evaluation order, callbacks and handler application belong
//! to later layers. These functions do not skip evaluating an expression tree.

use crate::expression_table::Function;
use crate::table::ValueRef;
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
    pub(crate) fn as_float(self) -> Option<f64> {
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
        Self::try_from(ValueRef::from(value))
    }
}

impl TryFrom<ValueRef<'_>> for Number {
    type Error = ValueType;

    /// Accept only numeric or missing values; return the incompatible type otherwise.
    fn try_from(value: ValueRef<'_>) -> Result<Self, Self::Error> {
        match value {
            ValueRef::Missing => Ok(Self::Missing),
            ValueRef::Int(number) => Ok(Self::Int(number)),
            ValueRef::Float(number) => Ok(Self::Float(number)),
            ValueRef::Str(_) => Err(ValueType::Str),
            ValueRef::Bool(_) => Err(ValueType::Bool),
            ValueRef::Date(_) => Err(ValueType::Date),
            ValueRef::DateTime(_) => Err(ValueType::DateTime),
        }
    }
}

impl<'a> From<Number> for ValueRef<'a> {
    /// Lend a number as the cell it would be.
    fn from(number: Number) -> Self {
        match number {
            Number::Missing => Self::Missing,
            Number::Int(value) => Self::Int(value),
            Number::Float(value) => Self::Float(value),
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
    /// A logarithm of a zero or negative argument, naming the function or
    /// reducer that takes it as FORMULA-13 or FORMULA-18 spells it.
    LnOfNonpositive {
        function: &'static str,
    },
    /// Promoted finite arguments retained as exact bits for future diagnostic transport.
    InvalidPower {
        base_bits: u64,
        exponent_bits: u64,
    },
    /// `NORM_INV` of a promoted argument outside the open interval (0, 1),
    /// retained as exact bits.
    InvalidProbability {
        value_bits: u64,
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
#[inline]
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
#[inline]
pub fn binary(
    operator: BinaryOperator,
    left: Number,
    right: Number,
    expression: &str,
) -> Result<Number, ArithmeticError> {
    let (Some(a), Some(b)) = (left.as_float(), right.as_float()) else {
        return Ok(Number::Missing);
    };
    if zero_divisor(operator, b) {
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
    Ok(float_binary(operator, a, b))
}

/// Whether `operator` refuses its divisor `b`: `/` and `MOD` refuse zero.
#[inline]
pub(crate) fn zero_divisor(operator: BinaryOperator, b: f64) -> bool {
    matches!(operator, BinaryOperator::Divide | BinaryOperator::Modulo) && b == 0.0
}

/// The binary64 result of `operator` over two operands whose divisor
/// [`zero_divisor`] admits, missing when it is not finite: the float
/// arithmetic of [`binary`], which the column form's loops over floats
/// share (#4218).
#[inline]
pub(crate) fn float_binary(operator: BinaryOperator, a: f64, b: f64) -> Number {
    Number::float(float_operation(operator, a, b))
}

/// The binary64 operation of `operator`, before [`float_binary`] maps a
/// result that is not finite to missing.
#[inline]
pub(crate) fn float_operation(operator: BinaryOperator, a: f64, b: f64) -> f64 {
    match operator {
        BinaryOperator::Add => a + b,
        BinaryOperator::Subtract => a - b,
        BinaryOperator::Multiply => a * b,
        BinaryOperator::Divide => a / b,
        BinaryOperator::Modulo => a % b,
    }
}

/// Apply a unary primitive, preserving numeric type and missing; reject i64 overflow.
#[inline]
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
#[inline]
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
#[inline]
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
#[inline]
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

/// Portable unary math (FORMULA-13): the pinned libm, or a named algorithm
/// over it, the same binary64 operations on every platform; this is not
/// platform-Python bit parity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathFunction {
    Exp,
    Ln,
    Log10,
    NormCdf,
    NormInv,
}

impl MathFunction {
    /// The grammar function this computes (FORMULA-13).
    pub const fn function(self) -> Function {
        match self {
            Self::Exp => Function::Exp,
            Self::Ln => Function::Ln,
            Self::Log10 => Function::Log10,
            Self::NormCdf => Function::NormCdf,
            Self::NormInv => Function::NormInv,
        }
    }

    /// The function's spelling, which a domain failure names.
    pub const fn name(self) -> &'static str {
        self.function().name()
    }
}

/// Apply the explicitly selected portable math policy (FORMULA-13 to FORMULA-15
/// and VALUE-10). Promote before math, propagate missing before the domain
/// checks, and normalize overflow to missing.
pub fn math(
    function: MathFunction,
    number: Number,
    expression: &str,
) -> Result<Number, ArithmeticError> {
    let Some(value) = number.as_float() else {
        return Ok(Number::Missing);
    };
    let refused = match function {
        MathFunction::Ln | MathFunction::Log10 if value <= 0.0 => {
            Some(ArithmeticErrorKind::LnOfNonpositive {
                function: function.name(),
            })
        }
        // 0 and 1 too: their quantiles are infinite.
        MathFunction::NormInv if value <= 0.0 || value >= 1.0 => {
            Some(ArithmeticErrorKind::InvalidProbability {
                value_bits: value.to_bits(),
            })
        }
        _ => None,
    };
    if let Some(kind) = refused {
        return Err(ArithmeticError {
            expression: expression.into(),
            kind,
        });
    }
    Ok(Number::float(match function {
        MathFunction::Exp => libm::exp(value),
        MathFunction::Ln => libm::log(value),
        MathFunction::Log10 => libm::log10(value),
        MathFunction::NormCdf => 0.5 * libm::erfc(-value / std::f64::consts::SQRT_2),
        MathFunction::NormInv => normal_quantile(value),
    }))
}

/// Wichura's algorithm AS 241, PPND16 (Applied Statistics 37(3), 1988,
/// 477-484): the standard normal quantile of `p` in the open interval (0, 1),
/// accurate to about 1 part in 10^16, as FORMULA-13 states it. Each
/// polynomial holds the published coefficients from degree 0 to 7 and is
/// evaluated by Horner's rule from degree 7, each step one binary64
/// operation, with the pinned `log` and `sqrt`. The coefficients keep the
/// paper's digits, so the transcription reads against it; each literal is
/// the binary64 value nearest them.
#[allow(clippy::excessive_precision)]
fn normal_quantile(p: f64) -> f64 {
    // Near 0.5: q * A(r) / B(r), r = 0.180625 - q * q.
    const A: [f64; 8] = [
        3.387_132_872_796_366_608,
        1.331_416_678_917_843_774_5e2,
        1.971_590_950_306_551_442_7e3,
        1.373_169_376_550_946_112_5e4,
        4.592_195_393_154_987_145_7e4,
        6.726_577_092_700_870_085_3e4,
        3.343_057_558_358_812_810_5e4,
        2.509_080_928_730_122_672_7e3,
    ];
    const B: [f64; 8] = [
        1.0,
        4.231_333_070_160_091_125_2e1,
        6.871_870_074_920_579_083e2,
        5.394_196_021_424_751_107_7e3,
        2.121_379_430_158_659_586_7e4,
        3.930_789_580_009_271_061e4,
        2.872_908_573_572_194_267_4e4,
        5.226_495_278_852_854_561e3,
    ];
    // Neither near 0.5 nor in a tail: C(r - 1.6) / D(r - 1.6).
    const C: [f64; 8] = [
        1.423_437_110_749_683_577_34,
        4.630_337_846_156_545_295_9,
        5.769_497_221_460_691_405_5,
        3.647_848_324_763_204_605_04,
        1.270_458_252_452_368_382_58,
        2.417_807_251_774_506_117_7e-1,
        2.272_384_498_926_918_458_33e-2,
        7.745_450_142_783_414_076_4e-4,
    ];
    const D: [f64; 8] = [
        1.0,
        2.053_191_626_637_758_821_87,
        1.676_384_830_183_803_849_4,
        6.897_673_349_851_000_045_5e-1,
        1.481_039_764_274_800_745_9e-1,
        1.519_866_656_361_645_719_66e-2,
        5.475_938_084_995_344_946e-4,
        1.050_750_071_644_416_843_24e-9,
    ];
    // In a tail, min(p, 1 - p) below exp(-25): E(r - 5) / F(r - 5).
    const E: [f64; 8] = [
        6.657_904_643_501_103_777_2,
        5.463_784_911_164_114_369_9,
        1.784_826_539_917_291_335_8,
        2.965_605_718_285_048_912_3e-1,
        2.653_218_952_657_612_309_3e-2,
        1.242_660_947_388_078_438_6e-3,
        2.711_555_568_743_487_578_15e-5,
        2.010_334_399_292_288_132_65e-7,
    ];
    const F: [f64; 8] = [
        1.0,
        5.998_322_065_558_879_376_9e-1,
        1.369_298_809_227_358_053_1e-1,
        1.487_536_129_085_061_485_25e-2,
        7.868_691_311_456_132_591e-4,
        1.846_318_317_510_054_681_8e-5,
        1.421_511_758_316_445_888_7e-7,
        2.044_263_103_389_939_785_64e-15,
    ];
    fn horner(coefficients: &[f64; 8], r: f64) -> f64 {
        coefficients[..7]
            .iter()
            .rev()
            .fold(coefficients[7], |value, coefficient| {
                value * r + coefficient
            })
    }
    let q = p - 0.5;
    if q.abs() <= 0.425 {
        let r = 0.180_625 - q * q;
        return q * horner(&A, r) / horner(&B, r);
    }
    let r = libm::sqrt(-libm::log(if q < 0.0 { p } else { 1.0 - p }));
    let value = if r <= 5.0 {
        let r = r - 1.6;
        horner(&C, r) / horner(&D, r)
    } else {
        let r = r - 5.0;
        horner(&E, r) / horner(&F, r)
    };
    if q < 0.0 {
        -value
    } else {
        value
    }
}

/// Apply portable POWER after both operands evaluate; missing precedes domain checks.
/// Domain tests use promoted binary64 arguments, as the reference does. Diagnostic
/// bits preserve signed zero and full finite values without changing error equality.
#[inline]
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
