//! Case operands use the shared predicate evaluator and the attempt's own grain.
use super::*;
use crate::{
    bound_expression::{BoundPredicate, Read},
    predicate::{self, Resolver},
    value::Selection,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    predicate: &BoundPredicate,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    assignment: &Assignment,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<predicate::Truth, Box<ExecutionError<T::Error>>> {
    predicate
        .plan()
        .evaluate(&mut Context {
            predicate,
            table,
            candidate,
            plan,
            row,
            assignment,
            state,
        })
        .map_err(|error| {
            let predicate::EvaluationError {
                spec_path,
                expression,
                route,
                kind,
            } = error;
            match kind {
                predicate::ErrorKind::Resolution { error, .. } => error,
                predicate::ErrorKind::Regex(limit) => Box::new(regex_limit(limit)),
                predicate::ErrorKind::Condition(condition) => Box::new(ExecutionError::Predicate {
                    source_row: candidate.members[0],
                    error: predicate::EvaluationError {
                        spec_path,
                        expression,
                        route,
                        kind: predicate::ErrorKind::Condition(condition),
                    },
                }),
            }
        })
}
struct Context<'a, 'state, T: TableAccess + ?Sized> {
    predicate: &'a BoundPredicate,
    table: &'a T,
    candidate: &'a Candidate,
    plan: &'a DatasetPlan,
    row: usize,
    assignment: &'a Assignment,
    state: &'a mut EvaluationState<'state, T::Error>,
}
impl<T: TableAccess + ?Sized> Resolver for Context<'_, '_, T> {
    type Error = Box<ExecutionError<T::Error>>;
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        let index = self
            .predicate
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("case names are admitted before execution");
        let binding = &self.predicate.bindings()[index];
        let owned = match binding.read {
            Read::Joined(ref read) => Some(joined::read(
                self.table,
                read,
                self.assignment,
                self.candidate,
                self.plan,
                self.row,
                self.state,
            )?),
            Read::Intermediate { index, column } => Some(intermediates::read_predicate(
                index,
                column,
                self.assignment,
                self.candidate,
                self.plan,
                self.row,
                self.table,
                self.state,
            )?),
            Read::Source(column) if self.predicate.collects(identifier) => {
                Some(key_grain::collect_bound(
                    self.table,
                    key_grain::Collection {
                        column,
                        identifier,
                        filter: None,
                        selection: None,
                    },
                    key_grain::CollectionContext {
                        path: self.predicate.plan().spec_path(),
                        candidate: self.candidate,
                        plan: self.plan,
                        row: self.row,
                    },
                    self.state.handlers,
                )?)
            }
            _ => None,
        };
        if let Some(value) = owned {
            return Ok(Selection::Present(value));
        }
        let value = match binding.read {
            Read::Source(column) => {
                let source_row = self.candidate.members[0];
                self.table.cell(source_row, column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: self.predicate.plan().spec_path().into(),
                        source_row,
                        error,
                    })
                })?
            }
            Read::Column(column) => ValueRef::from(&self.candidate.values[column]),
            Read::Joined(_) | Read::Intermediate { .. } => {
                unreachable!("kept/joined values were owned above")
            }
        };

        Ok(Selection::Present(own(value)))
    }
}
