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
    match &value {
        Value::Missing => (),
        Value::Str(text) => state.budget.work(1, text.len())?,
        _ => state.budget.work(1, MAX_PRIMITIVE_TEXT_BYTES)?,
    }
    let prepared = operation.prepare_text(&value).map_err(|error| {
        retained_diagnostic(
            error.into_diagnostic(operation.path().into()),
            true,
            candidate,
            plan,
            row,
            state.budget,
        )
    })?;
    let Some(required) = prepared.required_bytes() else {
        return Err(Box::new(ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: limits.scalar_text_bytes,
            required: None,
        }));
    };
    state.budget.work(1, required)?;
    state.budget.scalar_text(required)?;
    prepared.render(limits.scalar_text_bytes).map_err(|error| {
        Box::new(ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: limits.scalar_text_bytes,
            required: error.required,
        })
    })
}
