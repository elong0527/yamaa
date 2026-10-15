//! Predicate binding to already available source/candidate columns, without joins.
use crate::bound_expression::{Binding, BoundPredicate, Read};
use crate::{
    predicate::{self, Resolved, Resolver},
    table::{CellError, TableAccess, ValueRef},
    value::{Selection, Value},
};

/// Resolve lazily in core's written order, sharing the engine's run-local counters.
pub(crate) fn evaluate<T: TableAccess + ?Sized>(
    predicate: &BoundPredicate,
    table: &T,
    source_row: usize,
    columns: &[Value],
) -> Result<predicate::Truth, predicate::EvaluationError<CellError<T::Error>>> {
    predicate.plan().evaluate(&mut RowResolver {
        table,
        source_row,
        columns,
        bindings: predicate.bindings(),
    })
}

struct RowResolver<'a, T: TableAccess + ?Sized> {
    table: &'a T,
    source_row: usize,
    columns: &'a [Value],
    bindings: &'a [Binding],
}

impl<T: TableAccess + ?Sized> Resolver for RowResolver<'_, T> {
    type Error = CellError<T::Error>;

    /// Materialize only when a caller requests the owning resolver.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        self.resolve_value(identifier).map(|value| match value {
            Resolved::Absent => Selection::Absent,
            Resolved::Owned(value) => Selection::Present(value),
            Resolved::Borrowed(value) => Selection::Present(super::own(value)),
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
            Read::Intermediate { .. } | Read::Joined(_) => {
                unreachable!("predicate admission rejects intermediate reads")
            }
            Read::Source(column) => self
                .table
                .cell(self.source_row, column)
                .map(Resolved::Borrowed),
            Read::Column(column) => Ok(Resolved::Borrowed(ValueRef::from(&self.columns[column]))),
        }
    }
}
