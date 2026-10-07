//! Complete held-container read, followed by closed-profile admission.
//! No filesystem reads, semantic fallback, publication or host date conversion.
//! WIP: the Arrow read below validates UTF-8 too early for multi-fault inputs.
//! Replace that stage with opaque physical values before exposing this reader.
use super::{compact, compression, expansion, framing, metadata, profile};
use arrow_array::{Array, Date32Array, RecordBatch, TimestampMicrosecondArray};
use arrow_schema::DataType;
use parquet::{
    arrow::arrow_reader::{
        ArrowReaderMetadata, ArrowReaderOptions, ParquetRecordBatchReaderBuilder,
    },
    file::metadata::ParquetMetaDataReader,
};
use std::sync::Arc;
use yamaa_core::{table::TableSchema, value::ColumnType};

#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    pub framing: framing::Limits,
    pub compression: compression::Limits,
    pub metadata: metadata::Limits,
    pub expanded_bytes: usize,
    pub retained_bytes: usize,
    pub array_elements: usize,
    pub batches: usize,
}
#[derive(Debug, PartialEq)]
pub(super) enum Error {
    Malformed,
    Limit,
    Unavailable(i64),
    Profile(profile::Error),
}
impl From<compact::Error> for Error {
    fn from(error: compact::Error) -> Self {
        match error {
            compact::Error::Malformed => Self::Malformed,
            compact::Error::Limit => Self::Limit,
        }
    }
}
impl From<compression::Error> for Error {
    fn from(error: compression::Error) -> Self {
        match error {
            compression::Error::Malformed => Self::Malformed,
            compression::Error::Limit => Self::Limit,
            compression::Error::Unavailable(codec) => Self::Unavailable(codec),
        }
    }
}
#[derive(Debug)]
pub(super) struct Decoded {
    pub schema: TableSchema,
    pub batches: Vec<RecordBatch>,
}

// An embedded Arrow hint can request fixed-size padding even where the physical
// file stores a single null level. Bound its conservative shape before decoding.
fn multiplier(kind: &DataType, limit: usize) -> Result<usize, Error> {
    let value = match kind {
        DataType::FixedSizeList(field, length) => {
            let count = usize::try_from(*length).map_err(|_| Error::Malformed)?;
            multiplier(field.data_type(), limit)?
                .checked_mul(count.max(1))
                .ok_or(Error::Limit)?
        }
        DataType::List(field)
        | DataType::LargeList(field)
        | DataType::ListView(field)
        | DataType::LargeListView(field)
        | DataType::Map(field, _) => multiplier(field.data_type(), limit)?,
        DataType::Struct(fields) => fields
            .iter()
            .try_fold(0usize, |total, field| {
                total
                    .checked_add(multiplier(field.data_type(), limit)?)
                    .ok_or(Error::Limit)
            })?
            .max(1),
        DataType::Union(fields, _) => fields
            .iter()
            .try_fold(0usize, |total, (_, field)| {
                total
                    .checked_add(multiplier(field.data_type(), limit)?)
                    .ok_or(Error::Limit)
            })?
            .max(1),
        DataType::Dictionary(_, value) => multiplier(value, limit)?,
        DataType::RunEndEncoded(_, value) => multiplier(value.data_type(), limit)?,
        _ => 1,
    };
    if value > limit {
        Err(Error::Limit)
    } else {
        Ok(value)
    }
}

pub(super) fn read(bytes: &[u8], limits: Limits) -> Result<Decoded, Error> {
    let frame = framing::admit(bytes, limits.framing)?;
    let metadata = Arc::new(
        ParquetMetaDataReader::decode_metadata(frame.metadata).map_err(|_| Error::Malformed)?,
    );
    metadata::admit(
        metadata.file_metadata().key_value_metadata(),
        limits.metadata,
    )?;
    let columns = metadata.file_metadata().schema_descr().columns();
    let mut expansion = expansion::Admission::new(limits.expanded_bytes);
    let mut actual_values = 0usize;
    for page in &frame.pages {
        let column = columns.get(page.column).ok_or(Error::Malformed)?;
        let decoded = compression::decode(page, limits.compression)?;
        expansion.admit(page, &decoded, column)?;
        if let framing::Payload::DataV1 { values, .. } | framing::Payload::DataV2 { values, .. } =
            page.payload
        {
            actual_values = actual_values.checked_add(values).ok_or(Error::Limit)?;
        }
    }
    let row_count =
        usize::try_from(metadata.file_metadata().num_rows()).map_err(|_| Error::Malformed)?;
    let arrow = ArrowReaderMetadata::try_new(metadata.clone(), ArrowReaderOptions::new())
        .map_err(|_| Error::Malformed)?;
    // This is deliberately conservative admission, separate from data/profile
    // validity. It includes padding multipliers before any hinted array exists.
    let base = row_count.max(actual_values);
    for field in arrow.schema().fields() {
        let multiplier = multiplier(field.data_type(), limits.array_elements)?;
        if base
            .checked_mul(multiplier)
            .is_none_or(|v| v > limits.array_elements)
        {
            return Err(Error::Limit);
        }
    }
    let schema = arrow.schema().clone();
    let input = bytes::Bytes::copy_from_slice(bytes);
    let reader = ParquetRecordBatchReaderBuilder::new_with_metadata(input, arrow)
        .with_batch_size(256)
        .build()
        .map_err(|_| Error::Malformed)?;
    let mut batches = Vec::new();
    let mut retained = 0usize;
    let mut rows = 0usize;
    for batch in reader {
        let batch = batch.map_err(|_| Error::Malformed)?;
        rows = rows
            .checked_add(batch.num_rows())
            .filter(|v| *v <= limits.framing.rows)
            .ok_or(Error::Limit)?;
        retained = retained
            .checked_add(batch.get_array_memory_size())
            .filter(|v| *v <= limits.retained_bytes)
            .ok_or(Error::Limit)?;
        if batches.len() >= limits.batches {
            return Err(Error::Limit);
        }
        batches.push(batch);
    }
    if rows != row_count {
        return Err(Error::Malformed);
    }
    // All container data is read before any authored field/type finding.
    let schema = profile::columns(&schema, metadata.file_metadata().schema_descr())
        .map_err(Error::Profile)?;
    for (column, field) in schema.columns().iter().enumerate() {
        if !matches!(field.kind, ColumnType::Date | ColumnType::DateTime) {
            continue;
        }
        let mut start = 0;
        for batch in &batches {
            let array = batch.column(column);
            for row in 0..array.len() {
                if array.is_null(row) {
                    continue;
                }
                let value = match field.kind {
                    ColumnType::Date => i64::from(
                        array
                            .as_any()
                            .downcast_ref::<Date32Array>()
                            .ok_or(Error::Malformed)?
                            .value(row),
                    ),
                    ColumnType::DateTime => array
                        .as_any()
                        .downcast_ref::<TimestampMicrosecondArray>()
                        .ok_or(Error::Malformed)?
                        .value(row),
                    _ => unreachable!(),
                };
                profile::temporal(field.kind, value, &field.name, start + row + 1)
                    .map_err(Error::Profile)?;
            }
            start += batch.num_rows();
        }
    }
    Ok(Decoded { schema, batches })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hinted_fixed_size_padding_is_bounded_before_array_allocation() {
        use arrow_schema::Field;
        let child = Arc::new(Field::new("item", DataType::Utf8, true));
        let array = DataType::FixedSizeList(child.clone(), i32::MAX);
        assert_eq!(multiplier(&array, 1024), Err(Error::Limit));
        assert_eq!(
            multiplier(&DataType::FixedSizeList(child, -1), 1024),
            Err(Error::Malformed)
        );
        let array = DataType::FixedSizeList(
            Arc::new(Field::new(
                "item",
                DataType::FixedSizeList(Arc::new(Field::new("inner", DataType::Int64, true)), 32),
                true,
            )),
            32,
        );
        assert_eq!(multiplier(&array, 1024), Ok(1024));
        assert_eq!(multiplier(&array, 1023), Err(Error::Limit));
    }
}
