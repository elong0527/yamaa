//! The tables every host receives: a run's output and report, issues, lint
//! findings, self-test cases and ODM clinical items.
//!
//! A public table holds its columns in the engine's own storage
//! ([`crate::columns`]), each value in its public form: a date as days and a
//! datetime as whole seconds since 1970-01-01, with no zone. Collected
//! temporal precision stays in the execution table. The table also records
//! how its rows split into the record batches of its Arrow form, which the
//! table builds once, on first use, for the hosts that take Arrow.
use crate::columns::{Text, Validity, Values};
use crate::epoch::{date_days, datetime_seconds};
#[cfg(feature = "arrow")]
use arrow_array::RecordBatch;
#[cfg(feature = "arrow")]
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use std::num::NonZeroUsize;
use std::{convert::Infallible, fmt, ops::Range, sync::Arc};
use yamaa_core::{
    limits::Limits,
    table::{ColumnReader, TableAccess, TableSchema, ValueRef},
    value::ColumnType,
};

/// A public table that could not be built, without disclosing raw input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    Limit {
        resource: &'static str,
        limit: usize,
        required: Option<usize>,
    },
    Representation {
        representation: &'static str,
    },
    InvalidTable,
    Internal,
}

impl fmt::Display for TableError {
    /// Stable host errors never disclose raw input or panic payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Limit { .. } => "table geometry exceeds fixed limit",
            Self::Representation { representation } => representation,
            Self::InvalidTable => "invalid table schema or value",
            Self::Internal => "internal table codec failure",
        })
    }
}

impl std::error::Error for TableError {}

pub(crate) fn limit(error: yamaa_core::limits::LimitExceeded) -> TableError {
    TableError::Limit {
        resource: error.resource,
        limit: error.limit,
        required: error.actual,
    }
}

/// One column of a public table.
#[derive(Clone, Debug, PartialEq)]
pub enum PublicColumn {
    Str(Text),
    Int(Values<i64>),
    Float(Values<f64>),
    /// Days since 1970-01-01.
    Date(Values<i32>),
    /// Whole seconds since 1970-01-01T00:00:00, with no zone.
    DateTime(Values<i64>),
    /// A list of values of one type in each row, which only engine-built
    /// tables such as issues carry.
    List(Box<List>),
}

impl PublicColumn {
    /// The number of rows.
    pub fn len(&self) -> usize {
        match self {
            Self::Str(values) => values.len(),
            Self::Int(values) | Self::DateTime(values) => values.len(),
            Self::Float(values) => values.len(),
            Self::Date(values) => values.len(),
            Self::List(lists) => lists.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The column type of a column the language profile has: every column
    /// but a list.
    pub fn kind(&self) -> Option<ColumnType> {
        Some(match self {
            Self::Str(_) => ColumnType::Str,
            Self::Int(_) => ColumnType::Int,
            Self::Float(_) => ColumnType::Float,
            Self::Date(_) => ColumnType::Date,
            Self::DateTime(_) => ColumnType::DateTime,
            Self::List(_) => return None,
        })
    }

    /// The language name of the column's type: `str`, `int`, `float`,
    /// `date`, `datetime`, or `list<T>` for a list of `T`.
    pub fn type_name(&self) -> String {
        match self {
            Self::List(lists) => format!("list<{}>", lists.items.type_name()),
            column => column
                .kind()
                .expect("every column but a list has a column type")
                .name()
                .to_owned(),
        }
    }

    /// Whether the value at `row` is missing.
    pub fn is_missing(&self, row: usize) -> bool {
        !match self {
            Self::Str(values) => values.validity().get(row),
            Self::Int(values) | Self::DateTime(values) => values.validity().get(row),
            Self::Float(values) => values.validity().get(row),
            Self::Date(values) => values.validity().get(row),
            Self::List(lists) => lists.validity.get(row),
        }
    }

    /// An empty column of the same type, a list keeping its item type.
    fn emptied(&self) -> Self {
        match self {
            Self::Str(_) => Self::Str(Text::default()),
            Self::Int(_) => Self::Int(Values::default()),
            Self::Float(_) => Self::Float(Values::default()),
            Self::Date(_) => Self::Date(Values::default()),
            Self::DateTime(_) => Self::DateTime(Values::default()),
            Self::List(lists) => Self::List(Box::new(List::new(lists.items.emptied()))),
        }
    }

    /// Append the rows `rows` of `other`, a column of the same type.
    pub(crate) fn extend(&mut self, other: &Self, rows: Range<usize>) {
        fn values<T: Copy + Default>(into: &mut Values<T>, from: &Values<T>, rows: Range<usize>) {
            for row in rows {
                into.push(from.get(row));
            }
        }
        match (self, other) {
            (Self::Str(into), Self::Str(from)) => {
                for row in rows {
                    into.push(from.get(row));
                }
            }
            (Self::Int(into), Self::Int(from)) | (Self::DateTime(into), Self::DateTime(from)) => {
                values(into, from, rows)
            }
            (Self::Float(into), Self::Float(from)) => values(into, from, rows),
            (Self::Date(into), Self::Date(from)) => values(into, from, rows),
            (Self::List(into), Self::List(from)) => {
                for row in rows {
                    let items = from.get(row);
                    if let Some(items) = &items {
                        into.items.extend(&from.items, items.clone());
                    }
                    into.close(items.is_some());
                }
            }
            _ => unreachable!("only columns of one type are joined"),
        }
    }

    /// The text bytes that the rows `rows` hold, a list counting its items'.
    fn text_bytes(&self, rows: Range<usize>) -> usize {
        match self {
            Self::Str(values) => values.bytes_in(rows),
            Self::List(lists) => match (lists.offsets.get(rows.start), lists.offsets.get(rows.end))
            {
                (Some(&start), Some(&end)) => lists.items.text_bytes(start..end),
                _ => 0,
            },
            _ => 0,
        }
    }
}

/// The lists of a list column: list `i` holds the items
/// `offsets[i]..offsets[i + 1]`, and a missing list holds none.
#[derive(Clone, Debug, PartialEq)]
pub struct List {
    offsets: Vec<usize>,
    validity: Validity,
    items: PublicColumn,
}

impl List {
    /// An empty list column whose items are columns like `items`, which
    /// holds no rows.
    pub fn new(items: PublicColumn) -> Self {
        debug_assert!(items.is_empty());
        Self {
            offsets: vec![0],
            validity: Validity::default(),
            items,
        }
    }

    /// The items column, to append the next list's items to.
    pub fn items_mut(&mut self) -> &mut PublicColumn {
        &mut self.items
    }

    /// End one list, holding the items appended since the previous one, or
    /// a missing list, which holds none.
    pub fn close(&mut self, present: bool) {
        debug_assert!(present || *self.offsets.last().unwrap() == self.items.len());
        self.validity.push(present);
        self.offsets.push(self.items.len());
    }

    /// A list of text items in each row, or a missing list.
    pub fn text<'a>(lists: impl IntoIterator<Item = Option<Vec<&'a str>>>) -> Self {
        let mut column = Self::new(PublicColumn::Str(Text::default()));
        for list in lists {
            if let (Some(items), PublicColumn::Str(text)) = (&list, &mut column.items) {
                for item in items {
                    text.push(Some(item));
                }
            }
            column.close(list.is_some());
        }
        column
    }

    /// A list of integer items in each row, or a missing list.
    pub fn int(lists: impl IntoIterator<Item = Option<Vec<i64>>>) -> Self {
        let mut column = Self::new(PublicColumn::Int(Values::default()));
        for list in lists {
            if let (Some(items), PublicColumn::Int(values)) = (&list, &mut column.items) {
                for item in items {
                    values.push(Some(*item));
                }
            }
            column.close(list.is_some());
        }
        column
    }

    /// The number of lists.
    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The items of list `row`, or `None` when it is missing.
    pub fn get(&self, row: usize) -> Option<Range<usize>> {
        self.validity
            .get(row)
            .then(|| self.offsets[row]..self.offsets[row + 1])
    }

    pub fn items(&self) -> &PublicColumn {
        &self.items
    }

    /// Each list's first item, then the end of the last.
    pub fn offsets(&self) -> &[usize] {
        &self.offsets
    }

    pub fn validity(&self) -> &Validity {
        &self.validity
    }
}

/// Rows per record batch of a run's output and report.
const BATCH_ROWS: usize = 65_536;

/// An owned immutable public table, cheap to clone: clones share the
/// columns and the Arrow form.
#[derive(Clone, Debug)]
pub struct PublicTable {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    names: Vec<String>,
    columns: Vec<PublicColumn>,
    rows: usize,
    /// The rows of each record batch of the Arrow form, in order.
    chunks: Vec<usize>,
    #[cfg(feature = "arrow")]
    arrow: std::sync::OnceLock<crate::arrow_bridge::ArrowView>,
}

impl PartialEq for PublicTable {
    /// The same names, columns and batches; the Arrow form follows from them.
    fn eq(&self, other: &Self) -> bool {
        let (left, right) = (&self.inner, &other.inner);
        left.names == right.names
            && left.columns == right.columns
            && left.rows == right.rows
            && left.chunks == right.chunks
    }
}

impl PublicTable {
    /// A table of `columns`, each with its name, as one record batch.
    pub fn new(columns: Vec<(String, PublicColumn)>) -> Result<Self, TableError> {
        let rows = columns.first().map_or(0, |(_, column)| column.len());
        Self::chunked(columns, vec![rows])
    }

    /// A table of `columns` whose rows split into record batches of
    /// `chunks` rows: a table without columns has as many rows as its
    /// batches hold. Every column has that many rows, the geometry is
    /// within the fixed limits, and each batch's text in each column fits
    /// the signed 32-bit offsets of Arrow's Utf8, which the Arrow form uses.
    pub fn chunked(
        columns: Vec<(String, PublicColumn)>,
        chunks: Vec<usize>,
    ) -> Result<Self, TableError> {
        let rows = chunks
            .iter()
            .try_fold(0usize, |sum, &rows| sum.checked_add(rows))
            .ok_or(TableError::Limit {
                resource: "table_rows",
                limit: Limits::TABLE_ROWS,
                required: None,
            })?;
        Limits::table(rows, columns.len()).map_err(limit)?;
        Self::assemble(columns, chunks, rows)
    }

    /// `columns` of `rows` rows each in record batches of `chunks` rows,
    /// once their geometry is admitted: every column has that many rows and
    /// each batch's text in each column fits Arrow's Utf8 offsets.
    fn assemble(
        columns: Vec<(String, PublicColumn)>,
        chunks: Vec<usize>,
        rows: usize,
    ) -> Result<Self, TableError> {
        if columns.iter().any(|(_, column)| column.len() != rows) {
            return Err(TableError::InvalidTable);
        }
        let mut start = 0;
        for &length in &chunks {
            for (_, column) in &columns {
                if i32::try_from(column.text_bytes(start..start + length)).is_err() {
                    return Err(TableError::Representation {
                        representation: crate::column_table::UTF8_OFFSETS,
                    });
                }
            }
            start += length;
        }
        let (names, columns) = columns.into_iter().unzip();
        Ok(Self {
            inner: Arc::new(Inner {
                names,
                columns,
                rows,
                chunks,
                #[cfg(feature = "arrow")]
                arrow: std::sync::OnceLock::new(),
            }),
        })
    }

    /// Materialize the checked projection `projection` of `table`, such as a
    /// run's output or report, once. Each record batch holds 65,536 rows, the
    /// last the rest, and an empty table has one empty batch. A value whose
    /// type is not its column's is missing, and a temporal value keeps its
    /// civil fields, not its collected precision.
    pub fn from_table<T: TableAccess<Error = Infallible>>(
        table: &T,
        projection: &[usize],
    ) -> Result<Self, TableError> {
        Self::from_partitions(table, projection, None)
    }

    /// [`Self::from_table`] for a table held in consecutive partitions of
    /// at most `partition` rows, such as a run's output admitted a partition
    /// at a time (D264): each partition is admitted on its own, the whole
    /// table when `partition` is none. The record batches are the same.
    pub fn from_partitions<T: TableAccess<Error = Infallible>>(
        table: &T,
        projection: &[usize],
        partition: Option<NonZeroUsize>,
    ) -> Result<Self, TableError> {
        let logical = check_table(table, projection, partition)?;
        Self::from_admitted(table, projection, &logical)
    }

    /// [`Self::from_partitions`] of a table [`check_table`] admitted under
    /// `projection`, with the logical schema it returned, which a host view
    /// builds when it is first read (#4201). Every check the build makes,
    /// the admission made first.
    pub fn from_admitted<T: TableAccess<Error = Infallible>>(
        table: &T,
        projection: &[usize],
        logical: &TableSchema,
    ) -> Result<Self, TableError> {
        let rows = table.row_count();
        let chunks = if rows == 0 {
            vec![0]
        } else {
            let mut chunks = vec![BATCH_ROWS; (rows - 1) / BATCH_ROWS];
            chunks.push(rows - chunks.len() * BATCH_ROWS);
            chunks
        };
        let selected = logical.columns().to_vec();
        Self::from_rows(table, projection, selected, 0..rows, chunks)
    }

    /// Build only the requested rows after the table's admission.
    fn from_rows<T: TableAccess<Error = Infallible>>(
        table: &T,
        projection: &[usize],
        selected: Vec<yamaa_core::table::Column>,
        rows: Range<usize>,
        chunks: Vec<usize>,
    ) -> Result<Self, TableError> {
        let columns = selected
            .into_iter()
            .zip(projection)
            .map(|(field, &index)| {
                let reader = ColumnReader::new(table, index);
                let len = rows.len();
                let column = match field.kind {
                    ColumnType::Str => {
                        // The arena's size, from the text bounds a view lends.
                        let bytes = reader
                            .view()
                            .map_or(0, |view| rows.clone().map(|row| view.text_len(row)).sum());
                        let mut text = Text::with_capacity(len, bytes);
                        fill(&reader, rows.clone(), |value| {
                            text.push(match value {
                                ValueRef::Str(text) => Some(text),
                                _ => None,
                            })
                        });
                        PublicColumn::Str(text)
                    }
                    ColumnType::Int => {
                        let mut values = Values::with_capacity(len);
                        fill(&reader, rows.clone(), |value| {
                            values.push(match value {
                                ValueRef::Int(value) => Some(value),
                                _ => None,
                            })
                        });
                        PublicColumn::Int(values)
                    }
                    ColumnType::Float => {
                        let mut values = Values::with_capacity(len);
                        fill(&reader, rows.clone(), |value| {
                            values.push(match value {
                                ValueRef::Float(value) => Some(value.get()),
                                _ => None,
                            })
                        });
                        PublicColumn::Float(values)
                    }
                    ColumnType::Date => {
                        let mut values = Values::with_capacity(len);
                        fill(&reader, rows.clone(), |value| {
                            values.push(match value {
                                ValueRef::Date(value) => Some(date_days(value)),
                                _ => None,
                            })
                        });
                        PublicColumn::Date(values)
                    }
                    ColumnType::DateTime => {
                        let mut values = Values::with_capacity(len);
                        fill(&reader, rows.clone(), |value| {
                            values.push(match value {
                                ValueRef::DateTime(value) => Some(datetime_seconds(value)),
                                _ => None,
                            })
                        });
                        PublicColumn::DateTime(values)
                    }
                };
                (field.name, column)
            })
            .collect();
        Self::assemble(columns, chunks, rows.len())
    }

    /// `tables`, which have the same column names and types, stacked in
    /// order, each keeping its record batches.
    pub fn stack(tables: &[&PublicTable]) -> Result<Self, TableError> {
        let Some(first) = tables.first() else {
            return Self::new(Vec::new());
        };
        let same = |table: &&PublicTable| {
            table.inner.names == first.inner.names
                && table
                    .inner
                    .columns
                    .iter()
                    .zip(&first.inner.columns)
                    .all(|(left, right)| left.type_name() == right.type_name())
        };
        if !tables.iter().all(same) {
            return Err(TableError::InvalidTable);
        }
        let rows = tables
            .iter()
            .try_fold(0usize, |sum, table| sum.checked_add(table.rows()))
            .ok_or(TableError::Limit {
                resource: "table_rows",
                limit: Limits::TABLE_ROWS,
                required: None,
            })?;
        Limits::table(rows, first.width()).map_err(limit)?;
        let columns = (0..first.width())
            .map(|index| {
                let mut column = first.column(index).emptied();
                for table in tables {
                    column.extend(table.column(index), 0..table.rows());
                }
                (first.name(index).to_owned(), column)
            })
            .collect();
        let chunks = tables
            .iter()
            .flat_map(|table| table.chunks().iter().copied())
            .collect();
        Self::chunked(columns, chunks)
    }

    /// The number of rows.
    pub fn rows(&self) -> usize {
        self.inner.rows
    }

    /// The number of columns.
    pub fn width(&self) -> usize {
        self.inner.columns.len()
    }

    /// The name of column `index`.
    pub fn name(&self, index: usize) -> &str {
        &self.inner.names[index]
    }

    /// The column names, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.inner.names.iter().map(String::as_str)
    }

    pub fn column(&self, index: usize) -> &PublicColumn {
        &self.inner.columns[index]
    }

    /// The rows of each record batch of the Arrow form, in order.
    pub fn chunks(&self) -> &[usize] {
        &self.inner.chunks
    }

    /// The Arrow form, built on first use and shared by every clone.
    #[cfg(feature = "arrow")]
    fn arrow(&self) -> &crate::arrow_bridge::ArrowView {
        self.inner
            .arrow
            .get_or_init(|| crate::arrow_bridge::ArrowView::of(self))
    }

    /// The Arrow fields in declaration order.
    #[cfg(feature = "arrow")]
    pub fn columns(&self) -> &arrow_schema::Fields {
        self.arrow().schema().fields()
    }

    /// The Arrow schema: each field nullable, a string `Utf8`, an integer
    /// `Int64`, a float `Float64`, a date `Date32`, a datetime
    /// `Timestamp(Second)` and a list a `List` of items named `item`.
    #[cfg(feature = "arrow")]
    pub fn schema(&self) -> &arrow_schema::SchemaRef {
        self.arrow().schema()
    }

    /// The Arrow record batches, in record order.
    #[cfg(feature = "arrow")]
    pub fn batches(&self) -> &[arrow_array::RecordBatch] {
        self.arrow().batches()
    }

    /// Each export owns an independent cursor and shared immutable buffer refs.
    #[cfg(feature = "arrow")]
    pub fn stream(&self) -> arrow_array::ffi_stream::FFI_ArrowArrayStream {
        crate::arrow_bridge::export_stream(self.schema().clone(), self.batches().to_vec())
    }
}

/// The public Arrow type of a column of the language's type `kind` in a
/// table the engine built: text is `Utf8`, as every batch it builds holds it.
#[cfg(feature = "arrow")]
fn public_type(kind: ColumnType) -> DataType {
    match kind {
        ColumnType::Str => DataType::Utf8,
        ColumnType::Int => DataType::Int64,
        ColumnType::Float => DataType::Float64,
        ColumnType::Date => DataType::Date32,
        ColumnType::DateTime => DataType::Timestamp(arrow_schema::TimeUnit::Second, None),
    }
}

/// The public schema of a table the engine built with the logical
/// `columns`: what [`PublicTable::from_table`] gives it.
#[cfg(feature = "arrow")]
fn public_schema(columns: &[yamaa_core::table::Column]) -> SchemaRef {
    Arc::new(Schema::new(
        columns
            .iter()
            .map(|column| Field::new(&column.name, public_type(column.kind), true))
            .collect::<Vec<_>>(),
    ))
}

/// The public schema of the output `compiled` builds, known before its run:
/// the schema of the table its run's host view holds, and of every batch a
/// stream of it hands out (#3697). None when a projected column is not one
/// the specification declares.
#[cfg(feature = "arrow")]
pub fn output_schema(
    compiled: &yamaa_core::specification::PreparedSpecification,
) -> Option<SchemaRef> {
    let columns = compiled
        .projection()
        .iter()
        .map(|name| {
            compiled
                .output()
                .columns()
                .iter()
                .find(|column| &column.name == name)
                .cloned()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(public_schema(&columns))
}

/// Push the value of each of `rows` of `column`, which an admitted table
/// holds, in order.
fn fill<'a, T: TableAccess<Error = Infallible>>(
    column: &ColumnReader<'a, T>,
    rows: Range<usize>,
    mut push: impl FnMut(ValueRef<'a>),
) {
    let visit = |value| {
        push(value);
        Ok(())
    };
    column
        .try_for_each(rows, visit, |_| ())
        .expect("validated coordinate");
}

/// The columns `projection` names of `table`, as the host view's logical
/// schema; a position outside the table, or a repeated name, is refused.
fn projected<T: TableAccess>(table: &T, projection: &[usize]) -> Result<TableSchema, TableError> {
    TableSchema::new(
        projection
            .iter()
            .map(|&index| {
                table
                    .schema()
                    .columns()
                    .get(index)
                    .cloned()
                    .ok_or(TableError::InvalidTable)
            })
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|_| TableError::InvalidTable)
}

/// Admission shared by whole-table and deferred output, before any batch
/// is handed to a host: the table whole, or each of its partitions of at most
/// `partition` rows for an output held in partitions (D264).
pub fn check_table<T: TableAccess<Error = Infallible>>(
    table: &T,
    projection: &[usize],
    partition: Option<NonZeroUsize>,
) -> Result<TableSchema, TableError> {
    let logical = projected(table, projection)?;
    let columns = table.schema().columns();
    yamaa_core::limits::Limits::partitioned(table.row_count(), partition, columns.len())
        .map_err(limit)?;
    for (column, field) in columns.iter().enumerate() {
        if field.kind != ColumnType::Str {
            continue;
        }
        let column = ColumnReader::new(table, column);
        for start in (0..table.row_count()).step_by(BATCH_ROWS) {
            let end = table.row_count().min(start + BATCH_ROWS);
            // The batch's text bytes: its offsets fit when their last does,
            // as lengths only add.
            let mut bytes = 0usize;
            let sum = |value| {
                if let ValueRef::Str(text) = value {
                    bytes = bytes.saturating_add(text.len());
                }
                Ok(())
            };
            // A port that refuses a row it holds cannot be built.
            column.try_for_each(start..end, sum, |_| TableError::InvalidTable)?;
            if !crate::column_table::fits_utf8_offsets([bytes]) {
                return Err(TableError::Representation {
                    representation: crate::column_table::UTF8_OFFSETS,
                });
            }
        }
    }
    Ok(logical)
}

/// The rows of an accepted output handed out as public batches, one built per
/// pull (#3697): a run whose output is read as a stream holds its accepted
/// rows here instead of a [`PublicTable`]. Nothing is built ahead of a pull,
/// and nothing built is kept.
///
/// The batches hold the values, in the order and with the types,
/// [`PublicTable::from_table`] gives the same table and projection, under the
/// schema [`PublicBatches::schema`] gives before any batch is built. Each
/// holds at most `rows` records, and none crosses the boundary between two
/// of the whole table's batches, so a stream of batches can always be built
/// when the whole table can.
#[cfg(feature = "arrow")]
pub struct PublicBatches<T = yamaa_core::dataset::Dataset> {
    table: T,
    projection: Vec<usize>,
    logical: TableSchema,
    schema: SchemaRef,
    rows: usize,
    start: usize,
    done: bool,
}

#[cfg(feature = "arrow")]
impl<T: TableAccess<Error = Infallible>> PublicBatches<T> {
    /// The batches of `table`'s columns `projection`, at most `rows` records
    /// each. `table` is refused here, before any batch is built, exactly when
    /// [`PublicTable::from_partitions`] refuses it with the same `partition`
    /// (see [`PublicBatches::check`]).
    pub fn new(
        table: T,
        projection: Vec<usize>,
        rows: NonZeroUsize,
        partition: Option<NonZeroUsize>,
    ) -> Result<Self, TableError> {
        let logical = Self::check(&table, &projection, partition)?;
        Ok(Self {
            schema: public_schema(logical.columns()),
            logical,
            projection,
            table,
            rows: rows.get().min(BATCH_ROWS),
            start: 0,
            done: false,
        })
    }

    /// Whether [`PublicTable::from_partitions`] can build the host view of
    /// `table`'s columns `projection`, admitted whole or in partitions of at
    /// most `partition` rows, checked without building it: the projection
    /// and the table's geometry, then the one refusal that depends on the
    /// values, a batch whose text in one column exceeds Arrow `Utf8`'s
    /// 32-bit offsets. The whole table is rebuilt in batches of 65,536
    /// records, every column of each, so each such batch of each text column
    /// is measured. Returns the projection's logical schema.
    pub fn check(
        table: &T,
        projection: &[usize],
        partition: Option<NonZeroUsize>,
    ) -> Result<TableSchema, TableError> {
        check_table(table, projection, partition)
    }

    /// The public schema of every batch, known before any is built.
    pub fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    /// The records all the batches hold.
    pub fn rows(&self) -> usize {
        self.table.row_count()
    }

    /// Whether every batch has been built, so the next pull ends.
    pub fn ended(&self) -> bool {
        self.done
    }

    /// Build the public batch of rows `start..start + rows`.
    fn build(&self, start: usize, rows: usize) -> Result<RecordBatch, TableError> {
        let table = PublicTable::from_rows(
            &self.table,
            &self.projection,
            self.logical.columns().to_vec(),
            start..start + rows,
            vec![rows],
        )?;
        table.batches().first().cloned().ok_or(TableError::Internal)
    }
}

#[cfg(feature = "arrow")]
impl<T: TableAccess<Error = Infallible>> Iterator for PublicBatches<T> {
    type Item = Result<RecordBatch, TableError>;

    /// Build the next batch, or end. A table with no record has one empty
    /// batch, as the whole table does; a batch that cannot be built ends the
    /// batches.
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let total = self.table.row_count();
        let boundary = (self.start / BATCH_ROWS + 1) * BATCH_ROWS;
        let end = total.min(self.start + self.rows).min(boundary);
        let batch = self.build(self.start, end - self.start);
        self.start = end;
        self.done = batch.is_err() || end >= total;
        Some(batch)
    }
}

#[cfg(feature = "arrow")]
impl<T: TableAccess<Error = Infallible>> std::iter::FusedIterator for PublicBatches<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    fn issues(rows: usize) -> Vec<(String, PublicColumn)> {
        let paths = (0..rows).map(|row| Some(format!("columns[{row}]")));
        let paths = paths.collect::<Vec<_>>();
        vec![
            (
                "spec_paths".into(),
                PublicColumn::List(Box::new(List::text(
                    paths
                        .iter()
                        .map(|path| path.as_deref().map(|path| vec![path])),
                ))),
            ),
            (
                "operand_route".into(),
                PublicColumn::List(Box::new(List::int(
                    (0..rows).map(|row| Some(vec![row as i64, -1])),
                ))),
            ),
        ]
    }

    #[test]
    #[cfg(feature = "arrow")]
    fn importing_public_results_preserves_batches_nested_lists_and_empty_schemas() {
        // The result importer is distinct from specification input admission:
        // public diagnostics can carry lists and zero-column row geometry.
        use crate::arrow_bridge::import_result_stream;
        let mut lists = List::new(PublicColumn::List(Box::new(List::int([]))));
        if let PublicColumn::List(inner) = lists.items_mut() {
            inner.items_mut().extend(
                &PublicColumn::Int([Some(1), None].into_iter().collect()),
                0..2,
            );
            inner.close(true);
            inner.close(false);
        }
        lists.close(true);
        lists.close(false);
        lists.close(true);
        let cases = [
            PublicTable::chunked(issues(5), vec![2, 0, 3]).unwrap(),
            PublicTable::new(vec![("nested".into(), PublicColumn::List(Box::new(lists)))]).unwrap(),
            PublicTable::chunked(
                vec![("typed_empty".into(), PublicColumn::Date(Values::default()))],
                vec![],
            )
            .unwrap(),
            PublicTable::chunked(Vec::new(), vec![2, 0, 3]).unwrap(),
        ];
        for (case, table) in cases.iter().enumerate() {
            assert_eq!(
                import_result_stream(table.stream()).unwrap(),
                *table,
                "case {case}"
            );
        }
        assert!(import_result_stream(crate::arrow_bridge::FFI_ArrowArrayStream::empty()).is_err());
    }

    #[test]
    #[cfg(feature = "arrow")]
    fn result_imports_share_float_and_temporal_boundary_admission() {
        // The result importer must uphold the scalar invariants its preview
        // and host conversions rely on, including inside diagnostic lists.
        use crate::arrow_bridge::import_result_stream;
        let table = |column| PublicTable::new(vec![("value".into(), column)]).unwrap();
        let floats = table(PublicColumn::Float(
            [
                Some(f64::NAN),
                Some(f64::INFINITY),
                Some(f64::NEG_INFINITY),
                Some(-0.0),
                None,
            ]
            .into_iter()
            .collect(),
        ));
        let expected = table(PublicColumn::Float(
            [None, None, None, Some(-0.0), None].into_iter().collect(),
        ));
        assert_eq!(import_result_stream(floats.stream()), Ok(expected));
        // Epoch limits derived from 0001-01-01 and 9999-12-31, inclusive;
        // the last datetime is the last second of that last day.
        for column in [
            PublicColumn::Date(
                [Some(-719_162), Some(2_932_896), None]
                    .into_iter()
                    .collect(),
            ),
            PublicColumn::DateTime(
                [Some(-62_135_596_800), Some(253_402_300_799), None]
                    .into_iter()
                    .collect(),
            ),
        ] {
            let expected = table(column);
            assert_eq!(import_result_stream(expected.stream()), Ok(expected));
        }
        for column in [
            PublicColumn::Date([Some(-719_163)].into_iter().collect()),
            PublicColumn::Date([Some(2_932_897)].into_iter().collect()),
            PublicColumn::DateTime([Some(-62_135_596_801)].into_iter().collect()),
            PublicColumn::DateTime([Some(253_402_300_800)].into_iter().collect()),
        ] {
            assert_eq!(
                import_result_stream(table(column).stream()),
                Err(TableError::InvalidTable)
            );
        }
        let mut nested = List::new(PublicColumn::Date(Values::default()));
        nested.items_mut().extend(
            &PublicColumn::Date([Some(-719_163)].into_iter().collect()),
            0..1,
        );
        nested.close(true);
        assert_eq!(
            import_result_stream(table(PublicColumn::List(Box::new(nested))).stream()),
            Err(TableError::InvalidTable)
        );
    }

    #[test]
    #[cfg(feature = "arrow")]
    fn result_imports_refuse_empty_and_duplicate_column_names_even_without_rows() {
        // Foreign result schemas must retain every column when projected to a
        // host dictionary; the owned public constructor otherwise trusts names.
        use crate::arrow_bridge::import_result_stream;
        for names in [vec![""], vec!["same", "same"]] {
            for rows in [0, 1] {
                let columns = names
                    .iter()
                    .map(|name| {
                        (
                            (*name).to_owned(),
                            PublicColumn::Int(std::iter::repeat_n(Some(1), rows).collect()),
                        )
                    })
                    .collect();
                let table = PublicTable::chunked(columns, vec![rows]).unwrap();
                assert_eq!(
                    import_result_stream(table.stream()),
                    Err(TableError::InvalidTable),
                    "names {names:?}, rows {rows}"
                );
            }
        }
    }

    #[test]
    fn engine_built_tables_admit_list_columns_and_count_rows_across_batches() {
        let mut columns = issues(5);
        let table = PublicTable::chunked(columns.clone(), vec![2, 0, 3]).unwrap();
        assert_eq!(table.rows(), 5);
        assert_eq!(table.chunks(), [2, 0, 3]);
        assert_eq!(
            table.names().collect::<Vec<_>>(),
            ["spec_paths", "operand_route"]
        );
        assert_eq!(table.column(0).type_name(), "list<str>");
        assert_eq!(table.column(1).type_name(), "list<int>");
        let PublicColumn::List(routes) = table.column(1) else {
            panic!("a list column")
        };
        assert_eq!(routes.get(4), Some(8..10));
        let items = (0..5).flat_map(|row| [Some(row), Some(-1)]).collect();
        assert_eq!(routes.items(), &PublicColumn::Int(items));
        // Batches that do not cover every row, and columns of other lengths,
        // form no table.
        assert_eq!(
            PublicTable::chunked(columns.clone(), vec![2, 2]),
            Err(TableError::InvalidTable)
        );
        columns[1].1 = PublicColumn::Int(Values::default());
        assert_eq!(PublicTable::new(columns), Err(TableError::InvalidTable));
        let empty = PublicTable::new(Vec::new()).unwrap();
        assert_eq!((empty.rows(), empty.chunks()), (0, &[0][..]));
    }

    /// The Arrow form keeps each batch, names a list's items `item`, as
    /// `schema()` documents, and holds a validity buffer only for a batch
    /// with a missing value.
    #[test]
    #[cfg(feature = "arrow")]
    fn the_arrow_form_keeps_batches_list_items_and_validity() {
        use arrow_array::{Array, Int64Array, ListArray};
        use arrow_schema::{DataType, Field};
        let table = PublicTable::chunked(issues(5), vec![2, 0, 3]).unwrap();
        let list = |item| DataType::List(Arc::new(Field::new("item", item, true)));
        let fields = table
            .columns()
            .iter()
            .map(|field| {
                (
                    field.name().as_str(),
                    field.data_type().clone(),
                    field.is_nullable(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            fields,
            [
                ("spec_paths", list(DataType::Utf8), true),
                ("operand_route", list(DataType::Int64), true)
            ]
        );
        let rows = table.batches().iter().map(|batch| batch.num_rows());
        assert_eq!(rows.collect::<Vec<_>>(), [2, 0, 3]);
        let routes = table.batches()[2].column(1);
        let routes = routes.as_any().downcast_ref::<ListArray>().unwrap();
        let first = routes.value(0);
        let first = first.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(first.values().as_ref(), [2, -1]);
        assert!(routes.nulls().is_none());
        let values = [Some(1), Some(2), None].into_iter().collect();
        let column = ("A".to_owned(), PublicColumn::Int(values));
        let table = PublicTable::chunked(vec![column], vec![2, 1]).unwrap();
        let nulls = table
            .batches()
            .iter()
            .map(|batch| batch.column(0).nulls().cloned());
        let nulls = nulls.collect::<Vec<_>>();
        assert!(nulls[0].is_none());
        assert_eq!(nulls[1].as_ref().map(|nulls| nulls.null_count()), Some(1));
    }
}
