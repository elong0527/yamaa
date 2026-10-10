//! Closed numeric binding to immutable table cells and completed output values.
use super::*;
use crate::dataset_predicate::Read;
pub use yamaa_core::bound_expression::BoundNumeric;
use yamaa_core::{
    evaluation::{EvaluationError, EvaluationErrorKind, NumericResolver},
    numeric_compiler::CompiledEvaluationError,
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
    state.budget.work(1, bound.expression().node_count())?;
    let result = bound.expression().evaluate(&mut Resolver {
        bound,
        table,
        candidate,
        state,
        plan,
        row,
        assignment,
    });
    match result {
        Ok(value) => Ok(Value::from(value)),
        Err(error) => {
            let CompiledEvaluationError {
                source_span,
                evaluation,
            } = error;
            let EvaluationError { location, kind } = *evaluation;
            match kind {
                EvaluationErrorKind::Resolution { error, .. } => Err(error),
                EvaluationErrorKind::Numeric(condition) => {
                    let identity = if condition.phase() == "derivation" {
                        failure_identity(candidate, plan.keys(), row, state.budget)?
                    } else {
                        None
                    };
                    Err(Box::new(ExecutionError::Numeric {
                        error: CompiledEvaluationError {
                            source_span,
                            evaluation: Box::new(EvaluationError {
                                location,
                                kind: EvaluationErrorKind::Numeric(condition),
                            }),
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

impl<T: TableAccess + ?Sized> NumericResolver for Resolver<'_, '_, T> {
    type Error = Box<ExecutionError<T::Error>>;

    /// Charge every occurrence and scalar copy, including repeated identifiers.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        self.state.budget.work(1, 1)?;
        let index = self
            .bound
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("numeric names were bound before execution");
        let value = match self.bound.bindings()[index].read {
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
        if let ValueRef::Str(text) = value {
            self.state.budget.scalar_text(text.len())?;
        }
        Ok(Selection::Present(own(value)))
    }
}
