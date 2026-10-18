//! Replacement scans the whole value under one step guard, then builds its result.
//! A missing source is counted under `missing`, declared or by default.
use super::*;
use crate::text_replace::{Error, RenderError, Step};

#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    source: &Assignment,
    operation: &StringReplace,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let source = evaluate_raw(table, source, candidate, plan, row, state)?;
    match operation.apply(ValueRef::from(&source)) {
        Ok(Step::Matched { subject, spans }) => operation
            .render(subject, &spans)
            .map(Value::Str)
            .map_err(|error| {
                Box::new(match error {
                    RenderError::Size => ExecutionError::Capacity,
                    RenderError::Allocation => ExecutionError::Allocation,
                })
            }),
        Ok(Step::Recovered) => {
            state
                .handlers
                .record(
                    &std::format!("{}.missing", operation.path()),
                    HandlerKind::Missing,
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            Ok(operation.missing_substitute())
        }
        Err(Error::Diagnostic(diagnostic)) => {
            Err(retained_diagnostic(diagnostic, false, candidate, plan, row))
        }
    }
}
