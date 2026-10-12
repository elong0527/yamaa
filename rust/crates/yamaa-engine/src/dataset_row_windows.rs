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
        let mut run = if let Expression::Window(window) = &assignment.expression {
            Some(windows::Run::new(
                window,
                candidates,
                plan,
                limits,
                state.budget,
            )?)
        } else {
            None
        };
        for row in 0..candidates.len() {
            let value = if let (Some(run), Expression::Window(window)) =
                (&mut run, &assignment.expression)
            {
                let value = run.value(
                    row,
                    window,
                    candidates,
                    table,
                    state.budget,
                    &windows::Context { assignment, plan },
                )?;
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
                    state,
                )?
            };
            candidates[row].values[assignment.column] = value;
            candidates[row].completed[assignment.column] = true;
        }
    }
    Ok(())
}
