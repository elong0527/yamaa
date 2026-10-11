//! Case substitution has one named read and one admitted byte-preserving copy.
use super::*;
use yamaa_core::text_case::{Error, Step};

pub(super) fn register(expression: &Expression, counter: &mut HandlerCounter) {
    match expression {
        Expression::StrCase { source, operation } => {
            if operation.missing().is_some() {
                counter.register(
                    &alloc::format!("{}.missing", operation.path()),
                    HandlerKind::Missing,
                );
            }
            register(&source.expression, counter);
        }
        Expression::Case(branches) => {
            for branch in branches {
                register(&branch.assignment.expression, counter);
            }
        }
        Expression::Imputation { operands, .. } | Expression::Temporal { operands, .. } => {
            for source in operands {
                register(&source.expression, counter);
            }
        }
        Expression::Round { source, .. } | Expression::StrPad { source, .. } => {
            register(&source.expression, counter)
        }
        _ => (),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    source: &Assignment,
    operation: &StringCase,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    state.budget.work(1, 1)?;
    let source = evaluate_raw(table, source, candidate, plan, row, limits, state)?;
    if let Value::Str(text) = &source {
        state.budget.work(1, text.len())?;
        state.budget.scalar_text(text.len())?;
    }
    match operation.apply(ValueRef::from(&source), limits.scalar_text_bytes) {
        Ok(Step::Value(value)) => Ok(value),
        Ok(Step::Recovered) => {
            let value = operation.missing().expect("declared missing handler");
            if let Value::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            state
                .handlers
                .record(
                    &alloc::format!("{}.missing", operation.path()),
                    HandlerKind::Missing,
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            Ok(value.clone())
        }
        Err(Error::Diagnostic(diagnostic)) => {
            let retain_keys = diagnostic.definition().phase == "mapping";
            Err(retained_diagnostic(
                diagnostic,
                retain_keys,
                candidate,
                plan,
                row,
                state.budget,
            ))
        }
        Err(Error::Limit(limit)) => Err(Box::new(ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: limits.scalar_text_bytes,
            required: Some(limit.required),
        })),
    }
}
