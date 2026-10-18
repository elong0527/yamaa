//! Case substitution has one named read and one admitted byte-preserving copy.
//! A missing source is counted under `missing`, declared or by default.
use super::*;
use crate::text_case::{Error, Step};

pub(super) fn register(expression: &Expression, counter: &mut HandlerCounter) {
    match expression {
        Expression::StrCase { source, operation } => {
            counter.register(
                &std::format!("{}.missing", operation.path()),
                HandlerKind::Missing,
            );
            register(&source.expression, counter);
        }
        Expression::Case(branches) => {
            for branch in branches {
                register(&branch.assignment.expression, counter);
            }
        }
        Expression::Extreme { operands, .. }
        | Expression::Imputation { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. } => {
            for source in operands {
                register(&source.expression, counter);
            }
        }
        Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::Round { source, .. } => register(&source.expression, counter),
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
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let source = evaluate_raw(table, source, candidate, plan, row, state)?;
    match operation.apply(ValueRef::from(&source)) {
        Ok(Step::Value(value)) => Ok(value),
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
        // Only a source of another type fails, a validation finding without keys.
        Err(Error::Diagnostic(diagnostic)) => {
            Err(retained_diagnostic(diagnostic, false, candidate, plan, row))
        }
    }
}
