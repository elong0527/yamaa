//! Scalar evaluation retains authored reads, missing-handler counts and raw values.
use super::*;
use crate::scalar_language;
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
        _ => unreachable!("scalar dispatch"),
    }
}
