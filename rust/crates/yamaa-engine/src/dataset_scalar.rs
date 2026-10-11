//! Scalar evaluation retains authored reads, missing-handler counts and raw values.
use super::*;
use yamaa_core::{
    scalar_language,
    text_extract::{Error, Step},
};
pub(super) fn register(assignment: &Assignment, counter: &mut HandlerCounter) {
    match &assignment.expression {
        Expression::SourceLookup {
            selection: Some(_), ..
        } => {
            counter.register(
                &alloc::format!("{}.multiple_matches", assignment.path),
                HandlerKind::MultipleMatches,
            );
        }
        Expression::SourceBinding {
            source,
            path,
            absent,
            ..
        } => {
            if absent.is_some() {
                counter.register(&alloc::format!("{path}.absent"), HandlerKind::Absent);
            }
            if matches!(
                &source.expression,
                Expression::Collect {
                    selection: Some(_),
                    ..
                }
            ) {
                counter.register(
                    &alloc::format!("{}.multiple_matches", source.path),
                    HandlerKind::MultipleMatches,
                );
            }
            register(source, counter);
        }
        Expression::FirstAvailable(selection) => {
            for source in selection.sources() {
                if let yamaa_core::dataset::SelectionRead::Expression(source) = &source.read {
                    register(source, counter);
                }
            }
        }
        Expression::StrContains { source, operation } => {
            if operation.handler("missing").is_some() {
                counter.register(
                    &alloc::format!("{}.missing", operation.path()),
                    HandlerKind::Missing,
                );
            }
            register(source, counter);
        }
        Expression::StrConcat {
            operands,
            missing,
            path,
        } => {
            if missing.is_some() {
                counter.register(&alloc::format!("{path}.missing"), HandlerKind::Missing);
            }
            for source in operands {
                register(source, counter);
            }
        }
        Expression::Extreme { operands, .. }
        | Expression::Imputation { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::StrTemplate { operands, .. } => {
            for source in operands {
                register(source, counter);
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                register(&branch.assignment, counter);
            }
        }
        Expression::StrCase { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::Round { source, .. } => register(source, counter),
        _ => (),
    }
}
fn recover<T: TableAccess + ?Sized>(
    value: &Value,
    path: &str,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    if let Value::Str(text) = value {
        state.budget.scalar_text(text.len())?;
    }
    state
        .handlers
        .record(&alloc::format!("{path}.missing"), HandlerKind::Missing)
        .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
    Ok(value.clone())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    assignment: &Assignment,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    state.budget.work(1, 1)?;
    match &assignment.expression {
        Expression::Extreme { operands, largest } => {
            let mut selected = Value::Missing;
            for source in operands {
                state.budget.work(1, 1)?;
                let value = evaluate_raw(table, source, candidate, plan, row, limits, state)?;
                if scalar_language::replace_extreme(&selected, &value, *largest)
                    .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?
                {
                    selected = value;
                }
            }
            Ok(selected)
        }
        Expression::StrConcat {
            operands,
            missing,
            path,
        } => {
            let mut rendered = String::new();
            for (index, source) in operands.iter().enumerate() {
                let value = evaluate_raw(table, source, candidate, plan, row, limits, state)?;
                if matches!(value, Value::Missing) {
                    if let Some(value) = missing {
                        return recover::<T>(value, path, state);
                    }
                }
                if !matches!(&value, Value::Str(_)) {
                    state.budget.scalar_text(path.len() + 64)?;
                }
                if let Some(diagnostic) = scalar_language::concat_diagnostic(path, index, &value) {
                    let keys = diagnostic.definition().phase == "mapping";
                    return Err(retained_diagnostic(
                        diagnostic,
                        keys,
                        candidate,
                        plan,
                        row,
                        state.budget,
                    ));
                }
                let Value::Str(text) = value else {
                    unreachable!()
                };
                // Charge the complete transient result before every allocation.
                state.budget.scalar_text(text.len())?;
                state.budget.work(text.len(), 1)?;
                rendered.try_reserve(text.len()).map_err(|_| {
                    Box::new(ExecutionError::Limit {
                        resource: Resource::ScalarTextBytes,
                        limit: limits.scalar_text_bytes,
                        required: None,
                    })
                })?;
                rendered.push_str(&text);
            }
            Ok(Value::Str(rendered))
        }
        Expression::StrContains { source, operation } => {
            let value = evaluate_raw(table, source, candidate, plan, row, limits, state)?;
            let matching = yamaa_core::regex::MatchLimits {
                subject_bytes: limits.scalar_text_bytes,
                work: limits.work_cells,
                state_cells: limits.work_cells,
            };
            match operation.apply(ValueRef::from(&value), matching, state.budget.predicate()) {
                Ok(Step::Capture(_)) => Ok(Value::Bool(true)),
                Ok(Step::NoMatch(_)) => Ok(Value::Bool(false)),
                Ok(Step::Recovered(_)) => recover::<T>(
                    operation
                        .handler("missing")
                        .expect("declared missing handler"),
                    operation.path(),
                    state,
                ),
                Ok(Step::Missing) => {
                    state
                        .budget
                        .scalar_text(operation.path().len() + operation.source().len() + 512)?;
                    let diagnostic = operation
                        .failure_diagnostic(
                            Step::Missing,
                            yamaa_core::text_extract::MAX_DECLARATION_BYTES,
                        )
                        .expect("admitted metadata");
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
        _ => unreachable!("scalar dispatch"),
    }
}
