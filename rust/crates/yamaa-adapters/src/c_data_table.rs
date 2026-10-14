//! Trusted, in-process Arrow C Stream ingestion. No IPC writer or reader is used.
//!
//! Foreign producers must supply valid immutable buffers and release callbacks.
//! Shape/value checks cannot make arbitrary pointers safe. Imported arrays retain
//! producer ownership until the synchronous calling-thread execution finishes.
use crate::arrow_table::{physical_schema, ArrowTable, TableLimits};
pub use arrow_array::ffi_stream::FFI_ArrowArrayStream;
use arrow_array::{
    ffi_stream::ArrowArrayStreamReader, Array, ArrayRef, Date32Array, Float32Array, Float64Array,
    Int16Array, Int32Array, Int64Array, Int8Array, LargeStringArray, RecordBatch,
    RecordBatchOptions, RecordBatchReader, StringArray, StringViewArray, StructArray,
    TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt8Array,
};
use arrow_schema::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::{fmt, sync::Arc};
use yamaa_core::{
    table::{Column, TableAccess, TableSchema},
    value::ColumnType,
};

pub const LIMITS: TableLimits = TableLimits {
    max_rows: yamaa_engine::capacity::SOURCE_ROWS,
    max_columns: yamaa_core::specification::DEFAULT_MAX_COLUMNS,
    max_batches: 256,
    max_cells: yamaa_engine::capacity::SOURCE_CELLS,
};
const MAX_BYTES: usize = yamaa_engine::capacity::SOURCE_BYTES;
const MAX_SCHEMA_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Stream,
    Schema,
    Value,
    Limit,
    Internal,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stream => "invalid or released Arrow C stream",
            Self::Schema => "unsupported Arrow C Data table schema",
            Self::Value => "invalid Arrow C Data table value",
            Self::Limit => "Arrow C Data input exceeds resource limit",
            Self::Internal => "internal Arrow C Data import failure",
        })
    }
}
impl std::error::Error for Error {}

/// Cumulative admission for one execution, including every source and empty batch.
#[derive(Default)]
pub struct ImportBudget {
    rows: usize,
    cells: usize,
    batches: usize,
    buffers: usize,
    text: usize,
    schema: usize,
}
fn charge(used: &mut usize, amount: usize, limit: usize) -> Result<(), Error> {
    *used = used
        .checked_add(amount)
        .filter(|&value| value <= limit)
        .ok_or(Error::Limit)?;
    Ok(())
}
impl ImportBudget {
    /// Move a valid stream into Rust, retaining its arrays without serialization.
    /// On any return path the stream and every imported array are released once.
    ///
    /// # Safety
    /// The stream, schemas, arrays and callbacks must conform to the Arrow C
    /// Stream/Data interfaces, including buffer bounds, alignment and lifetimes.
    /// Each array must be a record-batch struct with no parent nulls.
    /// Buffers must remain immutable while retained. All callbacks must be valid
    /// on the calling thread; the caller must keep its host runtime attached.
    pub unsafe fn import(&mut self, stream: FFI_ArrowArrayStream) -> Result<ArrowTable, Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.import_inner(stream)))
            .map_err(|_| Error::Internal)?
    }

    fn import_inner(&mut self, stream: FFI_ArrowArrayStream) -> Result<ArrowTable, Error> {
        let reader = ArrowArrayStreamReader::try_new(stream).map_err(|_| Error::Stream)?;
        let input_schema = reader.schema();
        if input_schema.fields().len() > LIMITS.max_columns {
            return Err(Error::Limit);
        }
        for (key, value) in input_schema.metadata() {
            charge(&mut self.schema, key.len(), MAX_SCHEMA_BYTES)?;
            charge(&mut self.schema, value.len(), MAX_SCHEMA_BYTES)?;
        }
        let mut columns = Vec::with_capacity(input_schema.fields().len());
        for field in input_schema.fields() {
            charge(&mut self.schema, field.name().len(), MAX_SCHEMA_BYTES)?;
            for (key, value) in field.metadata() {
                charge(&mut self.schema, key.len(), MAX_SCHEMA_BYTES)?;
                charge(&mut self.schema, value.len(), MAX_SCHEMA_BYTES)?;
                if key == "ARROW:extension:name" {
                    return Err(Error::Schema);
                }
            }
            columns.push(Column {
                name: field.name().clone(),
                kind: logical_type(field.data_type()).ok_or(Error::Schema)?,
            });
        }
        let logical = TableSchema::new(columns).map_err(|_| Error::Schema)?;
        let physical = imported_schema(&logical, &input_schema)?;
        let mut batches = Vec::new();
        for batch in reader {
            let batch = batch.map_err(|_| Error::Stream)?;
            charge(&mut self.batches, 1, LIMITS.max_batches)?;
            charge(&mut self.rows, batch.num_rows(), LIMITS.max_rows)?;
            charge(
                &mut self.cells,
                batch
                    .num_rows()
                    .checked_mul(batch.num_columns())
                    .ok_or(Error::Limit)?,
                LIMITS.max_cells,
            )?;
            if batch.schema() != input_schema {
                return Err(Error::Schema);
            }
            let mut arrays = Vec::with_capacity(batch.num_columns());
            for array in batch.columns() {
                charge(&mut self.buffers, array.get_buffer_memory_size(), MAX_BYTES)?;
                array.to_data().validate_full().map_err(|_| Error::Value)?;
                arrays.push(normalize(array)?);
            }
            batches.push(
                RecordBatch::try_new_with_options(
                    physical.clone(),
                    arrays,
                    &RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
                )
                .map_err(|_| Error::Value)?,
            );
            // Bound repeated/aliased string values as well as retained buffers.
            for array in batches.last().expect("imported batch").columns() {
                macro_rules! text {
                    ($kind:ty) => {
                        if let Some(values) = array.as_any().downcast_ref::<$kind>() {
                            for value in values.iter().flatten() {
                                charge(&mut self.text, value.len(), MAX_BYTES)?;
                            }
                        }
                    };
                }
                text!(StringArray);
                text!(LargeStringArray);
                text!(StringViewArray);
            }
        }
        ArrowTable::try_new_with_schema(logical, batches, LIMITS, physical)
            .map_err(|_| Error::Value)
    }
}

fn logical_type(kind: &DataType) -> Option<ColumnType> {
    Some(match kind {
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => ColumnType::Str,
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => ColumnType::Int,
        DataType::Float32 | DataType::Float64 => ColumnType::Float,
        DataType::Date32 => ColumnType::Date,
        DataType::Timestamp(_, None) => ColumnType::DateTime,
        DataType::Struct(children) if children.len() == 2 => match children[0].data_type() {
            DataType::Date32 => ColumnType::Date,
            DataType::Timestamp(TimeUnit::Second, None) => ColumnType::DateTime,
            _ => return None,
        },
        _ => return None,
    })
}
fn imported_schema(logical: &TableSchema, input: &Schema) -> Result<SchemaRef, Error> {
    let canonical = physical_schema(logical);
    let mut fields = Vec::with_capacity(input.fields().len());
    for (field, standard) in input.fields().iter().zip(canonical.fields()) {
        let kind = match field.data_type() {
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => field.data_type(),
            DataType::Struct(_) if field.data_type() != standard.data_type() => {
                return Err(Error::Schema);
            }
            _ => standard.data_type(),
        };
        fields.push(Field::new(field.name(), kind.clone(), true));
    }
    Ok(Arc::new(Schema::new(fields)))
}

fn normalize(array: &ArrayRef) -> Result<ArrayRef, Error> {
    macro_rules! widen {
        ($from:ty, $to:ty, $convert:expr) => {{
            let values = array.as_any().downcast_ref::<$from>().ok_or(Error::Value)?;
            Arc::new(<$to>::from_iter(
                values.iter().map(|value| value.map($convert)),
            )) as ArrayRef
        }};
    }
    Ok(match array.data_type() {
        DataType::Int8 => widen!(Int8Array, Int64Array, i64::from),
        DataType::Int16 => widen!(Int16Array, Int64Array, i64::from),
        DataType::Int32 => widen!(Int32Array, Int64Array, i64::from),
        DataType::Float32 => widen!(Float32Array, Float64Array, f64::from),
        DataType::Date32 | DataType::Timestamp(_, None) => {
            let kind = logical_type(array.data_type()).ok_or(Error::Schema)?;
            let schema = TableSchema::new(vec![Column {
                name: "value".into(),
                kind,
            }])
            .map_err(|_| Error::Schema)?;
            let canonical = physical_schema(&schema);
            let DataType::Struct(fields) = canonical.field(0).data_type() else {
                return Err(Error::Schema);
            };
            let values = if kind == ColumnType::Date {
                if !array.as_any().is::<Date32Array>() {
                    return Err(Error::Value);
                }
                array.clone()
            } else {
                timestamp_seconds(array)?
            };
            Arc::new(
                StructArray::try_new(
                    fields.clone(),
                    vec![
                        values,
                        Arc::new(UInt8Array::from(vec![
                            if kind == ColumnType::Date {
                                2
                            } else {
                                1
                            };
                            array.len()
                        ])),
                    ],
                    array.nulls().cloned(),
                )
                .map_err(|_| Error::Value)?,
            )
        }
        _ => array.clone(),
    })
}
fn timestamp_seconds(array: &ArrayRef) -> Result<ArrayRef, Error> {
    macro_rules! seconds {
        ($kind:ty, $factor:expr) => {{
            let values = array.as_any().downcast_ref::<$kind>().ok_or(Error::Value)?;
            let seconds = values
                .iter()
                .map(|value| {
                    value
                        .map(|value| {
                            if value % $factor != 0 {
                                Err(Error::Value)
                            } else {
                                Ok(value / $factor)
                            }
                        })
                        .transpose()
                })
                .collect::<Result<Vec<_>, _>>()?;
            Arc::new(TimestampSecondArray::from(seconds)) as ArrayRef
        }};
    }
    Ok(match array.data_type() {
        DataType::Timestamp(TimeUnit::Second, None) => array.clone(),
        DataType::Timestamp(TimeUnit::Millisecond, None) => {
            seconds!(TimestampMillisecondArray, 1_000)
        }
        DataType::Timestamp(TimeUnit::Microsecond, None) => {
            seconds!(TimestampMicrosecondArray, 1_000_000)
        }
        DataType::Timestamp(TimeUnit::Nanosecond, None) => {
            seconds!(TimestampNanosecondArray, 1_000_000_000)
        }
        _ => return Err(Error::Schema),
    })
}

/// Apply the compiled source text policy by replacing validity alone. String
/// offsets, views and payload buffers stay shared, even for sliced input arrays.
pub(crate) fn source_policy(
    source: &yamaa_engine::specification::SourceDeclaration,
    table: ArrowTable,
) -> Result<ArrowTable, Error> {
    for (name, kind) in &source.types {
        if !table
            .schema()
            .columns()
            .iter()
            .any(|column| &column.name == name && &column.kind == kind)
        {
            return Err(Error::Schema);
        }
    }
    let mut batches = Vec::with_capacity(table.batches().len());
    for batch in table.batches() {
        let mut arrays = Vec::with_capacity(batch.num_columns());
        for (index, column) in table.schema().columns().iter().enumerate() {
            let array = batch.column(index);
            if column.kind != ColumnType::Str || source.empty_string_present {
                arrays.push(array.clone());
                continue;
            }
            macro_rules! missing {
                ($kind:ty) => {{
                    let values = array.as_any().downcast_ref::<$kind>().ok_or(Error::Value)?;
                    if values
                        .iter()
                        .flatten()
                        .any(|value| source.text_is_missing(value))
                    {
                        let validity = arrow_buffer::NullBuffer::from(
                            values
                                .iter()
                                .map(|value| {
                                    value.is_some_and(|value| !source.text_is_missing(value))
                                })
                                .collect::<Vec<_>>(),
                        );
                        let (offsets, data, _) = values.clone().into_parts();
                        Arc::new(<$kind>::new(offsets, data, Some(validity))) as ArrayRef
                    } else {
                        array.clone()
                    }
                }};
            }
            arrays.push(match array.data_type() {
                DataType::Utf8 => missing!(StringArray),
                DataType::LargeUtf8 => missing!(LargeStringArray),
                DataType::Utf8View => missing!(StringViewArray),
                _ => return Err(Error::Value),
            });
        }
        batches.push(
            RecordBatch::try_new_with_options(
                batch.schema(),
                arrays,
                &RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
            )
            .map_err(|_| Error::Value)?,
        );
    }
    let physical = batches
        .first()
        .map(RecordBatch::schema)
        .unwrap_or_else(|| physical_schema(table.schema()));
    ArrowTable::try_new_with_schema(table.schema().clone(), batches, LIMITS, physical)
        .map_err(|_| Error::Value)
}

#[derive(Debug)]
pub enum ExecutionError {
    Transport(Error),
    Specification(crate::specification_run::Error),
    Interpretation,
}
impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => error.fmt(f),
            Self::Specification(error) => {
                f.write_str(&crate::specification_diagnostics::failure(error, None))
            }
            Self::Interpretation => {
                f.write_str("Arrow inputs require ordinary source interpretation")
            }
        }
    }
}
impl std::error::Error for ExecutionError {}

/// Source-count and interpretation checks precede invoking any foreign exporter.
pub fn admit_sources(
    run: &crate::specification_run::PreparedRun,
    count: usize,
) -> Result<(), ExecutionError> {
    if count != run.compiled().sources().len() {
        return Err(ExecutionError::Specification(
            crate::specification_run::Error::Bind(
                yamaa_engine::specification::BindError::SourceCount,
            ),
        ));
    }
    for index in 0..count {
        if run.compiled().source_interpretation(index)
            != Some(yamaa_engine::specification::SourceInterpretation::Ordinary)
        {
            return Err(ExecutionError::Interpretation);
        }
    }
    Ok(())
}

/// Bind and execute imported typed snapshots in authored input order. No file or
/// callback authority is granted; output keeps the existing execution/1 envelope.
pub fn execute(
    run: &crate::specification_run::PreparedRun,
    tables: Vec<ArrowTable>,
) -> Result<crate::dataset_result::DatasetResponse, ExecutionError> {
    use crate::{dataset_result, specification_run};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use yamaa_engine::dataset::DatasetExecution;
    admit_sources(run, tables.len())?;
    catch_unwind(AssertUnwindSafe(|| {
        let tables = tables
            .into_iter()
            .zip(run.compiled().sources())
            .map(|(table, source)| {
                let table = source_policy(source, table).map_err(ExecutionError::Transport)?;
                crate::source_ordinal::append(source, table, LIMITS)
                    .map(dataset_result::Snapshot)
                    .map_err(ExecutionError::Specification)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let schemas = tables.iter().map(TableAccess::schema).collect::<Vec<_>>();
        let plan = run.compiled().bind_sources(&schemas).map_err(|error| {
            ExecutionError::Specification(specification_run::Error::Bind(error))
        })?;
        let mut rows = 0;
        let mut cells = 0;
        for table in &tables {
            charge(&mut rows, table.row_count(), LIMITS.max_rows)
                .map_err(ExecutionError::Transport)?;
            charge(
                &mut cells,
                table
                    .row_count()
                    .checked_mul(table.schema().columns().len())
                    .ok_or(ExecutionError::Transport(Error::Limit))?,
                LIMITS.max_cells,
            )
            .map_err(ExecutionError::Transport)?;
        }
        let driver = run.compiled().driver_index();
        let secondary = tables
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != driver)
            .map(|(_, table)| table as &dyn TableAccess<Error = crate::callback::CallbackError>)
            .collect::<Vec<_>>();
        dataset_result::specification_response(plan.execute_observed_sources(
            &tables[driver],
            &secondary,
            dataset_result::LIMITS,
        ))
        .map_err(|error| ExecutionError::Specification(specification_run::Error::Execution(error)))
    }))
    .map_err(|_| {
        ExecutionError::Specification(specification_run::Error::Execution(
            dataset_result::DatasetError::Internal,
        ))
    })?
}
