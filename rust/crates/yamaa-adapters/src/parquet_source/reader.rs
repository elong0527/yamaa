//! Complete held-container read, followed by closed-profile admission.
//! No filesystem reads, semantic fallback, publication or host date conversion.
use super::{compact, compression, expansion, framing, metadata, physical, profile, schema};
use arrow_array::{
    ArrayRef, Date32Array, Float64Array, Int64Array, LargeStringArray, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow_schema::DataType;
use parquet::{
    arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions},
    file::metadata::ParquetMetaDataReader,
};
use std::sync::Arc;
use yamaa_core::{table::TableSchema, value::ColumnType};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
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

pub(super) fn read(bytes: &[u8], limits: Limits) -> Result<Decoded, Error> {
    let frame = framing::admit(bytes, limits.framing)?;
    let metadata = Arc::new(
        ParquetMetaDataReader::decode_metadata(frame.metadata).map_err(|_| Error::Malformed)?,
    );
    let hint = metadata::admit(
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
    if actual_values > limits.array_elements
        || row_count
            .checked_mul(columns.len())
            .is_none_or(|n| n > limits.array_elements)
    {
        return Err(Error::Limit);
    }
    let arrow_schema = schema::normalize(arrow.schema(), hint.as_ref());
    // Determine retention only. Profile errors remain deferred until every
    // physical chunk has been decoded, including fields outside the profile.
    let keep: Vec<_> = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            arrow_schema.fields().len() == columns.len()
                && arrow_schema.fields().get(index).is_some_and(|field| {
                    column.max_rep_level() == 0
                        && column.path().string() == *field.name()
                        && profile::column_type(column, field.data_type()).is_some()
                })
        })
        .collect();
    let raw = physical::read(bytes, &keep, limits.retained_bytes, &arrow_schema)?;
    let group_rows = metadata
        .row_groups()
        .iter()
        .try_fold(0usize, |sum, group| {
            sum.checked_add(usize::try_from(group.num_rows()).map_err(|_| Error::Malformed)?)
                .ok_or(Error::Limit)
        })?;
    if group_rows != row_count {
        return Err(Error::Malformed);
    }
    let schema = profile::columns(&arrow_schema, metadata.file_metadata().schema_descr())
        .map_err(Error::Profile)?;
    for (field, column) in schema.columns().iter().zip(&raw) {
        match (field.kind, column) {
            (ColumnType::Date, physical::Column::Int32(values)) => {
                for (row, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        profile::temporal(field.kind, i64::from(*value), &field.name, row + 1)
                            .map_err(Error::Profile)?;
                    }
                }
            }
            (ColumnType::DateTime, physical::Column::Int64(values)) => {
                for (row, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        profile::temporal(field.kind, *value, &field.name, row + 1)
                            .map_err(Error::Profile)?;
                    }
                }
            }
            _ => {}
        }
    }
    // The reference validates text when constructing its typed table, after
    // field and temporal findings. Never construct unchecked Arrow strings.
    let mut arrays: Vec<ArrayRef> = Vec::new();
    for (index, (field, column)) in schema.columns().iter().zip(raw).enumerate() {
        let array: ArrayRef = match (field.kind, column) {
            (ColumnType::Str, physical::Column::Bytes(values)) => {
                let strings = values
                    .iter()
                    .map(|v| {
                        v.as_ref()
                            .map(|b| std::str::from_utf8(b.data()).map_err(|_| Error::Malformed))
                            .transpose()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if matches!(arrow_schema.field(index).data_type(), DataType::LargeUtf8) {
                    Arc::new(LargeStringArray::from(strings))
                } else {
                    Arc::new(StringArray::from(strings))
                }
            }
            (ColumnType::Int, physical::Column::Int64(values)) => {
                Arc::new(Int64Array::from(values))
            }
            (ColumnType::Float, physical::Column::Double(values)) => Arc::new(
                Float64Array::from_iter(values.into_iter().map(|v| v.filter(|v| v.is_finite()))),
            ),
            (ColumnType::Date, physical::Column::Int32(values)) => {
                Arc::new(Date32Array::from(values))
            }
            (ColumnType::DateTime, physical::Column::Int64(values)) => {
                Arc::new(TimestampMicrosecondArray::from(values))
            }
            _ => return Err(Error::Malformed),
        };
        if array.len() != row_count {
            return Err(Error::Malformed);
        }
        arrays.push(array);
    }
    if limits.batches == 0 {
        return Err(Error::Limit);
    }
    // Normalization can make a required stored float missing. The language's
    // typed table admits missing values for every column, regardless of storage.
    let output_schema = Arc::new(arrow_schema::Schema::new(
        arrow_schema
            .fields()
            .iter()
            .map(|field| arrow_schema::Field::new(field.name(), field.data_type().clone(), true))
            .collect::<Vec<_>>(),
    ));
    let batch = RecordBatch::try_new(output_schema, arrays).map_err(|_| Error::Malformed)?;
    if batch.get_array_memory_size() > limits.retained_bytes {
        return Err(Error::Limit);
    }
    Ok(Decoded {
        schema,
        batches: vec![batch],
    })
}
