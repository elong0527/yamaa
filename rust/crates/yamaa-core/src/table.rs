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

/// One column's typed storage, lent whole by a port that holds it (#4190):
/// fixed-width payloads, or text in one arena, beside a validity bitmap. A
/// layout, never a value: [`ColumnRef::get`] lends for each row the same
/// `ValueRef` the port's [`TableAccess::cell`] does.
#[derive(Clone, Copy, Debug)]
pub struct ColumnRef<'a> {
    values: ColumnValues<'a>,
    /// One bit per row of `values`, least significant first, set when the
    /// row holds a value; `None` while every row does.
    validity: Option<&'a [u64]>,
    /// The view's first row in `values` and `validity`.
    start: usize,
    len: usize,
}

/// The payloads a [`ColumnRef`] lends, of one column type. A payload behind
/// an unset validity bit is never read.
#[derive(Clone, Copy, Debug)]
pub enum ColumnValues<'a> {
    /// Row `i`'s text is `bytes[offsets[i]..offsets[i + 1]]`.
    Str {
        offsets: &'a [usize],
        bytes: &'a str,
    },
    Int(&'a [i64]),
    /// A nonfinite payload reads missing, as VALUE-4 admits it.
    Float(&'a [f64]),
    /// Each row carries its own presence; the bitmap must agree with it.
    Date(&'a [Option<Date>]),
    DateTime(&'a [Option<DateTime>]),
}

impl<'a> ColumnRef<'a> {
    /// A view of every row of `values`, or `None` when the payloads, the
    /// text offsets or the bitmap do not cover the same rows, so that a port
    /// lends only storage every read can address.
    pub fn new(values: ColumnValues<'a>, validity: Option<&'a [u64]>) -> Option<Self> {
        let len = match values {
            ColumnValues::Str { offsets, bytes } => {
                let len = offsets.len().checked_sub(1)?;
                let ordered = offsets.windows(2).all(|pair| pair[0] <= pair[1]);
                let bounded = offsets.last().is_some_and(|&end| end <= bytes.len());
                let boundaries = offsets.iter().all(|&offset| bytes.is_char_boundary(offset));
                if !(ordered && bounded && boundaries) {
                    return None;
                }
                len
            }
            ColumnValues::Int(values) => values.len(),
            ColumnValues::Float(values) => values.len(),
            ColumnValues::Date(values) => values.len(),
            ColumnValues::DateTime(values) => values.len(),
        };
        if validity.is_some_and(|words| words.len() < len.div_ceil(64)) {
            return None;
        }
        Some(Self {
            values,
            validity,
            start: 0,
            len,
        })
    }

    /// The number of rows.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Rows `range` of the view, or `None` when the range leaves it.
    pub fn slice(self, range: std::ops::Range<usize>) -> Option<Self> {
        if range.start > range.end || range.end > self.len {
            return None;
        }
        Some(Self {
            start: self.start + range.start,
            len: range.len(),
            ..self
        })
    }

    /// The payloads, whose row `i` is row `i - start()` of the view.
    pub fn values(&self) -> ColumnValues<'a> {
        self.values
    }

    /// The view's first row in [`ColumnRef::values`].
    pub fn start(&self) -> usize {
        self.start
    }

    /// Whether row `row`, which must be below [`ColumnRef::len`], holds a
    /// value by the bitmap.
    fn valid(&self, row: usize) -> bool {
        let index = self.start + row;
        self.validity
            .is_none_or(|words| words[index / 64] >> (index % 64) & 1 == 1)
    }

    /// Lend row `row`'s value, which must be below [`ColumnRef::len`].
    pub fn get(&self, row: usize) -> ValueRef<'a> {
        debug_assert!(row < self.len);
        if !self.valid(row) {
            return ValueRef::Missing;
        }
        let index = self.start + row;
        match self.values {
            ColumnValues::Str { offsets, bytes } => {
                ValueRef::Str(&bytes[offsets[index]..offsets[index + 1]])
            }
            ColumnValues::Int(values) => ValueRef::Int(values[index]),
            ColumnValues::Float(values) => {
                FiniteFloat::new(values[index]).map_or(ValueRef::Missing, ValueRef::Float)
            }
            ColumnValues::Date(values) => values[index].map_or(ValueRef::Missing, ValueRef::Date),
            ColumnValues::DateTime(values) => {
                values[index].map_or(ValueRef::Missing, ValueRef::DateTime)
            }
        }
    }
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
    /// Lend column `column` whole, every row of it, when the port holds it
    /// as typed storage (#4190); `None` for a port that does not, or for a
    /// column out of bounds, whose reads go through [`TableAccess::cell`].
    fn column(&self, column: usize) -> Option<ColumnRef<'_>> {
        let _ = column;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The seam is the column view's own admission (#4190): a port lends
    // only storage every row of the view can read.
    #[test]
    fn a_column_view_refuses_storage_whose_parts_disagree() {
        let refused = [
            (
                ColumnValues::Str {
                    offsets: &[0, 2, 1],
                    bytes: "ab",
                },
                None,
                "offsets out of order",
            ),
            (
                ColumnValues::Str {
                    offsets: &[0, 3],
                    bytes: "ab",
                },
                None,
                "an offset past the arena",
            ),
            (
                ColumnValues::Str {
                    offsets: &[0, 1, 2],
                    bytes: "\u{e9}",
                },
                None,
                "an offset inside a character",
            ),
            (
                ColumnValues::Str {
                    offsets: &[],
                    bytes: "",
                },
                None,
                "no end offset",
            ),
            (
                ColumnValues::Int(&[0; 65]),
                Some(&[0u64][..]),
                "a bitmap of too few words",
            ),
        ];
        for (values, validity, case) in refused {
            assert!(ColumnRef::new(values, validity).is_none(), "{case}");
        }
        let view = ColumnRef::new(ColumnValues::Int(&[1, 2, 3]), None).unwrap();
        assert_eq!(
            view.slice(1..3).map(|view| view.get(0)),
            Some(ValueRef::Int(2))
        );
        assert!(view.slice(2..4).is_none(), "a range past the view");
    }
}
