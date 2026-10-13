//! Case operands use the shared predicate evaluator and the attempt's own grain.
use super::*;
use yamaa_core::{
    bound_expression::{BoundPredicate, Read},
    predicate::{self, ContextError, EvaluationContext},
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
    evaluate_context(predicate, table, candidate, plan, row, assignment, state).map_err(|error| {
        let predicate::EvaluationError {
            spec_path,
            expression,
            route,
            kind,
        } = error;
        match kind {
            predicate::ErrorKind::Resolution { error, .. } => error,
            predicate::ErrorKind::Limit(limit) => Box::new(predicate_limit(limit)),
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
/// Reuse exactly the native operand context while the caller owns its semantic phase.
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate_context<T: TableAccess + ?Sized>(
    predicate: &BoundPredicate,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    assignment: &Assignment,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<predicate::Truth, predicate::EvaluationError<Box<ExecutionError<T::Error>>>> {
    predicate.plan().evaluate_in_context(&mut Context {
        predicate,
        table,
        candidate,
        plan,
        row,
        assignment,
        state,
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
        let index = self
            .predicate
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("case names are admitted before execution");
        let binding = &self.predicate.bindings()[index];
        let owned = match binding.read {
            Read::Joined(ref read) => Some(
                joined::read(
                    read,
                    self.assignment,
                    self.candidate,
                    self.plan,
                    self.row,
                    self.state,
                )
                .map_err(ContextError::Resolution)?,
            ),
            Read::Intermediate { index, column } => {
                self.state
                    .budget
                    .work(1, 1)
                    .map_err(ContextError::Resolution)?;
                Some(
                    intermediates::read_predicate(
                        index,
                        column,
                        self.assignment,
                        self.candidate,
                        self.plan,
                        self.row,
                        self.table,
                        self.state,
                    )
                    .map_err(ContextError::Resolution)?,
                )
            }
            Read::Source(column) if self.predicate.collects(identifier) => {
                self.state
                    .budget
                    .work(self.candidate.members.len(), 1)
                    .map_err(ContextError::Resolution)?;
                Some(
                    key_grain::collect_bound(
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
                        self.state.budget,
                        self.state.handlers,
                    )
                    .map_err(ContextError::Resolution)?,
                )
            }
            _ => None,
        };
        if let Some(value) = owned {
            let bytes = match &value {
                Value::Str(text) => text.len(),
                _ => 0,
            };
            self.state
                .budget
                .predicate_operand_text(local, bytes)
                .map_err(ContextError::Limit)?;
            return Ok(Selection::Present(value));
        }
        let value = match binding.read {
            Read::Source(column) => {
                let source_row = self.candidate.members[0];
                self.table.cell(source_row, column).map_err(|error| {
                    ContextError::Resolution(Box::new(ExecutionError::Cell {
                        path: self.predicate.plan().spec_path().into(),
                        source_row,
                        error,
                    }))
                })?
            }
            Read::Column(column) => ValueRef::from(&self.candidate.values[column]),
            Read::Joined(_) | Read::Intermediate { .. } => {
                unreachable!("kept/joined values were owned above")
            }
        };
        let bytes = match value {
            ValueRef::Str(text) => text.len(),
            _ => 0,
        };
        self.state
            .budget
            .predicate_operand_text(local, bytes)
            .map_err(ContextError::Limit)?;
        Ok(Selection::Present(own(value)))
    }
}
