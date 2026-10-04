//! Bounded access to a selected relation, followed by an ordered numeric fold.

use alloc::vec::Vec;
use yamaa_core::{
    numeric::Number,
    reduction::{reduce_numeric, NumericReducer, ReductionError},
    table::{CellError, TableAccess},
};

#[derive(Debug, PartialEq, Eq)]
pub enum TableReductionError<E> {
    RowLimit {
        limit: usize,
        required: usize,
    },
    InvalidColumn {
        column: usize,
    },
    InvalidRow {
        position: usize,
        row: usize,
    },
    UnorderedSelection {
        position: usize,
    },
    Allocation,
    Cell {
        position: usize,
        error: CellError<E>,
    },
    Reduction(ReductionError),
}

/// Reduce a bound column in an immutable table snapshot. Rows are a strictly
/// increasing subsequence of the snapshot (REQ-0471/0480): duplicates and
/// reordering are rejected, never silently sorted or deduplicated. An upstream
/// sort constructs a new relation with its own row order.
///
/// The caller supplies a resource budget, not a language limit. Preflight checks
/// limit, column, then rows before any cell access or allocation. All selected
/// arguments are read before folding, so a later port error wins over potential
/// earlier arithmetic/type failures. Port errors remain opaque and need not Clone.
/// This handles a bare bound column, not aggregate parsing/filtering/grouping.
pub fn reduce_column<T: TableAccess + ?Sized>(
    table: &T,
    column: usize,
    rows: &[usize],
    max_rows: usize,
    reducer: NumericReducer,
    expression: &str,
) -> Result<Number, TableReductionError<T::Error>> {
    if rows.len() > max_rows {
        return Err(TableReductionError::RowLimit {
            limit: max_rows,
            required: rows.len(),
        });
    }
    if column >= table.schema().columns().len() {
        return Err(TableReductionError::InvalidColumn { column });
    }
    let row_count = table.row_count();
    for (position, &row) in rows.iter().enumerate() {
        if row >= row_count {
            return Err(TableReductionError::InvalidRow { position, row });
        }
        if position > 0 && rows[position - 1] >= row {
            return Err(TableReductionError::UnorderedSelection { position });
        }
    }
    let mut values = Vec::new();
    values
        .try_reserve_exact(rows.len())
        .map_err(|_| TableReductionError::Allocation)?;
    for (position, &row) in rows.iter().enumerate() {
        values.push(
            table
                .cell(row, column)
                .map_err(|error| TableReductionError::Cell { position, error })?,
        );
    }
    reduce_numeric(&values, reducer, expression).map_err(TableReductionError::Reduction)
}
