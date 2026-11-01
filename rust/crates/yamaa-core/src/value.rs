//! Closed runtime types, boundary normalization and comparison.

use std::cmp::Ordering;

use crate::temporal::{Date, DateTime};

/// A float which cannot carry NaN or infinity into a consumer (VALUE-4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteFloat(f64);

impl FiniteFloat {
    /// Return a finite wrapper, or None for any NaN or infinity; preserve signed zero.
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    /// Return the held finite binary64 value without rounding or normalization.
    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    Str,
    Int,
    Float,
    Bool,
    Date,
    DateTime,
}

impl ValueType {
    /// The type's spelling in specifications and diagnostic context.
    pub fn name(self) -> &'static str {
        match self {
            Self::Str => "str",
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::Date => "date",
            Self::DateTime => "datetime",
        }
    }
}

/// Column types deliberately exclude bool (VALUE-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnType {
    Str,
    Int,
    Float,
    Date,
    DateTime,
}

impl ColumnType {
    /// The type's spelling in specifications and diagnostic context.
    pub fn name(self) -> &'static str {
        ValueType::from(self).name()
    }
}
impl From<ColumnType> for ValueType {
    fn from(kind: ColumnType) -> Self {
        match kind {
            ColumnType::Str => Self::Str,
            ColumnType::Int => Self::Int,
            ColumnType::Float => Self::Float,
            ColumnType::Date => Self::Date,
            ColumnType::DateTime => Self::DateTime,
        }
    }
}

/// Missing is a value; an absent source/record is a separate selection outcome.
/// Derived equality distinguishes int and float variants. Every language
/// comparison, including predicates and NULLIF, uses `compare_present`, which
/// compares a mixed int/float pair by exact mathematical value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Missing,
    Str(String),
    Int(i64),
    Float(FiniteFloat),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

impl Value {
    /// Normalize a host binary64 value to a finite float or the missing value.
    pub fn float(value: f64) -> Self {
        FiniteFloat::new(value).map_or(Self::Missing, Self::Float)
    }

    /// Return the closed runtime type, or None for the explicit missing value.
    pub fn value_type(&self) -> Option<ValueType> {
        match self {
            Self::Missing => None,
            Self::Str(_) => Some(ValueType::Str),
            Self::Int(_) => Some(ValueType::Int),
            Self::Float(_) => Some(ValueType::Float),
            Self::Bool(_) => Some(ValueType::Bool),
            Self::Date(_) => Some(ValueType::Date),
            Self::DateTime(_) => Some(ValueType::DateTime),
        }
    }
}

/// No implicit conversion maps absence to the missing value.
#[derive(Clone, Debug, PartialEq)]
pub enum Selection {
    Absent,
    Present(Value),
}

/// The consuming operation owns the missing policy and diagnostic context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ComparisonError {
    #[error("value missing operand")]
    MissingOperand,
    #[error("value incompatible types")]
    IncompatibleTypes,
}

/// Compare by exact mathematical value, without rounding a large integer
/// through binary64 first: 9007199254740993 is greater than 9007199254740992.0.
fn compare_int_float(integer: i64, float: f64) -> Ordering {
    const UPPER_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
    const LOWER: f64 = -9_223_372_036_854_775_808.0;
    if float >= UPPER_EXCLUSIVE {
        return Ordering::Less;
    }
    if float < LOWER {
        return Ordering::Greater;
    }
    let integral = float as i64;
    match integer.cmp(&integral) {
        Ordering::Equal => compare_floats(integral as f64, float),
        order => order,
    }
}

/// Order finite binary64 values numerically, treating both zero signs as equal.
fn compare_floats(left: f64, right: f64) -> Ordering {
    if left < right {
        Ordering::Less
    } else if left > right {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

/// A present number of either numeric type, as a comparison orders it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Numeric {
    Int(i64),
    Float(f64),
}

/// Order two present numbers by exact mathematical value: the numeric
/// ordering of [`compare_present_refs`], which the column form's typed loops
/// share (#4218).
#[inline]
pub(crate) fn compare_numeric(left: Numeric, right: Numeric) -> Ordering {
    match (left, right) {
        (Numeric::Int(a), Numeric::Int(b)) => a.cmp(&b),
        (Numeric::Int(a), Numeric::Float(b)) => compare_int_float(a, b),
        (Numeric::Float(a), Numeric::Int(b)) => compare_int_float(b, a).reverse(),
        (Numeric::Float(a), Numeric::Float(b)) => compare_floats(a, b),
    }
}

/// Compare present ordered values; never invent null placement or coerce text.
/// Boolean predicate truth is handled separately from ordered column values.
pub fn compare_present(left: &Value, right: &Value) -> Result<Ordering, ComparisonError> {
    compare_present_refs(left.into(), right.into())
}

/// Present operand kinds share one comparison contract with metadata proofs.
pub(crate) fn comparable_types(left: ValueType, right: ValueType) -> bool {
    use ValueType::*;
    matches!(
        (left, right),
        (Str, Str) | (Date, Date) | (DateTime, DateTime) | (Int | Float, Int | Float)
    )
}

/// Borrow immutable cells for exact matching without retaining donor text.
pub fn compare_present_refs(
    left: crate::table::ValueRef<'_>,
    right: crate::table::ValueRef<'_>,
) -> Result<Ordering, ComparisonError> {
    use crate::table::ValueRef as Value;
    if let (Some(left), Some(right)) = (left.value_type(), right.value_type()) {
        if !comparable_types(left, right) {
            return Err(ComparisonError::IncompatibleTypes);
        }
    }
    match (left, right) {
        (Value::Missing, _) | (_, Value::Missing) => Err(ComparisonError::MissingOperand),
        (Value::Int(a), Value::Int(b)) => Ok(compare_numeric(Numeric::Int(a), Numeric::Int(b))),
        (Value::Int(a), Value::Float(b)) => {
            Ok(compare_numeric(Numeric::Int(a), Numeric::Float(b.get())))
        }
        (Value::Float(a), Value::Int(b)) => {
            Ok(compare_numeric(Numeric::Float(a.get()), Numeric::Int(b)))
        }
        (Value::Float(a), Value::Float(b)) => Ok(compare_numeric(
            Numeric::Float(a.get()),
            Numeric::Float(b.get()),
        )),
        (Value::Str(a), Value::Str(b)) => Ok(a.cmp(b)),
        (Value::Date(a), Value::Date(b)) => Ok(a.cmp(&b)),
        (Value::DateTime(a), Value::DateTime(b)) => Ok(a.cmp(&b)),
        _ => Err(ComparisonError::IncompatibleTypes),
    }
}

/// Encode a runtime scalar once at the owned JSON boundary. Arbitrary diagnostic
/// integers use crate::json::Number directly rather than passing through f64.
impl From<&Value> for crate::json::Value {
    fn from(value: &Value) -> Self {
        match value {
            Value::Missing => Self::Null,
            Value::Str(value) => Self::String(value.clone()),
            Value::Int(value) => Self::from(*value),
            Value::Float(value) => Self::Number(
                crate::json::Number::from_f64(value.get()).expect("finite runtime float"),
            ),
            Value::Bool(value) => Self::Bool(*value),
            Value::Date(value) => Self::String(value.to_string()),
            Value::DateTime(value) => Self::String(value.to_string()),
        }
    }
}
impl From<Value> for crate::json::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::Str(value) => Self::String(value),
            value => Self::from(&value),
        }
    }
}
