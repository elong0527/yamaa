//! Borrow a template's admitted stored-source port without copying or replaying it.
use super::*;
pub(super) enum Driver<'a, T: TableAccess + ?Sized> {
    Primary(&'a T),
    Secondary(&'a dyn TableAccess<Error = T::Error>),
}
impl<'a, T: TableAccess + ?Sized> Driver<'a, T> {
    pub(super) fn new(
        primary: &'a T,
        secondary: &'a [&'a dyn TableAccess<Error = T::Error>],
        source: usize,
    ) -> Self {
        if source == 0 {
            Self::Primary(primary)
        } else {
            Self::Secondary(secondary[source - 1])
        }
    }
}
impl<T: TableAccess + ?Sized> TableAccess for Driver<'_, T> {
    type Error = T::Error;
    fn schema(&self) -> &TableSchema {
        match self {
            Self::Primary(table) => table.schema(),
            Self::Secondary(table) => table.schema(),
        }
    }
    fn row_count(&self) -> usize {
        match self {
            Self::Primary(table) => table.row_count(),
            Self::Secondary(table) => table.row_count(),
        }
    }
    fn cell(
        &self,
        row: usize,
        column: usize,
    ) -> Result<ValueRef<'_>, yamaa_core::table::CellError<Self::Error>> {
        match self {
            Self::Primary(table) => table.cell(row, column),
            Self::Secondary(table) => table.cell(row, column),
        }
    }
}
