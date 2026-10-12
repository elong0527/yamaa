//! Lazy donor/current predicate execution shared by correlated relation operations.
use super::*;
use yamaa_core::{
    bound_expression::Read,
    predicate::{self, ContextError, EvaluationContext},
    value::Selection,
};

/// Retain the native owner of opaque donor access failures.
#[derive(Clone, Copy)]
pub(super) enum DonorOwner {
    Primary,
    Secondary(usize),
    Completed,
}
pub(super) struct Donor<'a, E> {
    pub table: &'a dyn TableAccess<Error = E>,
    pub row: usize,
    pub owner: DonorOwner,
}
pub(super) enum CurrentObservation {
    Predicate,
    Operand,
}
/// Predicate and range operands share typed current reads and native failure owners.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_current<T: TableAccess + ?Sized>(
    field: &CurrentField,
    path: &str,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    assignment: &Assignment,
    observation: CurrentObservation,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    match field.read {
        CurrentRead::Record { source, column } => {
            state.budget.work(candidate.members.len(), 1)?;
            let table = if source == 0 {
                state.primary
            } else {
                state.secondary[source - 1]
            };
            key_grain::collect_bound(
                table,
                key_grain::Collection {
                    column,
                    identifier: &field.identifier,
                    filter: None,
                    selection: None,
                },
                key_grain::CollectionContext {
                    path,
                    candidate,
                    plan,
                    row,
                },
                state.budget,
                state.handlers,
            )
            .map_err(|mut error| {
                if source > 0 && matches!(error.as_ref(), ExecutionError::Cell { .. }) {
                    if let ExecutionError::Cell {
                        path,
                        source_row,
                        error: original,
                    } = core::mem::replace(error.as_mut(), ExecutionError::SchemaMismatch)
                    {
                        *error = ExecutionError::SecondaryCell {
                            path,
                            source: source - 1,
                            source_row,
                            error: original,
                        };
                    }
                }
                error
            })
        }
        CurrentRead::Output(column) => {
            let value = &candidate.values[column];
            if let Value::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            Ok(value.clone())
        }
        CurrentRead::Intermediate { index, column } => match observation {
            CurrentObservation::Predicate => intermediates::read_predicate(
                index, column, assignment, candidate, plan, row, table, state,
            ),
            CurrentObservation::Operand => intermediates::read(
                index, column, assignment, candidate, plan, row, table, state,
            ),
        },
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    filter: &RelationPredicate,
    donor: Donor<'_, T::Error>,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    assignment: &Assignment,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<predicate::Truth, Box<ExecutionError<T::Error>>> {
    state.budget.work(1, 1)?;
    filter
        .plan()
        .evaluate_in_context(&mut Context {
            filter,
            donor,
            table,
            candidate,
            plan,
            row,
            assignment,
            state,
        })
        .map_err(|error| {
            if matches!(&error.kind, predicate::ErrorKind::Condition(_)) {
                return Box::new(
                    error
                        .diagnostic()
                        .map(ExecutionError::ScalarDiagnostic)
                        .unwrap_or(ExecutionError::Capacity),
                );
            }
            match error.kind {
                predicate::ErrorKind::Resolution { error, .. } => error,
                predicate::ErrorKind::Limit(limit) => Box::new(predicate_limit(limit)),
                predicate::ErrorKind::Condition(_) => unreachable!("condition was projected above"),
            }
        })
}
struct Context<'a, 'state, T: TableAccess + ?Sized> {
    filter: &'a RelationPredicate,
    donor: Donor<'a, T::Error>,
    table: &'a T,
    candidate: &'a Candidate,
    plan: &'a DatasetPlan,
    row: usize,
    assignment: &'a Assignment,
    state: &'a mut EvaluationState<'state, T::Error>,
}
impl<T: TableAccess + ?Sized> EvaluationContext for Context<'_, '_, T> {
    type Error = Box<ExecutionError<T::Error>>;
    fn budget(&mut self) -> &mut predicate::Budget {
        self.state.budget.predicate()
    }
    fn resolve(
        &mut self,
        identifier: &str,
        local: &mut predicate::Budget,
    ) -> Result<Selection, ContextError<Self::Error>> {
        let binding = self
            .filter
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("all predicate names admitted before execution");
        let value = match self.filter.bindings()[binding].read {
            Read::Source(column) => {
                self.state
                    .budget
                    .work(1, 1)
                    .map_err(ContextError::Resolution)?;
                let value = self
                    .donor
                    .table
                    .cell(self.donor.row, column)
                    .map_err(|error| {
                        ContextError::Resolution(Box::new(match self.donor.owner {
                            DonorOwner::Primary => ExecutionError::Cell {
                                path: self.filter.plan().spec_path().into(),
                                source_row: self.donor.row,
                                error,
                            },
                            DonorOwner::Secondary(source) => ExecutionError::SecondaryCell {
                                path: self.filter.plan().spec_path().into(),
                                source,
                                source_row: self.donor.row,
                                error,
                            },
                            DonorOwner::Completed => ExecutionError::SchemaMismatch,
                        }))
                    })?;
                let bytes = if let ValueRef::Str(text) = value {
                    text.len()
                } else {
                    0
                };
                self.state
                    .budget
                    .predicate_operand_text(local, bytes)
                    .map_err(ContextError::Limit)?;
                return Ok(Selection::Present(own(value)));
            }
            Read::Column(slot) => {
                let field = &self.filter.current()[slot];
                resolve_current(
                    field,
                    self.filter.plan().spec_path(),
                    self.table,
                    self.candidate,
                    self.plan,
                    self.row,
                    self.assignment,
                    CurrentObservation::Predicate,
                    self.state,
                )
                .map_err(ContextError::Resolution)?
            }
            Read::Intermediate { .. } => {
                unreachable!("relation predicate uses explicit current metadata")
            }
        };
        let bytes = if let Value::Str(text) = &value {
            text.len()
        } else {
            0
        };
        self.state
            .budget
            .predicate_operand_text(local, bytes)
            .map_err(ContextError::Limit)?;
        Ok(Selection::Present(value))
    }
}
