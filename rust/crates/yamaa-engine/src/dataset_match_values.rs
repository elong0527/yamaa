//! Current dependency resolution precedes donor filtering for authored expressions.
use super::*;

type KeyValues<E> = Result<Vec<(usize, Value)>, Box<ExecutionError<E>>>;

pub(super) struct Current<'a> {
    values: Vec<(&'a MatchRead, Value)>,
}
impl Current<'_> {
    fn value(&self, read: &MatchRead) -> &Value {
        &self
            .values
            .iter()
            .find(|(bound, _)| bound.identifier() == read.identifier())
            .expect("all actual references resolved once")
            .1
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_one<T: TableAccess + ?Sized>(
    read: &MatchRead,
    item: &Intermediate,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    match read {
        MatchRead::Output { column, .. } => {
            let value = &candidate.values[*column];
            if let Value::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            Ok(value.clone())
        }
        MatchRead::Record { column, identifier } => {
            state.budget.work(candidate.members.len(), 1)?;
            key_grain::collect_bound(
                primary,
                key_grain::Collection {
                    column: *column,
                    identifier,
                    filter: None,
                    selection: None,
                },
                key_grain::CollectionContext {
                    path: &item.path,
                    candidate,
                    plan,
                    row,
                },
                state.budget,
                state.handlers,
            )
        }
    }
}

/// Reference order: ordinary match reads, then expression references; first use wins.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve<'a, T: TableAccess + ?Sized>(
    item: &'a Intermediate,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Current<'a>, Box<ExecutionError<T::Error>>> {
    state.budget.work(1, item.keys.len())?;
    let mut values: Vec<(&MatchRead, Value)> = Vec::new();
    values
        .try_reserve_exact(item.keys.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for expression in [false, true] {
        for key in &item.keys {
            if key.value.is_expression() != expression {
                continue;
            }
            if let Some(read) = key.value.read() {
                if values
                    .iter()
                    .any(|(bound, _)| bound.identifier() == read.identifier())
                {
                    continue;
                }
                state.budget.work(1, 1)?;
                values.push((
                    read,
                    resolve_one(read, item, candidate, plan, row, primary, state)?,
                ));
            }
        }
    }
    Ok(Current { values })
}

/// Ordinary selectors retain their established output-then-record traversal.
#[allow(clippy::too_many_arguments)]
pub(super) fn ordinary<T: TableAccess + ?Sized>(
    item: &Intermediate,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    state: &mut EvaluationState<'_, T::Error>,
) -> KeyValues<T::Error> {
    state.budget.work(1, item.keys.len())?;
    let mut keys = Vec::new();
    keys.try_reserve_exact(item.keys.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for record in [false, true] {
        for key in &item.keys {
            let MatchValue::Variable(read) = &key.value else {
                unreachable!("ordinary selector contains variables only");
            };
            if matches!(read, MatchRead::Record { .. }) == record {
                keys.push((
                    key.source_column,
                    resolve_one(read, item, candidate, plan, row, primary, state)?,
                ));
            }
        }
    }
    Ok(keys)
}

/// Evaluate every key before considering missingness; never finish an output assignment.
pub(super) fn evaluate<E>(
    item: &Intermediate,
    current: &Current<'_>,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, E>,
) -> KeyValues<E> {
    state.budget.work(1, item.keys.len())?;
    let mut keys = Vec::new();
    keys.try_reserve_exact(item.keys.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for key in &item.keys {
        state.budget.work(1, 1)?;
        let value = match &key.value {
            MatchValue::Variable(read) | MatchValue::Source(read) => {
                clone_value(current.value(read), state.budget)?
            }
            MatchValue::Literal(value) => clone_value(value, state.budget)?,
            MatchValue::StrPad { read, operation } => str_pad::render(
                current.value(read),
                operation.as_ref().expect("padding declaration admitted"),
                &key.path,
                candidate,
                plan,
                row,
                state.budget,
            )?,
        };
        keys.push((key.source_column, value));
    }
    Ok(keys)
}

fn clone_value<E>(value: &Value, budget: &mut Budget) -> Result<Value, Box<ExecutionError<E>>> {
    if let Value::Str(text) = value {
        budget.scalar_text(text.len())?;
    }
    Ok(value.clone())
}
