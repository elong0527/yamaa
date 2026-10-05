//! Bounded selected-relation access for counts and ordered numeric folds.

use alloc::vec::Vec;
use yamaa_core::{
    numeric::Number,
    reduction::{count_records, count_values, reduce_numeric, NumericReducer, ReductionError},
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
    validate_selection(table, Some(column), rows, max_rows)?;
    let values = collect_column(table, column, rows)?;
    reduce_numeric(&values, reducer, expression).map_err(TableReductionError::Reduction)
}

/// Count selected records or present field values without imposing a numeric type.
/// Record counts access no cells. Field counts collect all selected cells first,
/// preserving opaque port failures and source order before counting present values.
pub fn count_selected<T: TableAccess + ?Sized>(
    table: &T,
    column: Option<usize>,
    rows: &[usize],
    max_rows: usize,
) -> Result<Number, TableReductionError<T::Error>> {
    validate_selection(table, column, rows, max_rows)?;
    match column {
        None => count_records(rows.len()).map_err(TableReductionError::Reduction),
        Some(column) => {
            let values = collect_column(table, column, rows)?;
            count_values(&values).map_err(TableReductionError::Reduction)
        }
    }
}

/// Validate limits and selected coordinates before allocation or source cell reads.
fn validate_selection<T: TableAccess + ?Sized>(
    table: &T,
    column: Option<usize>,
    rows: &[usize],
    max_rows: usize,
) -> Result<(), TableReductionError<T::Error>> {
    if rows.len() > max_rows {
        return Err(TableReductionError::RowLimit {
            limit: max_rows,
            required: rows.len(),
        });
    }
    if let Some(column) = column {
        if column >= table.schema().columns().len() {
            return Err(TableReductionError::InvalidColumn { column });
        }
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
    Ok(())
}

/// Collect validated field reads in order before applying a reduction policy.
fn collect_column<'a, T: TableAccess + ?Sized>(
    table: &'a T,
    column: usize,
    rows: &[usize],
) -> Result<Vec<yamaa_core::table::ValueRef<'a>>, TableReductionError<T::Error>> {
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
    Ok(values)
}
