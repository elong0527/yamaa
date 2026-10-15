//! Current dependency resolution precedes donor filtering for authored expressions.
use super::*;

type KeyValues<E> = Result<Vec<(usize, Value)>, Box<ExecutionError<E>>>;

#[derive(Clone, Copy)]
pub(crate) struct Request<'a> {
    path: &'a str,
    keys: &'a [IntermediateKey],
}
impl<'a> Request<'a> {
    pub(crate) fn new(path: &'a str, keys: &'a [IntermediateKey]) -> Self {
        Self { path, keys }
    }
}
impl<'a> From<&'a Intermediate> for Request<'a> {
    fn from(item: &'a Intermediate) -> Self {
        Self::new(&item.path, &item.keys)
    }
}

pub(crate) struct Current<'a> {
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
    item: &Request<'_>,
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
        MatchRead::Record { column, identifier }
        | MatchRead::SecondaryRecord {
            column, identifier, ..
        } => {
            if let MatchRead::SecondaryRecord { source, .. } = read {
                let current = candidate
                    .template
                    .and_then(|index| plan.templates().get(index))
                    .map_or(0, |template| template.mode.source_index());
                if source + 1 != current {
                    return Err(Box::new(ExecutionError::SchemaMismatch));
                }
            }
            let current = match read {
                MatchRead::SecondaryRecord { source, .. } => row_drivers::Driver::Secondary(
                    *state
                        .secondary
                        .get(*source)
                        .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?,
                ),
                _ => row_drivers::Driver::Primary(primary),
            };
            state.budget.work(candidate.members.len(), 1)?;
            key_grain::collect_bound(
                &current,
                key_grain::Collection {
                    column: *column,
                    identifier,
                    filter: None,
                    selection: None,
                },
                key_grain::CollectionContext {
                    path: item.path,
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
pub(crate) fn resolve<'a, T: TableAccess + ?Sized>(
    item: impl Into<Request<'a>>,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Current<'a>, Box<ExecutionError<T::Error>>> {
    let item = item.into();
    state.budget.work(1, item.keys.len())?;
    let operands = item
        .keys
        .iter()
        .try_fold(0usize, |count, key| {
            count.checked_add(key.value.reads().count())
        })
        .ok_or_else(|| Box::new(ExecutionError::Capacity))?;
    state.budget.work(1, operands)?;
    let mut values: Vec<(&MatchRead, Value)> = Vec::new();
    values
        .try_reserve_exact(operands)
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for expression in [false, true] {
        for key in item.keys {
            if key.value.is_expression() != expression {
                continue;
            }
            for read in key.value.reads() {
                if values
                    .iter()
                    .any(|(bound, _)| bound.identifier() == read.identifier())
                {
                    continue;
                }
                state.budget.work(1, 1)?;
                values.push((
                    read,
                    resolve_one(read, &item, candidate, plan, row, primary, state)?,
                ));
            }
        }
    }
    Ok(Current { values })
}

/// Ordinary selectors retain their established output-then-record traversal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ordinary<'a, T: TableAccess + ?Sized>(
    item: impl Into<Request<'a>>,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    state: &mut EvaluationState<'_, T::Error>,
) -> KeyValues<T::Error> {
    let item = item.into();
    state.budget.work(1, item.keys.len())?;
    let mut keys = Vec::new();
    keys.try_reserve_exact(item.keys.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for record in [false, true] {
        for key in item.keys {
            let MatchValue::Variable(read) = &key.value else {
                unreachable!("ordinary selector contains variables only");
            };
            if matches!(
                read,
                MatchRead::Record { .. } | MatchRead::SecondaryRecord { .. }
            ) == record
            {
                keys.push((
                    key.source_column,
                    resolve_one(read, &item, candidate, plan, row, primary, state)?,
                ));
            }
        }
    }
    Ok(keys)
}

/// Evaluate every key before considering missingness; never finish an output assignment.
#[allow(clippy::too_many_arguments)]
pub(crate) fn evaluate<'a, T: TableAccess + ?Sized>(
    item: impl Into<Request<'a>>,
    current: &Current<'_>,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    primary: &T,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
) -> KeyValues<T::Error> {
    let item = item.into();
    state.budget.work(1, item.keys.len())?;
    let mut keys = Vec::new();
    keys.try_reserve_exact(item.keys.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for key in item.keys {
        state.budget.work(1, 1)?;
        let value = match &key.value {
            MatchValue::Expression(expression) => {
                if expression.windowed {
                    evaluate_raw(
                        primary,
                        &expression.assignment,
                        candidate,
                        plan,
                        row,
                        limits,
                        state,
                    )?
                } else {
                    // All authored current references were resolved with their
                    // actual source membership. The scalar evaluator now sees
                    // one current record, without reusing any physical driver
                    // coordinate as an operand-table coordinate.
                    state.budget.work(candidate.values.len(), 2)?;
                    let mut values = Vec::new();
                    values
                        .try_reserve_exact(candidate.values.len())
                        .map_err(|_| Box::new(ExecutionError::Allocation))?;
                    for value in &candidate.values {
                        values.push(clone_value(value, state.budget)?);
                    }
                    let mut completed = Vec::new();
                    completed
                        .try_reserve_exact(candidate.completed.len())
                        .map_err(|_| Box::new(ExecutionError::Allocation))?;
                    completed.extend_from_slice(&candidate.completed);
                    let scalar = Candidate {
                        template: candidate.template,
                        selection_identity: candidate.selection_identity,
                        members: vec![0],
                        values,
                        completed,
                    };
                    let snapshot = Operands::<T::Error> {
                        expression,
                        current,
                        marker: std::marker::PhantomData,
                    };
                    evaluate_raw(
                        &snapshot,
                        &expression.assignment,
                        &scalar,
                        plan,
                        row,
                        limits,
                        state,
                    )?
                }
            }
            MatchValue::Contextual(_) => return Err(Box::new(ExecutionError::SchemaMismatch)),
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

/// Resolved current operands are an immutable scalar snapshot, not a donor port.
struct Operands<'a, E> {
    expression: &'a crate::dataset::MatchExpression,
    current: &'a Current<'a>,
    marker: std::marker::PhantomData<E>,
}
impl<E> TableAccess for Operands<'_, E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        &self.expression.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        if row != 0 {
            return Err(CellError::OutOfBounds { row, column });
        }
        self.expression
            .reads
            .get(column)
            .map(|read| ValueRef::from(self.current.value(read)))
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
