//! Bounded copied IPC streams for installed hosts. No files, callbacks or fallback.
use crate::arrow_table::{date_array, datetime_array, physical_schema, ArrowTable, TableLimits};
use arrow_array::{
    ArrayRef, Float64Array, Int64Array, RecordBatch, RecordBatchOptions, StringArray,
};
use arrow_ipc::{reader::StreamReader, writer::StreamWriter, MessageHeader, MetadataVersion};
use arrow_schema::DataType;
use serde::Serialize;
use std::{
    fmt,
    io::{self, Cursor, Write},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};
use yamaa_core::{
    table::{Column, TableAccess, TableSchema, ValueRef},
    value::ColumnType,
};
use yamaa_engine::dataset::{Dataset, ExecutionBudget};

pub const MAX_INPUT_BYTES: usize = ExecutionBudget::bounded().max_ipc_bytes;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
// Keep per-message bytes and verifier traversal bounded while admitting the
// schema and record-batch metadata for the wider default column policy.
const MAX_METADATA: usize = yamaa_core::specification::DEFAULT_MAX_COLUMNS * 1024;
const MAX_METADATA_TABLES: usize = yamaa_core::specification::DEFAULT_MAX_COLUMNS * 32;

fn source_limits(budget: ExecutionBudget) -> TableLimits {
    TableLimits {
        max_rows: budget.execution.source_rows,
        max_columns: budget.max_columns,
        max_batches: budget.max_batches,
        max_cells: budget.max_decoded_cells,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    InputLimit,
    OutputLimit,
    ShapeLimit,
    InvalidStream,
    InvalidTable,
    OutputIo,
    Internal,
}
impl fmt::Display for TableError {
    /// Stable host errors never disclose raw input or panic payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InputLimit => "table IPC input exceeds byte limit",
            Self::OutputLimit => "table output exceeds byte limit",
            Self::ShapeLimit => "table IPC exceeds shape limit",
            Self::InvalidStream => "invalid or unsupported table IPC stream",
            Self::InvalidTable => "invalid table schema or value",
            Self::OutputIo => "table IPC output write failed",
            Self::Internal => "internal table codec failure",
        })
    }
}
impl std::error::Error for TableError {}
type Error = TableError;

/// Bound input before FlatBuffers verification and preserve panic containment.
fn boundary<T>(
    input: &[u8],
    budget: ExecutionBudget,
    run: impl FnOnce() -> Result<T, Error> + std::panic::UnwindSafe,
) -> Result<T, Error> {
    if input.len() > budget.max_ipc_bytes {
        return Err(Error::InputLimit);
    }
    if budget.max_sources == 0 {
        return Err(Error::ShapeLimit);
    }
    catch_unwind(run).map_err(|_| Error::Internal)?
}

/// Return a sanitized IPC stream with exact schema/rows/chunks and no hidden null
/// payloads. Returned bytes are independently owned; no host buffer is retained.
pub fn table_round_trip(input: &[u8]) -> Result<Vec<u8>, Error> {
    table_round_trip_with_budget(input, ExecutionBudget::default())
}

/// Apply caller-selected byte/shape limits while preserving input chunk order.
pub fn table_round_trip_with_budget(
    input: &[u8],
    budget: ExecutionBudget,
) -> Result<Vec<u8>, Error> {
    boundary(input, budget, || {
        let table = decode(input, budget)?;
        let mut output = LimitedWriter::new(budget.max_output_bytes);
        let result = (|| {
            let mut writer = StreamWriter::try_new(&mut output, &physical_schema(table.schema()))?;
            let mut start = 0;
            for batch in table.batches() {
                let clean = sanitized_batch(&table, start, batch.num_rows())?;
                writer.write(&clean)?;
                start += batch.num_rows();
            }
            writer.finish()
        })();
        if result.is_err() {
            return Err(if output.exceeded {
                Error::OutputLimit
            } else {
                Error::Internal
            });
        }
        Ok(output.bytes)
    })
}

/// Decode only after every frame, schema and declared shape has been preflighted.
fn decode(input: &[u8], budget: ExecutionBudget) -> Result<ArrowTable, Error> {
    let shape = preflight(input, budget)?;
    decode_preflighted(input, shape, budget, budget.max_ipc_bytes).map(|(table, _)| table)
}

pub(crate) fn decode_preflighted(
    input: &[u8],
    shape: SnapshotShape,
    budget: ExecutionBudget,
    text_limit: usize,
) -> Result<(ArrowTable, usize), Error> {
    let mut reader = SnapshotBatches::new(input, shape.schema.clone(), budget, text_limit)?;
    let batches = reader.by_ref().collect::<Result<Vec<_>, _>>()?;
    let table = ArrowTable::try_new(shape.schema, batches, source_limits(budget))
        .map_err(|_| Error::InvalidTable)?;
    Ok((table, text_limit - reader.text_left))
}

fn logical_text_bytes(table: &ArrowTable, text_limit: usize) -> Result<usize, Error> {
    // Aliased IPC buffers can multiply logical text far beyond the input length.
    let mut text_bytes = 0_usize;
    for (column, field) in table.schema().columns().iter().enumerate() {
        if field.kind == ColumnType::Str {
            for row in 0..table.row_count() {
                if let ValueRef::Str(value) = table.cell(row, column).expect("validated coordinate")
                {
                    text_bytes = text_bytes
                        .checked_add(value.len())
                        .ok_or(Error::ShapeLimit)?;
                    if text_bytes > text_limit {
                        return Err(Error::ShapeLimit);
                    }
                }
            }
        }
    }
    Ok(text_bytes)
}

/// Retain the same byte/shape/UTF-8/alias checks for dataset execution inputs.
pub fn decode_snapshot(input: &[u8]) -> Result<ArrowTable, Error> {
    decode_snapshot_with_budget(input, ExecutionBudget::default())
}

/// Preflight the entire stream before decoding under a caller-selected policy.
pub fn decode_snapshot_with_budget(
    input: &[u8],
    budget: ExecutionBudget,
) -> Result<ArrowTable, Error> {
    boundary(input, budget, || decode(input, budget))
}

/// Decode one normalized, checked batch at a time after whole-stream framing
/// admission. Consumers must exhaust the iterator to validate all values.
/// The input bytes stay borrowed; previously yielded batches need not be retained.
pub fn snapshot_batches(
    input: &[u8],
    budget: ExecutionBudget,
) -> Result<SnapshotBatches<'_>, Error> {
    boundary(input, budget, || {
        let shape = preflight(input, budget)?;
        SnapshotBatches::new(input, shape.schema, budget, budget.max_ipc_bytes)
    })
}

pub struct SnapshotBatches<'a> {
    reader: StreamReader<Cursor<&'a [u8]>>,
    schema: TableSchema,
    budget: ExecutionBudget,
    text_left: usize,
    finished: bool,
}

impl<'a> SnapshotBatches<'a> {
    fn new(
        input: &'a [u8],
        schema: TableSchema,
        budget: ExecutionBudget,
        text_limit: usize,
    ) -> Result<Self, Error> {
        Ok(Self {
            reader: StreamReader::try_new(Cursor::new(input), None)
                .map_err(|_| Error::InvalidStream)?,
            schema,
            budget,
            text_left: text_limit,
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
                let table = ArrowTable::try_new(
                    self.schema.clone(),
                    vec![batch],
                    source_limits(self.budget),
                )
                .map_err(|_| Error::InvalidTable)?;
                self.text_left -= logical_text_bytes(&table, self.text_left)?;
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
pub(crate) fn encode_dataset(table: &yamaa_engine::dataset::Dataset) -> Result<Vec<u8>, Error> {
    encode_dataset_projection(table, None)
}

/// Export the projection already admitted by the output use case.
pub(crate) fn encode_projected_dataset(
    table: &yamaa_engine::dataset::Dataset,
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
    encode_table_projection(table, projection, ExecutionBudget::default())
}

/// Export accepted output without a fixed byte or single-batch restriction.
pub fn encode_dataset_with_budget(
    table: &Dataset,
    budget: ExecutionBudget,
) -> Result<Vec<u8>, Error> {
    encode_table_projection(table, None, budget)
}

fn encode_table_projection<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    projection: Option<&[usize]>,
    budget: ExecutionBudget,
) -> Result<Vec<u8>, Error> {
    let mut output = LimitedWriter::new(budget.max_output_bytes);
    write_table_projection(table, projection, &mut output, budget)?;
    Ok(output.bytes)
}

/// Lazily rebuild accepted values into sanitized Arrow batches.
/// No batch is exposed until the complete dataset has passed engine checks.
pub struct DatasetBatches<'a, T = Dataset> {
    table: &'a T,
    projection: Option<&'a [usize]>,
    start: usize,
    batch_rows: usize,
    remaining: usize,
}

/// Chunk an already accepted dataset; None preserves single-batch export.
pub fn dataset_batches(
    table: &Dataset,
    budget: ExecutionBudget,
) -> Result<DatasetBatches<'_>, Error> {
    table_batches(table, None, budget)
}

fn table_batches<'a, T: TableAccess<Error = std::convert::Infallible>>(
    table: &'a T,
    projection: Option<&'a [usize]>,
    budget: ExecutionBudget,
) -> Result<DatasetBatches<'a, T>, Error> {
    if table.row_count() > budget.execution.output_rows
        || table.schema().columns().len() > budget.max_columns
        || table
            .row_count()
            .checked_mul(table.schema().columns().len())
            .is_none_or(|cells| cells > budget.execution.output_cells)
    {
        return Err(Error::ShapeLimit);
    }
    if projection.is_some_and(|columns| {
        columns
            .iter()
            .any(|&column| column >= table.schema().columns().len())
    }) {
        return Err(Error::InvalidTable);
    }
    let batch_rows = budget
        .batch_rows
        .map_or(table.row_count().max(1), |rows| rows.get());
    let batches = if table.row_count() == 0 {
        1
    } else {
        (table.row_count() - 1) / batch_rows + 1
    };
    if batches > budget.max_batches {
        return Err(Error::ShapeLimit);
    }
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

/// Write chunks directly to the supplied sink, bounding cumulative encoded bytes.
/// A sink failure can leave a partial stream; publication belongs to the caller.
pub fn write_dataset_with_budget(
    table: &Dataset,
    output: &mut impl Write,
    budget: ExecutionBudget,
) -> Result<(), Error> {
    write_table_projection(table, None, output, budget)
}

fn write_table_projection<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    projection: Option<&[usize]>,
    output: &mut impl Write,
    budget: ExecutionBudget,
) -> Result<(), Error> {
    let batches = table_batches(table, projection, budget)?;
    let schema = physical_schema(table.schema());
    let schema = match projection {
        Some(projection) => Arc::new(
            schema
                .project(projection)
                .map_err(|_| Error::InvalidTable)?,
        ),
        None => schema,
    };
    let mut output = CappedWriter {
        output,
        remaining: budget.max_output_bytes,
        exceeded: false,
    };
    let result = (|| {
        let mut writer =
            StreamWriter::try_new(&mut output, &schema).map_err(|_| Error::OutputIo)?;
        for batch in batches {
            writer.write(&batch?).map_err(|_| Error::OutputIo)?;
        }
        writer.finish().map_err(|_| Error::OutputIo)
    })();
    if output.exceeded {
        Err(Error::OutputLimit)
    } else {
        result
    }
}

struct CappedWriter<'a, W> {
    output: &'a mut W,
    remaining: usize,
    exceeded: bool,
}

impl<W: Write> Write for CappedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            self.exceeded = true;
            return Err(io::Error::other("output limit"));
        }
        let written = self.output.write(bytes)?;
        self.remaining -= written;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

/// Serialize directly through a byte cap without constructing a JSON value tree.
pub fn bounded_json(value: &impl Serialize, maximum: usize) -> Result<String, Error> {
    let mut output = LimitedWriter::new(maximum);
    if serde_json::to_writer(&mut output, value).is_err() {
        return Err(if output.exceeded {
            Error::OutputLimit
        } else {
            Error::Internal
        });
    }
    String::from_utf8(output.bytes).map_err(|_| Error::Internal)
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
fn schema_from_message(
    schema: arrow_ipc::Schema<'_>,
    max_columns: usize,
) -> Result<TableSchema, Error> {
    if schema.endianness() != arrow_ipc::Endianness::Little
        || schema.custom_metadata().is_some_and(|m| !m.is_empty())
    {
        return Err(Error::InvalidTable);
    }
    let fields = schema.fields().ok_or(Error::InvalidTable)?;
    if fields.len() > max_columns {
        return Err(Error::ShapeLimit);
    }
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
    pub rows: usize,
    pub cells: usize,
    pub batches: usize,
}

pub(crate) fn preflight(mut input: &[u8], budget: ExecutionBudget) -> Result<SnapshotShape, Error> {
    if input.len() > budget.max_ipc_bytes {
        return Err(Error::InputLimit);
    }
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
            let cells = rows
                .checked_mul(schema.columns().len())
                .ok_or(Error::ShapeLimit)?;
            return Ok(SnapshotShape {
                schema,
                rows,
                cells,
                batches,
            });
        }
        let length = usize::try_from(length).map_err(|_| Error::InvalidStream)?;
        if length > MAX_METADATA {
            return Err(Error::ShapeLimit);
        }
        let metadata = take(&mut input, length)?;
        let options = flatbuffers::VerifierOptions {
            max_depth: 16,
            max_tables: MAX_METADATA_TABLES,
            max_apparent_size: MAX_METADATA * 4,
            ..Default::default()
        };
        let message = arrow_ipc::root_as_message_with_opts(&options, metadata)
            .map_err(|_| Error::InvalidStream)?;
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
                    budget.max_columns,
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
                rows = rows.checked_add(count).ok_or(Error::ShapeLimit)?;
                batches = batches.checked_add(1).ok_or(Error::ShapeLimit)?;
                let schema = schema.as_ref().expect("schema present");
                if rows > budget.execution.source_rows
                    || batches > budget.max_batches
                    || rows
                        .checked_mul(schema.columns().len())
                        .ok_or(Error::ShapeLimit)?
                        > budget.max_decoded_cells
                {
                    return Err(Error::ShapeLimit);
                }
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

struct LimitedWriter {
    maximum: usize,
    bytes: Vec<u8>,
    exceeded: bool,
}
impl LimitedWriter {
    /// Apply the protocol-specific cap; IPC output must be valid input again.
    fn new(maximum: usize) -> Self {
        Self {
            maximum,
            bytes: Vec::new(),
            exceeded: false,
        }
    }
}
impl Write for LimitedWriter {
    /// Never append beyond the transport output budget, including JSON expansion.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum - self.bytes.len() {
            self.exceeded = true;
            return Err(io::Error::other("output limit"));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    /// The owned memory buffer has no external flush action.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
