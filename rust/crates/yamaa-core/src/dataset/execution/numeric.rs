//! Closed numeric binding to immutable table cells and completed output values.
use super::*;
pub use crate::bound_expression::BoundNumeric;
use crate::bound_expression::Read;
use crate::{
    expression::{ErrorKind, EvaluationError, Resolver as ExpressionResolver},
    value::Selection,
};

/// Resolve in written order, preserving numeric validation versus derivation identity rules.
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    bound: &BoundNumeric,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    assignment: &Assignment,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let result = bound.expression().evaluate_number(&mut Resolver {
        bound,
        table,
        candidate,
        state,
        plan,
        row,
        assignment,
    });
    project_result(result, candidate, plan, row)
}

/// Keep scalar and aggregate arithmetic failures on the same typed route.
pub(super) fn project_result<E>(
    result: Result<crate::numeric::Number, Box<EvaluationError<Box<ExecutionError<E>>>>>,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
) -> Result<Value, Box<ExecutionError<E>>> {
    match result {
        Ok(value) => Ok(Value::from(value)),
        Err(error) => {
            let EvaluationError {
                spec_path,
                expression,
                route,
                source_span,
                kind,
            } = *error;
            match kind {
                ErrorKind::Resolution { error, .. } => Err(error),
                ErrorKind::Condition(condition) => {
                    let identity = if condition.phase() == "derivation" {
                        failure_identity(candidate, plan.keys(), row)?
                    } else {
                        None
                    };
                    Err(Box::new(ExecutionError::Numeric {
                        error: EvaluationError {
                            spec_path,
                            expression,
                            route,
                            source_span,
                            kind: ErrorKind::Condition(condition),
                        },
                        identity,
                    }))
                }
            }
        }
    }
}

struct Resolver<'a, 'state, T: TableAccess + ?Sized> {
    bound: &'a BoundNumeric,
    table: &'a T,
    candidate: &'a Candidate,
    state: &'a mut EvaluationState<'state, T::Error>,
    plan: &'a DatasetPlan,
    row: usize,
    assignment: &'a Assignment,
}

impl<T: TableAccess + ?Sized> ExpressionResolver for Resolver<'_, '_, T> {
    type Error = Box<ExecutionError<T::Error>>;

    /// Resolve each occurrence, including repeated identifiers, against admitted bindings.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        let index = self
            .bound
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("numeric names were bound before execution");
        let value = match self.bound.bindings()[index].read {
            Read::Joined(ref read) => {
                return joined::read(
                    self.table,
                    read,
                    self.assignment,
                    self.candidate,
                    self.plan,
                    self.row,
                    self.state,
                )
                .map(Selection::Present)
            }
            Read::Intermediate { index, column } => {
                return intermediates::read(
                    index,
                    column,
                    self.assignment,
                    self.candidate,
                    self.plan,
                    self.row,
                    self.table,
                    self.state,
                )
                .map(Selection::Present);
            }
            Read::Column(column) => ValueRef::from(&self.candidate.values[column]),
            Read::Source(column) => {
                let source_row = self.candidate.members[0];
                self.table.cell(source_row, column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: self.bound.expression().spec_path().into(),
                        source_row,
                        error,
                    })
                })?
            }
        };
        Ok(Selection::Present(own(value)))
    }
}
