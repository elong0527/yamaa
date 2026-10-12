//! Shared typed reducer results; collection and relation selection belong to callers.
use crate::{
    aggregate_parser::Reducer,
    numeric::Number,
    reduction::{count_values, reduce_numeric, NumericReducer, ReductionError},
    table::ValueRef,
    value::{compare_present_refs, ValueType},
};
use core::cmp::Ordering;

#[derive(Debug, PartialEq, Eq)]
pub enum AggregateReductionError {
    Numeric(ReductionError),
    MultipleRecords {
        count: usize,
    },
    IncompatibleInputs {
        argument: usize,
        left: ValueType,
        right: ValueType,
    },
}

fn number(value: Number) -> ValueRef<'static> {
    match value {
        Number::Missing => ValueRef::Missing,
        Number::Int(value) => ValueRef::Int(value),
        Number::Float(value) => ValueRef::Float(value),
    }
}

/// Reduce all collected arguments in relation order. Text remains borrowed.
/// ONLY counts records even when their values are missing; MIN/MAX retain the
/// selected value's type and use exact mixed numeric comparison.
pub fn reduce_values<'a>(
    values: &[ValueRef<'a>],
    reducer: Reducer,
    expression: &str,
) -> Result<ValueRef<'a>, AggregateReductionError> {
    use AggregateReductionError as Error;
    match reducer {
        Reducer::Count => count_values(values).map(number).map_err(Error::Numeric),
        Reducer::Sum | Reducer::Mean => reduce_numeric(
            values,
            if reducer == Reducer::Sum {
                NumericReducer::Sum
            } else {
                NumericReducer::Mean
            },
            expression,
        )
        .map(number)
        .map_err(Error::Numeric),
        Reducer::Only => match values {
            [] => Ok(ValueRef::Missing),
            [value] => Ok(*value),
            _ => Err(Error::MultipleRecords {
                count: values.len(),
            }),
        },
        Reducer::Min | Reducer::Max => {
            let mut selected = None;
            for (argument, value) in values.iter().copied().enumerate() {
                let Some(right) = value.value_type() else {
                    continue;
                };
                let Some(previous) = selected else {
                    selected = Some(value);
                    continue;
                };
                let order = compare_present_refs(previous, value).map_err(|_| {
                    Error::IncompatibleInputs {
                        argument,
                        left: previous.value_type().expect("present selected argument"),
                        right,
                    }
                })?;
                if (reducer == Reducer::Min && order == Ordering::Greater)
                    || (reducer == Reducer::Max && order == Ordering::Less)
                {
                    selected = Some(value);
                }
            }
            Ok(selected.unwrap_or(ValueRef::Missing))
        }
    }
}
