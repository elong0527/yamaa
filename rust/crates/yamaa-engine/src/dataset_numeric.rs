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
    budget: &mut Budget,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    budget.work(1, bound.expression().node_count())?;
    let result = bound.expression().evaluate(&mut Resolver {
        bound,
        table,
        candidate,
        budget,
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
                        failure_identity(candidate, plan.keys(), row, budget)?
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

struct Resolver<'a, T: TableAccess + ?Sized> {
    bound: &'a BoundNumeric,
    table: &'a T,
    candidate: &'a Candidate,
    budget: &'a mut Budget,
}

impl<T: TableAccess + ?Sized> NumericResolver for Resolver<'_, T> {
    type Error = Box<ExecutionError<T::Error>>;

    /// Charge every occurrence and scalar copy, including repeated identifiers.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        self.budget.work(1, 1)?;
        let index = self
            .bound
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("numeric names were bound before execution");
        let value = match self.bound.bindings()[index].read {
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
            self.budget.scalar_text(text.len())?;
        }
        Ok(Selection::Present(own(value)))
    }
}
