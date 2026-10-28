//! Stable, exact partitioning of an immutable normalized table snapshot.

use crate::limits::{LimitExceeded, Limits};
use crate::{
    table::{CellError, TableAccess, ValueRef},
    temporal::{Date, DateTime},
};
use std::collections::BTreeMap;

/// Map ordering is internal: returned groups always follow first source occurrence.
/// Finite float bits suffice for equality after canonicalizing signed zero. Under
/// grouping equality (`From`) types remain distinct; normalized table columns
/// have one declared logical type. Under key equality ([`Key::matching`]) an
/// integral float is the integer it equals.
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

/// Hash consistent with key equality: temporal values hash their civil
/// fields, as their equality compares only those.
impl std::hash::Hash for Key<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Missing => (),
            Self::Str(value) => value.hash(state),
            Self::Int(value) => value.hash(state),
            Self::Float(bits) => bits.hash(state),
            Self::Bool(value) => value.hash(state),
            Self::Date(value) => value.fields().hash(state),
            Self::DateTime(value) => value.fields().hash(state),
        }
    }
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

impl<'a> Key<'a> {
    /// `value` under key equality (VALUE-19, LOOKUP-9), as key matching and
    /// an intermediate's `unique` check compare it: numbers by value, so an
    /// integral float within the `int` range is the `Int` it equals, and
    /// every other value as grouping equality has it, a missing value its
    /// own key. Key matching never offers a missing key; `unique` counts two
    /// missing values as one combination (LOOKUP-17).
    pub(crate) fn matching(value: ValueRef<'a>) -> Self {
        if let ValueRef::Float(number) = value {
            let number = number.get();
            if number == number.trunc()
                && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&number)
            {
                return Self::Int(number as i64);
            }
        }
        Self::from(value)
    }
}

/// A [`Key`] that owns its text, with the same equality and order: what a
/// reader keeps of a key once the records it was read from are dropped.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OwnedKey {
    Missing,
    Str(Box<str>),
    Int(i64),
    Float(u64),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

impl OwnedKey {
    /// The key this owns.
    pub(crate) fn key(&self) -> Key<'_> {
        match self {
            Self::Missing => Key::Missing,
            Self::Str(value) => Key::Str(value),
            Self::Int(value) => Key::Int(*value),
            Self::Float(bits) => Key::Float(*bits),
            Self::Bool(value) => Key::Bool(*value),
            Self::Date(value) => Key::Date(*value),
            Self::DateTime(value) => Key::DateTime(*value),
        }
    }
}

impl From<Key<'_>> for OwnedKey {
    fn from(key: Key<'_>) -> Self {
        match key {
            Key::Missing => Self::Missing,
            Key::Str(value) => Self::Str(value.into()),
            Key::Int(value) => Self::Int(value),
            Key::Float(bits) => Self::Float(bits),
            Key::Bool(value) => Self::Bool(value),
            Key::Date(value) => Self::Date(value),
            Key::DateTime(value) => Self::DateTime(value),
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum GroupingError<E> {
    #[error("table grouping empty columns")]
    EmptyColumns,
    #[error("table grouping invalid column: position={position:?}, column={column:?}")]
    InvalidColumn { position: usize, column: usize },
    #[error("table grouping duplicate column: position={position:?}, column={column:?}")]
    DuplicateColumn { position: usize, column: usize },
    #[error("table grouping boundary: {0}")]
    Limit(LimitExceeded),
    #[error("table grouping allocation")]
    Allocation,
    #[error("table grouping cell: row={row:?}, column={column:?}, error={error:?}")]
    Cell {
        row: usize,
        column: usize,
        error: CellError<E>,
    },
}

/// Partition all rows by bound columns (ROW-4). Missing values
/// compare equal; text is exact; temporal precision and float zero signs do not
/// affect equality. Each member list is strictly increasing for ordered folds.
/// Admission validates the complete column list and table geometry before any
/// cell access. No sorting of source rows, host callbacks or content hashing occurs.
pub fn partition<T: TableAccess + ?Sized>(
    table: &T,
    columns: &[usize],
) -> Result<Vec<Vec<usize>>, GroupingError<T::Error>> {
    partition_by(table, columns, Key::from)
}

/// Partition all rows as [`partition`] does, comparing each value under key
/// equality ([`Key::matching`]): an intermediate's `unique` check groups the
/// combinations key matching would meet, and missing values still compare
/// equal (LOOKUP-17).
pub(crate) fn partition_matching<T: TableAccess + ?Sized>(
    table: &T,
    columns: &[usize],
) -> Result<Vec<Vec<usize>>, GroupingError<T::Error>> {
    partition_by(table, columns, Key::matching)
}

/// The one partition both equalities share: each row's key is `key_of` of
/// each of its cells.
fn partition_by<'t, T: TableAccess + ?Sized>(
    table: &'t T,
    columns: &[usize],
    key_of: fn(ValueRef<'t>) -> Key<'t>,
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
    Limits::table(rows, table.schema().columns().len()).map_err(GroupingError::Limit)?;
    let mut index = BTreeMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for row in 0..rows {
        let mut key = Vec::new();
        key.try_reserve_exact(columns.len())
            .map_err(|_| GroupingError::Allocation)?;
        for &column in columns {
            key.push(key_of(
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
