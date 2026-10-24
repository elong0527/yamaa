//! COUNT and ordered SUM/MEAN over collected arguments (FORMULA-18, FORMULA-19 and FORMULA-24).

use crate::{
    numeric::{binary, ArithmeticError, BinaryOperator, Number},
    table::ValueRef,
    value::ValueType,
};

/// `SUM` or `MEAN`, the reducers that fold their arguments in order; a
/// diagnostic names the reducer by the grammar's spelling
/// (`crate::expression::Reducer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericReducer {
    Sum,
    Mean,
}

/// Caller retains expression/source/path context. Argument indices include missing
/// values. IncompatibleInputType maps to validation/FORMULA-23; arithmetic retains
/// the scalar diagnostic. CountOverflow is a capacity failure, not language truth.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReductionError {
    #[error("reduction incompatible input type: argument={argument:?}, actual={actual:?}")]
    IncompatibleInputType { argument: usize, actual: ValueType },
    #[error("reduction arithmetic: argument={argument:?}, error={error:?}")]
    Arithmetic {
        argument: usize,
        error: ArithmeticError,
    },
    #[error("reduction count overflow")]
    CountOverflow,
}

/// Count records without resolving a field; an absent relation remains missing.
pub fn count_records(records: usize) -> Result<Number, ReductionError> {
    if records == 0 {
        return Ok(Number::Missing);
    }
    i64::try_from(records)
        .map(Number::Int)
        .map_err(|_| ReductionError::CountOverflow)
}

/// Count present values of any type after every argument has been collected.
/// An existing all-missing group yields zero, unlike a group containing no record.
pub fn count_values(values: &[ValueRef<'_>]) -> Result<Number, ReductionError> {
    if values.is_empty() {
        return Ok(Number::Missing);
    }
    let count = values
        .iter()
        .filter(|value| !matches!(value, ValueRef::Missing))
        .count();
    i64::try_from(count)
        .map(Number::Int)
        .map_err(|_| ReductionError::CountOverflow)
}

/// Count the distinct present values of any type, as `COUNT(x)` with
/// `distinct` does (FORMULA-18): text equal scalar for scalar (VALUE-12),
/// numbers equal by exact value across `int` and `float` (VALUE-19), dates
/// and datetimes equal on the day or second they name (VALUE-21). Values of
/// types that never compare are different values. Like [`count_values`], a
/// group with no record yields missing and an all-missing group zero.
pub fn count_distinct(values: &[ValueRef<'_>]) -> Result<Number, ReductionError> {
    if values.is_empty() {
        return Ok(Number::Missing);
    }
    // Sorted within each family of mutually comparable types, equal values
    // are adjacent, so each run of them is one distinct value.
    fn family(value: ValueRef<'_>) -> u8 {
        match value {
            ValueRef::Missing => 0,
            ValueRef::Str(_) => 1,
            ValueRef::Int(_) | ValueRef::Float(_) => 2,
            ValueRef::Date(_) => 3,
            ValueRef::DateTime(_) => 4,
            ValueRef::Bool(_) => 5,
        }
    }
    fn order(left: ValueRef<'_>, right: ValueRef<'_>) -> std::cmp::Ordering {
        family(left)
            .cmp(&family(right))
            .then_with(|| match (left, right) {
                (ValueRef::Bool(left), ValueRef::Bool(right)) => left.cmp(&right),
                _ => crate::value::compare_present_refs(left, right)
                    .expect("values of one family compare"),
            })
    }
    let mut present = values
        .iter()
        .copied()
        .filter(|value| !matches!(value, ValueRef::Missing))
        .collect::<Vec<_>>();
    present.sort_by(|left, right| order(*left, *right));
    let count = present
        .iter()
        .enumerate()
        .filter(|&(index, value)| {
            index == 0 || order(present[index - 1], *value) != std::cmp::Ordering::Equal
        })
        .count();
    i64::try_from(count)
        .map(Number::Int)
        .map_err(|_| ReductionError::CountOverflow)
}

/// Collect arguments before calling this function: a later resolution failure
/// precedes an earlier potential fold failure in the reference evaluator.
/// Missing arguments are excluded; types are checked during the ordered fold.
/// The first present value seeds SUM (preserving -0), with no whole-group
/// promotion, reassociation, or compensation. MEAN divides this sum by count.
pub fn reduce_numeric(
    values: &[ValueRef<'_>],
    reducer: NumericReducer,
    expression: &str,
) -> Result<Number, ReductionError> {
    let mut total = None;
    let mut count = 0_i64;
    for (argument, value) in values.iter().copied().enumerate() {
        let number = match value {
            ValueRef::Missing => continue,
            ValueRef::Int(value) => Number::Int(value),
            ValueRef::Float(value) => Number::Float(value),
            other => {
                return Err(ReductionError::IncompatibleInputType {
                    argument,
                    actual: other.value_type().expect("present nonnumeric value"),
                })
            }
        };
        count = count.checked_add(1).ok_or(ReductionError::CountOverflow)?;
        total = Some(match total {
            None => number,
            Some(previous) => binary(BinaryOperator::Add, previous, number, expression)
                .map_err(|error| ReductionError::Arithmetic { argument, error })?,
        });
    }
    let Some(total) = total else {
        return Ok(Number::Missing);
    };
    if reducer == NumericReducer::Sum {
        return Ok(total);
    }
    // A positive count cannot fail division; retain the diagnostic if that
    // scalar contract changes, without silently swallowing an error.
    binary(
        BinaryOperator::Divide,
        total,
        Number::Int(count),
        expression,
    )
    .map_err(|error| ReductionError::Arithmetic {
        argument: values.len() - 1,
        error,
    })
}
