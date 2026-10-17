//! Arrow/Parquet release-specific API boundary.
//!
//! The shipped lock remains on 60. Adapting another release (for example 57)
//! changes this module, rather than the readers, writer or core type policy.
//! In particular, keep schema conversion fallible: older `fb_to_schema` can
//! panic on a verified but semantically malformed schema.
//!
//! The row-group reader construction also lives here: its page-index
//! argument changed shape between releases. On 60 it is a
//! `RowGroupPageIndex` from `ParquetMetaData::page_index_for_row_group`;
//! on 57 the constructor instead took an `Option<&[OffsetIndexMetaData]>`
//! sliced from `ParquetMetaData::offset_index`. Callers must not construct
//! a `SerializedRowGroupReader` directly.
//!
//! Reading a column descriptor's logical type also lives here: 60 exposes
//! it by reference as `logical_type_ref`, while 57 only had the by-value
//! `logical_type()`. Callers must not call either accessor directly.
#[cfg(feature = "parquet")]
use parquet::{
    basic::{LogicalType, TimeUnit},
    errors::ParquetError,
    file::{
        metadata::{ParquetMetaData, RowGroupMetaData},
        properties::ReaderProperties,
        serialized_reader::SerializedRowGroupReader,
    },
    schema::types::ColumnDescriptor,
};
#[cfg(feature = "parquet")]
use std::sync::Arc;
#[cfg(feature = "parquet")]
use yamaa_core::parquet_source::{Logical, Unit};

pub(crate) fn ipc_schema(
    schema: arrow_ipc::Schema<'_>,
) -> Result<arrow_schema::Schema, arrow_schema::ArrowError> {
    arrow_ipc::convert::try_fb_to_schema(schema)
}

#[cfg(feature = "parquet")]
pub(crate) fn row_group_reader<'a>(
    source: Arc<bytes::Bytes>,
    group: &'a RowGroupMetaData,
    metadata: &'a ParquetMetaData,
    row_group: usize,
    properties: Arc<ReaderProperties>,
) -> Result<SerializedRowGroupReader<'a, bytes::Bytes>, ParquetError> {
    SerializedRowGroupReader::new(
        source,
        group,
        metadata.page_index_for_row_group(row_group),
        properties,
    )
}

#[cfg(feature = "parquet")]
pub(crate) fn timestamp_micros(utc: bool) -> LogicalType {
    LogicalType::timestamp(utc, TimeUnit::MICROS)
}

#[cfg(all(test, feature = "parquet"))]
pub(crate) fn timestamp_millis(utc: bool) -> LogicalType {
    LogicalType::timestamp(utc, TimeUnit::MILLIS)
}

#[cfg(all(test, feature = "parquet"))]
pub(crate) fn integer(bits: i8, signed: bool) -> LogicalType {
    LogicalType::integer(bits, signed)
}

#[cfg(feature = "parquet")]
pub(crate) fn column_logical(descriptor: &ColumnDescriptor) -> Logical {
    logical_type(descriptor.logical_type_ref())
}

#[cfg(feature = "parquet")]
pub(crate) fn logical_type(logical: Option<&LogicalType>) -> Logical {
    match logical {
        None => Logical::None,
        Some(LogicalType::String) => Logical::String,
        Some(LogicalType::Date) => Logical::Date,
        Some(LogicalType::Timestamp(timestamp)) => Logical::Timestamp {
            unit: if timestamp.unit == TimeUnit::MICROS {
                Unit::Microsecond
            } else if timestamp.unit == TimeUnit::MILLIS {
                Unit::Millisecond
            } else {
                Unit::Nanosecond
            },
            utc: timestamp.is_adjusted_to_u_t_c,
        },
        _ => Logical::Other,
    }
}
