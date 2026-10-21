//! Arrow/Parquet release-specific API boundary.
//!
//! The manifests pin Arrow/Parquet `=55.2.0`, the newest release that builds
//! with the workspace's minimum Rust. Adapting another release changes this
//! module, rather than the readers, writer or core type policy.
//!
//! Keep schema conversion fallible: 55 has only `fb_to_schema`, which
//! unwraps the field list and panics on a verified but semantically
//! malformed schema, so `ipc_schema` refuses the missing field list and
//! contains any remaining conversion panic.
//!
//! Arrow 55 has no 32- or 64-bit decimal, and its IPC conversion reads no
//! list view, all of which later Arrow writers name in a Parquet file's
//! embedded schema. So `ipc_schema` holds each as its nearest Arrow 55 type
//! (a 128-bit decimal, a list or a large list) and returns where it did
//! (`LaterType`), and `arrow_schema` gives the native schema conversion the
//! held schema in place of the file's. An embedded schema chooses a layout,
//! never a field type (PARQUET-7), and the stored type of a refused field
//! still names the type the file names (`parquet_source::type_name`).
//!
//! The row-group reader construction also lives here: on 55 the constructor
//! takes the group's `Option<&[OffsetIndexMetaData]>` sliced from
//! `ParquetMetaData::offset_index` (later releases take a
//! `RowGroupPageIndex` instead). Callers must not construct a
//! `SerializedRowGroupReader` directly.
//!
//! Logical timestamp and integer types are struct variants on 55, with the
//! time unit a union of empty structs (later releases wrap them in tuple
//! variants with constructors), so their construction and inspection also
//! live here, as does reading a column descriptor's logical type, which 55
//! returns as an owned copy.
//!
//! Importing a C stream also lives here (`StreamReader`): 55's
//! `ArrowArrayStreamReader` unwraps the producer's callbacks and its
//! last-error message, which the C stream interface lets be NULL, and builds
//! each batch with `RecordBatch::from`, which panics when the producer's
//! struct array has a null. 55's import also keeps buffers where the producer
//! placed them, while the interface only recommends alignment, so the reader
//! copies any under-aligned buffer before typed arrays read it (later
//! releases align on import).
//!
//! 55 decodes footers and page headers with the readers the Thrift compiler
//! generated (`parquet::format`). The compact protocol it reads a footer with
//! indexes eight bytes for a double without checking that they are there, and
//! its statistics conversion asserts that INT96 statistics hold exactly 12
//! bytes, for the footer's column chunks and for each data page header, so a
//! malformed footer or page header can panic inside the Parquet crate. Each native Parquet or
//! Arrow call on admitted input runs through `contain_decode`, which also
//! keeps the panic it contains off stderr.
//!
//! 55 converts a page type and an encoding only from their known values
//! (`parquet::format`), which bound a column chunk's encoding statistics
//! (`ENCODING_STATS`).
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

/// An Arrow type that an embedded schema names and Arrow 55 has no
/// counterpart for, held as its nearest Arrow 55 type.
#[cfg(feature = "parquet")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LaterType {
    /// A 32- or 64-bit decimal, held as a 128-bit one of the same precision
    /// and scale.
    Decimal { bits: i32, precision: u8, scale: i8 },
    /// A list view, held as a list.
    ListView,
    /// A large list view, held as a large list.
    LargeListView,
}

/// Each later type an embedded schema names, by the path of the field that
/// holds it: the index of the schema's field, then of each child below it,
/// in the order the IPC schema lists them.
#[cfg(feature = "parquet")]
pub(crate) type LaterTypes = Vec<(Vec<usize>, LaterType)>;

/// Convert a verified IPC schema to Arrow 55's types, holding each later
/// type as its nearest one, and say where it held one.
#[cfg(feature = "parquet")]
pub(crate) fn ipc_schema(
    schema: arrow_ipc::Schema<'_>,
) -> Result<(arrow_schema::Schema, LaterTypes), arrow_schema::ArrowError> {
    let malformed = || {
        arrow_schema::ArrowError::ParseError("IPC schema is not a valid Arrow schema".to_string())
    };
    let Some(fields) = schema.fields() else {
        return Err(malformed());
    };
    // 55 refuses a big-endian decimal, as later releases do.
    let little = schema.endianness() == arrow_ipc::Endianness::Little;
    if little && fields.iter().any(names_later) {
        let mut later = Vec::new();
        let held = fields
            .iter()
            .enumerate()
            .map(|(index, field)| hold(field, &mut vec![index], &mut later))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(malformed)?;
        return Ok((arrow_schema::Schema::new(held), later));
    }
    contain_decode(|| arrow_ipc::convert::fb_to_schema(schema))
        .ok_or_else(malformed)
        .map(|schema| (schema, Vec::new()))
}

/// Whether `field`'s type, or a type below it, is a later one.
#[cfg(feature = "parquet")]
fn names_later(field: arrow_ipc::Field<'_>) -> bool {
    use arrow_ipc::Type;
    let later = match field.type_type() {
        Type::Decimal => field
            .type_as_decimal()
            .is_some_and(|decimal| matches!(decimal.bitWidth(), 32 | 64)),
        Type::ListView | Type::LargeListView => true,
        _ => false,
    };
    later
        || field
            .children()
            .is_some_and(|children| children.iter().any(names_later))
}

/// Convert `field`, at `path`, holding each later type below it as its
/// nearest Arrow 55 type and noting it in `later`. Only a decimal, and the
/// lists, structs and maps above one or above a list view, are held: `None`
/// refuses any other type with a later type below it, as Arrow 55 does, and
/// a malformed field.
#[cfg(feature = "parquet")]
fn hold(
    field: arrow_ipc::Field<'_>,
    path: &mut Vec<usize>,
    later: &mut LaterTypes,
) -> Option<arrow_schema::Field> {
    use arrow_ipc::Type;
    use arrow_schema::DataType;
    if !names_later(field) {
        return contain_decode(|| arrow_schema::Field::from(field));
    }
    if field.dictionary().is_some() {
        return None;
    }
    let mut children = Vec::new();
    for (index, child) in field.children().into_iter().flatten().enumerate() {
        path.push(index);
        children.push(hold(child, path, later));
        path.pop();
    }
    let children = children.into_iter().collect::<Option<Vec<_>>>()?;
    let only = || match children.as_slice() {
        [child] => Some(Arc::new(child.clone())),
        _ => None,
    };
    let data_type = match field.type_type() {
        Type::Decimal => {
            let decimal = field.type_as_decimal()?;
            let precision = u8::try_from(decimal.precision()).ok()?;
            let scale = i8::try_from(decimal.scale()).ok()?;
            let bits = decimal.bitWidth();
            later.push((
                path.clone(),
                LaterType::Decimal {
                    bits,
                    precision,
                    scale,
                },
            ));
            DataType::Decimal128(precision, scale)
        }
        Type::ListView => {
            later.push((path.clone(), LaterType::ListView));
            DataType::List(only()?)
        }
        Type::LargeListView => {
            later.push((path.clone(), LaterType::LargeListView));
            DataType::LargeList(only()?)
        }
        Type::List => DataType::List(only()?),
        Type::LargeList => DataType::LargeList(only()?),
        Type::FixedSizeList => {
            DataType::FixedSizeList(only()?, field.type_as_fixed_size_list()?.listSize())
        }
        Type::Struct_ => DataType::Struct(children.into()),
        Type::Map => DataType::Map(only()?, field.type_as_map()?.keysSorted()),
        _ => return None,
    };
    Some(arrow_schema::Field::new(
        field.name()?,
        data_type,
        field.nullable(),
    ))
}

/// The Arrow schema the native conversion gives the file `metadata`, with
/// its embedded schema's hints, or with `held` in their place: the embedded
/// schema as `ipc_schema` held it, when it names a later type.
#[cfg(feature = "parquet")]
pub(crate) fn arrow_schema(
    metadata: &ParquetMetaData,
    held: Option<&arrow_schema::Schema>,
) -> Result<arrow_schema::Schema, ParquetError> {
    use parquet::{
        arrow::{encode_arrow_schema, parquet_to_arrow_schema, ARROW_SCHEMA_META_KEY},
        file::metadata::KeyValue,
    };
    let schema = metadata.file_metadata().schema_descr();
    match held {
        Some(held) => {
            let hint = encode_arrow_schema(held);
            let held = vec![KeyValue::new(ARROW_SCHEMA_META_KEY.to_string(), hint)];
            parquet_to_arrow_schema(schema, Some(&held))
        }
        None => parquet_to_arrow_schema(schema, metadata.file_metadata().key_value_metadata()),
    }
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
#[cfg(feature = "parquet")]
pub(crate) fn contain_decode<T>(decode: impl FnOnce() -> T) -> Option<T> {
    quiet::install();
    let _inside = quiet::Inside::enter();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(decode)).ok()
}

/// The most encoding statistics a column chunk lists in a file that 55's
/// decoder reads. Each entry counts the pages of one page type and encoding,
/// so a chunk lists each pair at most once, and 55 converts a page type and
/// an encoding only from their known values.
#[cfg(feature = "parquet")]
pub(crate) const ENCODING_STATS: usize =
    parquet::format::PageType::ENUM_VALUES.len() * parquet::format::Encoding::ENUM_VALUES.len();

/// The panic hook wrapper that keeps contained panics silent.
#[cfg(feature = "parquet")]
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
        unit: TimeUnit::MICROS(Default::default()),
    }
}

#[cfg(all(test, feature = "parquet"))]
pub(crate) fn timestamp_millis(utc: bool) -> LogicalType {
    LogicalType::Timestamp {
        is_adjusted_to_u_t_c: utc,
        unit: TimeUnit::MILLIS(Default::default()),
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
    logical_type(descriptor.logical_type().as_ref())
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
            unit: match unit {
                TimeUnit::MICROS(_) => Unit::Microsecond,
                TimeUnit::MILLIS(_) => Unit::Millisecond,
                TimeUnit::NANOS(_) => Unit::Nanosecond,
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

#[cfg(all(test, feature = "parquet"))]
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
