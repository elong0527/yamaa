//! The engine's own table behind the core's table port: what every source
//! decodes to (CSV, typed CSV, SAS, ODM, Parquet and host frames), held
//! column by column in [`crate::columns`] storage.
//!
//! A table is admitted whole before any evaluator reads it: its geometry is
//! within the fixed limits, each column holds values of its declared type, a
//! nonfinite float is missing, and a temporal value keeps its collected
//! precision. Tables share columns, so a projection, an added field or a new
//! validity copies no other column.
use crate::columns::{Text, Values};
use crate::epoch::{date_from_days, datetime_from_seconds};
use std::{convert::Infallible, sync::Arc};
use yamaa_core::{
    limits::Limits,
    table::{CellError, ColumnRef, ColumnValues, TableAccess, TableSchema, ValueRef},
    temporal::{Date, DateTime, Precision},
    value::{ColumnType, FiniteFloat},
};

/// Why a table was not admitted. A host frame's failures name the batch,
/// column and row of its stream; every other table is one batch.
#[derive(Debug)]
pub enum TableError {
    Limit {
        resource: &'static str,
        limit: usize,
        required: Option<usize>,
    },
    SchemaMismatch {
        batch: usize,
    },
    ArrayType {
        batch: usize,
        column: usize,
    },
    TemporalRange {
        batch: usize,
        column: usize,
        row: usize,
    },
    Representation {
        representation: &'static str,
    },
    /// A host frame's Arrow stream failed.
    #[cfg(feature = "arrow")]
    Arrow(arrow_schema::ArrowError),
}

pub(crate) fn limit(error: yamaa_core::limits::LimitExceeded) -> TableError {
    TableError::Limit {
        resource: error.resource,
        limit: error.limit,
        required: error.actual,
    }
}

/// The representation a decoded text column fits, as the public tables'
/// Arrow form needs it: Arrow Utf8 addresses a column's bytes with signed
/// 32-bit offsets.
pub(crate) const UTF8_OFFSETS: &str = "Arrow Utf8 signed 32-bit offsets";

/// Whether text values of these lengths fit [`UTF8_OFFSETS`] together.
pub fn fits_utf8_offsets(lengths: impl IntoIterator<Item = usize>) -> bool {
    lengths
        .into_iter()
        .try_fold(0usize, |total, length| {
            total
                .checked_add(length)
                .filter(|&sum| i32::try_from(sum).is_ok())
        })
        .is_some()
}

/// One column of a table, of one column type.
#[derive(Clone, Debug, PartialEq)]
pub enum Column {
    Str(Text),
    Int(Values<i64>),
    /// Finite values: a nonfinite float is missing.
    Float(Values<f64>),
    Date(Vec<Option<Date>>),
    DateTime(Vec<Option<DateTime>>),
}

impl Column {
    /// An empty column of `kind`.
    pub fn empty(kind: ColumnType) -> Self {
        match kind {
            ColumnType::Str => Self::Str(Text::default()),
            ColumnType::Int => Self::Int(Values::default()),
            ColumnType::Float => Self::Float(Values::default()),
            ColumnType::Date => Self::Date(Vec::new()),
            ColumnType::DateTime => Self::DateTime(Vec::new()),
        }
    }

    /// Floats, each nonfinite one missing.
    pub fn floats(values: impl IntoIterator<Item = Option<f64>>) -> Self {
        Self::Float(
            values
                .into_iter()
                .map(|value| value.and_then(FiniteFloat::new).map(FiniteFloat::get))
                .collect(),
        )
    }

    pub fn kind(&self) -> ColumnType {
        match self {
            Self::Str(_) => ColumnType::Str,
            Self::Int(_) => ColumnType::Int,
            Self::Float(_) => ColumnType::Float,
            Self::Date(_) => ColumnType::Date,
            Self::DateTime(_) => ColumnType::DateTime,
        }
    }

    /// The number of rows.
    pub fn len(&self) -> usize {
        match self {
            Self::Str(values) => values.len(),
            Self::Int(values) => values.len(),
            Self::Float(values) => values.len(),
            Self::Date(values) => values.len(),
            Self::DateTime(values) => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append one cell: a missing one, or a value of the column's type.
    /// A value of another type is missing.
    pub fn push(&mut self, value: ValueRef<'_>) {
        match self {
            Self::Str(values) => values.push(match value {
                ValueRef::Str(value) => Some(value),
                _ => None,
            }),
            Self::Int(values) => values.push(match value {
                ValueRef::Int(value) => Some(value),
                _ => None,
            }),
            Self::Float(values) => values.push(match value {
                ValueRef::Float(value) => Some(value.get()),
                _ => None,
            }),
            Self::Date(values) => values.push(match value {
                ValueRef::Date(value) => Some(value),
                _ => None,
            }),
            Self::DateTime(values) => values.push(match value {
                ValueRef::DateTime(value) => Some(value),
                _ => None,
            }),
        }
    }

    fn cell(&self, row: usize) -> ValueRef<'_> {
        let value = match self {
            Self::Str(values) => values.get(row).map(ValueRef::Str),
            Self::Int(values) => values.get(row).map(ValueRef::Int),
            Self::Float(values) => values
                .get(row)
                .map(|value| ValueRef::Float(FiniteFloat::new(value).expect("admitted finite"))),
            Self::Date(values) => values[row].map(ValueRef::Date),
            Self::DateTime(values) => values[row].map(ValueRef::DateTime),
        };
        value.unwrap_or(ValueRef::Missing)
    }

    /// Rows `range`, copied whole (#4190).
    fn slice(&self, range: std::ops::Range<usize>) -> Self {
        match self {
            Self::Str(values) => Self::Str(values.slice(range)),
            Self::Int(values) => Self::Int(values.slice(range)),
            Self::Float(values) => Self::Float(values.slice(range)),
            Self::Date(values) => Self::Date(values[range].to_vec()),
            Self::DateTime(values) => Self::DateTime(values[range].to_vec()),
        }
    }

    /// The column's storage, lent whole to the engine (#4190).
    fn storage(&self) -> Option<ColumnRef<'_>> {
        let (values, validity) = match self {
            Self::Str(text) => (
                ColumnValues::Str {
                    offsets: text.offsets(),
                    bytes: text.arena(),
                },
                text.validity().words(),
            ),
            Self::Int(values) => (
                ColumnValues::Int(values.payloads()),
                values.validity().words(),
            ),
            Self::Float(values) => (
                ColumnValues::Float(values.payloads()),
                values.validity().words(),
            ),
            Self::Date(values) => (ColumnValues::Date(values), None),
            Self::DateTime(values) => (ColumnValues::DateTime(values), None),
        };
        ColumnRef::new(values, validity)
    }

    /// The same column with each nonfinite float missing, or `None` when no
    /// value is nonfinite.
    fn finite(&self) -> Option<Self> {
        let Self::Float(values) = self else {
            return None;
        };
        let nonfinite = |row| values.get(row).is_some_and(|value: f64| !value.is_finite());
        (0..values.len())
            .any(nonfinite)
            .then(|| Self::floats((0..values.len()).map(|row| values.get(row))))
    }
}

/// An admitted immutable table, cheap to clone: clones share the columns.
#[derive(Clone, Debug)]
pub struct ColumnTable {
    schema: TableSchema,
    columns: Vec<Arc<Column>>,
    rows: usize,
}

impl ColumnTable {
    /// Admit `columns` as the table `schema` declares, with `rows` records:
    /// a table without columns still has its records. The geometry is
    /// checked first, then each column's type and length in order.
    pub fn new(schema: TableSchema, columns: Vec<Column>, rows: usize) -> Result<Self, TableError> {
        Self::shared(schema, columns.into_iter().map(Arc::new).collect(), rows)
    }

    fn shared(
        schema: TableSchema,
        mut columns: Vec<Arc<Column>>,
        rows: usize,
    ) -> Result<Self, TableError> {
        Limits::columns(schema.columns().len()).map_err(limit)?;
        Limits::table(rows, schema.columns().len()).map_err(limit)?;
        if columns.len() != schema.columns().len() {
            return Err(TableError::SchemaMismatch { batch: 0 });
        }
        for (index, (field, column)) in schema.columns().iter().zip(&mut columns).enumerate() {
            if column.kind() != field.kind || column.len() != rows {
                return Err(TableError::ArrayType {
                    batch: 0,
                    column: index,
                });
            }
            if let Some(finite) = column.finite() {
                *column = Arc::new(finite);
            }
        }
        Ok(Self {
            schema,
            columns,
            rows,
        })
    }

    /// Copy every cell of `table` into its own columns. A value whose type
    /// is not its column's is missing, and each text column fits
    /// [`UTF8_OFFSETS`] as a whole.
    pub fn from_access<T: TableAccess<Error = Infallible>>(table: &T) -> Result<Self, TableError> {
        let schema = table.schema().clone();
        let rows = table.row_count();
        Limits::table(rows, schema.columns().len()).map_err(limit)?;
        let mut columns = Vec::with_capacity(schema.columns().len());
        for (index, field) in schema.columns().iter().enumerate() {
            let mut column = Column::empty(field.kind);
            for row in 0..rows {
                column.push(table.cell(row, index).expect("validated coordinate"));
            }
            if let Column::Str(text) = &column {
                if i32::try_from(text.bytes_in(0..rows)).is_err() {
                    return Err(TableError::Representation {
                        representation: UTF8_OFFSETS,
                    });
                }
            }
            columns.push(column);
        }
        Self::new(schema, columns, rows)
    }

    /// The engine's table of a public table, such as one a caller saves: a
    /// date with day and a datetime with second precision. A list column,
    /// which no specification column has, is refused at the first one, and
    /// a temporal value outside the years 1 to 9999 names its batch and row.
    pub fn from_public(table: &crate::public_table::PublicTable) -> Result<Self, TableError> {
        use crate::public_table::PublicColumn;
        let fields = (0..table.width())
            .map(|index| {
                let kind = table.column(index).kind().ok_or(TableError::ArrayType {
                    batch: 0,
                    column: index,
                })?;
                Ok(yamaa_core::table::Column {
                    name: table.name(index).to_owned(),
                    kind,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let schema =
            TableSchema::new(fields).map_err(|_| TableError::SchemaMismatch { batch: 0 })?;
        Limits::table(table.rows(), table.width()).map_err(limit)?;
        let columns = (0..table.width())
            .map(|index| {
                Ok(match table.column(index) {
                    PublicColumn::Str(text) => Column::Str(text.clone()),
                    PublicColumn::Int(values) => Column::Int(values.clone()),
                    PublicColumn::Float(values) => {
                        Column::floats((0..values.len()).map(|row| values.get(row)))
                    }
                    PublicColumn::Date(days) => {
                        Column::Date(located(days, table.chunks(), index, |day| {
                            date_from_days(i64::from(day), Precision::Day)
                        })?)
                    }
                    PublicColumn::DateTime(seconds) => {
                        Column::DateTime(located(seconds, table.chunks(), index, |seconds| {
                            datetime_from_seconds(seconds, Precision::Second)
                        })?)
                    }
                    PublicColumn::List(_) => unreachable!("a list column is refused above"),
                })
            })
            .collect::<Result<Vec<_>, TableError>>()?;
        Self::new(schema, columns, table.rows())
    }

    /// The same table with `column` appended as the last field of `schema`,
    /// which declares the existing fields before it.
    pub fn appended(&self, schema: TableSchema, column: Column) -> Result<Self, TableError> {
        let mut columns = self.columns.clone();
        columns.push(Arc::new(column));
        Self::shared(schema, columns, self.rows)
    }

    /// The columns at `indices`, in that order, as `schema` declares them.
    pub fn project(&self, schema: TableSchema, indices: &[usize]) -> Result<Self, TableError> {
        let columns = indices
            .iter()
            .map(|&index| self.columns.get(index).cloned())
            .collect::<Option<Vec<_>>>()
            .ok_or(TableError::SchemaMismatch { batch: 0 })?;
        Self::shared(schema, columns, self.rows)
    }

    /// The same table with each column that `replace` gives a new one for
    /// replaced, or `None` when it gives none.
    pub fn replaced(
        &self,
        replace: impl Fn(&Column) -> Option<Column>,
    ) -> Option<Result<Self, TableError>> {
        let mut changed = false;
        let columns = self
            .columns
            .iter()
            .map(|column| match replace(column) {
                Some(column) => {
                    changed = true;
                    Arc::new(column)
                }
                None => Arc::clone(column),
            })
            .collect();
        changed.then(|| Self::shared(self.schema.clone(), columns, self.rows))
    }

    /// Column `index`, which must be below the schema's width.
    pub fn column(&self, index: usize) -> &Column {
        &self.columns[index]
    }

    /// Records `range` of the table, copied into a table of their own.
    pub fn rows(&self, range: std::ops::Range<usize>) -> Result<Self, TableError> {
        let rows = range.len();
        if range.end > self.rows {
            return Err(TableError::SchemaMismatch { batch: 0 });
        }
        let columns = self
            .columns
            .iter()
            .map(|column| column.slice(range.clone()))
            .collect();
        Self::new(self.schema.clone(), columns, rows)
    }
}

/// Decode each present value of `values`, a column of a table whose rows
/// split into batches of `chunks` rows; a value `decode` refuses names its
/// batch, `column` and row within that batch.
fn located<V: Copy + Default, T>(
    values: &Values<V>,
    chunks: &[usize],
    column: usize,
    decode: impl Fn(V) -> Option<T>,
) -> Result<Vec<Option<T>>, TableError> {
    let mut decoded = Vec::with_capacity(values.len());
    let mut start = 0;
    for (batch, &rows) in chunks.iter().enumerate() {
        for row in 0..rows {
            decoded.push(match values.get(start + row) {
                Some(value) => {
                    Some(decode(value).ok_or(TableError::TemporalRange { batch, column, row })?)
                }
                None => None,
            });
        }
        start += rows;
    }
    Ok(decoded)
}

impl TableAccess for ColumnTable {
    type Error = Infallible;

    /// The declared schema, independent of the row count.
    fn schema(&self) -> &TableSchema {
        &self.schema
    }

    /// The records, also of a table without columns.
    fn row_count(&self) -> usize {
        self.rows
    }

    /// Borrow one cell; admission has already checked its type and value.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        if row >= self.rows || column >= self.columns.len() {
            return Err(CellError::OutOfBounds { row, column });
        }
        Ok(self.columns[column].cell(row))
    }

    /// Lend a column's typed storage whole (#4190).
    fn column(&self, column: usize) -> Option<ColumnRef<'_>> {
        self.columns.get(column)?.storage()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_utf8_offsets_preserve_real_integer_boundaries() {
        let maximum = i32::MAX as usize;
        for lengths in [vec![], vec![maximum - 1], vec![maximum], vec![maximum, 0]] {
            assert!(fits_utf8_offsets(lengths));
        }
        for lengths in [vec![maximum, 1], vec![usize::MAX], vec![usize::MAX, 1]] {
            assert!(!fits_utf8_offsets(lengths));
        }
    }
}
