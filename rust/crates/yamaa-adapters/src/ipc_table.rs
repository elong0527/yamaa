//! Copied IPC streams for installed hosts. No files, callbacks or fallback.
use crate::arrow_table::{date_array, datetime_array, physical_schema, ArrowTable};
use arrow_array::{
    ArrayRef, Float64Array, Int64Array, RecordBatch, RecordBatchOptions, StringArray,
};
use arrow_ipc::{reader::StreamReader, writer::StreamWriter, MessageHeader, MetadataVersion};
use arrow_schema::DataType;
use serde::Serialize;
use std::{
    fmt,
    io::{Cursor, Write},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};
use yamaa_core::{dataset::Dataset, limits::Limits};
use yamaa_core::{
    table::{Column, TableAccess, TableSchema, ValueRef},
    value::ColumnType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    Limit {
        resource: &'static str,
        limit: usize,
        required: Option<usize>,
    },
    InvalidStream,
    InvalidTable,
    OutputIo,
    Internal,
}
impl fmt::Display for TableError {
    /// Stable host errors never disclose raw input or panic payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Limit { .. } => "table geometry exceeds fixed limit",
            Self::InvalidStream => "invalid or unsupported table IPC stream",
            Self::InvalidTable => "invalid table schema or value",
            Self::OutputIo => "table IPC output write failed",
            Self::Internal => "internal table codec failure",
        })
    }
}
impl std::error::Error for TableError {}
type Error = TableError;

/// Contain codec panics while preserving ordinary stream failures.
fn boundary<T>(
    run: impl FnOnce() -> Result<T, Error> + std::panic::UnwindSafe,
) -> Result<T, Error> {
    catch_unwind(run).map_err(|_| Error::Internal)?
}

fn shape_error(error: yamaa_core::limits::LimitExceeded) -> Error {
    Error::Limit {
        resource: error.resource,
        limit: error.limit,
        required: error.actual,
    }
}

/// Return a sanitized stream with exact schema/rows/chunks and no hidden null payloads.
pub fn table_round_trip(input: &[u8]) -> Result<Vec<u8>, Error> {
    boundary(|| {
        let table = decode(input)?;
        let mut output = Vec::new();
        let mut writer = StreamWriter::try_new(&mut output, &physical_schema(table.schema()))
            .map_err(|_| Error::OutputIo)?;
        let mut start = 0;
        for batch in table.batches() {
            let clean =
                sanitized_batch(&table, start, batch.num_rows()).map_err(|_| Error::Internal)?;
            writer.write(&clean).map_err(|_| Error::OutputIo)?;
            start += batch.num_rows();
        }
        writer.finish().map_err(|_| Error::OutputIo)?;
        Ok(output)
    })
}

fn decode(input: &[u8]) -> Result<ArrowTable, Error> {
    let shape = preflight(input)?;
    let batches =
        SnapshotBatches::new(input, shape.schema.clone())?.collect::<Result<Vec<_>, _>>()?;
    ArrowTable::try_new(shape.schema, batches).map_err(|_| Error::InvalidTable)
}

/// Decode only after complete framing, schema and fixed geometry validation.
pub fn decode_snapshot(input: &[u8]) -> Result<ArrowTable, Error> {
    boundary(|| decode(input))
}

/// Decode one normalized, checked batch at a time after whole-stream framing
/// admission. Consumers must exhaust the iterator to validate all values.
/// The input bytes stay borrowed; previously yielded batches need not be retained.
pub fn snapshot_batches(input: &[u8]) -> Result<SnapshotBatches<'_>, Error> {
    boundary(|| {
        let shape = preflight(input)?;
        SnapshotBatches::new(input, shape.schema)
    })
}

pub struct SnapshotBatches<'a> {
    reader: StreamReader<Cursor<&'a [u8]>>,
    schema: TableSchema,
    finished: bool,
}

impl<'a> SnapshotBatches<'a> {
    fn new(input: &'a [u8], schema: TableSchema) -> Result<Self, Error> {
        Ok(Self {
            reader: StreamReader::try_new(Cursor::new(input), None)
                .map_err(|_| Error::InvalidStream)?,
            schema,
            finished: false,
        })
    }

    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }
}

impl Iterator for SnapshotBatches<'_> {
    type Item = Result<RecordBatch, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let next = catch_unwind(AssertUnwindSafe(|| {
            self.reader.next().map(|batch| {
                let batch = batch.map_err(|_| Error::InvalidStream)?;
                let table = ArrowTable::try_new(self.schema.clone(), vec![batch])
                    .map_err(|_| Error::InvalidTable)?;
                Ok(table.batches()[0].clone())
            })
        }))
        .unwrap_or(Some(Err(Error::Internal)));
        if next.is_none() || next.as_ref().is_some_and(Result::is_err) {
            self.finished = true;
        }
        next
    }
}

impl std::iter::FusedIterator for SnapshotBatches<'_> {}

/// Export only a checked owned dataset; this is not a generic untrusted table API.
/// The caller's execution policy has already bounded retained cell/text payloads.
pub(crate) fn encode_dataset(table: &yamaa_core::dataset::Dataset) -> Result<Vec<u8>, Error> {
    encode_dataset_projection(table, None)
}

/// Export the projection already admitted by the output use case.
pub(crate) fn encode_projected_dataset(
    table: &yamaa_core::dataset::Dataset,
    projection: &[usize],
) -> Result<Vec<u8>, Error> {
    encode_dataset_projection(table, Some(projection))
}

/// Only owned governed log tables enter this transport; arbitrary host inputs
/// still pass the complete snapshot decoder admission.
pub(crate) fn encode_log_table(
    table: &yamaa_core::verification_logs::LogTable,
) -> Result<Vec<u8>, Error> {
    encode_dataset_projection(table, None)
}
fn encode_dataset_projection<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    projection: Option<&[usize]>,
) -> Result<Vec<u8>, Error> {
    encode_table_projection(table, projection)
}

fn encode_table_projection<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    projection: Option<&[usize]>,
) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    write_table_projection(table, projection, &mut output)?;
    Ok(output)
}

/// Lazily rebuild accepted values into sanitized Arrow batches.
/// No batch is exposed until the complete dataset has passed engine checks.
pub(crate) struct DatasetBatches<'a, T = Dataset> {
    table: &'a T,
    projection: Option<&'a [usize]>,
    start: usize,
    batch_rows: usize,
    remaining: usize,
}

fn table_batches<'a, T: TableAccess<Error = std::convert::Infallible>>(
    table: &'a T,
    projection: Option<&'a [usize]>,
) -> Result<DatasetBatches<'a, T>, Error> {
    Limits::table(table.row_count(), table.schema().columns().len()).map_err(shape_error)?;
    if projection.is_some_and(|columns| {
        columns
            .iter()
            .any(|&column| column >= table.schema().columns().len())
    }) {
        return Err(Error::InvalidTable);
    }
    let batch_rows = 65_536;
    let batches = if table.row_count() == 0 {
        1
    } else {
        (table.row_count() - 1) / batch_rows + 1
    };
    Ok(DatasetBatches {
        table,
        projection,
        start: 0,
        batch_rows,
        remaining: batches,
    })
}

impl<T: TableAccess<Error = std::convert::Infallible>> Iterator for DatasetBatches<'_, T> {
    type Item = Result<RecordBatch, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let rows = self.batch_rows.min(self.table.row_count() - self.start);
        let result = sanitized_batch(self.table, self.start, rows)
            .map_err(|_| Error::Internal)
            .and_then(|batch| match self.projection {
                Some(projection) => batch.project(projection).map_err(|_| Error::InvalidTable),
                None => Ok(batch),
            });
        self.start += rows;
        self.remaining = if result.is_err() {
            0
        } else {
            self.remaining - 1
        };
        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::from(self.remaining != 0), Some(self.remaining))
    }
}

impl<T: TableAccess<Error = std::convert::Infallible>> std::iter::FusedIterator
    for DatasetBatches<'_, T>
{
}

fn write_table_projection<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    projection: Option<&[usize]>,
    output: &mut impl Write,
) -> Result<(), Error> {
    let batches = table_batches(table, projection)?;
    let schema = physical_schema(table.schema());
    let schema = match projection {
        Some(projection) => Arc::new(
            schema
                .project(projection)
                .map_err(|_| Error::InvalidTable)?,
        ),
        None => schema,
    };
    let mut writer = StreamWriter::try_new(output, &schema).map_err(|_| Error::OutputIo)?;
    for batch in batches {
        writer.write(&batch?).map_err(|_| Error::OutputIo)?;
    }
    writer.finish().map_err(|_| Error::OutputIo)
}

/// Serialize the actual observation without a second transport byte policy.
pub fn serialize_json(value: &impl Serialize) -> Result<String, Error> {
    serde_json::to_string(value).map_err(|_| Error::Internal)
}

/// Take bytes only after checked remaining-length validation; no declared-size allocation.
fn take<'a>(input: &mut &'a [u8], length: usize) -> Result<&'a [u8], Error> {
    let value = input.get(..length).ok_or(Error::InvalidStream)?;
    *input = &input[length..];
    Ok(value)
}

/// Read a little-endian IPC framing length without unchecked offsets.
fn word(input: &mut &[u8]) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        take(input, 4)?
            .try_into()
            .map_err(|_| Error::InvalidStream)?,
    ))
}

/// Validate canonical schema shape before Arrow's schema conversion can recurse.
fn schema_from_message(schema: arrow_ipc::Schema<'_>) -> Result<TableSchema, Error> {
    if schema.endianness() != arrow_ipc::Endianness::Little
        || schema.custom_metadata().is_some_and(|m| !m.is_empty())
    {
        return Err(Error::InvalidTable);
    }
    let fields = schema.fields().ok_or(Error::InvalidTable)?;
    Limits::columns(fields.len()).map_err(shape_error)?;
    for field in fields {
        check_field(field, true)?;
    }
    let converted =
        arrow_ipc::convert::try_fb_to_schema(schema).map_err(|_| Error::InvalidTable)?;
    let mut columns = Vec::new();
    for field in converted.fields() {
        let kind = match field.data_type() {
            DataType::Utf8 => ColumnType::Str,
            DataType::Int64 => ColumnType::Int,
            DataType::Float64 => ColumnType::Float,
            DataType::Struct(children) if children.len() == 2 => match children[0].data_type() {
                DataType::Date32 => ColumnType::Date,
                DataType::Timestamp(arrow_schema::TimeUnit::Second, None) => ColumnType::DateTime,
                _ => return Err(Error::InvalidTable),
            },
            _ => return Err(Error::InvalidTable),
        };
        columns.push(Column {
            name: field.name().clone(),
            kind,
        });
    }
    let logical = TableSchema::new(columns).map_err(|_| Error::InvalidTable)?;
    if physical_schema(&logical).as_ref() != &converted {
        return Err(Error::InvalidTable);
    }
    Ok(logical)
}

/// Permit only scalar fields and one temporal struct level; dictionaries and
/// extension metadata never reach Arrow conversion/decoding.
fn check_field(field: arrow_ipc::Field<'_>, top: bool) -> Result<(), Error> {
    if field.dictionary().is_some() || field.custom_metadata().is_some_and(|m| !m.is_empty()) {
        return Err(Error::InvalidTable);
    }
    let children = field.children();
    if top && field.type_type() == arrow_ipc::Type::Struct_ {
        let children = children.ok_or(Error::InvalidTable)?;
        if children.len() != 2 {
            return Err(Error::InvalidTable);
        }
        for child in children {
            check_field(child, false)?;
        }
    } else {
        if children.is_some_and(|c| !c.is_empty()) {
            return Err(Error::InvalidTable);
        }
        if !matches!(
            field.type_type(),
            arrow_ipc::Type::Utf8
                | arrow_ipc::Type::Int
                | arrow_ipc::Type::FloatingPoint
                | arrow_ipc::Type::Date
                | arrow_ipc::Type::Timestamp
        ) {
            return Err(Error::InvalidTable);
        }
    }
    Ok(())
}

/// Check all message/body lengths and declared work before a decoder allocation.
/// Only little-endian uncompressed V5 streams with explicit EOS are accepted.
pub(crate) struct SnapshotShape {
    pub schema: TableSchema,
}

pub(crate) fn preflight(mut input: &[u8]) -> Result<SnapshotShape, Error> {
    let mut schema: Option<TableSchema> = None;
    let mut batches = 0_usize;
    let mut rows = 0_usize;
    loop {
        let mut length = word(&mut input)?;
        if length == u32::MAX {
            length = word(&mut input)?;
        }
        if length == 0 {
            if !input.is_empty() {
                return Err(Error::InvalidStream);
            }
            let schema = schema.ok_or(Error::InvalidStream)?;
            return Ok(SnapshotShape { schema });
        }
        let length = usize::try_from(length).map_err(|_| Error::InvalidStream)?;
        let metadata = take(&mut input, length)?;
        let message = arrow_ipc::root_as_message(metadata).map_err(|_| Error::InvalidStream)?;
        if message.version() != MetadataVersion::V5
            || message.custom_metadata().is_some_and(|m| !m.is_empty())
        {
            return Err(Error::InvalidStream);
        }
        let body = usize::try_from(message.bodyLength()).map_err(|_| Error::InvalidStream)?;
        take(&mut input, body)?;
        match message.header_type() {
            MessageHeader::Schema if schema.is_none() && batches == 0 && body == 0 => {
                schema = Some(schema_from_message(
                    message.header_as_schema().ok_or(Error::InvalidStream)?,
                )?);
            }
            MessageHeader::RecordBatch if schema.is_some() => {
                let batch = message
                    .header_as_record_batch()
                    .ok_or(Error::InvalidStream)?;
                if batch.compression().is_some()
                    || batch.variadicBufferCounts().is_some_and(|v| !v.is_empty())
                {
                    return Err(Error::InvalidStream);
                }
                let count = usize::try_from(batch.length()).map_err(|_| Error::InvalidStream)?;
                rows = rows.checked_add(count).ok_or(Error::Limit {
                    resource: "table_rows",
                    limit: Limits::TABLE_ROWS,
                    required: None,
                })?;
                batches += 1;
                let schema = schema.as_ref().expect("schema present");
                Limits::table(rows, schema.columns().len()).map_err(shape_error)?;
                let expected_nodes: usize = schema
                    .columns()
                    .iter()
                    .map(|c| {
                        if matches!(c.kind, ColumnType::Date | ColumnType::DateTime) {
                            3
                        } else {
                            1
                        }
                    })
                    .sum();
                let expected_buffers: usize = schema
                    .columns()
                    .iter()
                    .map(|c| match c.kind {
                        ColumnType::Str => 3,
                        ColumnType::Date | ColumnType::DateTime => 5,
                        _ => 2,
                    })
                    .sum();
                let nodes = batch.nodes().ok_or(Error::InvalidStream)?;
                let buffers = batch.buffers().ok_or(Error::InvalidStream)?;
                if nodes.len() != expected_nodes || buffers.len() != expected_buffers {
                    return Err(Error::InvalidStream);
                }
                for node in nodes {
                    if node.length() != batch.length()
                        || node.null_count() < 0
                        || node.null_count() > node.length()
                    {
                        return Err(Error::InvalidStream);
                    }
                }
                for buffer in buffers {
                    let offset =
                        usize::try_from(buffer.offset()).map_err(|_| Error::InvalidStream)?;
                    let length =
                        usize::try_from(buffer.length()).map_err(|_| Error::InvalidStream)?;
                    if offset.checked_add(length).ok_or(Error::InvalidStream)? > body {
                        return Err(Error::InvalidStream);
                    }
                }
            }
            _ => return Err(Error::InvalidStream),
        }
    }
}

/// Rebuild visible values into fresh arrays, removing all masked physical payloads
/// and out-of-slice buffers before the public IPC writer sees them.
pub(crate) fn sanitized_batch<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    start: usize,
    rows: usize,
) -> Result<RecordBatch, arrow_schema::ArrowError> {
    let mut arrays: Vec<ArrayRef> = Vec::new();
    for (column, field) in table.schema().columns().iter().enumerate() {
        let values =
            (start..start + rows).map(|row| table.cell(row, column).expect("validated coordinate"));
        arrays.push(match field.kind {
            ColumnType::Str => Arc::new(StringArray::from_iter(values.map(|v| match v {
                ValueRef::Str(v) => Some(v),
                _ => None,
            }))),
            ColumnType::Int => Arc::new(Int64Array::from_iter(values.map(|v| match v {
                ValueRef::Int(v) => Some(v),
                _ => None,
            }))),
            ColumnType::Float => Arc::new(Float64Array::from_iter(values.map(|v| match v {
                ValueRef::Float(v) => Some(v.get()),
                _ => None,
            }))),
            ColumnType::Date => Arc::new(date_array(
                &values
                    .map(|v| match v {
                        ValueRef::Date(v) => Some(v),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            )?),
            ColumnType::DateTime => Arc::new(datetime_array(
                &values
                    .map(|v| match v {
                        ValueRef::DateTime(v) => Some(v),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            )?),
        });
    }
    RecordBatch::try_new_with_options(
        physical_schema(table.schema()),
        arrays,
        &RecordBatchOptions::new().with_row_count(Some(rows)),
    )
}
