//! Arrow at the hosts' edge: a public table's Arrow form and its C Stream
//! export, and the import of a host frame's C Stream into a column table.
//!
//! A public table builds its Arrow form once; every export shares that
//! form's immutable buffers. A stream owns its reader, and every emitted C
//! Data array owns independent Arc references to its buffers, so releasing
//! the stream never invalidates already imported arrays. Arrow-rs callbacks
//! clear release on move/release, and dropping either an unconsumed or a
//! moved-from header is safe. An import copies the frame's values into the
//! engine's own columns and releases the stream. Foreign streams must
//! satisfy the Arrow C Data/Stream producer contract.
use crate::column_table::{limit, Column, ColumnTable, TableError};
use crate::columns::{Text, Validity};
use crate::epoch::{date_from_days, datetime_from_seconds};
use crate::public_table::{PublicColumn, PublicTable};
use crate::storage_api::StreamReader;
pub use arrow_array::ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream};
use arrow_array::{
    Array, ArrayRef, Date32Array, Float64Array, Int64Array, LargeStringArray, ListArray,
    RecordBatch, RecordBatchIterator, RecordBatchOptions, RecordBatchReader, StringArray,
    StringViewArray, TimestampSecondArray,
};
use arrow_buffer::{Buffer, NullBuffer, OffsetBuffer, ScalarBuffer};
use arrow_schema::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::{ops::Range, sync::Arc};
use yamaa_core::{
    limits::Limits,
    table::{Column as Declared, TableSchema},
    temporal::Precision,
    value::{ColumnType, FiniteFloat},
};

/// A public table's Arrow form: its schema and record batches.
#[derive(Debug)]
pub struct ArrowView {
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
}

impl ArrowView {
    /// Build `table`'s Arrow form: a nullable field for each column, and a
    /// record batch for each of the table's batches. A missing value holds
    /// no payload, and a batch without missing values has no validity
    /// buffer.
    pub(crate) fn of(table: &PublicTable) -> Self {
        let schema = Arc::new(Schema::new(
            (0..table.width())
                .map(|index| Field::new(table.name(index), data_type(table.column(index)), true))
                .collect::<Vec<_>>(),
        ));
        let mut start = 0;
        let batches = table
            .chunks()
            .iter()
            .map(|&rows| {
                let range = start..start + rows;
                start += rows;
                let arrays = (0..table.width())
                    .map(|index| array(table.column(index), range.clone()))
                    .collect();
                RecordBatch::try_new_with_options(
                    Arc::clone(&schema),
                    arrays,
                    &RecordBatchOptions::new().with_row_count(Some(rows)),
                )
                .expect("a public table's columns form each of its batches")
            })
            .collect();
        Self { schema, batches }
    }

    pub fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    pub fn batches(&self) -> &[RecordBatch] {
        &self.batches
    }
}

/// The Arrow type of a public column.
fn data_type(column: &PublicColumn) -> DataType {
    match column {
        PublicColumn::Str(_) => DataType::Utf8,
        PublicColumn::Int(_) => DataType::Int64,
        PublicColumn::Float(_) => DataType::Float64,
        PublicColumn::Date(_) => DataType::Date32,
        PublicColumn::DateTime(_) => DataType::Timestamp(TimeUnit::Second, None),
        PublicColumn::List(lists) => {
            DataType::List(Arc::new(Field::new("item", data_type(lists.items()), true)))
        }
    }
}

/// The validity of `rows`, or none when no value there is missing.
fn nulls(validity: &Validity, rows: Range<usize>) -> Option<NullBuffer> {
    validity
        .any_missing(rows.clone())
        .then(|| NullBuffer::from(rows.map(|row| validity.get(row)).collect::<Vec<_>>()))
}

/// `offsets` rebased to start at zero, as Arrow's signed 32-bit offsets.
fn rebased(offsets: &[usize]) -> OffsetBuffer<i32> {
    let base = offsets[0];
    OffsetBuffer::new(ScalarBuffer::from(
        offsets
            .iter()
            .map(|&offset| {
                i32::try_from(offset - base).expect("a public table's batch fits Utf8 offsets")
            })
            .collect::<Vec<_>>(),
    ))
}

/// The Arrow array of `rows` of `column`.
fn array(column: &PublicColumn, rows: Range<usize>) -> ArrayRef {
    fn text(values: &Text, rows: Range<usize>) -> StringArray {
        let offsets = &values.offsets()[rows.start..=rows.end];
        let bytes = &values.arena().as_bytes()[offsets[0]..offsets[offsets.len() - 1]];
        StringArray::new(
            rebased(offsets),
            Buffer::from_slice_ref(bytes),
            nulls(values.validity(), rows),
        )
    }
    match column {
        PublicColumn::Str(values) => Arc::new(text(values, rows)),
        PublicColumn::Int(values) => Arc::new(Int64Array::new(
            values.payloads()[rows.clone()].to_vec().into(),
            nulls(values.validity(), rows),
        )),
        PublicColumn::Float(values) => Arc::new(Float64Array::new(
            values.payloads()[rows.clone()].to_vec().into(),
            nulls(values.validity(), rows),
        )),
        PublicColumn::Date(values) => Arc::new(Date32Array::new(
            values.payloads()[rows.clone()].to_vec().into(),
            nulls(values.validity(), rows),
        )),
        PublicColumn::DateTime(values) => Arc::new(TimestampSecondArray::new(
            values.payloads()[rows.clone()].to_vec().into(),
            nulls(values.validity(), rows),
        )),
        PublicColumn::List(lists) => {
            let offsets = &lists.offsets()[rows.start..=rows.end];
            Arc::new(ListArray::new(
                Arc::new(Field::new("item", data_type(lists.items()), true)),
                rebased(offsets),
                array(lists.items(), offsets[0]..offsets[offsets.len() - 1]),
                nulls(lists.validity(), rows),
            ))
        }
    }
}

pub fn export_stream(schema: SchemaRef, batches: Vec<RecordBatch>) -> FFI_ArrowArrayStream {
    FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(
        batches.into_iter().map(Ok),
        schema,
    )))
}

/// Import the public result form, including diagnostic list columns. Unlike
/// an input frame this preserves public epoch values without interpreting them
/// as specification columns. Nonfinite floats are missing, and temporal
/// values outside years 1 to 9999 are refused, as at the input boundary.
/// Used to restore a host's serialized result table.
pub fn import_result_stream(
    stream: FFI_ArrowArrayStream,
) -> Result<PublicTable, crate::public_table::TableError> {
    use crate::public_table::TableError as Error;
    let reader = ArrowArrayStreamReader::try_new(stream).map_err(|_| Error::InvalidTable)?;
    let schema = reader.schema();
    TableSchema::validate_names(schema.fields().iter().map(|field| field.name().as_str()))
        .map_err(|_| Error::InvalidTable)?;
    let mut tables = Vec::new();
    let mut rows = 0usize;
    for batch in reader {
        let batch = batch.map_err(|_| Error::InvalidTable)?;
        rows = rows
            .checked_add(batch.num_rows())
            .ok_or(Error::InvalidTable)?;
        Limits::table(rows, schema.fields().len()).map_err(crate::public_table::limit)?;
        if batch.schema() != schema {
            return Err(Error::InvalidTable);
        }
        tables.push(result_batch(&batch, vec![batch.num_rows()])?);
    }
    if tables.is_empty() {
        tables.push(result_batch(&RecordBatch::new_empty(schema), Vec::new())?);
    }
    PublicTable::stack(&tables.iter().collect::<Vec<_>>())
}

fn result_batch(
    batch: &RecordBatch,
    chunks: Vec<usize>,
) -> Result<PublicTable, crate::public_table::TableError> {
    let columns = batch
        .schema()
        .fields()
        .iter()
        .zip(batch.columns())
        .map(|(field, array)| Ok((field.name().clone(), result_column(array.as_ref())?)))
        .collect::<Result<Vec<_>, _>>()?;
    PublicTable::chunked(columns, chunks)
}

fn result_column(array: &dyn Array) -> Result<PublicColumn, crate::public_table::TableError> {
    use crate::public_table::{List, TableError as Error};
    fn cast<T: 'static>(array: &dyn Array) -> Result<&T, Error> {
        array.as_any().downcast_ref().ok_or(Error::InvalidTable)
    }
    fn temporal<T: Copy + Default, V>(
        values: impl Iterator<Item = Option<T>>,
        admit: impl Fn(T) -> Option<V>,
    ) -> Result<crate::columns::Values<T>, Error> {
        values
            .map(|value| {
                value
                    .map(|value| admit(value).map(|_| value).ok_or(Error::InvalidTable))
                    .transpose()
            })
            .collect()
    }
    Ok(match array.data_type() {
        DataType::Utf8 => PublicColumn::Str(cast::<StringArray>(array)?.iter().collect()),
        DataType::Int64 => PublicColumn::Int(cast::<Int64Array>(array)?.iter().collect()),
        DataType::Float64 => PublicColumn::Float(
            cast::<Float64Array>(array)?
                .iter()
                .map(|value| value.and_then(FiniteFloat::new).map(FiniteFloat::get))
                .collect(),
        ),
        DataType::Date32 => {
            PublicColumn::Date(temporal(cast::<Date32Array>(array)?.iter(), |days| {
                date_from_days(i64::from(days), Precision::Day)
            })?)
        }
        DataType::Timestamp(TimeUnit::Second, None) => PublicColumn::DateTime(temporal(
            cast::<TimestampSecondArray>(array)?.iter(),
            |seconds| datetime_from_seconds(seconds, Precision::Second),
        )?),
        DataType::List(field) => {
            let array = cast::<ListArray>(array)?;
            let empty = arrow_array::new_empty_array(field.data_type());
            let mut lists = List::new(result_column(empty.as_ref())?);
            for row in 0..array.len() {
                if !array.is_null(row) {
                    let items = result_column(array.value(row).as_ref())?;
                    lists.items_mut().extend(&items, 0..items.len());
                }
                lists.close(!array.is_null(row));
            }
            PublicColumn::List(Box::new(lists))
        }
        _ => return Err(Error::InvalidTable),
    })
}

/// Consume a standard stream of public Arrow types into the engine's column
/// table: UTF-8 (including large and view arrays) as text, `Int64`,
/// `Float64`, `Date32` and zone-free `TimestampSecond`, a date with day and
/// a datetime with second precision. Other types require an explicit
/// preserving conversion by the caller before exporting. A nonfinite float
/// is missing. The whole stream is read before its values are converted,
/// and the stream is released before this returns.
pub fn import_public_stream(stream: FFI_ArrowArrayStream) -> Result<ColumnTable, TableError> {
    let reader = StreamReader::try_new(stream).map_err(TableError::Arrow)?;
    let schema = reader.schema();
    let logical = public_schema(&schema)?;
    let width = logical.columns().len();
    Limits::columns(width).map_err(limit)?;
    let mut rows = 0usize;
    let mut batches = Vec::new();
    for batch in reader {
        let batch = batch.map_err(TableError::Arrow)?;
        rows = rows
            .checked_add(batch.num_rows())
            .ok_or(TableError::Limit {
                resource: "table_rows",
                limit: Limits::TABLE_ROWS,
                required: None,
            })?;
        Limits::table(rows, width).map_err(limit)?;
        batches.push(batch);
    }
    let mut columns = logical
        .columns()
        .iter()
        .map(|field| Column::empty(field.kind))
        .collect::<Vec<_>>();
    for (index, batch) in batches.into_iter().enumerate() {
        if batch.schema() != schema {
            return Err(TableError::SchemaMismatch { batch: index });
        }
        for (column, (values, array)) in columns.iter_mut().zip(batch.columns()).enumerate() {
            append(values, array.as_ref(), index, column)?;
        }
    }
    ColumnTable::new(logical, columns, rows)
}

/// The column type of each public Arrow field.
fn public_schema(schema: &Schema) -> Result<TableSchema, TableError> {
    TableSchema::new(
        schema
            .fields()
            .iter()
            .enumerate()
            .map(|(column, field)| {
                let kind = match field.data_type() {
                    DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => ColumnType::Str,
                    DataType::Int64 => ColumnType::Int,
                    DataType::Float64 => ColumnType::Float,
                    DataType::Date32 => ColumnType::Date,
                    DataType::Timestamp(TimeUnit::Second, None) => ColumnType::DateTime,
                    _ => return Err(TableError::ArrayType { batch: 0, column }),
                };
                Ok(Declared {
                    name: field.name().clone(),
                    kind,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|_| TableError::SchemaMismatch { batch: 0 })
}

/// Append `array`, column `column` of batch `batch`, to `values`: a
/// temporal value outside the years 1 to 9999 is refused.
fn append(
    values: &mut Column,
    array: &dyn Array,
    batch: usize,
    column: usize,
) -> Result<(), TableError> {
    fn cast<T: 'static>(array: &dyn Array, batch: usize, column: usize) -> Result<&T, TableError> {
        array
            .as_any()
            .downcast_ref::<T>()
            .ok_or(TableError::ArrayType { batch, column })
    }
    let present = |row| !array.is_null(row);
    match values {
        Column::Str(text) => match array.data_type() {
            DataType::Utf8 => {
                let strings = cast::<StringArray>(array, batch, column)?;
                for row in 0..array.len() {
                    text.push(present(row).then(|| strings.value(row)));
                }
            }
            DataType::LargeUtf8 => {
                let strings = cast::<LargeStringArray>(array, batch, column)?;
                for row in 0..array.len() {
                    text.push(present(row).then(|| strings.value(row)));
                }
            }
            _ => {
                let strings = cast::<StringViewArray>(array, batch, column)?;
                for row in 0..array.len() {
                    text.push(present(row).then(|| strings.value(row)));
                }
            }
        },
        Column::Int(integers) => {
            let array = cast::<Int64Array>(array, batch, column)?;
            for row in 0..array.len() {
                integers.push(present(row).then(|| array.value(row)));
            }
        }
        Column::Float(floats) => {
            let array = cast::<Float64Array>(array, batch, column)?;
            for row in 0..array.len() {
                floats.push(
                    present(row)
                        .then(|| array.value(row))
                        .and_then(FiniteFloat::new)
                        .map(FiniteFloat::get),
                );
            }
        }
        Column::Date(dates) => {
            let days = cast::<Date32Array>(array, batch, column)?;
            for row in 0..array.len() {
                dates.push(if present(row) {
                    Some(
                        date_from_days(i64::from(days.value(row)), Precision::Day)
                            .ok_or(TableError::TemporalRange { batch, column, row })?,
                    )
                } else {
                    None
                });
            }
        }
        Column::DateTime(datetimes) => {
            let seconds = cast::<TimestampSecondArray>(array, batch, column)?;
            for row in 0..array.len() {
                datetimes.push(if present(row) {
                    Some(
                        datetime_from_seconds(seconds.value(row), Precision::Second)
                            .ok_or(TableError::TemporalRange { batch, column, row })?,
                    )
                } else {
                    None
                });
            }
        }
    }
    Ok(())
}
