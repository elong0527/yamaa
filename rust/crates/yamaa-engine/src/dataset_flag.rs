//! Lazy flag operands share the attempt's counters and named-selection cache.
use super::*;
use yamaa_core::{
    bound_expression::Read,
    dataset::{Flag, FlagRead},
    predicate::{self, ContextError, EvaluationContext},
    value::Selection,
};

pub(super) fn evaluate<T: TableAccess + ?Sized>(
    flag: &Flag,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    assignment: &Assignment,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    state.budget.work(1, 1)?;
    let truth = flag
        .condition()
        .evaluate_in_context(&mut Context {
            flag,
            table,
            candidate,
            plan,
            row,
            assignment,
            state,
        })
        .map_err(|error| match error.kind {
            predicate::ErrorKind::Resolution { error, .. } => error,
            predicate::ErrorKind::Limit(limit) => Box::new(predicate_limit(limit)),
            predicate::ErrorKind::Condition(_) => Box::new(ExecutionError::ScalarDiagnostic(
                yamaa_core::dataset::flag_condition_diagnostic(&error)
                    .expect("condition has a core flag projection"),
            )),
        })?;
    let value = flag.value(truth);
    if let ValueRef::Str(text) = value {
        state.budget.scalar_text(text.len())?;
    }
    Ok(own(value))
}
struct Context<'a, 'state, T: TableAccess + ?Sized> {
    flag: &'a Flag,
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
            .flag
            .bindings()
            .binary_search_by(|b| b.name.as_str().cmp(identifier))
            .expect("flag names are completely bound before execution");
        let read = &self.flag.bindings()[index].read;
        let owned = match read {
            FlagRead::Read(Read::Joined(read)) => Some(
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
            FlagRead::Read(Read::Intermediate { index, column }) => {
                self.state
                    .budget
                    .work(1, 1)
                    .map_err(ContextError::Resolution)?;
                Some(
                    intermediates::read_predicate(
                        *index,
                        *column,
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
            FlagRead::Collect { column, identifier } => {
                self.state
                    .budget
                    .work(self.candidate.members.len(), 1)
                    .map_err(ContextError::Resolution)?;
                Some(
                    key_grain::collect_bound(
                        self.table,
                        key_grain::Collection {
                            column: *column,
                            identifier,
                            filter: None,
                            selection: None,
                        },
                        key_grain::CollectionContext {
                            path: self.flag.condition().spec_path(),
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
        let value = match read {
            FlagRead::Read(Read::Column(column)) => ValueRef::from(&self.candidate.values[*column]),
            FlagRead::Read(Read::Source(column)) => {
                let source_row = self.candidate.members[0];
                self.table.cell(source_row, *column).map_err(|error| {
                    ContextError::Resolution(Box::new(ExecutionError::Cell {
                        path: self.flag.condition().spec_path().into(),
                        source_row,
                        error,
                    }))
                })?
            }
            _ => unreachable!("owned lazy operands were handled above"),
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
