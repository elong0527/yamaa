//! Bounded copied IPC streams for installed hosts. No files, callbacks or fallback.
use crate::{
    arrow_table::{date_array, datetime_array, physical_schema, ArrowTable, TableLimits},
    scalar_transport::ScalarValue,
};
use arrow_array::{
    ArrayRef, Float64Array, Int64Array, RecordBatch, RecordBatchOptions, StringArray,
};
use arrow_ipc::{reader::StreamReader, writer::StreamWriter, MessageHeader, MetadataVersion};
use arrow_schema::DataType;
use serde::{
    ser::{SerializeSeq, SerializeStruct},
    Serialize, Serializer,
};
use std::{
    fmt,
    io::{self, Cursor, Write},
    panic::catch_unwind,
    sync::Arc,
};
use yamaa_core::{
    table::{Column, TableAccess, TableSchema, ValueRef},
    value::ColumnType,
};

pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_METADATA: usize = 64 * 1024;
const LIMITS: TableLimits = TableLimits {
    max_rows: 65_536,
    max_columns: 64,
    max_batches: 256,
    max_cells: 262_144,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableTransportError {
    InputLimit,
    OutputLimit,
    ShapeLimit,
    InvalidStream,
    InvalidTable,
    Internal,
}
impl fmt::Display for TableTransportError {
    /// Stable host errors never disclose raw input or panic payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InputLimit => "table IPC input exceeds byte limit",
            Self::OutputLimit => "table output exceeds byte limit",
            Self::ShapeLimit => "table IPC exceeds shape limit",
            Self::InvalidStream => "invalid or unsupported table IPC stream",
            Self::InvalidTable => "invalid table schema or value",
            Self::Internal => "internal table transport failure",
        })
    }
}
impl std::error::Error for TableTransportError {}
type Error = TableTransportError;

/// Bound input before FlatBuffers verification and preserve panic containment.
fn boundary<T>(
    input: &[u8],
    run: impl FnOnce() -> Result<T, Error> + std::panic::UnwindSafe,
) -> Result<T, Error> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(Error::InputLimit);
    }
    catch_unwind(run).map_err(|_| Error::Internal)?
}

/// Return a sanitized IPC stream with exact schema/rows/chunks and no hidden null
/// payloads. Returned bytes are independently owned; no host buffer is retained.
pub fn table_round_trip(input: &[u8]) -> Result<Vec<u8>, Error> {
    boundary(input, || {
        let table = decode(input)?;
        let mut output = LimitedWriter::new(MAX_INPUT_BYTES);
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

/// Inspect logical values using the scalar codec; integers/counts use decimal
/// strings and floats use exact bits. Streaming serialization bounds expansion.
pub fn table_snapshot(input: &[u8]) -> Result<String, Error> {
    boundary(input, || {
        let table = decode(input)?;
        let mut output = LimitedWriter::new(MAX_OUTPUT_BYTES);
        if serde_json::to_writer(&mut output, &Snapshot(&table)).is_err() {
            return Err(if output.exceeded {
                Error::OutputLimit
            } else {
                Error::Internal
            });
        }
        String::from_utf8(output.bytes).map_err(|_| Error::Internal)
    })
}

/// Decode only after every frame, schema and declared shape has been preflighted.
fn decode(input: &[u8]) -> Result<ArrowTable, Error> {
    let logical = preflight(input)?;
    let reader =
        StreamReader::try_new(Cursor::new(input), None).map_err(|_| Error::InvalidStream)?;
    let batches = reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::InvalidStream)?;
    let table = ArrowTable::try_new(logical, batches, LIMITS).map_err(|_| Error::InvalidTable)?;
    // Aliased IPC buffers can multiply logical text far beyond the input length.
    let mut text_bytes = 0_usize;
    for row in 0..table.row_count() {
        for column in 0..table.schema().columns().len() {
            if let ValueRef::Str(value) = table.cell(row, column).expect("validated coordinate") {
                text_bytes = text_bytes
                    .checked_add(value.len())
                    .ok_or(Error::ShapeLimit)?;
                if text_bytes > MAX_INPUT_BYTES {
                    return Err(Error::ShapeLimit);
                }
            }
        }
    }
    Ok(table)
}

/// Retain the same byte/shape/UTF-8/alias checks for dataset execution inputs.
pub(crate) fn decode_snapshot(input: &[u8]) -> Result<ArrowTable, Error> {
    boundary(input, || decode(input))
}

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

fn encode_dataset_projection(
    table: &yamaa_engine::dataset::Dataset,
    projection: Option<&[usize]>,
) -> Result<Vec<u8>, Error> {
    if table.row_count() > LIMITS.max_rows
        || table.schema().columns().len() > LIMITS.max_columns
        || table
            .row_count()
            .checked_mul(table.schema().columns().len())
            .is_none_or(|cells| cells > LIMITS.max_cells)
    {
        return Err(Error::ShapeLimit);
    }
    let batch = sanitized_batch(table, 0, table.row_count()).map_err(|_| Error::Internal)?;
    let batch = match projection {
        Some(projection) => batch.project(projection).map_err(|_| Error::Internal)?,
        None => batch,
    };
    let mut output = LimitedWriter::new(MAX_INPUT_BYTES);
    let result = (|| {
        let mut writer = StreamWriter::try_new(&mut output, &batch.schema())?;
        writer.write(&batch)?;
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
}

/// Serialize directly through a byte cap without constructing a JSON value tree.
pub(crate) fn bounded_json(value: &impl Serialize, maximum: usize) -> Result<String, Error> {
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
fn schema_from_message(schema: arrow_ipc::Schema<'_>) -> Result<TableSchema, Error> {
    if schema.endianness() != arrow_ipc::Endianness::Little
        || schema.custom_metadata().is_some_and(|m| !m.is_empty())
    {
        return Err(Error::InvalidTable);
    }
    let fields = schema.fields().ok_or(Error::InvalidTable)?;
    if fields.len() > LIMITS.max_columns {
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
fn preflight(mut input: &[u8]) -> Result<TableSchema, Error> {
    let mut schema = None;
    let mut batches = 0;
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
            return schema.ok_or(Error::InvalidStream);
        }
        let length = usize::try_from(length).map_err(|_| Error::InvalidStream)?;
        if length > MAX_METADATA {
            return Err(Error::ShapeLimit);
        }
        let metadata = take(&mut input, length)?;
        let options = flatbuffers::VerifierOptions {
            max_depth: 16,
            max_tables: 2048,
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
                batches += 1;
                let schema = schema.as_ref().expect("schema present");
                if rows > LIMITS.max_rows
                    || batches > LIMITS.max_batches
                    || rows
                        .checked_mul(schema.columns().len())
                        .ok_or(Error::ShapeLimit)?
                        > LIMITS.max_cells
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

struct Snapshot<'a>(&'a ArrowTable);
impl Serialize for Snapshot<'_> {
    /// Stream rows through the cap instead of building an expanded JSON value tree.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let table = self.0;
        let mut output = serializer.serialize_struct("Snapshot", 5)?;
        output.serialize_field("protocol", "table/1")?;
        let columns: Vec<_> = table
            .schema()
            .columns()
            .iter()
            .map(|c| {
                (
                    &c.name,
                    match c.kind {
                        ColumnType::Str => "str",
                        ColumnType::Int => "int",
                        ColumnType::Float => "float",
                        ColumnType::Date => "date",
                        ColumnType::DateTime => "datetime",
                    },
                )
            })
            .collect();
        output.serialize_field("columns", &columns)?;
        output.serialize_field("row_count", &table.row_count().to_string())?;
        output.serialize_field(
            "chunks",
            &table
                .batches()
                .iter()
                .map(|b| b.num_rows().to_string())
                .collect::<Vec<_>>(),
        )?;
        output.serialize_field("rows", &Rows(table))?;
        output.end()
    }
}
struct Rows<'a>(&'a ArrowTable);
impl Serialize for Rows<'_> {
    /// Serialize each row independently without retaining previous cell strings.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut rows = serializer.serialize_seq(Some(self.0.row_count()))?;
        for row in 0..self.0.row_count() {
            rows.serialize_element(&Row(self.0, row))?;
        }
        rows.end()
    }
}
struct Row<'a>(&'a ArrowTable, usize);
impl Serialize for Row<'_> {
    /// Reuse the established scalar codec one bounded cell at a time.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use yamaa_core::value::Value;
        let mut row = serializer.serialize_seq(Some(self.0.schema().columns().len()))?;
        for column in 0..self.0.schema().columns().len() {
            let value = match self.0.cell(self.1, column).expect("validated coordinate") {
                ValueRef::Missing => Value::Missing,
                ValueRef::Str(v) => Value::Str(v.into()),
                ValueRef::Int(v) => Value::Int(v),
                ValueRef::Float(v) => Value::Float(v),
                ValueRef::Date(v) => Value::Date(v),
                ValueRef::DateTime(v) => Value::DateTime(v),
                ValueRef::Bool(v) => Value::Bool(v),
            };
            row.serialize_element(&ScalarValue::from_core(value))?;
        }
        row.end()
    }
}
