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
use std::{io::Write, ops::Range, sync::Arc};
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

/// One projected column's levels and values for the row group being filled.
struct Pending {
    levels: Vec<i16>,
    values: Values,
}

/// The values of one column type, in Parquet's physical form.
enum Values {
    Text(Vec<ByteArray>),
    Long(Vec<i64>),
    Double(Vec<f64>),
    Days(Vec<i32>),
}

impl Pending {
    fn new(kind: ColumnType) -> Self {
        Self {
            levels: Vec::with_capacity(ROW_GROUP_ROWS),
            values: match kind {
                ColumnType::Str => Values::Text(Vec::new()),
                ColumnType::Int | ColumnType::DateTime => Values::Long(Vec::new()),
                ColumnType::Float => Values::Double(Vec::new()),
                ColumnType::Date => Values::Days(Vec::new()),
            },
        }
    }

    /// Append one cell of a column of `kind`: a missing value as a null level.
    fn push<E>(&mut self, kind: ColumnType, value: ValueRef<'_>) -> Result<(), Error<E>> {
        if matches!(value, ValueRef::Missing) {
            self.levels.push(0);
            return Ok(());
        }
        match (&mut self.values, kind, value) {
            (Values::Text(values), ColumnType::Str, ValueRef::Str(value)) => {
                values.push(ByteArray::from(value))
            }
            (Values::Long(values), ColumnType::Int, ValueRef::Int(value)) => values.push(value),
            (Values::Double(values), ColumnType::Float, ValueRef::Float(value)) => {
                values.push(value.get())
            }
            (Values::Days(values), ColumnType::Date, ValueRef::Date(value)) => {
                values.push(date_days(value))
            }
            (Values::Long(values), ColumnType::DateTime, ValueRef::DateTime(value)) => values.push(
                datetime_seconds(value)
                    .checked_mul(1_000_000)
                    .ok_or(Error::ValueType)?,
            ),
            _ => return Err(Error::ValueType),
        }
        self.levels.push(1);
        Ok(())
    }

    /// Write the pending levels and values as one batch of the column chunk,
    /// then empty them for the next row group.
    fn write<E>(&mut self, writer: &mut SerializedColumnWriter<'_>) -> Result<(), Error<E>> {
        fn batch<D: DataType, E>(
            writer: &mut SerializedColumnWriter<'_>,
            values: &[D::T],
            levels: &[i16],
        ) -> Result<(), Error<E>> {
            writer
                .typed::<D>()
                .write_batch(values, Some(levels), None)
                .map(drop)
                .map_err(Error::Codec)
        }
        match &mut self.values {
            Values::Text(values) => batch::<ByteArrayType, E>(writer, values, &self.levels)?,
            Values::Long(values) => batch::<Int64Type, E>(writer, values, &self.levels)?,
            Values::Double(values) => batch::<DoubleType, E>(writer, values, &self.levels)?,
            Values::Days(values) => batch::<Int32Type, E>(writer, values, &self.levels)?,
        }
        self.levels.clear();
        match &mut self.values {
            Values::Text(values) => values.clear(),
            Values::Long(values) => values.clear(),
            Values::Double(values) => values.clear(),
            Values::Days(values) => values.clear(),
        }
        Ok(())
    }
}

/// Write every pending row as one row group, column by column.
fn row_group<W: Write + Send, E>(
    file: &mut SerializedFileWriter<W>,
    pending: &mut [Pending],
) -> Result<(), Error<E>> {
    let mut group = file.next_row_group().map_err(Error::Codec)?;
    for column in pending {
        let mut output = group
            .next_column()
            .map_err(Error::Codec)?
            .ok_or(Error::Projection)?;
        column.write(&mut output)?;
        output.close().map_err(Error::Codec)?;
    }
    group.close().map_err(Error::Codec)?;
    Ok(())
}

/// Encode `partitions` of `table`, one after another, returning where each
/// partition's bytes end. Rows wait in `pending` until a row group is full,
/// and the last group is written only when the file ends, so the row groups
/// are the whole table's whatever the partitions (D264, D160).
fn encode<T: TableAccess>(
    table: &T,
    projection: &[usize],
    partitions: &[Range<usize>],
    sink: &mut Sink,
) -> Result<Vec<usize>, Error<T::Error>> {
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
    let kinds: Vec<ColumnType> = projection
        .iter()
        .map(|&index| table.schema().columns()[index].kind)
        .collect();
    let mut file = SerializedFileWriter::new(sink, schema, properties).map_err(Error::Codec)?;
    let mut pending: Vec<Pending> = kinds.iter().map(|&kind| Pending::new(kind)).collect();
    let mut held = 0;
    let mut ends = Vec::with_capacity(partitions.len());
    for rows in partitions {
        Limits::table(rows.len(), projection.len()).map_err(|error| Error::Limit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
        })?;
        for row in rows.clone() {
            for ((column, &kind), &index) in pending.iter_mut().zip(&kinds).zip(projection) {
                column.push(kind, table.cell(row, index).map_err(Error::Cell)?)?;
            }
            held += 1;
            if held == ROW_GROUP_ROWS {
                row_group(&mut file, &mut pending)?;
                held = 0;
            }
        }
        ends.push(file.bytes_written());
    }
    if held > 0 {
        row_group(&mut file, &mut pending)?;
    }
    file.close().map_err(Error::Codec)?;
    Ok(ends)
}

/// Retain only a complete file. Codec errors or any limit discard all bytes.
/// Zero records still produce a readable schema with every projected field.
/// The last partition's bytes end with the file, its last row group and
/// footer included.
pub(super) fn render_partitions<T: TableAccess>(
    table: &T,
    projection: &[usize],
    partitions: &[Range<usize>],
) -> Result<super::Partitioned, Error<T::Error>> {
    let mut sink = Sink {
        bytes: Vec::new(),
        exceeded: false,
        actual: None,
    };
    let result = encode(table, projection, partitions, &mut sink);
    if sink.exceeded {
        return Err(Error::Limit {
            resource: "output_bytes",
            limit: Limits::OUTPUT_BYTES,
            actual: sink.actual,
        });
    }
    let mut ends = result?;
    if let Some(last) = ends.last_mut() {
        *last = sink.bytes.len();
    }
    Ok((sink.bytes, ends))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every host's default partition (#3794) holds whole row groups, so the
    /// row groups a partitioned write ends are the whole-table write's, and
    /// it is the most of them whose partition of the widest table the cell
    /// limit admits.
    #[test]
    fn the_default_partition_is_the_most_whole_row_groups_the_cell_limit_admits() {
        let rows = Limits::PARTITION_ROWS;
        assert_eq!(
            (
                rows % ROW_GROUP_ROWS,
                Limits::table(rows, Limits::COLUMNS),
                Limits::table(rows + ROW_GROUP_ROWS, Limits::COLUMNS)
                    .map_err(|error| error.resource),
            ),
            (0, Ok(()), Err("table_cells")),
            "the default partition of {rows} rows"
        );
    }
}
