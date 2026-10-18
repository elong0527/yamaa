//! Lazy flag operands share the attempt's counters and named-selection cache.
use super::*;
use crate::{
    bound_expression::Read,
    dataset::{Flag, FlagRead},
    expression::{ErrorKind, Resolver},
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
    let truth = flag
        .condition()
        .evaluate_truth(&mut Context {
            flag,
            table,
            candidate,
            plan,
            row,
            assignment,
            state,
        })
        .map_err(|error| match error.kind {
            ErrorKind::Resolution { error, .. } => error,
            ErrorKind::Condition(_) => Box::new(ExecutionError::ScalarDiagnostic(
                crate::dataset::flag_condition_diagnostic(&error)
                    .expect("condition has a core flag projection"),
            )),
        })?;
    if truth == crate::expression::Truth::Unknown {
        state
            .handlers
            .record(&flag.missing_path(), HandlerKind::Missing)
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
    }
    let value = flag.value(truth);
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
impl<T: TableAccess + ?Sized> Resolver for Context<'_, '_, T> {
    type Error = Box<ExecutionError<T::Error>>;
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        let index = self
            .flag
            .bindings()
            .binary_search_by(|b| b.name.as_str().cmp(identifier))
            .expect("flag names are completely bound before execution");
        let read = &self.flag.bindings()[index].read;
        let owned = match read {
            FlagRead::Read(Read::Joined(read)) => Some(joined::read(
                self.table,
                read,
                lookup::JoinSite::Uncounted(self.flag.condition().spec_path()),
                self.assignment,
                self.candidate,
                self.plan,
                self.row,
                self.state,
            )?),
            FlagRead::Read(Read::Intermediate { index, column }) => {
                Some(intermediates::read_predicate(
                    *index,
                    *column,
                    self.assignment,
                    self.candidate,
                    self.plan,
                    self.row,
                    self.table,
                    self.state,
                )?)
            }
            FlagRead::Collect { column, identifier } => Some(key_grain::collect_bound(
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
                self.state.handlers,
            )?),
            _ => None,
        };
        if let Some(value) = owned {
            return Ok(Selection::Present(value));
        }
        let value = match read {
            FlagRead::Read(Read::Column(column)) => ValueRef::from(&self.candidate.values[*column]),
            FlagRead::Read(Read::Source(column)) => {
                let source_row = self.candidate.members[0];
                self.table.cell(source_row, *column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: self.flag.condition().spec_path().into(),
                        source_row,
                        error,
                    })
                })?
            }
            _ => unreachable!("owned lazy operands were handled above"),
        };

        Ok(Selection::Present(own(value)))
    }
}
