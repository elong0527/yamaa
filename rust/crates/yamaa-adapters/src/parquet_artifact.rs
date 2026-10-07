//! Closed Parquet output profile over an admitted, immutable typed table.
//! The sink, schema, table shape and per-column row-group staging are bounded.
//! No filesystem authority, Arrow metadata, compression or numeric rounding.
use crate::arrow_temporal::{date_days, datetime_seconds};
use parquet::{
    basic::{Compression, LogicalType, Repetition, TimeUnit, Type as Physical},
    data_type::{ByteArray, ByteArrayType, DataType, DoubleType, Int32Type, Int64Type},
    errors::ParquetError,
    file::{
        properties::{EnabledStatistics, WriterProperties},
        writer::{SerializedColumnWriter, SerializedFileWriter},
    },
    schema::types::Type,
};
use std::{io::Write, sync::Arc};
use yamaa_core::{
    table::{CellError, TableAccess, ValueRef},
    value::ColumnType,
};

const ROW_GROUP_ROWS: usize = 1024;

/// Trusted resource policy, separate from the format's semantic requirements.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub output_bytes: usize,
    pub staged_bytes: usize,
    pub cells: usize,
    pub columns: usize,
}

#[derive(Debug)]
pub enum Error<E> {
    Projection,
    Limit,
    Cell(CellError<E>),
    ValueType,
    Codec(ParquetError),
}

struct Sink {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.maximum)
        {
            self.exceeded = true;
            return Err(std::io::Error::other("Parquet output byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn charge<E>(used: &mut usize, amount: usize, maximum: usize) -> Result<(), Error<E>> {
    *used = used
        .checked_add(amount)
        .filter(|n| *n <= maximum)
        .ok_or(Error::Limit)?;
    Ok(())
}

fn field<E>(name: &str, kind: ColumnType) -> Result<Arc<Type>, Error<E>> {
    let (physical, logical) = match kind {
        ColumnType::Str => (Physical::BYTE_ARRAY, Some(LogicalType::String)),
        ColumnType::Int => (Physical::INT64, None),
        ColumnType::Float => (Physical::DOUBLE, None),
        ColumnType::Date => (Physical::INT32, Some(LogicalType::Date)),
        ColumnType::DateTime => (
            Physical::INT64,
            Some(LogicalType::timestamp(false, TimeUnit::MICROS)),
        ),
    };
    Type::primitive_type_builder(name, physical)
        .with_repetition(Repetition::OPTIONAL)
        .with_logical_type(logical)
        .build()
        .map(Arc::new)
        .map_err(Error::Codec)
}

fn column<T: TableAccess, D: DataType>(
    table: &T,
    index: usize,
    rows: std::ops::Range<usize>,
    maximum: usize,
    writer: &mut SerializedColumnWriter<'_>,
    convert: impl Fn(ValueRef<'_>) -> Option<D::T>,
) -> Result<(), Error<T::Error>> {
    let mut charged = 0;
    charge(
        &mut charged,
        rows.len().checked_mul(2).ok_or(Error::Limit)?,
        maximum,
    )?;
    let mut levels = Vec::with_capacity(rows.len());
    let mut values = Vec::new();
    for row in rows {
        let value = table.cell(row, index).map_err(Error::Cell)?;
        if matches!(value, ValueRef::Missing) {
            levels.push(0);
        } else {
            // Charge before copying text. Fixed overhead covers value slots and
            // geometric Vec growth; the codec buffers at most this row group.
            let payload = match value {
                ValueRef::Str(text) => text.len(),
                _ => 8,
            };
            charge(
                &mut charged,
                payload.checked_add(64).ok_or(Error::Limit)?,
                maximum,
            )?;
            values.push(convert(value).ok_or(Error::ValueType)?);
            levels.push(1);
        }
    }
    writer
        .typed::<D>()
        .write_batch(&values, Some(&levels), None)
        .map_err(Error::Codec)?;
    Ok(())
}

fn encode<T: TableAccess>(
    table: &T,
    projection: &[usize],
    limits: Limits,
    sink: &mut Sink,
) -> Result<(), Error<T::Error>> {
    if projection.len() > limits.columns
        || table
            .row_count()
            .checked_mul(projection.len())
            .is_none_or(|n| n > limits.cells)
    {
        return Err(Error::Limit);
    }
    if projection.is_empty()
        || projection
            .iter()
            .enumerate()
            .any(|(i, &c)| c >= table.schema().columns().len() || projection[..i].contains(&c))
    {
        return Err(Error::Projection);
    }
    let mut schema_bytes = 0;
    let mut fields = Vec::new();
    for &index in projection {
        let selected = &table.schema().columns()[index];
        charge(
            &mut schema_bytes,
            selected.name.len().checked_add(256).ok_or(Error::Limit)?,
            limits.staged_bytes,
        )?;
        fields.push(field(&selected.name, selected.kind)?);
    }
    let schema = Arc::new(
        Type::group_type_builder("schema")
            .with_fields(fields)
            .build()
            .map_err(Error::Codec)?,
    );
    let properties = Arc::new(
        WriterProperties::builder()
            .set_compression(Compression::UNCOMPRESSED)
            .set_dictionary_enabled(false)
            .set_statistics_enabled(EnabledStatistics::None)
            .set_offset_index_disabled(true)
            .set_key_value_metadata(None)
            .set_write_batch_size(ROW_GROUP_ROWS)
            .build(),
    );
    let mut file = SerializedFileWriter::new(sink, schema, properties).map_err(Error::Codec)?;
    for start in (0..table.row_count()).step_by(ROW_GROUP_ROWS) {
        let end = start.saturating_add(ROW_GROUP_ROWS).min(table.row_count());
        let mut group = file.next_row_group().map_err(Error::Codec)?;
        for &index in projection {
            let mut output = group
                .next_column()
                .map_err(Error::Codec)?
                .ok_or(Error::Projection)?;
            let rows = start..end;
            match table.schema().columns()[index].kind {
                ColumnType::Str => column::<T, ByteArrayType>(
                    table,
                    index,
                    rows,
                    limits.staged_bytes,
                    &mut output,
                    |value| match value {
                        ValueRef::Str(value) => Some(ByteArray::from(value)),
                        _ => None,
                    },
                )?,
                ColumnType::Int => column::<T, Int64Type>(
                    table,
                    index,
                    rows,
                    limits.staged_bytes,
                    &mut output,
                    |value| match value {
                        ValueRef::Int(value) => Some(value),
                        _ => None,
                    },
                )?,
                ColumnType::Float => column::<T, DoubleType>(
                    table,
                    index,
                    rows,
                    limits.staged_bytes,
                    &mut output,
                    |value| match value {
                        ValueRef::Float(value) => Some(value.get()),
                        _ => None,
                    },
                )?,
                ColumnType::Date => column::<T, Int32Type>(
                    table,
                    index,
                    rows,
                    limits.staged_bytes,
                    &mut output,
                    |value| match value {
                        ValueRef::Date(value) => Some(date_days(value)),
                        _ => None,
                    },
                )?,
                ColumnType::DateTime => column::<T, Int64Type>(
                    table,
                    index,
                    rows,
                    limits.staged_bytes,
                    &mut output,
                    |value| match value {
                        ValueRef::DateTime(value) => datetime_seconds(value).checked_mul(1_000_000),
                        _ => None,
                    },
                )?,
            }
            output.close().map_err(Error::Codec)?;
        }
        group.close().map_err(Error::Codec)?;
    }
    file.close().map_err(Error::Codec)?;
    Ok(())
}

/// Retain only a complete file. Codec errors or any limit discard all bytes.
/// Zero records still produce a readable schema with every projected field.
pub fn render<T: TableAccess>(
    table: &T,
    projection: &[usize],
    limits: Limits,
) -> Result<Vec<u8>, Error<T::Error>> {
    let mut sink = Sink {
        bytes: Vec::new(),
        maximum: limits.output_bytes,
        exceeded: false,
    };
    let result = encode(table, projection, limits, &mut sink);
    if sink.exceeded {
        return Err(Error::Limit);
    }
    result?;
    Ok(sink.bytes)
}

struct RecordText {
    current: String,
    used: usize,
    maximum: usize,
}
impl RecordText {
    fn append<E>(&mut self, text: &str) -> Result<(), Error<E>> {
        charge(&mut self.used, text.len(), self.maximum)?;
        self.current.push_str(text);
        Ok(())
    }
    fn quoted<E>(&mut self, text: &str) -> Result<(), Error<E>> {
        self.append("\"")?;
        for ch in text.chars() {
            match ch {
                '"' => self.append("\\\"")?,
                '\\' => self.append("\\\\")?,
                '\n' => self.append("\\n")?,
                '\r' => self.append("\\r")?,
                '\t' => self.append("\\t")?,
                '\u{8}' => self.append("\\b")?,
                '\u{c}' => self.append("\\f")?,
                ' '..='~' => self.append(ch.encode_utf8(&mut [0; 4]))?,
                _ => {
                    for value in ch.encode_utf16(&mut [0; 2]) {
                        self.append(&format!("\\u{value:04x}"))?;
                    }
                }
            }
        }
        self.append("\"")
    }
}

/// Portable report records use the reference JSON spelling, independently of
/// Parquet's binary encoding. Dates name complete stored calendar fields.
pub fn records<T: TableAccess>(
    table: &T,
    projection: &[usize],
    maximum: usize,
) -> Result<Vec<String>, Error<T::Error>> {
    if projection.is_empty()
        || projection
            .iter()
            .any(|&i| i >= table.schema().columns().len())
    {
        return Err(Error::Projection);
    }
    let mut text = RecordText {
        current: String::new(),
        used: 0,
        maximum,
    };
    text.append("[")?;
    for (i, &column) in projection.iter().enumerate() {
        if i != 0 {
            text.append(", ")?;
        }
        text.quoted(&table.schema().columns()[column].name)?;
    }
    text.append("]")?;
    let mut records = vec![std::mem::take(&mut text.current)];
    for row in 0..table.row_count() {
        text.append("[")?;
        for (i, &column) in projection.iter().enumerate() {
            if i != 0 {
                text.append(", ")?;
            }
            match table.cell(row, column).map_err(Error::Cell)? {
                ValueRef::Missing => text.append("null")?,
                ValueRef::Str(value) => text.quoted(value)?,
                ValueRef::Int(value) => text.append(&value.to_string())?,
                ValueRef::Float(value) => {
                    // The shared finite-scalar renderer already pins Python
                    // shortest-round-trip spelling, exponent padding and -0.0.
                    let node = yamaa_core::schema::DocumentNode::Float(value.get());
                    let scalar = yamaa_core::schema::scalar_diagnostic_label(&node)
                        .ok_or(Error::ValueType)?;
                    text.append(&scalar)?;
                }
                ValueRef::Date(value) => {
                    let (year, month, day) = value.fields();
                    text.quoted(&format!("{year:04}-{month:02}-{day:02}"))?;
                }
                ValueRef::DateTime(value) => {
                    let (year, month, day, hour, minute, second) = value.fields();
                    text.quoted(&format!(
                        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
                    ))?;
                }
                ValueRef::Bool(_) => return Err(Error::ValueType),
            }
        }
        text.append("]")?;
        records.push(std::mem::take(&mut text.current));
    }
    Ok(records)
}
