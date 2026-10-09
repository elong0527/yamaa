//! Decode existing CSV/Parquet profiles while retaining a caller's opaque error.
//! Snapshot cells remain infallible; the error type is only the engine's shared
//! callback boundary. This grants no host access or activation authority.
use crate::{arrow_table::ArrowTable, specification_run::Error};
use std::marker::PhantomData;
use yamaa_core::{
    specification::SourceDeclaration,
    table::{CellError, TableAccess, TableSchema, ValueRef},
};
use yamaa_engine::specification_run::SourceDecoder;

pub struct Snapshot<E> {
    table: ArrowTable,
    error: PhantomData<fn() -> E>,
}
impl<E> Snapshot<E> {
    pub fn table(&self) -> &ArrowTable {
        &self.table
    }
    pub fn into_table(self) -> ArrowTable {
        self.table
    }
}
impl<E> TableAccess for Snapshot<E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        self.table.schema()
    }
    fn row_count(&self) -> usize {
        self.table.row_count()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        self.table.cell(row, column).map_err(|error| match error {
            CellError::OutOfBounds { row, column } => CellError::OutOfBounds { row, column },
            CellError::Access(never) => match never {},
        })
    }
}
pub struct Decoder<E>(PhantomData<fn() -> E>);
impl<E> Default for Decoder<E> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<E> SourceDecoder for Decoder<E> {
    type Error = Error;
    type Table = Snapshot<E>;
    fn continue_after(&self, error: &Error) -> bool {
        crate::specification_run::SourceDecoder.continue_after(error)
    }
    fn decode(&mut self, source: &SourceDeclaration, bytes: &[u8]) -> Result<Snapshot<E>, Error> {
        crate::specification_run::SourceDecoder
            .decode(source, bytes)
            .map(|snapshot| Snapshot {
                table: snapshot.0,
                error: PhantomData,
            })
    }
}
