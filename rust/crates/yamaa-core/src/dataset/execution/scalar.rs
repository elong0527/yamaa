//! Scalar evaluation retains authored reads, missing-handler counts and raw values.
use super::*;
use crate::{
    scalar_language,
    text_extract::{Error, Step},
};
pub(super) fn register(assignment: &Assignment, counter: &mut HandlerCounter) {
    match &assignment.expression {
        Expression::Aggregate(aggregate) => {
            if let crate::dataset::AggregateRelation::Matched { keys, .. } = aggregate.relation() {
                for key in keys {
                    if let MatchValue::Expression(expression) = &key.value {
                        register(&expression.assignment, counter);
                        imputation::register(&expression.assignment.expression, counter);
                        str_case::register(&expression.assignment.expression, counter);
                        str_extract::register(&expression.assignment.expression, counter);
                        str_replace::register(&expression.assignment.expression, counter);
                        str_template::register(&expression.assignment.expression, counter);
                    }
                }
            }
            for binding in aggregate.derivations() {
                if let Some(handler) = &binding.handler {
                    counter.register(&handler.spec_path, HandlerKind::Unconvertible);
                }
                register(&binding.assignment, counter);
                imputation::register(&binding.assignment.expression, counter);
                str_case::register(&binding.assignment.expression, counter);
                str_extract::register(&binding.assignment.expression, counter);
                str_replace::register(&binding.assignment.expression, counter);
                str_template::register(&binding.assignment.expression, counter);
            }
        }
        Expression::MappingCut { source, operation } => {
            if operation.missing().is_some() {
                counter.register(
                    &std::format!("{}.missing", operation.path()),
                    HandlerKind::Missing,
                );
            }
            if operation.unmapped().is_some() {
                counter.register(
                    &std::format!("{}.unmapped", operation.path()),
                    HandlerKind::Unmapped,
                );
            }
            register(source, counter);
        }
        Expression::SourceLookup {
            selection: Some(_), ..
        }
        | Expression::JoinedSource {
            selection: Some(_), ..
        } => {
            counter.register(
                &std::format!("{}.multiple_matches", assignment.path),
                HandlerKind::MultipleMatches,
            );
        }
        Expression::SourceBinding {
            source,
            path,
            absent,
            selection_declared,
            ..
        } => {
            if absent.is_some() {
                counter.register(&std::format!("{path}.absent"), HandlerKind::Absent);
            }
            if *selection_declared {
                counter.register(
                    &std::format!("{}.multiple_matches", source.path),
                    HandlerKind::MultipleMatches,
                );
            }
            register(source, counter);
        }
        Expression::FirstAvailable(selection) => {
            for source in selection.sources() {
                if let crate::dataset::SelectionRead::Expression(source) = &source.read {
                    register(source, counter);
                }
            }
        }
        Expression::StrContains { source, operation } => {
            if operation.handler("missing").is_some() {
                counter.register(
                    &std::format!("{}.missing", operation.path()),
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
                counter.register(&std::format!("{path}.missing"), HandlerKind::Missing);
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
        | Expression::StrReplace { source, .. }
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
    state
        .handlers
        .record(&std::format!("{path}.missing"), HandlerKind::Missing)
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
        Expression::StrConcat {
            operands,
            missing,
            path,
        } => {
            let mut rendered = String::new();
            for (index, source) in operands.iter().enumerate() {
                let value = evaluate_raw(table, source, candidate, plan, row, state)?;
                if matches!(value, Value::Missing) {
                    if let Some(value) = missing {
                        return recover::<T>(value, path, state);
                    }
                }
                if let Some(diagnostic) = scalar_language::concat_diagnostic(path, index, &value) {
                    let keys = diagnostic.definition().phase == "mapping";
                    return Err(retained_diagnostic(diagnostic, keys, candidate, plan, row));
                }
                let Value::Str(text) = value else {
                    unreachable!()
                };
                rendered
                    .try_reserve(text.len())
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                rendered.push_str(&text);
            }
            Ok(Value::Str(rendered))
        }
        Expression::StrContains { source, operation } => {
            let value = evaluate_raw(table, source, candidate, plan, row, state)?;
            match operation.apply(ValueRef::from(&value)) {
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
                    let diagnostic = operation
                        .failure_diagnostic(Step::Missing)
                        .expect("admitted metadata");
                    Err(retained_diagnostic(diagnostic, true, candidate, plan, row))
                }
                Err(Error::Diagnostic(diagnostic)) => {
                    Err(retained_diagnostic(diagnostic, false, candidate, plan, row))
                }
                Err(Error::Regex(limit)) => Err(Box::new(regex_limit(limit))),
            }
        }
        _ => unreachable!("scalar dispatch"),
    }
}
