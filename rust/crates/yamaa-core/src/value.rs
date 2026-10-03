//! Closed runtime types, boundary normalization and comparison.

use alloc::string::String;
use core::cmp::Ordering;

use crate::temporal::{Date, DateTime};

/// A float which cannot carry NaN or infinity into a consumer (REQ-0006).
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

/// Column types deliberately exclude bool (REQ-0002).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnType {
    Str,
    Int,
    Float,
    Date,
    DateTime,
}

/// Missing is a value; an absent source/record is a separate selection outcome.
/// Derived equality distinguishes int and float variants; language numeric
/// comparisons must use `compare_present`.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonError {
    MissingOperand,
    IncompatibleTypes,
}

/// Compare without rounding a large integer through binary64 first.
/// This preserves the current Python value comparator at and beyond 2^53.
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

/// Compare present ordered values; never invent null placement or coerce text.
/// Boolean predicate truth is handled separately from ordered column values.
pub fn compare_present(left: &Value, right: &Value) -> Result<Ordering, ComparisonError> {
    match (left, right) {
        (Value::Missing, _) | (_, Value::Missing) => Err(ComparisonError::MissingOperand),
        (Value::Int(a), Value::Int(b)) => Ok(a.cmp(b)),
        (Value::Int(a), Value::Float(b)) => Ok(compare_int_float(*a, b.get())),
        (Value::Float(a), Value::Int(b)) => Ok(compare_int_float(*b, a.get()).reverse()),
        (Value::Float(a), Value::Float(b)) => Ok(compare_floats(a.get(), b.get())),
        (Value::Str(a), Value::Str(b)) => Ok(a.cmp(b)),
        (Value::Date(a), Value::Date(b)) => Ok(a.cmp(b)),
        (Value::DateTime(a), Value::DateTime(b)) => Ok(a.cmp(b)),
        _ => Err(ComparisonError::IncompatibleTypes),
    }
}
