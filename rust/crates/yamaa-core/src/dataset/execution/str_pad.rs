//! One named read, shared canonical conversion and an admitted padding copy.
use super::*;
use crate::text_pad::StringPad;

#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    source: &Assignment,
    operation: &StringPad,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let value = evaluate_raw(table, source, candidate, plan, row, state)?;
    render(&value, operation, operation.path(), candidate, plan, row)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render<E>(
    value: &Value,
    operation: &StringPad,
    path: &str,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
) -> Result<Value, Box<ExecutionError<E>>> {
    let prepared = operation.prepare_text(value).map_err(|error| {
        retained_diagnostic(
            error.into_diagnostic(path.into()),
            true,
            candidate,
            plan,
            row,
        )
    })?;
    // This can be intermediate text; the saved-file byte boundary belongs to the writer.
    prepared
        .render()
        .map_err(|_| Box::new(ExecutionError::Allocation))
}
