//! Shared typed reducer results; collection, record order and relation
//! selection belong to callers.
use crate::{
    conversion::{convert, ConversionError},
    expression::Reducer,
    limits::{LimitExceeded, Limits},
    numeric::{
        binary, math, sqrt, ArithmeticError, ArithmeticErrorKind, BinaryOperator, MathFunction,
        Number,
    },
    reduction::{count_distinct, count_values, reduce_numeric, NumericReducer, ReductionError},
    table::ValueRef,
    value::{compare_present_refs, ColumnType, Value, ValueType},
};
use std::borrow::Cow;
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
    /// A `STR_JOIN` text longer than `Limits::TEXT_BYTES`, refused before
    /// it is built.
    #[error("aggregate reduction: {0}")]
    Limit(LimitExceeded),
}

/// The fields of an aggregate a reducer reads besides its argument
/// (FORMULA-18): the text `STR_JOIN` writes between two values, whether
/// `STR_JOIN` and `COUNT(x)` take each value once, and the probability, from
/// 0 to 1, at which `QUANTILE` reads its quantile. Every other reducer reads
/// none of them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReductionFields<'a> {
    pub separator: &'a str,
    pub distinct: bool,
    pub quantile: f64,
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
/// and use exact mixed numeric comparison. The statistical reducers return
/// a float, or missing for no present value (FORMULA-24).
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
        Reducer::Median => quantile(values, 0.5, expression),
        Reducer::Q1 => quantile(values, 0.25, expression),
        Reducer::Q3 => quantile(values, 0.75, expression),
        Reducer::Quantile => quantile(values, fields.quantile, expression),
        Reducer::Variance => spread(values, false, expression),
        Reducer::StdDev => spread(values, true, expression),
        Reducer::GeoMean => geometric_mean(values, expression),
    }
}

/// A present number, or None for a missing value; any other present value
/// fails at its position, as `SUM` does (FORMULA-23).
fn number(argument: usize, value: ValueRef<'_>) -> Result<Option<Number>, AggregateReductionError> {
    match value {
        ValueRef::Missing => Ok(None),
        ValueRef::Int(value) => Ok(Some(Number::Int(value))),
        ValueRef::Float(value) => Ok(Some(Number::Float(value))),
        other => Err(AggregateReductionError::Numeric(
            ReductionError::IncompatibleInputType {
                argument,
                actual: other.value_type().expect("a present value has a type"),
            },
        )),
    }
}

/// The failure of a numeric step at the position of the value it read.
fn arithmetic(
    argument: usize,
) -> impl Fn(crate::numeric::ArithmeticError) -> AggregateReductionError {
    move |error| AggregateReductionError::Numeric(ReductionError::Arithmetic { argument, error })
}

/// A logarithm that fails inside a reducer names the reducer that takes it,
/// as FORMULA-18 spells it (FORMULA-15).
fn within(reducer: Reducer, mut error: ArithmeticError) -> ArithmeticError {
    if let ArithmeticErrorKind::LnOfNonpositive { function } = &mut error.kind {
        *function = reducer.name();
    }
    error
}

/// R's default quantile, type 7 of Hyndman and Fan, at probability `p`
/// from 0 to 1 (FORMULA-18). The n present values sort ascending by exact
/// value (VALUE-19), a stable sort keeping equal values in the order given,
/// as x_1 to x_n. With h = (n - 1) * p + 1 in binary64, j its floor and
/// g = h - j, the result is x_j when g is 0 or j is n, and otherwise
/// x_j + g * (x_{j+1} - x_j), evaluated in that written form with the
/// numeric `-`, `*` and `+` (VALUE-10), so an int difference outside the
/// int range fails (VALUE-5). An int result widens to a float (VALUE-8).
fn quantile(
    values: &[ValueRef<'_>],
    p: f64,
    expression: &str,
) -> Result<Value, AggregateReductionError> {
    let mut sorted = Vec::with_capacity(values.len());
    for (argument, value) in values.iter().copied().enumerate() {
        if let Some(present) = number(argument, value)? {
            sorted.push((argument, present, value));
        }
    }
    if sorted.is_empty() {
        return Ok(Value::Missing);
    }
    sorted.sort_by(|(_, _, left), (_, _, right)| {
        compare_present_refs(*left, *right).expect("two numbers compare")
    });
    let n = sorted.len();
    // Preparation admits a probability from 0 to 1 only, so h lies from 1
    // to n and j needs no bound but the one a defensive clamp keeps.
    let h = (n - 1) as f64 * p + 1.0;
    let j = (h.floor() as usize).clamp(1, n);
    let g = h - j as f64;
    let (_, low, _) = sorted[j - 1];
    if g == 0.0 || j == n {
        return Ok(Value::from(low.widened()));
    }
    let (argument, high, _) = sorted[j];
    let failed = arithmetic(argument);
    let difference = binary(BinaryOperator::Subtract, high, low, expression).map_err(&failed)?;
    let step = binary(
        BinaryOperator::Multiply,
        Number::float(g),
        difference,
        expression,
    )
    .map_err(&failed)?;
    binary(BinaryOperator::Add, low, step, expression)
        .map(|result| Value::from(result.widened()))
        .map_err(failed)
}

/// The sample variance, with divisor n - 1, and with `root` its square root
/// (FORMULA-19). The first pass is the ordered `MEAN` of FORMULA-19, which
/// checks each value's type and fails on its sum's int overflow as `MEAN`
/// does. The second adds, in the order given, the first squared deviation
/// (x - mean) * (x - mean) and then each later one, with the numeric `-`,
/// `*` and `+`, and divides the total by n - 1 with the numeric `/`.
/// Fewer than two present values give missing.
fn spread(
    values: &[ValueRef<'_>],
    root: bool,
    expression: &str,
) -> Result<Value, AggregateReductionError> {
    let mean = reduce_numeric(values, NumericReducer::Mean, expression)
        .map_err(AggregateReductionError::Numeric)?;
    let mut total = None;
    let mut count = 0_i64;
    let mut last = 0;
    for (argument, value) in values.iter().copied().enumerate() {
        let Some(present) = number(argument, value)? else {
            continue;
        };
        let failed = arithmetic(argument);
        let deviation =
            binary(BinaryOperator::Subtract, present, mean, expression).map_err(&failed)?;
        let square =
            binary(BinaryOperator::Multiply, deviation, deviation, expression).map_err(&failed)?;
        total = Some(match total {
            None => square,
            Some(previous) => {
                binary(BinaryOperator::Add, previous, square, expression).map_err(&failed)?
            }
        });
        count += 1;
        last = argument;
    }
    let Some(total) = total.filter(|_| count >= 2) else {
        return Ok(Value::Missing);
    };
    let failed = arithmetic(last);
    let variance = binary(
        BinaryOperator::Divide,
        total,
        Number::Int(count - 1),
        expression,
    )
    .map_err(&failed)?;
    if !root {
        return Ok(Value::from(variance));
    }
    sqrt(variance, expression).map(Value::from).map_err(failed)
}

/// `EXP` of the ordered mean of `LN(x)` (FORMULA-19): in the order given,
/// the first logarithm plus each later one with the numeric `+`, divided
/// by their count with the numeric `/`, under the portable `LN` and `EXP`
/// of FORMULA-13. A zero or negative value fails as `LN` does, at its
/// position (FORMULA-15).
fn geometric_mean(
    values: &[ValueRef<'_>],
    expression: &str,
) -> Result<Value, AggregateReductionError> {
    let mut total = None;
    let mut count = 0_i64;
    let mut last = 0;
    for (argument, value) in values.iter().copied().enumerate() {
        let Some(present) = number(argument, value)? else {
            continue;
        };
        let failed = arithmetic(argument);
        let logarithm = math(MathFunction::Ln, present, expression)
            .map_err(|error| failed(within(Reducer::GeoMean, error)))?;
        total = Some(match total {
            None => logarithm,
            Some(previous) => {
                binary(BinaryOperator::Add, previous, logarithm, expression).map_err(&failed)?
            }
        });
        count += 1;
        last = argument;
    }
    let Some(total) = total else {
        return Ok(Value::Missing);
    };
    let failed = arithmetic(last);
    let mean = binary(
        BinaryOperator::Divide,
        total,
        Number::Int(count),
        expression,
    )
    .map_err(&failed)?;
    math(MathFunction::Exp, mean, expression)
        .map(Value::from)
        .map_err(failed)
}

/// Join the present values in the order given, each as the text a `str`
/// column converts it to (VALUE-3: decimal text, the shortest positional
/// text of VALUE-8, the canonical text of VALUE-17, or text as it is), with
/// the separator between two of them. With `distinct`, each text is written
/// once, where it first occurs. A group with no present value joins to
/// missing, never to the empty string (FORMULA-24). Every value is written
/// as text first, and a joined text longer than `Limits::TEXT_BYTES` is
/// refused before it is built.
fn join(
    values: &[ValueRef<'_>],
    fields: ReductionFields<'_>,
) -> Result<Value, AggregateReductionError> {
    let mut texts = Vec::with_capacity(values.len());
    for (argument, value) in values.iter().copied().enumerate() {
        texts.push(match value {
            ValueRef::Missing => continue,
            ValueRef::Str(text) => Cow::Borrowed(text),
            other => match convert(&owned(other), ColumnType::Str) {
                Ok(Value::Str(text)) => Cow::Owned(text),
                Ok(_) => unreachable!("a value converts to str as text"),
                Err(error) => return Err(AggregateReductionError::Text { argument, error }),
            },
        });
    }
    let mut seen = HashSet::new();
    let written: Vec<&str> = texts
        .iter()
        .map(|text| text.as_ref())
        .filter(|text| !fields.distinct || seen.insert(*text))
        .collect();
    let Some((first, rest)) = written.split_first() else {
        return Ok(Value::Missing);
    };
    let total = rest.iter().try_fold(first.len(), |total, text| {
        total
            .checked_add(fields.separator.len())?
            .checked_add(text.len())
    });
    Limits::text_bytes(total).map_err(AggregateReductionError::Limit)?;
    let mut joined = String::with_capacity(total.expect("an admitted length"));
    joined.push_str(first);
    for text in rest {
        joined.push_str(fields.separator);
        joined.push_str(text);
    }
    Ok(Value::Str(joined))
}
