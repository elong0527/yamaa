//! Complete template-local windows and their scalar readers using the shared evaluator.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn complete<T: TableAccess + ?Sized>(
    table: &T,
    template: &crate::dataset::RowTemplate,
    deferred: &[bool],
    candidates: &mut [Candidate],
    plan: &DatasetPlan,
    offset: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<(), Box<ExecutionError<T::Error>>> {
    for assignment in template
        .assignments
        .iter()
        .filter(|assignment| deferred[assignment.column])
    {
        let mut nested_windows = windows::Cache::default();
        let mut run =
            windows::Evaluation::new(assignment, candidates, plan, table, state.handlers)?;
        for row in 0..candidates.len() {
            let value = if let Some(run) = &mut run {
                let value = run.value(row, candidates, table, assignment, plan)?;
                finish(
                    value,
                    assignment,
                    &candidates[row],
                    plan,
                    offset + row,
                    state.handlers,
                )?
            } else {
                evaluate(
                    table,
                    assignment,
                    &candidates[row],
                    plan,
                    offset + row,
                    &mut EvaluationState {
                        primary: state.primary,
                        functions: state.functions,
                        secondary: state.secondary,
                        self_rows: state.self_rows,
                        handlers: state.handlers,
                        intermediates: state.intermediates,
                        output_rows: candidates,
                        output_offset: offset,
                        windows: &mut nested_windows,
                        aggregate_row: None,
                    },
                )?
            };
            candidates[row].values[assignment.column] = value;
            candidates[row].completed[assignment.column] = true;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
