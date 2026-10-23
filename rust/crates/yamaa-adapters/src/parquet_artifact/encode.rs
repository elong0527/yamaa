use super::Error;
use crate::epoch::{date_days, datetime_seconds};
use parquet::{
    basic::{Compression, LogicalType, Repetition, Type as Physical},
    data_type::{ByteArray, ByteArrayType, DataType, DoubleType, Int32Type, Int64Type},
    file::{
        properties::{EnabledStatistics, WriterProperties},
        writer::{SerializedColumnWriter, SerializedFileWriter},
    },
    schema::types::Type,
};
use std::{io::Write, sync::Arc};
use yamaa_core::{
    limits::Limits,
    table::{TableAccess, ValueRef},
    value::ColumnType,
};

const ROW_GROUP_ROWS: usize = 1024;

struct Sink {
    bytes: Vec<u8>,
    exceeded: bool,
    actual: Option<usize>,
}
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > Limits::OUTPUT_BYTES)
        {
            self.exceeded = true;
            self.actual = self.bytes.len().checked_add(bytes.len());
            return Err(std::io::Error::other("Parquet output byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn field<E>(name: &str, kind: ColumnType) -> Result<Arc<Type>, Error<E>> {
    let (physical, logical) = match kind {
        ColumnType::Str => (Physical::BYTE_ARRAY, Some(LogicalType::String)),
        ColumnType::Int => (Physical::INT64, None),
        ColumnType::Float => (Physical::DOUBLE, None),
        ColumnType::Date => (Physical::INT32, Some(LogicalType::Date)),
        ColumnType::DateTime => (
            Physical::INT64,
            Some(crate::storage_api::timestamp_micros(false)),
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
    writer: &mut SerializedColumnWriter<'_>,
    convert: impl Fn(ValueRef<'_>) -> Option<D::T>,
) -> Result<(), Error<T::Error>> {
    let mut levels = Vec::with_capacity(rows.len());
    let mut values = Vec::new();
    for row in rows {
        let value = table.cell(row, index).map_err(Error::Cell)?;
        if matches!(value, ValueRef::Missing) {
            levels.push(0);
        } else {
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
    sink: &mut Sink,
) -> Result<(), Error<T::Error>> {
    Limits::table(table.row_count(), projection.len()).map_err(|error| Error::Limit {
        resource: error.resource,
        limit: error.limit,
        actual: error.actual,
    })?;
    if projection.is_empty()
        || projection
            .iter()
            .enumerate()
            .any(|(i, &c)| c >= table.schema().columns().len() || projection[..i].contains(&c))
    {
        return Err(Error::Projection);
    }
    let mut fields = Vec::new();
    for &index in projection {
        let selected = &table.schema().columns()[index];
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
                    &mut output,
                    |value| match value {
                        ValueRef::Str(value) => Some(ByteArray::from(value)),
                        _ => None,
                    },
                )?,
                ColumnType::Int => {
                    column::<T, Int64Type>(table, index, rows, &mut output, |value| match value {
                        ValueRef::Int(value) => Some(value),
                        _ => None,
                    })?
                }
                ColumnType::Float => {
                    column::<T, DoubleType>(table, index, rows, &mut output, |value| match value {
                        ValueRef::Float(value) => Some(value.get()),
                        _ => None,
                    })?
                }
                ColumnType::Date => {
                    column::<T, Int32Type>(table, index, rows, &mut output, |value| match value {
                        ValueRef::Date(value) => Some(date_days(value)),
                        _ => None,
                    })?
                }
                ColumnType::DateTime => {
                    column::<T, Int64Type>(table, index, rows, &mut output, |value| match value {
                        ValueRef::DateTime(value) => datetime_seconds(value).checked_mul(1_000_000),
                        _ => None,
                    })?
                }
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
pub(super) fn render<T: TableAccess>(
    table: &T,
    projection: &[usize],
) -> Result<Vec<u8>, Error<T::Error>> {
    let mut sink = Sink {
        bytes: Vec::new(),
        exceeded: false,
        actual: None,
    };
    let result = encode(table, projection, &mut sink);
    if sink.exceeded {
        return Err(Error::Limit {
            resource: "output_bytes",
            limit: Limits::OUTPUT_BYTES,
            actual: sink.actual,
        });
    }
    result?;
    Ok(sink.bytes)
}
