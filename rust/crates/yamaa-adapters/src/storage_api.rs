//! Arrow/Parquet release-specific API boundary.
//!
//! The manifests pin Arrow/Parquet `=57.3.0`. Adapting another release
//! changes this module, rather than the readers, writer or core type policy.
//!
//! Keep schema conversion fallible: 57 has only `fb_to_schema`, which
//! unwraps the field list and panics on a verified but semantically
//! malformed schema, so `ipc_schema` refuses the missing field list and
//! contains any remaining conversion panic.
//!
//! The row-group reader construction also lives here: on 57 the constructor
//! takes the group's `Option<&[OffsetIndexMetaData]>` sliced from
//! `ParquetMetaData::offset_index` (later releases take a
//! `RowGroupPageIndex` instead). Callers must not construct a
//! `SerializedRowGroupReader` directly.
//!
//! Logical timestamp and integer types are struct variants on 57 (later
//! releases wrap them in tuple variants with constructors), so their
//! construction and inspection also live here, as does reading a column
//! descriptor's logical type.
//!
//! Importing a C stream also lives here (`StreamReader`): 57's
//! `ArrowArrayStreamReader` rebuilds each batch from its columns alone, so it
//! refuses a zero-column batch that has rows, and it unwraps the producer's
//! last-error message, which the C stream interface lets be NULL. 57's import
//! also keeps buffers where the producer placed them, while the interface only
//! recommends alignment, so the reader copies any under-aligned buffer before
//! typed arrays read it (later releases align on import).
//!
//! 57's thrift decoders unwrap the value of a known boolean field that
//! carries another wire type (later releases return an error), and its
//! metadata decoder asserts that INT96 statistics hold exactly 12 bytes, so a
//! malformed footer or page header can panic inside the Parquet crate. Each
//! native Parquet or Arrow call on admitted input runs through
//! `contain_decode`, which also keeps the panic it contains off stderr. The
//! Arrow IPC snapshot decoder (`ipc_table`) converts its schema through the
//! same contained `ipc_schema`.
//!
//! 57's footer decoder numbers row groups with an `i16` ordinal and refuses a
//! footer that lists more than `FOOTER_ROW_GROUPS` (later releases use `i32`),
//! so admission presents a longer row-group list in slices of at most that
//! many (`parquet_source::row_groups`).
//!
//! 57 reads a page type and an encoding only from their known values, which
//! bound a column chunk's encoding statistics (`ENCODING_STATS`).
#[cfg(feature = "parquet")]
use parquet::{
    basic::{Encoding, LogicalType, PageType, TimeUnit},
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
    let malformed = || {
        arrow_schema::ArrowError::ParseError("IPC schema is not a valid Arrow schema".to_string())
    };
    if schema.fields().is_none() {
        return Err(malformed());
    }
    contain_decode(|| arrow_ipc::convert::fb_to_schema(schema)).ok_or_else(malformed)
}

pub(crate) use stream::StreamReader;

/// Reviewed C stream consumer: the producer contract is the one
/// `arrow_bridge` already documents for foreign streams.
#[allow(unsafe_code)]
mod stream {
    use arrow_array::{
        ffi::{from_ffi_and_data_type, FFI_ArrowArray, FFI_ArrowSchema},
        ffi_stream::FFI_ArrowArrayStream,
        RecordBatch, RecordBatchOptions, StructArray,
    };
    use arrow_schema::{ArrowError, DataType, Schema, SchemaRef};
    use std::{
        ffi::{c_int, CStr},
        sync::Arc,
    };

    /// Owns a live C stream; dropping it releases the stream.
    pub(crate) struct StreamReader {
        stream: FFI_ArrowArrayStream,
        schema: SchemaRef,
    }

    impl StreamReader {
        pub(crate) fn try_new(mut stream: FFI_ArrowArrayStream) -> Result<Self, ArrowError> {
            let (Some(_), Some(get_schema), Some(_)) =
                (stream.release, stream.get_schema, stream.get_next)
            else {
                return Err(ArrowError::CDataInterface(
                    "input stream is released or lacks a callback".to_string(),
                ));
            };
            let mut schema = FFI_ArrowSchema::empty();
            // SAFETY: the stream is live (release is set) and its producer
            // writes a schema into the empty header on success.
            let code = unsafe { get_schema(&mut stream, &mut schema) };
            if code != 0 {
                return Err(producer_error(&mut stream, "schema", code));
            }
            let schema = Arc::new(Schema::try_from(&schema)?);
            Ok(Self { stream, schema })
        }

        pub(crate) fn schema(&self) -> SchemaRef {
            self.schema.clone()
        }
    }

    impl Iterator for StreamReader {
        type Item = Result<RecordBatch, ArrowError>;

        fn next(&mut self) -> Option<Self::Item> {
            let Some(get_next) = self.stream.get_next else {
                return Some(Err(ArrowError::CDataInterface(
                    "input stream lost its get_next callback".to_string(),
                )));
            };
            let mut array = FFI_ArrowArray::empty();
            // SAFETY: as in `try_new`; the producer writes the next array, or
            // a released one at the end of the stream.
            let code = unsafe { get_next(&mut self.stream, &mut array) };
            if code != 0 {
                return Some(Err(producer_error(&mut self.stream, "next batch", code)));
            }
            if array.is_released() {
                return None;
            }
            let kind = DataType::Struct(self.schema.fields().clone());
            // SAFETY: the producer's array has the struct type of the schema
            // it reported for this stream. The import neither validates nor
            // aligns its buffers; `align_buffers` below copies any
            // under-aligned one before a typed array reads it.
            let data = unsafe { from_ffi_and_data_type(array, kind) };
            Some(data.and_then(|mut data| {
                data.align_buffers();
                let rows = data.len();
                RecordBatch::try_new_with_options(
                    self.schema.clone(),
                    StructArray::from(data).into_parts().1,
                    &RecordBatchOptions::new().with_row_count(Some(rows)),
                )
            }))
        }
    }

    fn producer_error(
        stream: &mut FFI_ArrowArrayStream,
        operation: &str,
        code: c_int,
    ) -> ArrowError {
        let mut message = format!("Cannot get {operation} from input stream. Error code: {code}");
        if let Some(get_last_error) = stream.get_last_error {
            // SAFETY: the last call on this live stream failed, which is when
            // the interface allows asking for its message.
            let text = unsafe { get_last_error(stream) };
            if !text.is_null() {
                // SAFETY: a non-NULL message is NUL-terminated and valid until
                // the next call on the stream.
                let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
                message = format!("{message}. Producer error: {text}");
            }
        }
        ArrowError::CDataInterface(message)
    }
}

/// Run one native Arrow or Parquet call on admitted input with its panic
/// contained and silent; `None` reports that the call panicked on a malformed
/// footer, page header, page or embedded schema. Wrap only the call into the
/// Arrow or Parquet crate: a panic in yamaa's own code around it is a bug,
/// which must print and reach the caller's own boundary rather than read as
/// malformed input.
///
/// The process's panic hook prints every panic, caught or not. So the first
/// call wraps the hook installed at that moment, once for the process
/// (`quiet::install`). The wrapper prints nothing for the panic of a call
/// inside `contain_decode`, and passes every other panic to the hook it wraps.
/// That includes a further panic on the same thread while the contained one
/// unwinds, such as a destructor that panics, after which the process aborts:
/// so the abort keeps its message. A hook installed later replaces the
/// wrapper. A `panic = "abort"` build cannot contain a panic, so it keeps the
/// hook it has.
pub(crate) fn contain_decode<T>(decode: impl FnOnce() -> T) -> Option<T> {
    quiet::install();
    let _inside = quiet::Inside::enter();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(decode)).ok()
}

/// The most row groups 57's decoder reads from one footer: their ordinals
/// `0..=i16::MAX`.
#[cfg(feature = "parquet")]
pub(crate) const FOOTER_ROW_GROUPS: usize = 1 << 15;

/// The most encoding statistics a column chunk lists in a file that 57's
/// decoder reads. Each entry counts the pages of one page type and encoding,
/// so a chunk lists each pair at most once, and 57 reads a page type and an
/// encoding only from their known values.
#[cfg(feature = "parquet")]
pub(crate) const ENCODING_STATS: usize = PageType::VARIANTS.len() * Encoding::VARIANTS.len();

/// The panic hook wrapper that keeps contained panics silent.
mod quiet {
    use std::{cell::Cell, panic, sync::Once};

    thread_local! {
        /// How many `contain_decode` calls the current thread is inside.
        static DEPTH: Cell<usize> = const { Cell::new(0) };
        /// Whether the panic of the innermost `contain_decode` call on the
        /// current thread is unwinding.
        static UNWINDING: Cell<bool> = const { Cell::new(false) };
    }

    /// Whether the current thread is inside `contain_decode`.
    pub(super) fn contained() -> bool {
        DEPTH.try_with(|depth| depth.get() > 0).unwrap_or(false)
    }

    /// Whether the wrapper keeps the panic being reported silent: the first
    /// panic of the innermost `contain_decode` call on the current thread. A
    /// panic while that one unwinds is reported, since the runtime then
    /// aborts the process.
    pub(super) fn silent() -> bool {
        contained()
            && !UNWINDING
                .try_with(|unwinding| unwinding.replace(true))
                .unwrap_or(true)
    }

    /// Wrap the installed panic hook, once for the process.
    ///
    /// Stable Rust has no atomic hook update (`std::panic::update_hook` is
    /// unstable), so this takes the installed hook and then sets the wrapper,
    /// in two steps. A panic on another thread between the two is reported
    /// by Rust's default hook, and a hook that another thread sets between
    /// them is replaced.
    pub(super) fn install() {
        static WRAPPED: Once = Once::new();
        // `take_hook` panics on a thread that is already panicking.
        if cfg!(panic = "unwind") && !std::thread::panicking() {
            WRAPPED.call_once(|| {
                let previous = panic::take_hook();
                panic::set_hook(Box::new(move |info| {
                    if !silent() {
                        previous(info);
                    }
                }));
            });
        }
    }

    /// Marks the current thread inside `contain_decode` until it is dropped.
    pub(super) struct Inside {
        /// Whether an enclosing call's panic was unwinding when this call
        /// began, as when a destructor run by that unwind calls in again.
        enclosing: bool,
    }

    impl Inside {
        pub(super) fn enter() -> Self {
            DEPTH.with(|depth| depth.set(depth.get() + 1));
            Self {
                enclosing: UNWINDING.with(|unwinding| unwinding.replace(false)),
            }
        }
    }

    impl Drop for Inside {
        fn drop(&mut self) {
            UNWINDING.with(|unwinding| unwinding.set(self.enclosing));
            DEPTH.with(|depth| depth.set(depth.get() - 1));
        }
    }
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
        metadata
            .offset_index()
            .and_then(|groups| groups.get(row_group))
            .map(Vec::as_slice),
        properties,
    )
}

#[cfg(feature = "parquet")]
pub(crate) fn timestamp_micros(utc: bool) -> LogicalType {
    LogicalType::Timestamp {
        is_adjusted_to_u_t_c: utc,
        unit: TimeUnit::MICROS,
    }
}

#[cfg(all(test, feature = "parquet"))]
pub(crate) fn timestamp_millis(utc: bool) -> LogicalType {
    LogicalType::Timestamp {
        is_adjusted_to_u_t_c: utc,
        unit: TimeUnit::MILLIS,
    }
}

#[cfg(all(test, feature = "parquet"))]
pub(crate) fn integer(bits: i8, signed: bool) -> LogicalType {
    LogicalType::Integer {
        bit_width: bits,
        is_signed: signed,
    }
}

/// Writer properties capping each row group at `rows` rows.
#[cfg(all(test, feature = "parquet"))]
pub(crate) fn writer_properties_max_rows(
    rows: usize,
) -> parquet::file::properties::WriterProperties {
    parquet::file::properties::WriterProperties::builder()
        .set_max_row_group_size(rows)
        .build()
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
        Some(LogicalType::Timestamp {
            unit,
            is_adjusted_to_u_t_c,
        }) => Logical::Timestamp {
            unit: if *unit == TimeUnit::MICROS {
                Unit::Microsecond
            } else if *unit == TimeUnit::MILLIS {
                Unit::Millisecond
            } else {
                Unit::Nanosecond
            },
            utc: *is_adjusted_to_u_t_c,
        },
        Some(LogicalType::Integer {
            bit_width,
            is_signed,
        }) => u8::try_from(*bit_width).map_or(Logical::Other, |bits| Logical::Integer {
            bits,
            signed: *is_signed,
        }),
        _ => Logical::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        contain_decode,
        quiet::{contained, silent},
    };

    #[test]
    fn only_a_thread_inside_contain_decode_is_silent() {
        assert!(!contained());
        // A nested call that panics leaves the outer call silent; another
        // thread is outside. Nothing asserts inside, where a failure would
        // be contained too.
        let inside = contain_decode(|| {
            let nested = contain_decode::<()>(|| panic!("a nested contained panic"));
            let other = std::thread::spawn(contained).join().unwrap();
            (nested.is_none(), contained(), other, silent())
        });
        assert_eq!(inside, Some((true, true, false, true)));
        assert!(!contained());
    }

    /// The wrapper asks `silent` once per panic it reports. Each call here
    /// stands for one report.
    #[test]
    fn only_the_first_panic_of_a_contained_call_is_silent() {
        assert!(!silent());
        let inside = contain_decode(|| {
            // The call's own panic, then another while that one unwinds.
            let first = (silent(), silent());
            // A call that a destructor of that unwind makes starts afresh,
            // and the enclosing call's panic is still unwinding after it.
            let nested = contain_decode(|| (silent(), silent()));
            (first, nested, silent())
        });
        assert_eq!(inside, Some(((true, false), Some((true, false)), false)));
        // Each call starts afresh, and a thread outside is never silent.
        assert_eq!(contain_decode(silent), Some(true));
        assert_eq!(contain_decode(silent), Some(true));
        assert!(!silent());
    }
}
