//! Complete template-local windows and their scalar readers using the shared evaluator.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn complete<T: TableAccess + ?Sized>(
    table: &T,
    template: &yamaa_core::dataset::RowTemplate,
    deferred: &[bool],
    candidates: &mut [Candidate],
    plan: &DatasetPlan,
    offset: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<(), Box<ExecutionError<T::Error>>> {
    for assignment in template
        .assignments
        .iter()
        .filter(|assignment| deferred[assignment.column])
    {
        let mut nested_windows = windows::Cache::default();
        let mut run = windows::Evaluation::new(
            assignment,
            candidates,
            plan,
            table,
            limits,
            state.budget,
            state.handlers,
        )?;
        for row in 0..candidates.len() {
            let value = if let Some(run) = &mut run {
                let value = run.value(row, candidates, table, state.budget, assignment, plan)?;
                finish(
                    value,
                    assignment,
                    &candidates[row],
                    plan,
                    offset + row,
                    state.budget,
                    state.handlers,
                )?
            } else {
                evaluate(
                    table,
                    assignment,
                    &candidates[row],
                    plan,
                    offset + row,
                    limits,
                    &mut EvaluationState {
                        primary: state.primary,
                        functions: state.functions,
                        secondary: state.secondary,
                        self_rows: state.self_rows,
                        budget: state.budget,
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
