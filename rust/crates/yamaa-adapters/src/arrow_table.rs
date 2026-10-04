//! Ordered owned Arrow snapshots behind the storage-independent core port.
//!
//! This is an internal in-process API, not an untrusted IPC decoder. Limits bound
//! admitted shapes and scan work, not already allocated buffers or allocation
//! failure inside Arrow. Host transports must impose their own byte/work policy.

use crate::arrow_temporal::{date_days, date_from_days, datetime_from_seconds, datetime_seconds};
use arrow_array::{
    Array, ArrayRef, Date32Array, Float64Array, Int64Array, RecordBatch, RecordBatchOptions,
    StringArray, StructArray, TimestampSecondArray, UInt8Array,
};
use arrow_buffer::NullBuffer;
use arrow_schema::{ArrowError, DataType, Field, Fields, Schema, SchemaRef, TimeUnit};
use std::{convert::Infallible, sync::Arc};
use yamaa_core::{
    table::{CellError, TableAccess, TableSchema, ValueRef},
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::{ColumnType, FiniteFloat},
};

/// Trusted caller policy, independent of language semantics. No implicit default.
#[derive(Clone, Copy, Debug)]
pub struct TableLimits {
    pub max_rows: usize,
    pub max_columns: usize,
    pub max_batches: usize,
    pub max_cells: usize,
}

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
    Temporal {
        batch: usize,
        column: usize,
        row: usize,
        reason: TemporalCellError,
    },
    Arrow(ArrowError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalCellError {
    ChildType,
    MissingChild,
    Precision,
    Range,
}

/// Immutable arrays own/reference their buffers independently of input handles.
/// Empty chunks are retained; row lookup skips them without changing record order.
#[derive(Debug)]
pub struct ArrowTable {
    schema: TableSchema,
    batches: Vec<RecordBatch>,
    ends: Vec<usize>,
    rows: usize,
}

/// Internal physical schema v1: nullable primitives, or a nullable temporal struct
/// with non-null `value` and `precision`. Date codes: year=0/month=1/day=2;
/// datetime codes: day=0/second=1. Civil timestamp unit is seconds, with no zone.
pub fn physical_schema(schema: &TableSchema) -> SchemaRef {
    Arc::new(Schema::new(
        schema
            .columns()
            .iter()
            .map(|column| Field::new(&column.name, physical_type(column.kind), true))
            .collect::<Vec<_>>(),
    ))
}

/// Return the canonical representation, retaining per-value temporal precision.
fn physical_type(kind: ColumnType) -> DataType {
    match kind {
        ColumnType::Str => DataType::Utf8,
        ColumnType::Int => DataType::Int64,
        ColumnType::Float => DataType::Float64,
        ColumnType::Date | ColumnType::DateTime => DataType::Struct(temporal_fields(kind)),
    }
}

/// Construct the exact ordered child schema for a temporal column.
fn temporal_fields(kind: ColumnType) -> Fields {
    let value = if kind == ColumnType::Date {
        DataType::Date32
    } else {
        DataType::Timestamp(TimeUnit::Second, None)
    };
    vec![
        Field::new("value", value, false),
        Field::new("precision", DataType::UInt8, false),
    ]
    .into()
}

/// Encode validated core dates, retaining imputed fields and collected precision.
/// Input values are already materialized; transport limits belong to the caller.
pub fn date_array(values: &[Option<Date>]) -> Result<StructArray, ArrowError> {
    let dates = Date32Array::from(
        values
            .iter()
            .map(|value| value.map_or(0, date_days))
            .collect::<Vec<_>>(),
    );
    let precision = UInt8Array::from(
        values
            .iter()
            .map(|value| {
                value.map_or(0, |date| match date.collected_precision() {
                    DatePrecision::Year => 0,
                    DatePrecision::Month => 1,
                    DatePrecision::Day => 2,
                })
            })
            .collect::<Vec<_>>(),
    );
    StructArray::try_new(
        temporal_fields(ColumnType::Date),
        vec![Arc::new(dates), Arc::new(precision)],
        Some(NullBuffer::from(
            values.iter().map(Option::is_some).collect::<Vec<_>>(),
        )),
    )
}

/// Encode validated civil datetimes in seconds with their per-value precision.
pub fn datetime_array(values: &[Option<DateTime>]) -> Result<StructArray, ArrowError> {
    let dates = TimestampSecondArray::from(
        values
            .iter()
            .map(|value| value.map_or(0, datetime_seconds))
            .collect::<Vec<_>>(),
    );
    let precision = UInt8Array::from(
        values
            .iter()
            .map(|value| {
                value.map_or(0, |date| match date.collected_precision() {
                    DateTimePrecision::Day => 0,
                    DateTimePrecision::Second => 1,
                })
            })
            .collect::<Vec<_>>(),
    );
    StructArray::try_new(
        temporal_fields(ColumnType::DateTime),
        vec![Arc::new(dates), Arc::new(precision)],
        Some(NullBuffer::from(
            values.iter().map(Option::is_some).collect::<Vec<_>>(),
        )),
    )
}

/// Check a shape budget, retaining arithmetic overflow as an unknown requirement.
fn limit(
    resource: &'static str,
    required: Option<usize>,
    maximum: usize,
) -> Result<usize, TableError> {
    match required {
        Some(value) if value <= maximum => Ok(value),
        _ => Err(TableError::Limit {
            resource,
            limit: maximum,
            required,
        }),
    }
}

impl ArrowTable {
    /// Validate the whole snapshot before it can reach any evaluator. Schemas
    /// must equal the canonical physical schema including order/nullability and
    /// metadata. Floats normalize nonfinite values to actual Arrow nulls. All
    /// budget checks precede schema and cell validation; no aggregate runs here.
    pub fn try_new(
        schema: TableSchema,
        batches: Vec<RecordBatch>,
        limits: TableLimits,
    ) -> Result<Self, TableError> {
        let columns = limit("columns", Some(schema.columns().len()), limits.max_columns)?;
        limit("batches", Some(batches.len()), limits.max_batches)?;
        let mut rows = 0_usize;
        let mut ends = Vec::with_capacity(batches.len());
        for batch in &batches {
            rows = limit("rows", rows.checked_add(batch.num_rows()), limits.max_rows)?;
            ends.push(rows);
        }
        limit("cells", rows.checked_mul(columns), limits.max_cells)?;
        let expected = physical_schema(&schema);
        let mut normalized = Vec::with_capacity(batches.len());
        for (batch_index, batch) in batches.into_iter().enumerate() {
            if batch.schema().as_ref() != expected.as_ref() {
                return Err(TableError::SchemaMismatch { batch: batch_index });
            }
            let mut arrays = Vec::with_capacity(columns);
            for (column, (field, array)) in schema.columns().iter().zip(batch.columns()).enumerate()
            {
                let array = normalize_array(array, field.kind).ok_or(TableError::ArrayType {
                    batch: batch_index,
                    column,
                })?;
                if matches!(field.kind, ColumnType::Date | ColumnType::DateTime) {
                    let dates = array
                        .as_any()
                        .downcast_ref::<StructArray>()
                        .expect("normalized struct");
                    for row in 0..dates.len() {
                        temporal_cell(dates, row, field.kind).map_err(|reason| {
                            TableError::Temporal {
                                batch: batch_index,
                                column,
                                row,
                                reason,
                            }
                        })?;
                    }
                }
                arrays.push(array);
            }
            normalized.push(
                RecordBatch::try_new_with_options(
                    expected.clone(),
                    arrays,
                    &RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
                )
                .map_err(TableError::Arrow)?,
            );
        }
        Ok(Self {
            schema,
            batches: normalized,
            ends,
            rows,
        })
    }

    /// Read normalized Arrow batches in original chunk order, including empties.
    pub fn batches(&self) -> &[RecordBatch] {
        &self.batches
    }
}

/// Narrow to concrete immutable Arrow arrays; arbitrary trait implementations
/// cannot retain mutable/custom behavior in the admitted snapshot.
fn normalize_array(array: &ArrayRef, kind: ColumnType) -> Option<ArrayRef> {
    Some(match kind {
        ColumnType::Str => Arc::new(array.as_any().downcast_ref::<StringArray>()?.clone()),
        ColumnType::Int => Arc::new(array.as_any().downcast_ref::<Int64Array>()?.clone()),
        ColumnType::Float => {
            let values = array.as_any().downcast_ref::<Float64Array>()?;
            if values.iter().flatten().all(f64::is_finite) {
                Arc::new(values.clone())
            } else {
                Arc::new(Float64Array::from_iter(
                    values
                        .iter()
                        .map(|value| value.filter(|value| value.is_finite())),
                ))
            }
        }
        ColumnType::Date | ColumnType::DateTime => {
            let values = array.as_any().downcast_ref::<StructArray>()?;
            let dates: ArrayRef = if kind == ColumnType::Date {
                Arc::new(
                    values
                        .column(0)
                        .as_any()
                        .downcast_ref::<Date32Array>()?
                        .clone(),
                )
            } else {
                Arc::new(
                    values
                        .column(0)
                        .as_any()
                        .downcast_ref::<TimestampSecondArray>()?
                        .clone(),
                )
            };
            let precision = values
                .column(1)
                .as_any()
                .downcast_ref::<UInt8Array>()?
                .clone();
            Arc::new(
                StructArray::try_new(
                    temporal_fields(kind),
                    vec![dates, Arc::new(precision)],
                    values.nulls().cloned(),
                )
                .ok()?,
            )
        }
    })
}

/// Validate/decode only visible temporal payloads; parent null masks child data.
fn temporal_cell(
    array: &StructArray,
    row: usize,
    kind: ColumnType,
) -> Result<ValueRef<'static>, TemporalCellError> {
    if array.is_null(row) {
        return Ok(ValueRef::Missing);
    }
    let precision = array
        .column(1)
        .as_any()
        .downcast_ref::<UInt8Array>()
        .ok_or(TemporalCellError::ChildType)?;
    if precision.is_null(row) || array.column(0).is_null(row) {
        return Err(TemporalCellError::MissingChild);
    }
    if kind == ColumnType::Date {
        let values = array
            .column(0)
            .as_any()
            .downcast_ref::<Date32Array>()
            .ok_or(TemporalCellError::ChildType)?;
        let precision = match precision.value(row) {
            0 => DatePrecision::Year,
            1 => DatePrecision::Month,
            2 => DatePrecision::Day,
            _ => return Err(TemporalCellError::Precision),
        };
        date_from_days(i64::from(values.value(row)), precision)
            .map(ValueRef::Date)
            .ok_or(TemporalCellError::Range)
    } else {
        let values = array
            .column(0)
            .as_any()
            .downcast_ref::<TimestampSecondArray>()
            .ok_or(TemporalCellError::ChildType)?;
        let precision = match precision.value(row) {
            0 => DateTimePrecision::Day,
            1 => DateTimePrecision::Second,
            _ => return Err(TemporalCellError::Precision),
        };
        datetime_from_seconds(values.value(row), precision)
            .map(ValueRef::DateTime)
            .ok_or(TemporalCellError::Range)
    }
}

impl TableAccess for ArrowTable {
    type Error = Infallible;

    /// Return the closed logical schema, independently of batch/row count.
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    /// Return total records, including records in zero-column tables.
    fn row_count(&self) -> usize {
        self.rows
    }
    /// Borrow a normalized cell; construction has already validated its type/data.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        if row >= self.rows || column >= self.schema.columns().len() {
            return Err(CellError::OutOfBounds { row, column });
        }
        let batch_index = self.ends.partition_point(|&end| end <= row);
        let offset = if batch_index == 0 {
            0
        } else {
            self.ends[batch_index - 1]
        };
        let local_row = row - offset;
        let array = self.batches[batch_index].column(column);
        if array.is_null(local_row) {
            return Ok(ValueRef::Missing);
        }
        Ok(match self.schema.columns()[column].kind {
            ColumnType::Str => ValueRef::Str(
                array
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("validated text")
                    .value(local_row),
            ),
            ColumnType::Int => ValueRef::Int(
                array
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("validated integer")
                    .value(local_row),
            ),
            ColumnType::Float => ValueRef::Float(
                FiniteFloat::new(
                    array
                        .as_any()
                        .downcast_ref::<Float64Array>()
                        .expect("validated float")
                        .value(local_row),
                )
                .expect("normalized finite value"),
            ),
            kind => temporal_cell(
                array
                    .as_any()
                    .downcast_ref::<StructArray>()
                    .expect("validated temporal"),
                local_row,
                kind,
            )
            .expect("validated temporal cell"),
        })
    }
}
