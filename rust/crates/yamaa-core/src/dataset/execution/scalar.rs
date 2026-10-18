//! Scalar evaluation retains authored reads, missing-handler counts and raw values.
use super::*;
use crate::scalar_language;
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    assignment: &Assignment,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    match &assignment.expression {
        Expression::Extreme { operands, largest } => {
            let mut selected = Value::Missing;
            for source in operands {
                let value = evaluate_raw(table, source, candidate, plan, row, state)?;
                if scalar_language::replace_extreme(&selected, &value, *largest)
                    .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?
                {
                    selected = value;
                }
            }
            Ok(selected)
        }
        _ => unreachable!("scalar dispatch"),
    }
}
