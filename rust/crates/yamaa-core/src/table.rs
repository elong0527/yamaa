//! Borrowed, normalized cells and ordered schema, independent of storage.

use std::collections::BTreeSet;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SchemaError {
    #[error("table column {column} has an empty name")]
    EmptyName { column: usize },
    #[error("table column {column} repeats an earlier name")]
    DuplicateName { column: usize },
}

impl TableSchema {
    /// Validate names once, preserving the supplied column declaration order.
    pub fn new(columns: Vec<Column>) -> Result<Self, SchemaError> {
        Self::validate_names(columns.iter().map(|column| column.name.as_str()))?;
        Ok(Self(columns))
    }

    /// Admit the column names shared by specification and public result
    /// schemas, independently of either schema's column type vocabulary.
    pub fn validate_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<(), SchemaError> {
        let mut seen = BTreeSet::new();
        for (column, name) in names.into_iter().enumerate() {
            if name.is_empty() {
                return Err(SchemaError::EmptyName { column });
            }
            if !seen.insert(name) {
                return Err(SchemaError::DuplicateName { column });
            }
        }
        Ok(())
    }

    /// Expose the immutable ordered schema, including when the table has no rows.
    pub fn columns(&self) -> &[Column] {
        &self.0
    }
}

/// Port failures and invalid coordinates never stand in for a missing cell.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CellError<E> {
    #[error("table cell ({row}, {column}) is out of bounds")]
    OutOfBounds { row: usize, column: usize },
    #[error("table access failed: {0:?}")]
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
