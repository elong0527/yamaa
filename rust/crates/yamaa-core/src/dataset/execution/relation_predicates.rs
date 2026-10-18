//! Lazy donor/current predicate execution shared by correlated relation operations.
use super::*;
use crate::{
    bound_expression::Read,
    expression::{ErrorKind, Resolver, Truth},
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
/// Predicate and range operands share typed current reads and native failure
/// owners. A current-row read made for a lookup, its filter, `between` value or
/// a derivation's current-record operand, is not a handler site: a named
/// intermediate it reads answers as it would in a predicate, uncounted
/// (COLUMN-14).
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_current<T: TableAccess + ?Sized>(
    field: &CurrentField,
    path: &str,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    assignment: &Assignment,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    match field.read {
        CurrentRead::Record { source, column } => {
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
                state.handlers,
            )
            .map_err(|mut error| {
                if source > 0 && matches!(error.as_ref(), ExecutionError::Cell { .. }) {
                    if let ExecutionError::Cell {
                        path,
                        source_row,
                        error: original,
                    } = std::mem::replace(error.as_mut(), ExecutionError::SchemaMismatch)
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
            Ok(value.clone())
        }
        CurrentRead::Intermediate { index, column } => intermediates::read_predicate(
            index, column, assignment, candidate, plan, row, table, state,
        ),
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
) -> Result<Truth, Box<ExecutionError<T::Error>>> {
    filter
        .plan()
        .evaluate_truth(&mut Context {
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
            if matches!(&error.kind, ErrorKind::Condition(_)) {
                return Box::new(
                    error
                        .diagnostic()
                        .map(ExecutionError::ScalarDiagnostic)
                        .unwrap_or(ExecutionError::Capacity),
                );
            }
            match error.kind {
                ErrorKind::Resolution { error, .. } => error,
                ErrorKind::Condition(_) => unreachable!("condition was projected above"),
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
impl<T: TableAccess + ?Sized> Resolver for Context<'_, '_, T> {
    type Error = Box<ExecutionError<T::Error>>;
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        let binding = self
            .filter
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("all predicate names admitted before execution");
        let value = match self.filter.bindings()[binding].read {
            Read::Source(column) => {
                let value = self
                    .donor
                    .table
                    .cell(self.donor.row, column)
                    .map_err(|error| {
                        Box::new(match self.donor.owner {
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
                        })
                    })?;

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
                    self.state,
                )?
            }
            Read::Intermediate { .. } | Read::Joined(_) => {
                unreachable!("relation predicate uses explicit current metadata")
            }
        };

        Ok(Selection::Present(value))
    }
}
