//! Scalar evaluation retains authored reads, missing-handler counts and raw values.
//! A default substitution is registered at zero like the declared handler it
//! stands for (COLUMN-14).
use super::*;
use crate::{bound_expression::Read, scalar_language};
/// A read through an implicit join to `input` counts its answer under
/// `absent` at its path, unless it fails instead (LOOKUP-24).
fn register_absent(
    absence: &JoinAbsence,
    input: &str,
    plan: &DatasetPlan,
    counter: &mut HandlerCounter,
) {
    if !absence.requires_match(plan.requires_match(input)) {
        counter.register(
            &std::format!("{}.absent", absence.path),
            HandlerKind::Absent,
        );
    }
}
/// A formula operand read through an implicit join counts at its owner's path.
fn register_operands<'a>(
    path: &str,
    reads: impl IntoIterator<Item = &'a Read>,
    plan: &DatasetPlan,
    counter: &mut HandlerCounter,
) {
    for read in reads {
        if let Read::Joined(read) = read {
            register_absent(&JoinAbsence::operand(path), &read.name, plan, counter);
        }
    }
}
pub(super) fn register(assignment: &Assignment, plan: &DatasetPlan, counter: &mut HandlerCounter) {
    match &assignment.expression {
        Expression::Aggregate(aggregate) => {
            if let crate::dataset::AggregateRelation::Matched { keys, .. } = aggregate.relation() {
                for key in keys {
                    if let MatchValue::Expression(expression) = &key.value {
                        register(&expression.assignment, plan, counter);
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
                register(&binding.assignment, plan, counter);
                imputation::register(&binding.assignment.expression, counter);
                str_case::register(&binding.assignment.expression, counter);
                str_extract::register(&binding.assignment.expression, counter);
                str_replace::register(&binding.assignment.expression, counter);
                str_template::register(&binding.assignment.expression, counter);
            }
        }
        Expression::MappingCut { source, operation } => {
            counter.register(
                &std::format!("{}.missing", operation.path()),
                HandlerKind::Missing,
            );
            if operation.unmapped().is_some() {
                counter.register(
                    &std::format!("{}.unmapped", operation.path()),
                    HandlerKind::Unmapped,
                );
            }
            register(source, plan, counter);
        }
        Expression::SourceLookup {
            lookup,
            selection,
            absence,
            ..
        } => {
            if selection.is_some() {
                counter.register(
                    &std::format!("{}.multiple_matches", assignment.path),
                    HandlerKind::MultipleMatches,
                );
            }
            if let Some(absence) = absence {
                let input = &plan.secondary()[lookup.source].name;
                register_absent(absence, input, plan, counter);
            }
        }
        Expression::JoinedSource {
            read,
            selection,
            absence,
            ..
        } => {
            if selection.is_some() {
                counter.register(
                    &std::format!("{}.multiple_matches", assignment.path),
                    HandlerKind::MultipleMatches,
                );
            }
            if let Some(absence) = absence {
                register_absent(absence, &read.name, plan, counter);
            }
        }
        Expression::Joined(read) => register_absent(
            &JoinAbsence::operand(&assignment.path),
            &read.name,
            plan,
            counter,
        ),
        Expression::Compute(numeric) => register_operands(
            &assignment.path,
            numeric.bindings().iter().map(|binding| &binding.read),
            plan,
            counter,
        ),
        Expression::Function(function) => register_operands(
            &assignment.path,
            function
                .arguments()
                .iter()
                .filter_map(|argument| match &argument.input {
                    crate::dataset::FunctionInput::Read(read) => Some(read),
                    _ => None,
                }),
            plan,
            counter,
        ),
        Expression::ProjectFunction(function) => register_operands(
            &assignment.path,
            function
                .arguments()
                .iter()
                .filter_map(|argument| match &argument.input {
                    crate::dataset::FunctionInput::Read(read) => Some(read),
                    _ => None,
                }),
            plan,
            counter,
        ),
        Expression::Flag(flag) => counter.register(&flag.missing_path(), HandlerKind::Missing),
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
            register(source, plan, counter);
        }
        Expression::FirstAvailable(selection) => {
            counter.register(&selection.missing_path(), HandlerKind::Missing);
            for source in selection.sources() {
                if let crate::dataset::SelectionRead::Expression(source) = &source.read {
                    register(source, plan, counter);
                }
            }
        }
        Expression::Extreme { operands, .. }
        | Expression::Imputation { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::StrTemplate { operands, .. } => {
            for source in operands {
                register(source, plan, counter);
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                register(&branch.assignment, plan, counter);
            }
        }
        Expression::StrCase { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::Round { source, .. } => register(source, plan, counter),
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
