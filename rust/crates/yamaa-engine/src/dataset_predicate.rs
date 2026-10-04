//! Predicate binding to already available source/candidate columns, without joins.
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use yamaa_core::{
    predicate::{self, Resolved, Resolver},
    table::{CellError, TableAccess, ValueRef},
    value::{Selection, Value},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    Source(usize),
    Column(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub read: Read,
}

/// A predicate owns its plan and complete, unique name-to-column map.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundPredicate {
    plan: predicate::Plan,
    bindings: Vec<Binding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingError {
    EmptyPath,
    DuplicateName,
    MissingName,
    UnusedName,
    UnavailableColumn,
    InvalidSource,
    GroupedSource,
}

impl BoundPredicate {
    /// Admit every occurrence before any table read, including unreachable nodes.
    pub fn new(plan: predicate::Plan, mut bindings: Vec<Binding>) -> Result<Self, BindingError> {
        if plan.spec_path().is_empty() {
            return Err(BindingError::EmptyPath);
        }
        let identifiers: BTreeSet<_> = plan.identifiers().into_iter().collect();
        bindings.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        for (index, binding) in bindings.iter().enumerate() {
            if index > 0 && bindings[index - 1].name == binding.name {
                return Err(BindingError::DuplicateName);
            }
            if !identifiers.contains(&binding.name.as_str()) {
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
        Ok(Self { plan, bindings })
    }

    /// A grouped filter sees completed output columns, never source records.
    pub(crate) fn validate(
        &self,
        source_width: usize,
        available: &[bool],
        grouped: bool,
    ) -> Result<(), BindingError> {
        for binding in &self.bindings {
            match binding.read {
                Read::Source(_) if grouped => return Err(BindingError::GroupedSource),
                Read::Source(index) if index >= source_width => {
                    return Err(BindingError::InvalidSource)
                }
                Read::Column(index) if !available.get(index).copied().unwrap_or(false) => {
                    return Err(BindingError::UnavailableColumn)
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Resolve lazily in the core's written order, sharing the dataset's counters.
    pub(crate) fn evaluate<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        source_row: usize,
        columns: &[Value],
        budget: &mut predicate::Budget,
    ) -> Result<predicate::Truth, predicate::EvaluationError<CellError<T::Error>>> {
        self.plan.evaluate_with_budget(
            &mut RowResolver {
                table,
                source_row,
                columns,
                bindings: &self.bindings,
            },
            budget,
        )
    }
}

struct RowResolver<'a, T: TableAccess + ?Sized> {
    table: &'a T,
    source_row: usize,
    columns: &'a [Value],
    bindings: &'a [Binding],
}

impl<T: TableAccess + ?Sized> Resolver for RowResolver<'_, T> {
    type Error = CellError<T::Error>;

    /// Compatibility owning call; actual predicate execution uses the borrowing port.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        self.resolve_value(identifier).map(|value| match value {
            Resolved::Absent => Selection::Absent,
            Resolved::Owned(value) => Selection::Present(value),
            Resolved::Borrowed(value) => Selection::Present(super::dataset::own(value)),
        })
    }

    /// Lend one cell; the core charges its bytes before making an owned operand.
    fn resolve_value(&mut self, identifier: &str) -> Result<Resolved<'_>, Self::Error> {
        let Ok(index) = self
            .bindings
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
        else {
            return Ok(Resolved::Absent);
        };
        let binding = &self.bindings[index];
        match binding.read {
            Read::Source(column) => self
                .table
                .cell(self.source_row, column)
                .map(Resolved::Borrowed),
            Read::Column(column) => Ok(Resolved::Borrowed(ValueRef::from(&self.columns[column]))),
        }
    }
}
