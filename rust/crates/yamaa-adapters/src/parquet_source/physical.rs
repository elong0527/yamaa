//! Opaque physical values: read every chunk before profile/text validation.
use super::reader::Error;
use parquet::{
    column::reader::{ColumnReader, ColumnReaderImpl},
    data_type::{ByteArray, DataType},
    file::{reader::FileReader, serialized_reader::SerializedFileReader},
    schema::types::ColumnDescriptor,
};

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
    (rows, levels): (usize, usize),
    keep: bool,
    budget: &mut usize,
    size: impl Fn(&T::T) -> usize,
) -> Result<Vec<Option<T::T>>, Error> {
    let mut result = Vec::new();
    let (mut seen_rows, mut seen_levels) = (0usize, 0usize);
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
            let charge = 32usize
                .checked_add(value.as_ref().map_or(0, &size))
                .ok_or(Error::Limit)?;
            *budget = budget.checked_sub(charge).ok_or(Error::Limit)?;
            result.push(value);
        }
        if values.next().is_some() {
            return Err(Error::Malformed);
        }
    }
    if seen_rows != rows || seen_levels != levels {
        return Err(Error::Malformed);
    }
    Ok(result)
}

pub(super) fn read(
    bytes: &[u8],
    keep: &[bool],
    retained_bytes: usize,
) -> Result<Vec<Column>, Error> {
    let reader = SerializedFileReader::new(bytes::Bytes::copy_from_slice(bytes))
        .map_err(|_| Error::Malformed)?;
    let descriptors = reader.metadata().file_metadata().schema_descr().columns();
    if keep.len() != descriptors.len() {
        return Err(Error::Malformed);
    }
    let mut result: Vec<_> = descriptors.iter().map(|_| Column::Discard).collect();
    let mut budget = retained_bytes;
    for group in 0..reader.num_row_groups() {
        let group = reader.get_row_group(group).map_err(|_| Error::Malformed)?;
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
                ($reader:expr,$variant:ident,$size:expr) => {{
                    let mut next = collect(
                        $reader,
                        descriptor,
                        (rows, levels),
                        keep[index],
                        &mut budget,
                        $size,
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
                ColumnReader::Int32ColumnReader(reader) => values!(reader, Int32, |_| 0),
                ColumnReader::Int64ColumnReader(reader) => values!(reader, Int64, |_| 0),
                ColumnReader::DoubleColumnReader(reader) => values!(reader, Double, |_| 0),
                ColumnReader::ByteArrayColumnReader(reader) => {
                    values!(reader, Bytes, |v: &ByteArray| v.len())
                }
                ColumnReader::BoolColumnReader(reader) => {
                    collect(
                        reader,
                        descriptor,
                        (rows, levels),
                        false,
                        &mut budget,
                        |_| 0,
                    )?;
                }
                ColumnReader::Int96ColumnReader(reader) => {
                    collect(
                        reader,
                        descriptor,
                        (rows, levels),
                        false,
                        &mut budget,
                        |_| 0,
                    )?;
                }
                ColumnReader::FloatColumnReader(reader) => {
                    collect(
                        reader,
                        descriptor,
                        (rows, levels),
                        false,
                        &mut budget,
                        |_| 0,
                    )?;
                }
                ColumnReader::FixedLenByteArrayColumnReader(reader) => {
                    collect(
                        reader,
                        descriptor,
                        (rows, levels),
                        false,
                        &mut budget,
                        |_| 0,
                    )?;
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
