//! Immutable typed source access with an opaque callback error channel.
use crate::callback::CallbackError;
use yamaa_core::table::{CellError, TableAccess, TableSchema, ValueRef};
/// Keep source access infallible while allowing an opaque callback failure in the same run.
pub(crate) struct Snapshot<T = crate::arrow_table::ArrowTable>(pub T);
impl<T: std::borrow::Borrow<crate::arrow_table::ArrowTable>> TableAccess for Snapshot<T> {
    type Error = CallbackError;
    /// Borrow the same admitted schema without copying or invoking host code.
    fn schema(&self) -> &TableSchema {
        self.0.borrow().schema()
    }
    /// Preserve snapshot row count exactly.
    fn row_count(&self) -> usize {
        self.0.borrow().row_count()
    }
    /// Preserve cell values and bounds failures without introducing host access.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.0
            .borrow()
            .cell(row, column)
            .map_err(|error| match error {
                CellError::OutOfBounds { row, column } => CellError::OutOfBounds { row, column },
                CellError::Access(never) => match never {},
            })
    }
}
