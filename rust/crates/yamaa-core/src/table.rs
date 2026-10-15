//! Borrowed, normalized cells and ordered schema, independent of storage.

use alloc::{collections::BTreeSet, string::String, vec::Vec};

use crate::{
    temporal::{Date, DateTime},
    value::{ColumnType, FiniteFloat, Value, ValueType},
};

/// A cell view borrows only text; primitive and temporal values retain exact data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueRef<'a> {
    Missing,
    Str(&'a str),
    Int(i64),
    Float(FiniteFloat),
    Bool(bool),
    Date(Date),
    DateTime(DateTime),
}

impl ValueRef<'_> {
    /// Return the closed type without allocating or materializing borrowed text.
    pub fn value_type(self) -> Option<ValueType> {
        match self {
            Self::Missing => None,
            Self::Str(_) => Some(ValueType::Str),
            Self::Int(_) => Some(ValueType::Int),
            Self::Float(_) => Some(ValueType::Float),
            Self::Bool(_) => Some(ValueType::Bool),
            Self::Date(_) => Some(ValueType::Date),
            Self::DateTime(_) => Some(ValueType::DateTime),
        }
    }
}

impl<'a> From<&'a Value> for ValueRef<'a> {
    /// Borrow an owned runtime value while retaining exact scalar representation.
    fn from(value: &'a Value) -> Self {
        match value {
            Value::Missing => Self::Missing,
            Value::Str(value) => Self::Str(value),
            Value::Int(value) => Self::Int(*value),
            Value::Float(value) => Self::Float(*value),
            Value::Bool(value) => Self::Bool(*value),
            Value::Date(value) => Self::Date(*value),
            Value::DateTime(value) => Self::DateTime(*value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub kind: ColumnType,
}

/// Declaration order survives empty tables. Names are unique and nonempty;
/// specification identifier grammar and binding belong to compilation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSchema(Vec<Column>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaError {
    EmptyName { column: usize },
    DuplicateName { column: usize },
}

impl TableSchema {
    /// Validate names once, preserving the supplied column declaration order.
    pub fn new(columns: Vec<Column>) -> Result<Self, SchemaError> {
        let mut names = BTreeSet::new();
        for (column, item) in columns.iter().enumerate() {
            if item.name.is_empty() {
                return Err(SchemaError::EmptyName { column });
            }
            if !names.insert(item.name.as_str()) {
                return Err(SchemaError::DuplicateName { column });
            }
        }
        Ok(Self(columns))
    }

    /// Expose the immutable ordered schema, including when the table has no rows.
    pub fn columns(&self) -> &[Column] {
        &self.0
    }
}

/// Port failures and invalid coordinates never stand in for a missing cell.
#[derive(Debug, PartialEq, Eq)]
pub enum CellError<E> {
    OutOfBounds { row: usize, column: usize },
    Access(E),
}

/// Immutable snapshot with normalized runtime values and stable schema/row order.
/// Implementations must check both coordinates, including on empty tables, and
/// retain backing storage for every borrowed cell. No host calls are implied.
/// Declared column types describe storage; conversion belongs at ingestion, not
/// inside consumers. A port must not mutate its snapshot through interior state.
pub trait TableAccess {
    type Error;

    /// Return the snapshot schema in declared column order.
    fn schema(&self) -> &TableSchema;
    /// Return the number of records, independent of column null counts.
    fn row_count(&self) -> usize;
    /// Borrow one normalized cell or report coordinate/access failure.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>>;
}
