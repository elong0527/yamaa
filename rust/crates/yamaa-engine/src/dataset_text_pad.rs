//! One named read, shared canonical conversion and an admitted padding copy.
use super::*;
use yamaa_core::text_pad::{StringPad, MAX_PRIMITIVE_TEXT_BYTES};

#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    source: &Assignment,
    operation: &StringPad,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    state.budget.work(1, 1)?;
    let value = evaluate_raw(table, source, candidate, plan, row, limits, state)?;
    render(
        &value,
        operation,
        operation.path(),
        candidate,
        plan,
        row,
        state.budget,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render<E>(
    value: &Value,
    operation: &StringPad,
    path: &str,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
) -> Result<Value, Box<ExecutionError<E>>> {
    match value {
        Value::Missing => (),
        Value::Str(text) => budget.work(1, text.len())?,
        _ => budget.work(1, MAX_PRIMITIVE_TEXT_BYTES)?,
    }
    let prepared = operation.prepare_text(value).map_err(|error| {
        retained_diagnostic(
            error.into_diagnostic(path.into()),
            true,
            candidate,
            plan,
            row,
            budget,
        )
    })?;
    let Some(required) = prepared.required_bytes() else {
        return Err(Box::new(ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: budget.scalar_text_limit(),
            required: None,
        }));
    };
    budget.work(1, required)?;
    budget.scalar_text(required)?;
    prepared
        .render(budget.scalar_text_limit())
        .map_err(|error| {
            Box::new(ExecutionError::Limit {
                resource: Resource::ScalarTextBytes,
                limit: budget.scalar_text_limit(),
                required: error.required,
            })
        })
}
