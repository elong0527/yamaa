//! parquet 55.2.0's column reader, which the reader read stored columns with
//! until #3878: the differential harness's oracle for the native column
//! reader (`columns`), reading the pages with each header presented as the
//! footer is (`differential`).
use super::{columns, footer::Footer, reader::Error, shape};
use parquet::{
    column::reader::{ColumnReader, ColumnReaderImpl},
    data_type::{ByteArray, DataType},
    errors::ParquetError,
    file::{
        metadata::ParquetMetaData, properties::ReaderProperties, reader::RowGroupReader,
        serialized_reader::SerializedRowGroupReader,
    },
    schema::types::ColumnDescriptor,
};
use std::sync::Arc;

/// Make one call into the Parquet crate on admitted input. Its panic, like its
/// error, reports a malformed page, page header or footer.
pub(super) fn native<T>(call: impl FnOnce() -> Result<T, ParquetError>) -> Result<T, Error> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call))
        .map_err(|_| Error::Malformed)?
        .map_err(|_| Error::Malformed)
}

#[derive(Debug)]
enum Column {
    Int32(Vec<Option<i32>>),
    Int64(Vec<Option<i64>>),
    Float(Vec<Option<f32>>),
    Double(Vec<Option<f64>>),
    Bytes(Vec<Option<ByteArray>>),
    Discard,
}

fn collect<T: DataType>(
    reader: &mut ColumnReaderImpl<T>,
    descriptor: &ColumnDescriptor,
    (rows, levels, keep): (usize, usize, bool),
    shapes: &[shape::List],
) -> Result<Vec<Option<T::T>>, Error> {
    let mut result = Vec::new();
    let (mut seen_rows, mut seen_levels) = (0usize, 0usize);
    let mut shapes = shapes.to_vec();
    loop {
        let (mut definitions, mut repetitions, mut values) = (Vec::new(), Vec::new(), Vec::new());
        let (records, non_null, count) = native(|| {
            reader.read_records(
                256,
                Some(&mut definitions),
                Some(&mut repetitions),
                &mut values,
            )
        })?;
        if records == 0 && count == 0 && non_null == 0 {
            break;
        }
        seen_rows = seen_rows
            .checked_add(records)
            .filter(|v| *v <= rows)
            .ok_or(Error::Malformed)?;
        seen_levels = seen_levels
            .checked_add(count)
            .filter(|v| *v <= levels)
            .ok_or(Error::Malformed)?;
        if non_null != values.len()
            || (descriptor.max_def_level() > 0 && definitions.len() != count)
            || (descriptor.max_rep_level() > 0 && repetitions.len() != count)
        {
            return Err(Error::Malformed);
        }
        for index in 0..count {
            let definition = definitions.get(index).copied().unwrap_or(0);
            let repetition = repetitions.get(index).copied().unwrap_or(0);
            for shape in &mut shapes {
                shape.observe(definition, repetition)?;
            }
        }
        if !keep {
            continue;
        }
        let mut values = values.into_iter();
        for definition in definitions
            .into_iter()
            .chain(std::iter::repeat(0))
            .take(count)
        {
            let present = definition == descriptor.max_def_level();
            let value = if present {
                Some(values.next().ok_or(Error::Malformed)?)
            } else {
                None
            };
            result.push(value);
        }
        if values.next().is_some() {
            return Err(Error::Malformed);
        }
    }
    if seen_rows != rows || seen_levels != levels {
        return Err(Error::Malformed);
    }
    for shape in &mut shapes {
        shape.finish()?;
    }
    Ok(result)
}

/// Read the pages of the row groups `groups` of the file `source`, whose
/// footer parquet 55 decoded to `metadata` and the native decoder to
/// `footer`, through parquet 55's column readers. Each call into the
/// Parquet crate is contained on its own (`native`).
pub(super) fn read(
    source: bytes::Bytes,
    metadata: &ParquetMetaData,
    footer: &Footer,
    keep: &[bool],
    schema: &arrow_schema::Schema,
    groups: std::ops::Range<usize>,
) -> Result<Vec<columns::Column>, Error> {
    // Reuse the admitted metadata instead of reparsing the footer through the
    // native decoder's unknown-field skip a second time.
    let source = Arc::new(source);
    let properties = Arc::new(ReaderProperties::builder().build());
    let descriptors = metadata.file_metadata().schema_descr().columns();
    let shapes = shape::constraints(schema, &footer.schema)?;
    if keep.len() != descriptors.len() {
        return Err(Error::Malformed);
    }
    let mut result: Vec<_> = descriptors.iter().map(|_| Column::Discard).collect();
    let row_groups = metadata.row_groups().get(groups).ok_or(Error::Malformed)?;
    for group in row_groups {
        let group = native(|| {
            SerializedRowGroupReader::new(source.clone(), group, None, properties.clone())
        })?;
        let rows = usize::try_from(group.metadata().num_rows()).map_err(|_| Error::Malformed)?;
        if group.num_columns() != descriptors.len() {
            return Err(Error::Malformed);
        }
        for (index, descriptor) in descriptors.iter().enumerate() {
            let levels = usize::try_from(group.metadata().column(index).num_values())
                .map_err(|_| Error::Malformed)?;
            let mut column = native(|| group.get_column_reader(index))?;
            macro_rules! values {
                ($reader:expr,$variant:ident) => {{
                    let mut next = collect(
                        $reader,
                        descriptor,
                        (rows, levels, keep[index]),
                        &shapes[index],
                    )?;
                    if keep[index] {
                        match &mut result[index] {
                            Column::Discard => result[index] = Column::$variant(next),
                            Column::$variant(values) => values.append(&mut next),
                            _ => return Err(Error::Malformed),
                        }
                    }
                }};
            }
            match &mut column {
                ColumnReader::Int32ColumnReader(reader) => values!(reader, Int32),
                ColumnReader::Int64ColumnReader(reader) => values!(reader, Int64),
                ColumnReader::DoubleColumnReader(reader) => values!(reader, Double),
                ColumnReader::ByteArrayColumnReader(reader) => {
                    values!(reader, Bytes)
                }
                ColumnReader::BoolColumnReader(reader) => {
                    collect(reader, descriptor, (rows, levels, false), &shapes[index])?;
                }
                ColumnReader::Int96ColumnReader(reader) => {
                    collect(reader, descriptor, (rows, levels, false), &shapes[index])?;
                }
                ColumnReader::FloatColumnReader(reader) => values!(reader, Float),
                ColumnReader::FixedLenByteArrayColumnReader(reader) => {
                    collect(reader, descriptor, (rows, levels, false), &shapes[index])?;
                }
            }
        }
    }
    // Zero-row files may have no row groups at all; retain their declared type.
    for (index, descriptor) in descriptors.iter().enumerate() {
        if keep[index] && matches!(result[index], Column::Discard) {
            result[index] = match descriptor.physical_type() {
                parquet::basic::Type::INT32 => Column::Int32(Vec::new()),
                parquet::basic::Type::INT64 => Column::Int64(Vec::new()),
                parquet::basic::Type::FLOAT => Column::Float(Vec::new()),
                parquet::basic::Type::DOUBLE => Column::Double(Vec::new()),
                parquet::basic::Type::BYTE_ARRAY => Column::Bytes(Vec::new()),
                _ => return Err(Error::Malformed),
            };
        }
    }
    Ok(result.into_iter().map(converted).collect())
}

/// A column as the native reader holds it.
fn converted(column: Column) -> columns::Column {
    match column {
        Column::Int32(values) => columns::Column::Int32(values),
        Column::Int64(values) => columns::Column::Int64(values),
        Column::Float(values) => columns::Column::Float(values),
        Column::Double(values) => columns::Column::Double(values),
        Column::Bytes(values) => {
            let mut binary = columns::Binary::default();
            for value in &values {
                binary.push(value.as_ref().map(ByteArray::data));
            }
            columns::Column::Bytes(binary)
        }
        Column::Discard => columns::Column::Discard,
    }
}
