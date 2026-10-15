//! Extraction shares regex counters and admits captured/context copies before retention.
use super::*;
use crate::text_extract::{Error, Step};

pub(super) fn register(expression: &Expression, counter: &mut HandlerCounter) {
    match expression {
        Expression::StrExtract { source, operation } => {
            for (name, kind) in [
                ("missing", HandlerKind::Missing),
                ("no_match", HandlerKind::NoMatch),
            ] {
                if operation.handler(name).is_some() {
                    counter.register(&std::format!("{}.{}", operation.path(), name), kind);
                }
            }
            register(&source.expression, counter);
        }
        Expression::Case(branches) => {
            for branch in branches {
                register(&branch.assignment.expression, counter);
            }
        }
        Expression::Extreme { operands, .. }
        | Expression::StrConcat { operands, .. }
        | Expression::Imputation { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. } => {
            for source in operands {
                register(&source.expression, counter);
            }
        }
        Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::Round { source, .. } => register(&source.expression, counter),
        _ => (),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    source: &Assignment,
    operation: &StringExtract,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let source = evaluate_raw(table, source, candidate, plan, row, state)?;
    let step = operation.apply(ValueRef::from(&source));
    match step {
        Ok(Step::Capture(Some(text))) => Ok(Value::Str(text.into())),
        Ok(Step::Capture(None)) => Ok(Value::Missing),
        Ok(Step::Recovered(name)) => {
            let value = operation.handler(name).expect("declared recovery");
            state
                .handlers
                .record(
                    &std::format!("{}.{}", operation.path(), name),
                    if name == "missing" {
                        HandlerKind::Missing
                    } else {
                        HandlerKind::NoMatch
                    },
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            Ok(value.clone())
        }
        Ok(step @ (Step::Missing | Step::NoMatch(_))) => {
            let diagnostic = operation
                .failure_diagnostic(step)
                .expect("admitted complete context");
            Err(retained_diagnostic(diagnostic, true, candidate, plan, row))
        }
        Err(Error::Diagnostic(diagnostic)) => {
            Err(retained_diagnostic(diagnostic, false, candidate, plan, row))
        }
        Err(Error::Regex(limit)) => Err(Box::new(regex_limit(limit))),
    }
}
