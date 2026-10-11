//! Extraction shares regex counters and admits captured/context copies before retention.
use super::*;
use yamaa_core::text_extract::{Error, Step, MAX_DECLARATION_BYTES};

pub(super) fn register(expression: &Expression, counter: &mut HandlerCounter) {
    match expression {
        Expression::StrExtract { source, operation } => {
            for (name, kind) in [
                ("missing", HandlerKind::Missing),
                ("no_match", HandlerKind::NoMatch),
            ] {
                if operation.handler(name).is_some() {
                    counter.register(&alloc::format!("{}.{}", operation.path(), name), kind);
                }
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
        Expression::StrCase { source, .. } | Expression::Round { source, .. } => {
            register(&source.expression, counter)
        }
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
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    state.budget.work(1, 1)?;
    let source = evaluate_raw(table, source, candidate, plan, row, limits, state)?;
    let matching = yamaa_core::regex::MatchLimits {
        subject_bytes: limits.scalar_text_bytes,
        work: limits.work_cells,
        state_cells: limits.work_cells,
    };
    let step = operation.apply(ValueRef::from(&source), matching, state.budget.predicate());
    match step {
        Ok(Step::Capture(Some(text))) => {
            state.budget.scalar_text(text.len())?;
            Ok(Value::Str(text.into()))
        }
        Ok(Step::Capture(None)) => Ok(Value::Missing),
        Ok(Step::Recovered(name)) => {
            let value = operation.handler(name).expect("declared recovery");
            if let Value::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            state
                .handlers
                .record(
                    &alloc::format!("{}.{}", operation.path(), name),
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
            let required = operation.failure_text_bytes(step);
            let Some(bytes) = required.filter(|n| *n <= MAX_DECLARATION_BYTES) else {
                return Err(Box::new(ExecutionError::Limit {
                    resource: Resource::ScalarTextBytes,
                    limit: MAX_DECLARATION_BYTES,
                    required,
                }));
            };
            state.budget.scalar_text(bytes)?;
            let diagnostic = operation
                .failure_diagnostic(step, MAX_DECLARATION_BYTES)
                .expect("admitted complete context");
            Err(retained_diagnostic(
                diagnostic,
                true,
                candidate,
                plan,
                row,
                state.budget,
            ))
        }
        Err(Error::Diagnostic(diagnostic)) => Err(retained_diagnostic(
            diagnostic,
            false,
            candidate,
            plan,
            row,
            state.budget,
        )),
        Err(Error::Limit(limit)) => Err(Box::new(predicate_limit(limit))),
    }
}
