//! The failure of a count or numeric fold over a table's selected records.

use crate::limits::LimitExceeded;
use crate::{reduction::ReductionError, table::CellError};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum TableReductionError<E> {
    #[error("table reduction boundary: {0}")]
    Limit(LimitExceeded),
    #[error("table reduction invalid column: column={column:?}")]
    InvalidColumn { column: usize },
    #[error("table reduction invalid row: position={position:?}, row={row:?}")]
    InvalidRow { position: usize, row: usize },
    #[error("table reduction unordered selection: position={position:?}")]
    UnorderedSelection { position: usize },
    #[error("table reduction allocation")]
    Allocation,
    #[error("table reduction cell: position={position:?}, error={error:?}")]
    Cell {
        position: usize,
        error: CellError<E>,
    },
    #[error("table reduction reduction: {0:?}")]
    Reduction(ReductionError),
}
