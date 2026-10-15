//! COUNT and ordered SUM/MEAN over collected arguments (REQ-0477/0479/0480/0492).

use crate::{
    numeric::{binary, ArithmeticError, BinaryOperator, Number},
    table::ValueRef,
    value::ValueType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericReducer {
    Sum,
    Mean,
}

impl NumericReducer {
    /// Return the canonical reducer spelling used in diagnostic context.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sum => "SUM",
            Self::Mean => "MEAN",
        }
    }
}

/// Caller retains expression/source/path context. Argument indices include missing
/// values. IncompatibleInputType maps to validation/REQ-0510; arithmetic retains
/// the scalar diagnostic. CountOverflow is a capacity failure, not language truth.
#[derive(Debug, PartialEq, Eq)]
pub enum ReductionError {
    IncompatibleInputType {
        argument: usize,
        actual: ValueType,
    },
    Arithmetic {
        argument: usize,
        error: ArithmeticError,
    },
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
