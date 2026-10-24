//! Shared typed reducer results; collection, record order and relation
//! selection belong to callers.
use crate::{
    conversion::{convert, ConversionError},
    expression::Reducer,
    reduction::{count_distinct, count_values, reduce_numeric, NumericReducer, ReductionError},
    table::ValueRef,
    value::{compare_present_refs, ColumnType, Value, ValueType},
};
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum AggregateReductionError {
    #[error("aggregate reduction numeric: {0:?}")]
    Numeric(ReductionError),
    #[error("aggregate reduction multiple records: count={count:?}")]
    MultipleRecords { count: usize },
    #[error("aggregate reduction incompatible inputs: argument={argument:?}, left={left:?}, right={right:?}")]
    IncompatibleInputs {
        argument: usize,
        left: ValueType,
        right: ValueType,
    },
    /// A value `STR_JOIN` cannot write as text, which no column holds.
    #[error("aggregate reduction text: argument={argument:?}, error={error:?}")]
    Text {
        argument: usize,
        error: ConversionError,
    },
}

/// The fields of an aggregate a reducer reads besides its argument
/// (FORMULA-18): the text `STR_JOIN` writes between two values, and whether
/// `STR_JOIN` and `COUNT(x)` take each value once. Every other reducer reads
/// neither.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReductionFields<'a> {
    pub separator: &'a str,
    pub distinct: bool,
}

fn owned(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Missing => Value::Missing,
        ValueRef::Str(value) => Value::Str(value.into()),
        ValueRef::Int(value) => Value::Int(value),
        ValueRef::Float(value) => Value::Float(value),
        ValueRef::Bool(value) => Value::Bool(value),
        ValueRef::Date(value) => Value::Date(value),
        ValueRef::DateTime(value) => Value::DateTime(value),
    }
}

/// Reduce all collected arguments in the order given: relation order, or
/// for `STR_JOIN` the aggregate's declared order. ONLY counts records even
/// when their values are missing; MIN/MAX retain the selected value's type
/// and use exact mixed numeric comparison.
pub fn reduce_values(
    values: &[ValueRef<'_>],
    reducer: Reducer,
    fields: ReductionFields<'_>,
    expression: &str,
) -> Result<Value, AggregateReductionError> {
    use AggregateReductionError as Error;
    match reducer {
        Reducer::Count if fields.distinct => count_distinct(values)
            .map(Value::from)
            .map_err(Error::Numeric),
        Reducer::Count => count_values(values)
            .map(Value::from)
            .map_err(Error::Numeric),
        Reducer::Sum | Reducer::Mean => reduce_numeric(
            values,
            if reducer == Reducer::Sum {
                NumericReducer::Sum
            } else {
                NumericReducer::Mean
            },
            expression,
        )
        .map(Value::from)
        .map_err(Error::Numeric),
        Reducer::Only => match values {
            [] => Ok(Value::Missing),
            [value] => Ok(owned(*value)),
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
            Ok(selected.map_or(Value::Missing, owned))
        }
        Reducer::StrJoin => join(values, fields),
    }
}

/// Join the present values in the order given, each as the text a `str`
/// column converts it to (VALUE-3: decimal text, the shortest positional
/// text of VALUE-8, the canonical text of VALUE-17, or text as it is), with
/// the separator between two of them. With `distinct`, each text is written
/// once, where it first occurs. A group with no present value joins to
/// missing, never to the empty string (FORMULA-24).
fn join(
    values: &[ValueRef<'_>],
    fields: ReductionFields<'_>,
) -> Result<Value, AggregateReductionError> {
    let mut joined: Option<String> = None;
    let mut seen = HashSet::new();
    for (argument, value) in values.iter().copied().enumerate() {
        let text = match value {
            ValueRef::Missing => continue,
            ValueRef::Str(text) => text.to_owned(),
            other => match convert(&owned(other), ColumnType::Str) {
                Ok(Value::Str(text)) => text,
                Ok(_) => unreachable!("a value converts to str as text"),
                Err(error) => return Err(AggregateReductionError::Text { argument, error }),
            },
        };
        if fields.distinct && !seen.insert(text.clone()) {
            continue;
        }
        match joined.as_mut() {
            None => joined = Some(text),
            Some(joined) => {
                joined.push_str(fields.separator);
                joined.push_str(&text);
            }
        }
    }
    Ok(joined.map_or(Value::Missing, Value::Str))
}
