//! Imputation reads its bounds and fallback only after a source needs them,
//! and counts default substitutions and crossed bounds like declared handlers.
use super::*;
use crate::dataset::ImputationStep;
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    operands: &[Assignment],
    operation: &Imputation,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let count = |state: &mut EvaluationState<'_, T::Error>, field: &str, kind| {
        state
            .handlers
            .record(&std::format!("{}.{field}", operation.path()), kind)
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))
    };
    let source = evaluate_raw(table, &operands[0], candidate, plan, row, state)?;
    let result = match operation.source_value(ValueRef::from(&source)) {
        Ok(ImputationStep::Value(value)) => Ok(value),
        Ok(ImputationStep::Recovered { handler }) => {
            count(state, handler, HandlerKind::Invalid)?;
            Ok(operation
                .handler(handler)
                .expect("declared recovery")
                .clone())
        }
        Ok(ImputationStep::Missing) => match operation.fallback() {
            Some(_) => {
                count(state, "fallback", HandlerKind::Missing)?;
                let fallback = operands.last().expect("declared fallback operand");
                let value = evaluate_raw(table, fallback, candidate, plan, row, state)?;
                operation.fallback_value(ValueRef::from(&value))
            }
            None => {
                count(state, "missing", HandlerKind::Missing)?;
                Ok(Value::Missing)
            }
        },
        Ok(ImputationStep::Partial) => {
            count(state, "partial", HandlerKind::Partial)?;
            Ok(Value::Missing)
        }
        Ok(ImputationStep::Bound(pending)) => {
            let (mut not_before, mut not_after) = (Value::Missing, Value::Missing);
            for ((field, _), operand) in operation.bounds().zip(&operands[1..]) {
                let value = evaluate_raw(table, operand, candidate, plan, row, state)?;
                match field {
                    "not_before" => not_before = value,
                    _ => not_after = value,
                }
            }
            match operation.apply_bounds(
                pending,
                ValueRef::from(&not_before),
                ValueRef::from(&not_after),
            ) {
                Ok((value, crossed)) => {
                    if crossed {
                        count(state, "not_after", HandlerKind::CrossedBounds)?;
                    }
                    Ok(value)
                }
                Err(diagnostic) => Err(diagnostic),
            }
        }
        Err(diagnostic) => Err(diagnostic),
    };
    result.map_err(|diagnostic| {
        let retain_keys = diagnostic.definition().phase == "impute";
        retained_diagnostic(diagnostic, retain_keys, candidate, plan, row)
    })
}
