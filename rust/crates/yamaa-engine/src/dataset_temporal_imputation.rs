//! Bounded imputation evaluates a lower bound only after a partial source needs it.
use super::*;
use yamaa_core::{dataset::ImputationStep, diagnostic::Diagnostic};
pub(super) fn register(expression: &Expression, counter: &mut HandlerCounter) {
    match expression {
        Expression::Imputation { operation, .. } => {
            for (name, kind) in [
                ("missing", HandlerKind::Missing),
                ("invalid", HandlerKind::Invalid),
            ] {
                if operation.handler(name).is_some() {
                    counter.register(&alloc::format!("{}.{}", operation.path(), name), kind);
                }
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                register(&branch.assignment.expression, counter)
            }
        }
        _ => (),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    operands: &[Assignment],
    operation: &Imputation,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    state.budget.work(1, 1)?;
    let source = evaluate_raw(table, &operands[0], candidate, plan, row, limits, state)?;
    if let Value::Str(text) = &source {
        state.budget.work(1, text.len())?;
        state.budget.scalar_text(text.len())?;
    }
    let result = operation.source_value(ValueRef::from(&source));
    let result = match result {
        Ok(step) => match step {
            ImputationStep::Value(value) => Ok(value),
            ImputationStep::Recovered { handler } => {
                let path = alloc::format!("{}.{}", operation.path(), handler);
                let value = operation.handler(handler).expect("declared recovery");
                if let Value::Str(text) = value {
                    state.budget.scalar_text(text.len())?;
                }
                state
                    .handlers
                    .record(
                        &path,
                        if handler == "missing" {
                            HandlerKind::Missing
                        } else {
                            HandlerKind::Invalid
                        },
                    )
                    .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
                Ok(value.clone())
            }
            ImputationStep::Bound(completed) => {
                let bound = evaluate_raw(table, &operands[1], candidate, plan, row, limits, state)?;
                operation.apply_bound(completed, ValueRef::from(&bound))
            }
        },
        Err(diagnostic) => Err(diagnostic),
    };
    result.map_err(|diagnostic| with_identity(diagnostic, candidate, plan, row, state.budget))
}
fn with_identity<E>(
    diagnostic: Diagnostic,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
) -> Box<ExecutionError<E>> {
    let identity = if diagnostic.definition().phase == "impute" {
        match failure_identity(candidate, plan.keys(), row, budget) {
            Ok(identity) => identity,
            Err(error) => return error,
        }
    } else {
        None
    };
    Box::new(ExecutionError::ImputationDiagnostic {
        diagnostic,
        identity,
    })
}
