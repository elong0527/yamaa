//! Extraction shares regex counters and admits captured/context copies before retention.
//! A missing source is counted under `missing`, declared or by default.
use super::*;
use crate::text_extract::{Error, Step};

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
            Ok(operation.substitute(name))
        }
        Ok(step @ Step::NoMatch(_)) => {
            let diagnostic = operation
                .failure_diagnostic(step)
                .expect("admitted complete context");
            Err(retained_diagnostic(diagnostic, true, candidate, plan, row))
        }
        Err(Error::Diagnostic(diagnostic)) => {
            Err(retained_diagnostic(diagnostic, false, candidate, plan, row))
        }
    }
}
