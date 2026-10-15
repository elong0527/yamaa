//! Stable, exact partitioning of an immutable normalized table snapshot.

use alloc::{collections::BTreeMap, vec::Vec};
use yamaa_core::{
    table::{CellError, TableAccess, ValueRef},
    temporal::{Date, DateTime},
};

/// Map ordering is internal: returned groups always follow first source occurrence.
/// Finite float bits suffice for equality after canonicalizing signed zero. Types
/// remain distinct; normalized table columns have one declared logical type.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key<'a> {
    Missing,
    Str(&'a str),
    Int(i64),
    Float(u64),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

impl<'a> From<ValueRef<'a>> for Key<'a> {
    /// Borrow text and retain logical equality, ignoring temporal precision.
    fn from(value: ValueRef<'a>) -> Self {
        match value {
            ValueRef::Missing => Self::Missing,
            ValueRef::Str(value) => Self::Str(value),
            ValueRef::Int(value) => Self::Int(value),
            ValueRef::Float(value) => Self::Float(if value.get() == 0.0 {
                0
            } else {
                value.get().to_bits()
            }),
            ValueRef::Bool(value) => Self::Bool(value),
            ValueRef::Date(value) => Self::Date(value),
            ValueRef::DateTime(value) => Self::DateTime(value),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GroupingError<E> {
    EmptyColumns,
    InvalidColumn {
        position: usize,
        column: usize,
    },
    DuplicateColumn {
        position: usize,
        column: usize,
    },
    RowLimit {
        limit: usize,
        required: usize,
    },
    KeyCellLimit {
        limit: usize,
    },
    Allocation,
    Cell {
        row: usize,
        column: usize,
        error: CellError<E>,
    },
}

/// Partition all rows by bound columns (REQ-0037/0038/0065). Missing values
/// compare equal; text is exact; temporal precision and float zero signs do not
/// affect equality. Each member list is strictly increasing for ordered folds.
/// Admission validates the complete column list and resource budgets before any
/// cell access. No sorting of source rows, host callbacks or content hashing occurs.
/// The key-cell budget bounds cells read, not allocator bytes or text comparison cost.
pub fn partition<T: TableAccess + ?Sized>(
    table: &T,
    columns: &[usize],
    max_rows: usize,
    max_key_cells: usize,
) -> Result<Vec<Vec<usize>>, GroupingError<T::Error>> {
    if columns.is_empty() {
        return Err(GroupingError::EmptyColumns);
    }
    for (position, &column) in columns.iter().enumerate() {
        if column >= table.schema().columns().len() {
            return Err(GroupingError::InvalidColumn { position, column });
        }
        if columns[..position].contains(&column) {
            return Err(GroupingError::DuplicateColumn { position, column });
        }
    }
    let rows = table.row_count();
    if rows > max_rows {
        return Err(GroupingError::RowLimit {
            limit: max_rows,
            required: rows,
        });
    }
    if rows
        .checked_mul(columns.len())
        .is_none_or(|size| size > max_key_cells)
    {
        return Err(GroupingError::KeyCellLimit {
            limit: max_key_cells,
        });
    }
    let mut index = BTreeMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for row in 0..rows {
        let mut key = Vec::new();
        key.try_reserve_exact(columns.len())
            .map_err(|_| GroupingError::Allocation)?;
        for &column in columns {
            key.push(Key::from(
                table
                    .cell(row, column)
                    .map_err(|error| GroupingError::Cell { row, column, error })?,
            ));
        }
        let group = if let Some(&group) = index.get(&key) {
            group
        } else {
            groups
                .try_reserve(1)
                .map_err(|_| GroupingError::Allocation)?;
            let group = groups.len();
            groups.push(Vec::new());
            index.insert(key, group);
            group
        };
        groups[group]
            .try_reserve(1)
            .map_err(|_| GroupingError::Allocation)?;
        groups[group].push(row);
    }
    Ok(groups)
}
