//! Standard Arrow C Stream ownership shared by both installed hosts.
//!
//! A stream owns its reader. Every emitted C Data array owns independent Arc
//! references to its immutable buffers; releasing the stream never invalidates
//! already imported arrays. Arrow-rs callbacks clear release on move/release,
//! and dropping either an unconsumed or a moved-from header is safe. Foreign
//! streams must satisfy the Arrow C Data/Stream producer contract.
use crate::arrow_table::{self, ArrowTable, TableError};
use crate::storage_api::StreamReader;
pub use arrow_array::ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream};
use arrow_array::{
    Array, ArrayRef, Date32Array, RecordBatch, RecordBatchIterator, RecordBatchOptions,
    StructArray, TimestampSecondArray, UInt8Array,
};
use arrow_schema::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::Arc;
use yamaa_core::{table::TableSchema, value::ColumnType};

pub fn export_stream(schema: SchemaRef, batches: Vec<RecordBatch>) -> FFI_ArrowArrayStream {
    FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(
        batches.into_iter().map(Ok),
        schema,
    )))
}

/// Consume a standard stream into the engine's validated owned snapshot.
/// Primitive and UTF-8 buffers are shared. Public temporal arrays gain a
/// precision child (complete date/second); their value/validity buffers remain
/// shared. No narrowing, timezone removal or fractional-second truncation occurs.
pub fn import_stream(
    stream: FFI_ArrowArrayStream,
    logical: TableSchema,
) -> Result<ArrowTable, TableError> {
    let reader = StreamReader::try_new(stream).map_err(TableError::Arrow)?;
    read_stream(reader, logical)
}

/// Infer only the public logical types; other physical types require an
/// explicit preserving conversion by the caller before exporting.
pub fn import_public_stream(stream: FFI_ArrowArrayStream) -> Result<ArrowTable, TableError> {
    let reader = StreamReader::try_new(stream).map_err(TableError::Arrow)?;
    let logical = yamaa_core::table::TableSchema::new(
        reader
            .schema()
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
                Ok(yamaa_core::table::Column {
                    name: field.name().clone(),
                    kind,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|_| TableError::SchemaMismatch { batch: 0 })?;
    read_stream(reader, logical)
}

fn read_stream(mut reader: StreamReader, logical: TableSchema) -> Result<ArrowTable, TableError> {
    let schema = reader.schema();
    let mut batches = Vec::new();
    let mut rows = 0usize;
    yamaa_core::limits::Limits::columns(logical.columns().len()).map_err(limit)?;
    for batch in &mut reader {
        let batch = batch.map_err(TableError::Arrow)?;
        rows = rows
            .checked_add(batch.num_rows())
            .ok_or(TableError::Limit {
                resource: "table_rows",
                limit: yamaa_core::limits::Limits::TABLE_ROWS,
                required: None,
            })?;
        yamaa_core::limits::Limits::table(rows, logical.columns().len()).map_err(limit)?;
        batches.push(batch);
    }
    import_batches(logical, schema, batches)
}

fn limit(error: yamaa_core::limits::LimitExceeded) -> TableError {
    TableError::Limit {
        resource: error.resource,
        limit: error.limit,
        required: error.actual,
    }
}

/// Safe Arrow-rs entry point used by host adapters after moving C ownership.
pub fn import_batches(
    logical: TableSchema,
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
) -> Result<ArrowTable, TableError> {
    let mut rows = 0usize;
    for batch in &batches {
        rows = rows
            .checked_add(batch.num_rows())
            .ok_or(TableError::Limit {
                resource: "table_rows",
                limit: yamaa_core::limits::Limits::TABLE_ROWS,
                required: None,
            })?;
    }
    yamaa_core::limits::Limits::table(rows, logical.columns().len()).map_err(limit)?;
    if schema.fields().len() != logical.columns().len()
        || schema
            .fields()
            .iter()
            .zip(logical.columns())
            .any(|(field, column)| field.name() != &column.name)
    {
        return Err(TableError::SchemaMismatch { batch: 0 });
    }
    let canonical = arrow_table::physical_schema(&logical);
    let fields = schema
        .fields()
        .iter()
        .zip(logical.columns())
        .enumerate()
        .map(|(column, (field, declared))| {
            let kind = match (declared.kind, field.data_type()) {
                (ColumnType::Str, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) => {
                    field.data_type().clone()
                }
                (ColumnType::Int, DataType::Int64) => DataType::Int64,
                (ColumnType::Float, DataType::Float64) => DataType::Float64,
                (ColumnType::Date, DataType::Date32)
                | (ColumnType::DateTime, DataType::Timestamp(TimeUnit::Second, None)) => {
                    canonical.field(column).data_type().clone()
                }
                (ColumnType::Date | ColumnType::DateTime, DataType::Struct(_))
                    if field.data_type() == canonical.field(column).data_type() =>
                {
                    field.data_type().clone()
                }
                _ => return Err(TableError::ArrayType { batch: 0, column }),
            };
            Ok(Field::new(&declared.name, kind, true))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let physical = Arc::new(Schema::new(fields));
    let mut converted = Vec::with_capacity(batches.len());
    for (batch_index, batch) in batches.into_iter().enumerate() {
        if batch.schema() != schema {
            return Err(TableError::SchemaMismatch { batch: batch_index });
        }
        let arrays = logical
            .columns()
            .iter()
            .zip(batch.columns())
            .enumerate()
            .map(|(column, (field, array))| {
                let (values, precision): (ArrayRef, u8) = match (field.kind, array.data_type()) {
                    (ColumnType::Date, DataType::Date32) => (
                        Arc::new(Date32Array::new(
                            array
                                .as_any()
                                .downcast_ref::<Date32Array>()
                                .ok_or(TableError::ArrayType {
                                    batch: batch_index,
                                    column,
                                })?
                                .values()
                                .clone(),
                            None,
                        )),
                        2,
                    ),
                    (ColumnType::DateTime, DataType::Timestamp(TimeUnit::Second, None)) => (
                        Arc::new(TimestampSecondArray::new(
                            array
                                .as_any()
                                .downcast_ref::<TimestampSecondArray>()
                                .ok_or(TableError::ArrayType {
                                    batch: batch_index,
                                    column,
                                })?
                                .values()
                                .clone(),
                            None,
                        )),
                        1,
                    ),
                    _ => return Ok(array.clone()),
                };
                let DataType::Struct(children) = physical.field(column).data_type() else {
                    unreachable!()
                };
                Ok(Arc::new(
                    StructArray::try_new(
                        children.clone(),
                        vec![
                            values,
                            Arc::new(UInt8Array::from(vec![precision; array.len()])),
                        ],
                        array.nulls().cloned(),
                    )
                    .map_err(TableError::Arrow)?,
                ) as ArrayRef)
            })
            .collect::<Result<Vec<_>, TableError>>()?;
        converted.push(
            RecordBatch::try_new_with_options(
                physical.clone(),
                arrays,
                &RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
            )
            .map_err(TableError::Arrow)?,
        );
    }
    ArrowTable::try_new_with_schema(logical, converted, physical)
}
