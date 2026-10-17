//! Arrow/Parquet release-specific API boundary.
//!
//! The manifests accept Arrow/Parquet `>=57.0.0` with no upper bound; the
//! canonical lock remains on 60.0.0. Adapting another release in that
//! range changes this module, rather than the readers, writer or core
//! type policy. The 57 API shape is selected by the `yamaa_parquet_57`
//! cfg, which build.rs emits when the workspace lock resolves parquet 57
//! or an embedder sets `YAMAA_PARQUET_MAJOR=57`; every other graph uses
//! the current API. In particular, keep schema conversion fallible:
//! 57's `fb_to_schema` can panic on a verified but semantically malformed
//! schema, so its path converts under `catch_unwind`.

#[cfg(not(yamaa_parquet_57))]
pub(crate) fn ipc_schema(
    schema: arrow_ipc::Schema<'_>,
) -> Result<arrow_schema::Schema, arrow_schema::ArrowError> {
    arrow_ipc::convert::try_fb_to_schema(schema)
}

#[cfg(yamaa_parquet_57)]
pub(crate) fn ipc_schema(
    schema: arrow_ipc::Schema<'_>,
) -> Result<arrow_schema::Schema, arrow_schema::ArrowError> {
    // `fb_to_schema` unwraps the field list and can panic on a malformed
    // schema; pre-check that case and catch any remaining panic so this
    // boundary stays fallible, as the current `try_fb_to_schema` is.
    let malformed = || {
        arrow_schema::ArrowError::ParseError("IPC schema is not a valid Arrow schema".to_string())
    };
    if schema.fields().is_none() {
        return Err(malformed());
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        arrow_ipc::convert::fb_to_schema(schema)
    }))
    .map_err(|_| malformed())
}

#[cfg(feature = "parquet")]
use parquet::basic::{LogicalType, TimeUnit};
#[cfg(feature = "parquet")]
use yamaa_core::parquet_source::{Logical, Unit};

#[cfg(all(feature = "parquet", not(yamaa_parquet_57)))]
pub(crate) fn timestamp_micros(utc: bool) -> LogicalType {
    LogicalType::timestamp(utc, TimeUnit::MICROS)
}

#[cfg(all(feature = "parquet", yamaa_parquet_57))]
pub(crate) fn timestamp_micros(utc: bool) -> LogicalType {
    LogicalType::Timestamp {
        is_adjusted_to_u_t_c: utc,
        unit: TimeUnit::MICROS,
    }
}

#[cfg(all(test, feature = "parquet", not(yamaa_parquet_57)))]
pub(crate) fn timestamp_millis(utc: bool) -> LogicalType {
    LogicalType::timestamp(utc, TimeUnit::MILLIS)
}

#[cfg(all(test, feature = "parquet", yamaa_parquet_57))]
pub(crate) fn timestamp_millis(utc: bool) -> LogicalType {
    LogicalType::Timestamp {
        is_adjusted_to_u_t_c: utc,
        unit: TimeUnit::MILLIS,
    }
}

#[cfg(all(test, feature = "parquet", not(yamaa_parquet_57)))]
pub(crate) fn integer(bits: i8, signed: bool) -> LogicalType {
    LogicalType::integer(bits, signed)
}

#[cfg(all(test, feature = "parquet", yamaa_parquet_57))]
pub(crate) fn integer(bits: i8, signed: bool) -> LogicalType {
    LogicalType::Integer {
        bit_width: bits,
        is_signed: signed,
    }
}

/// The `(unit, utc)` pair of a timestamp logical type, in either shape.
#[cfg(feature = "parquet")]
fn timestamp_parts(logical: Option<&LogicalType>) -> Option<(TimeUnit, bool)> {
    match logical {
        #[cfg(not(yamaa_parquet_57))]
        Some(LogicalType::Timestamp(timestamp)) => {
            Some((timestamp.unit, timestamp.is_adjusted_to_u_t_c))
        }
        #[cfg(yamaa_parquet_57)]
        Some(LogicalType::Timestamp {
            unit,
            is_adjusted_to_u_t_c,
        }) => Some((*unit, *is_adjusted_to_u_t_c)),
        _ => None,
    }
}

#[cfg(feature = "parquet")]
pub(crate) fn logical_type(logical: Option<&LogicalType>) -> Logical {
    if let Some((unit, utc)) = timestamp_parts(logical) {
        return Logical::Timestamp {
            unit: if unit == TimeUnit::MICROS {
                Unit::Microsecond
            } else if unit == TimeUnit::MILLIS {
                Unit::Millisecond
            } else {
                Unit::Nanosecond
            },
            utc,
        };
    }
    match logical {
        None => Logical::None,
        Some(LogicalType::String) => Logical::String,
        Some(LogicalType::Date) => Logical::Date,
        _ => Logical::Other,
    }
}

/// Writer properties capping a row group at `rows` rows: the current
/// release sets an optional row count, 57 sets its row-group size, which
/// its Arrow writer likewise counts in rows.
#[cfg(all(test, feature = "parquet", not(yamaa_parquet_57)))]
pub(crate) fn writer_properties_max_rows(
    rows: usize,
) -> parquet::file::properties::WriterProperties {
    parquet::file::properties::WriterProperties::builder()
        .set_max_row_group_row_count(Some(rows))
        .build()
}

#[cfg(all(test, feature = "parquet", yamaa_parquet_57))]
pub(crate) fn writer_properties_max_rows(
    rows: usize,
) -> parquet::file::properties::WriterProperties {
    parquet::file::properties::WriterProperties::builder()
        .set_max_row_group_size(rows)
        .build()
}

/// A column's logical type as an owned value: the current release lends
/// it by reference, 57 returns it by value.
#[cfg(all(feature = "parquet", not(yamaa_parquet_57)))]
pub(crate) fn column_logical_type(
    stored: &parquet::schema::types::ColumnDescriptor,
) -> Option<LogicalType> {
    stored.logical_type_ref().cloned()
}

#[cfg(all(feature = "parquet", yamaa_parquet_57))]
pub(crate) fn column_logical_type(
    stored: &parquet::schema::types::ColumnDescriptor,
) -> Option<LogicalType> {
    stored.logical_type()
}

/// Construct a row-group reader: the current release takes the file's
/// page index for the group, 57 takes its offset-index slice directly.
#[cfg(all(feature = "parquet", not(yamaa_parquet_57)))]
pub(crate) fn row_group_reader<'a>(
    source: std::sync::Arc<bytes::Bytes>,
    metadata: &'a parquet::file::metadata::ParquetMetaData,
    index: usize,
    group: &'a parquet::file::metadata::RowGroupMetaData,
    properties: std::sync::Arc<parquet::file::properties::ReaderProperties>,
) -> Result<
    parquet::file::serialized_reader::SerializedRowGroupReader<'a, bytes::Bytes>,
    parquet::errors::ParquetError,
> {
    parquet::file::serialized_reader::SerializedRowGroupReader::new(
        source,
        group,
        metadata.page_index_for_row_group(index),
        properties,
    )
}

#[cfg(all(feature = "parquet", yamaa_parquet_57))]
pub(crate) fn row_group_reader<'a>(
    source: std::sync::Arc<bytes::Bytes>,
    metadata: &'a parquet::file::metadata::ParquetMetaData,
    index: usize,
    group: &'a parquet::file::metadata::RowGroupMetaData,
    properties: std::sync::Arc<parquet::file::properties::ReaderProperties>,
) -> Result<
    parquet::file::serialized_reader::SerializedRowGroupReader<'a, bytes::Bytes>,
    parquet::errors::ParquetError,
> {
    parquet::file::serialized_reader::SerializedRowGroupReader::new(
        source,
        group,
        metadata
            .offset_index()
            .and_then(|indexes| indexes.get(index))
            .map(|group_index| group_index.as_slice()),
        properties,
    )
}
