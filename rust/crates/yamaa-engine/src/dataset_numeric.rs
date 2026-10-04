//! Closed numeric binding to immutable table cells and completed output values.
use super::*;
use crate::dataset_predicate::{Binding, Read};
use alloc::collections::BTreeSet;
use yamaa_core::{
    evaluation::{EvaluationError, EvaluationErrorKind, NumericResolver},
    numeric_compiler::{CompiledEvaluationError, CompiledNumeric},
    value::Selection,
};

/// An immutable compiled expression with a complete, unique static binding map.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundNumeric {
    expression: CompiledNumeric,
    bindings: Vec<Binding>,
}

impl BoundNumeric {
    /// A non-key key-grain computation cannot choose one of several feeding records.
    pub(super) fn reads_source(&self) -> bool {
        self.bindings
            .iter()
            .any(|binding| matches!(binding.read, Read::Source(_)))
    }
    /// Check every name without resolving cells, converting literals or folding arithmetic.
    pub fn new(
        expression: CompiledNumeric,
        mut bindings: Vec<Binding>,
    ) -> Result<Self, BindingError> {
        if expression.spec_path().is_empty() {
            return Err(BindingError::EmptyPath);
        }
        let identifiers: BTreeSet<_> = expression.identifiers().collect();
        bindings.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        for (index, binding) in bindings.iter().enumerate() {
            if index > 0 && bindings[index - 1].name == binding.name {
                return Err(BindingError::DuplicateName);
            }
            if !identifiers.contains(binding.name.as_str()) {
                return Err(BindingError::UnusedName);
            }
        }
        if identifiers.iter().any(|name| {
            bindings
                .binary_search_by(|binding| binding.name.as_str().cmp(name))
                .is_err()
        }) {
            return Err(BindingError::MissingName);
        }
        Ok(Self {
            expression,
            bindings,
        })
    }

    /// Validate source scope and completed-value dependencies at each assignment site.
    pub(super) fn validate(
        &self,
        source: &TableSchema,
        available: &[bool],
        mode: &RowMode,
    ) -> Result<(), PlanError> {
        for binding in &self.bindings {
            match binding.read {
                Read::Source(_) if matches!(mode, RowMode::Groups(_)) => {
                    return Err(PlanError::NonGroupSource)
                }
                Read::Source(column) if column >= source.columns().len() => {
                    return Err(PlanError::InvalidSource)
                }
                Read::Column(column) if !available.get(column).copied().unwrap_or(false) => {
                    return Err(PlanError::UnavailableColumn)
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Resolve in written order, preserving numeric validation versus derivation identity rules.
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    bound: &BoundNumeric,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    budget.work(1, bound.expression.node_count())?;
    let result = bound.expression.evaluate(&mut Resolver {
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
                        failure_identity(candidate, &plan.keys, row, budget)?
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
            .bindings
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("numeric names were bound before execution");
        let value = match self.bound.bindings[index].read {
            Read::Column(column) => ValueRef::from(&self.candidate.values[column]),
            Read::Source(column) => {
                let source_row = self.candidate.members[0];
                self.table.cell(source_row, column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: self.bound.expression.spec_path().into(),
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
