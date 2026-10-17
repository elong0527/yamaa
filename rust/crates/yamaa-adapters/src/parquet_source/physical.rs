//! Opaque physical values: read every chunk before profile/text validation.
use super::{reader::Error, shape};
use parquet::{
    column::reader::{ColumnReader, ColumnReaderImpl},
    data_type::{ByteArray, DataType},
    file::{metadata::ParquetMetaData, properties::ReaderProperties, reader::RowGroupReader},
    schema::types::ColumnDescriptor,
};
use std::sync::Arc;

#[derive(Debug)]
pub(super) enum Column {
    Int32(Vec<Option<i32>>),
    Int64(Vec<Option<i64>>),
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
        let (records, non_null, count) = reader
            .read_records(
                256,
                Some(&mut definitions),
                Some(&mut repetitions),
                &mut values,
            )
            .map_err(|_| Error::Malformed)?;
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

pub(super) fn read(
    bytes: &[u8],
    metadata: &ParquetMetaData,
    keep: &[bool],
    schema: &arrow_schema::Schema,
) -> Result<Vec<Column>, Error> {
    // Reuse the admitted metadata instead of reparsing the original footer
    // through the native unknown-boolean-collection skip path a second time.
    let source = Arc::new(bytes::Bytes::copy_from_slice(bytes));
    let properties = Arc::new(ReaderProperties::builder().build());
    let descriptors = metadata.file_metadata().schema_descr().columns();
    let shapes = shape::constraints(schema, metadata.file_metadata().schema_descr())?;
    if keep.len() != descriptors.len() {
        return Err(Error::Malformed);
    }
    let mut result: Vec<_> = descriptors.iter().map(|_| Column::Discard).collect();
    for (index, group) in metadata.row_groups().iter().enumerate() {
        let group = crate::storage_api::row_group_reader(
            source.clone(),
            group,
            metadata,
            index,
            properties.clone(),
        )
        .map_err(|_| Error::Malformed)?;
        let rows = usize::try_from(group.metadata().num_rows()).map_err(|_| Error::Malformed)?;
        if group.num_columns() != descriptors.len() {
            return Err(Error::Malformed);
        }
        for (index, descriptor) in descriptors.iter().enumerate() {
            let levels = usize::try_from(group.metadata().column(index).num_values())
                .map_err(|_| Error::Malformed)?;
            let mut column = group
                .get_column_reader(index)
                .map_err(|_| Error::Malformed)?;
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
                ColumnReader::FloatColumnReader(reader) => {
                    collect(reader, descriptor, (rows, levels, false), &shapes[index])?;
                }
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
                parquet::basic::Type::DOUBLE => Column::Double(Vec::new()),
                parquet::basic::Type::BYTE_ARRAY => Column::Bytes(Vec::new()),
                _ => return Err(Error::Malformed),
            };
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::{Int64Array, RecordBatch};
    use arrow_schema::{DataType as ArrowType, Field, Schema};
    use parquet::arrow::ArrowWriter;
    use parquet::file::{reader::FileReader, serialized_reader::SerializedFileReader};

    #[test]
    fn physical_values_are_decoded_in_order() {
        let schema = Arc::new(Schema::new(vec![Field::new("N", ArrowType::Int64, false)]));
        let batch =
            RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(vec![1, 2]))])
                .unwrap();
        let mut bytes = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut bytes, schema.clone(), None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        let reader = SerializedFileReader::new(bytes::Bytes::copy_from_slice(&bytes)).unwrap();
        let result = read(&bytes, reader.metadata(), &[true], &schema).unwrap();
        assert!(matches!(&result[0], Column::Int64(values) if values == &[Some(1), Some(2)]));
    }
}
