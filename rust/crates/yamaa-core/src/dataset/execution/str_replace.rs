//! Replacement scans the whole value under one step guard, then builds its result.
use super::*;
use crate::text_replace::{Error, RenderError, Step};

pub(super) fn register(expression: &Expression, counter: &mut HandlerCounter) {
    match expression {
        Expression::StrReplace { source, operation } => {
            if operation.missing().is_some() {
                counter.register(
                    &std::format!("{}.missing", operation.path()),
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
        Expression::Extreme { operands, .. }
        | Expression::Imputation { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. } => {
            for source in operands {
                register(&source.expression, counter);
            }
        }
        Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::Round { source, .. } => register(&source.expression, counter),
        _ => (),
    }
}
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
            let value = operation.missing().expect("declared missing handler");
            state
                .handlers
                .record(
                    &std::format!("{}.missing", operation.path()),
                    HandlerKind::Missing,
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            Ok(value.clone())
        }
        Ok(Step::Missing) => {
            let diagnostic = operation
                .missing_diagnostic()
                .expect("admitted complete context");
            Err(retained_diagnostic(diagnostic, true, candidate, plan, row))
        }
        Err(Error::Diagnostic(diagnostic)) => {
            Err(retained_diagnostic(diagnostic, false, candidate, plan, row))
        }
    }
}
