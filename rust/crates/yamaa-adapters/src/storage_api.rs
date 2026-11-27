//! Arrow/Parquet release-specific API boundary.
//!
//! The manifests pin Arrow/Parquet `=55.2.0`, the newest release that builds
//! with the workspace's minimum Rust. Adapting another release changes this
//! module, rather than the readers, writer or core type policy.
//!
//! A Parquet file's embedded Arrow schema is an Arrow IPC message, which
//! `arrow-ipc` verifies and gives access to. Its fields convert here
//! (`ipc_schema`), as Arrow 55's own conversion (`fb_to_schema`) converted
//! them, with one difference: where that conversion panicked on a verified
//! but semantically malformed schema, such as a missing field list, name or
//! child, or a type parameter outside the type's range, this one refuses
//! the schema.
//!
//! Arrow 55 has no 32- or 64-bit decimal, and its IPC conversion reads no
//! list view, all of which later Arrow writers name in a Parquet file's
//! embedded schema. So `ipc_schema` holds each as its nearest Arrow 55 type
//! (a 128-bit decimal, a list or a large list) and returns where it did
//! (`LaterType`). An embedded schema chooses a layout, never a field type
//! (PARQUET-7), and the stored type of a refused field still names the type
//! the file names (`parquet_source::type_name`).
//!
//! Importing a C stream also lives here (`StreamReader`): 55's
//! `ArrowArrayStreamReader` unwraps the producer's callbacks and its
//! last-error message, which the C stream interface lets be NULL, and builds
//! each batch with `RecordBatch::from`, which panics when the producer's
//! struct array has a null. 55's import also keeps buffers where the producer
//! placed them, while the interface only recommends alignment, so the reader
//! copies any under-aligned buffer before typed arrays read it (later
//! releases align on import). The import itself reads two buffers before
//! that copy, a text or binary array's offsets and a view array's buffer
//! lengths, so the reader gives the import an aligned copy of an
//! under-aligned one first. 55's schema import recurses over the producer's
//! schema and unwraps its child pointers, so the reader refuses a schema
//! nested deeper than `NESTING_DEPTH` or missing a child it counts first.
#[cfg(feature = "parquet")]
use std::sync::Arc;

/// An Arrow type that an embedded schema names and Arrow 55 has no
/// counterpart for, held as its nearest Arrow 55 type.
// A host frame's type names no later type, so only the Parquet reader
// builds one; the spelling that reads it compiles with Arrow alone.
#[cfg(feature = "arrow")]
#[cfg_attr(not(feature = "parquet"), allow(dead_code))]
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

/// A verified IPC schema that does not convert to an Arrow schema.
#[cfg(feature = "parquet")]
#[derive(Debug, PartialEq)]
pub(crate) struct Unconvertible;

/// Convert a verified IPC schema to Arrow 55's types, holding each later
/// type as its nearest one, and say where it held one.
#[cfg(feature = "parquet")]
pub(crate) fn ipc_schema(
    schema: arrow_ipc::Schema<'_>,
) -> Result<(arrow_schema::Schema, LaterTypes), Unconvertible> {
    let fields = schema.fields().ok_or(Unconvertible)?;
    // 55 refuses a big-endian decimal, as later releases do.
    let little = schema.endianness() == arrow_ipc::Endianness::Little;
    if little && fields.iter().any(names_later) {
        let mut later = Vec::new();
        let held = fields
            .iter()
            .enumerate()
            .map(|(index, field)| hold(field, &mut vec![index], &mut later))
            .collect::<Option<Vec<_>>>()
            .ok_or(Unconvertible)?;
        return Ok((arrow_schema::Schema::new(held), later));
    }
    let fields = fields
        .iter()
        .map(|field| {
            // Arrow 55 converts no decimal field of a big-endian schema. It
            // checked only the top-level fields; a decimal below one is the
            // same decimal of that schema, so it is refused at any depth.
            if !little && holds_decimal(field) {
                return None;
            }
            ipc_field(field)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(Unconvertible)?;
    Ok((arrow_schema::Schema::new(fields), Vec::new()))
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

/// Whether `field`'s type, or a type below it, is a decimal.
#[cfg(feature = "parquet")]
fn holds_decimal(field: arrow_ipc::Field<'_>) -> bool {
    field.type_type() == arrow_ipc::Type::Decimal
        || field
            .children()
            .is_some_and(|children| children.iter().any(holds_decimal))
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
        return ipc_field(field);
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
            // Only a 32- or 64-bit decimal is later, and no decimal has a child.
            if !matches!(bits, 32 | 64) || !children.is_empty() {
                return None;
            }
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

/// An IPC field as Arrow 55 converts it (`From<arrow_ipc::Field>`): its
/// name, type and nullability, and a dictionary's ordering; `None` where
/// that conversion panics. Its custom metadata, which nothing reads, is
/// left out.
#[cfg(feature = "parquet")]
fn ipc_field(field: arrow_ipc::Field<'_>) -> Option<arrow_schema::Field> {
    let name = field.name()?;
    let data_type = ipc_type(field, true)?;
    let ordered = field
        .dictionary()
        .is_some_and(|dictionary| dictionary.isOrdered());
    Some(arrow_schema::Field::new(name, data_type, field.nullable()).with_dict_is_ordered(ordered))
}

/// An IPC field's type as Arrow 55 converts it (`get_data_type`), or `None`
/// where that conversion panics: a type it does not know, a missing type
/// table, child or index type, a width, unit or mode it does not know, a
/// decimal's precision or scale out of range, or a union's repeated type id.
#[cfg(feature = "parquet")]
fn ipc_type(field: arrow_ipc::Field<'_>, dictionary: bool) -> Option<arrow_schema::DataType> {
    use arrow_ipc::{DateUnit, IntervalUnit as Interval, Precision, TimeUnit as Time, Type};
    use arrow_schema::{DataType, IntervalUnit, TimeUnit, UnionFields, UnionMode};
    if let (Some(encoding), true) = (field.dictionary(), dictionary) {
        let index = encoding.indexType()?;
        let index = integer(index.bitWidth(), index.is_signed())?;
        return Some(DataType::Dictionary(
            Box::new(index),
            Box::new(ipc_type(field, false)?),
        ));
    }
    let unit = |unit: Time| match unit {
        Time::SECOND => Some(TimeUnit::Second),
        Time::MILLISECOND => Some(TimeUnit::Millisecond),
        Time::MICROSECOND => Some(TimeUnit::Microsecond),
        Time::NANOSECOND => Some(TimeUnit::Nanosecond),
        _ => None,
    };
    let children = || field.children();
    let only = || match children()? {
        children if children.len() == 1 => ipc_field(children.get(0)).map(Arc::new),
        _ => None,
    };
    Some(match field.type_type() {
        Type::Null => DataType::Null,
        Type::Bool => DataType::Boolean,
        Type::Int => {
            let int = field.type_as_int()?;
            integer(int.bitWidth(), int.is_signed())?
        }
        Type::Binary => DataType::Binary,
        Type::BinaryView => DataType::BinaryView,
        Type::LargeBinary => DataType::LargeBinary,
        Type::Utf8 => DataType::Utf8,
        Type::Utf8View => DataType::Utf8View,
        Type::LargeUtf8 => DataType::LargeUtf8,
        Type::FixedSizeBinary => {
            DataType::FixedSizeBinary(field.type_as_fixed_size_binary()?.byteWidth())
        }
        Type::FloatingPoint => match field.type_as_floating_point()?.precision() {
            Precision::HALF => DataType::Float16,
            Precision::SINGLE => DataType::Float32,
            Precision::DOUBLE => DataType::Float64,
            _ => return None,
        },
        Type::Date => match field.type_as_date()?.unit() {
            DateUnit::DAY => DataType::Date32,
            DateUnit::MILLISECOND => DataType::Date64,
            _ => return None,
        },
        Type::Time => {
            let time = field.type_as_time()?;
            match (time.bitWidth(), unit(time.unit())?) {
                (32, unit @ (TimeUnit::Second | TimeUnit::Millisecond)) => DataType::Time32(unit),
                (64, unit @ (TimeUnit::Microsecond | TimeUnit::Nanosecond)) => {
                    DataType::Time64(unit)
                }
                _ => return None,
            }
        }
        Type::Timestamp => {
            let timestamp = field.type_as_timestamp()?;
            let zone = timestamp.timezone().map(Into::into);
            DataType::Timestamp(unit(timestamp.unit())?, zone)
        }
        Type::Interval => match field.type_as_interval()?.unit() {
            Interval::YEAR_MONTH => DataType::Interval(IntervalUnit::YearMonth),
            Interval::DAY_TIME => DataType::Interval(IntervalUnit::DayTime),
            Interval::MONTH_DAY_NANO => DataType::Interval(IntervalUnit::MonthDayNano),
            _ => return None,
        },
        Type::Duration => DataType::Duration(unit(field.type_as_duration()?.unit())?),
        Type::List => DataType::List(only()?),
        Type::LargeList => DataType::LargeList(only()?),
        Type::FixedSizeList => {
            let child = only()?;
            DataType::FixedSizeList(child, field.type_as_fixed_size_list()?.listSize())
        }
        Type::Struct_ => DataType::Struct(match children() {
            Some(children) => children.iter().map(ipc_field).collect::<Option<_>>()?,
            None => arrow_schema::Fields::empty(),
        }),
        Type::RunEndEncoded => match children()? {
            children if children.len() == 2 => DataType::RunEndEncoded(
                Arc::new(ipc_field(children.get(0))?),
                Arc::new(ipc_field(children.get(1))?),
            ),
            _ => return None,
        },
        Type::Map => {
            let sorted = field.type_as_map()?.keysSorted();
            DataType::Map(only()?, sorted)
        }
        Type::Decimal => {
            let decimal = field.type_as_decimal()?;
            let precision = u8::try_from(decimal.precision()).ok()?;
            let scale = i8::try_from(decimal.scale()).ok()?;
            match decimal.bitWidth() {
                128 => DataType::Decimal128(precision, scale),
                256 => DataType::Decimal256(precision, scale),
                _ => return None,
            }
        }
        Type::Union => {
            let union = field.type_as_union()?;
            let mode = match union.mode() {
                arrow_ipc::UnionMode::Dense => UnionMode::Dense,
                arrow_ipc::UnionMode::Sparse => UnionMode::Sparse,
                _ => return None,
            };
            let fields = children()
                .into_iter()
                .flatten()
                .map(ipc_field)
                .collect::<Option<Vec<_>>>()?;
            // Each type id once, as Arrow 55's union fields check them: its
            // bit in a 128-bit set, the shift wrapping as a release build's
            // does. Without ids, the fields take 0, 1 and so on, as many as
            // an `i8` counts their number.
            let ids: Vec<i8> = match union.typeIds() {
                Some(ids) => ids.iter().map(|id| id as i8).collect(),
                None => (0..fields.len() as i8).collect(),
            };
            let mut seen = 0u128;
            for &id in &ids {
                let bit = 1u128.wrapping_shl(i32::from(id) as u32);
                if seen & bit != 0 {
                    return None;
                }
                seen |= bit;
            }
            let fields: UnionFields = ids
                .into_iter()
                .zip(fields.into_iter().map(Arc::new))
                .collect();
            DataType::Union(fields, mode)
        }
        _ => return None,
    })
}

/// An IPC integer type of `bits` bits, signed or not, as Arrow 55 knows one.
#[cfg(feature = "parquet")]
fn integer(bits: i32, signed: bool) -> Option<arrow_schema::DataType> {
    use arrow_schema::DataType;
    Some(match (bits, signed) {
        (8, true) => DataType::Int8,
        (8, false) => DataType::UInt8,
        (16, true) => DataType::Int16,
        (16, false) => DataType::UInt16,
        (32, true) => DataType::Int32,
        (32, false) => DataType::UInt32,
        (64, true) => DataType::Int64,
        (64, false) => DataType::UInt64,
        _ => return None,
    })
}

#[cfg(feature = "arrow")]
pub(crate) use stream::StreamReader;
#[cfg(feature = "arrow")]
pub use stream::NESTING_DEPTH;

/// Reviewed C stream consumer: the producer contract is the one
/// `arrow_bridge` already documents for foreign streams.
#[cfg(feature = "arrow")]
#[allow(unsafe_code)]
mod stream {
    use arrow_array::{
        ffi::{from_ffi_and_data_type, FFI_ArrowArray, FFI_ArrowSchema},
        ffi_stream::FFI_ArrowArrayStream,
        RecordBatch, RecordBatchOptions, StructArray,
    };
    use arrow_schema::{ArrowError, DataType, Schema, SchemaRef};
    use std::{
        ffi::{c_char, c_int, c_void, CStr},
        sync::Arc,
    };

    /// The deepest a stream's column may nest types below it, counting each
    /// child and dictionary: a column of a list of a list of text nests two.
    /// An engine-built table nests one (an issue's paths and routes); the
    /// bound keeps every walk of a foreign schema, Arrow's and the engine's,
    /// far from the stack's reach.
    pub const NESTING_DEPTH: usize = 32;

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
            // SAFETY: `FFI_ArrowSchema` is the interface's `struct
            // ArrowSchema` (`repr(C)`), which `SchemaHeader` spells field by
            // field, and the producer just wrote this live schema.
            let header = unsafe { &*std::ptr::from_ref(&schema).cast::<SchemaHeader>() };
            if !nests_within(header, NESTING_DEPTH + 1) {
                return Err(ArrowError::CDataInterface(format!(
                    "input schema nests deeper than {NESTING_DEPTH} below a column \
                     or lacks a child it counts"
                )));
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
            if let Err(error) = prepare(&mut array, &kind) {
                // Dropping the batch releases it, restoring what changed.
                return Some(Err(error));
            }
            // SAFETY: the producer's array has the struct type of the schema
            // it reported for this stream, and `prepare` gave it an aligned
            // copy of each buffer the import reads. The import neither
            // validates nor aligns its buffers; `align_buffers` below copies
            // any other under-aligned one before a typed array reads it.
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

    /// The C Data interface's schema header (`struct ArrowSchema`), whose
    /// layout the interface fixes; arrow-rs keeps its fields private. Only
    /// the fields this consumer reads are named without an underscore.
    #[repr(C)]
    struct SchemaHeader {
        _format: *const c_char,
        _name: *const c_char,
        _metadata: *const c_char,
        _flags: i64,
        n_children: i64,
        children: *mut *mut SchemaHeader,
        dictionary: *mut SchemaHeader,
        _release: Option<unsafe extern "C" fn(*mut SchemaHeader)>,
        _private_data: *mut c_void,
    }

    /// Whether `schema` nests fewer than `below` levels of types below it,
    /// counting each child and dictionary, and holds every child it counts.
    /// The walk stops at the bound, so it recurses no deeper than `below`.
    fn nests_within(schema: &SchemaHeader, below: usize) -> bool {
        let Ok(count) = usize::try_from(schema.n_children) else {
            return false;
        };
        if count > 0 && schema.children.is_null() {
            return false;
        }
        let within = |child: *mut SchemaHeader| {
            // SAFETY: a non-NULL child or dictionary pointer of a live schema
            // is the producer's live header, owned with its parent.
            below > 0
                && unsafe { child.as_ref() }.is_some_and(|child| nests_within(child, below - 1))
        };
        // SAFETY: a live schema with children holds `n_children` pointers.
        (0..count).all(|index| within(unsafe { *schema.children.add(index) }))
            && (schema.dictionary.is_null() || within(schema.dictionary))
    }

    /// The C Data interface's array header (`struct ArrowArray`), whose
    /// layout the interface fixes; arrow-rs keeps its fields private. Only
    /// the fields this consumer reads or writes are named without an
    /// underscore.
    #[repr(C)]
    struct ArrayHeader {
        length: i64,
        _null_count: i64,
        offset: i64,
        n_buffers: i64,
        n_children: i64,
        buffers: *mut *const c_void,
        children: *mut *mut ArrayHeader,
        dictionary: *mut ArrayHeader,
        release: Option<unsafe extern "C" fn(*mut ArrayHeader)>,
        private_data: *mut c_void,
    }

    /// What `prepare` changed in a batch's headers: the buffer count of each
    /// child header it set to none, and each buffer pointer it replaced by
    /// an aligned copy. The copies are held until the batch is released;
    /// the imported arrays read them through the replaced pointers. Each
    /// change is recorded before the header is written, so a release after
    /// any early exit restores every change made.
    #[derive(Default)]
    struct Changes {
        counts: Vec<(*mut ArrayHeader, i64)>,
        pointers: Vec<(*mut *const c_void, *const c_void)>,
        copies: Vec<Vec<u64>>,
    }

    /// What a batch's producer gave it before `prepare` changed it: its
    /// release callback and private data, and the changes to undo.
    struct Producer {
        release: unsafe extern "C" fn(*mut ArrayHeader),
        private_data: *mut c_void,
        changes: Changes,
    }

    /// Release a batch `prepare` changed: restore each buffer count and
    /// pointer and the producer's own private data, then call the
    /// producer's release on the header as the producer wrote it; the
    /// aligned copies are freed after it.
    unsafe extern "C" fn release_restored(array: *mut ArrayHeader) {
        // SAFETY: this callback is installed only with a boxed `Producer`
        // as the header's private data, and runs once, on the live header.
        let header = unsafe { &mut *array };
        let producer = unsafe { Box::from_raw(header.private_data.cast::<Producer>()) };
        for &(child, count) in &producer.changes.counts {
            // SAFETY: each child header lives until its parent is released.
            unsafe { (*child).n_buffers = count };
        }
        for &(slot, pointer) in &producer.changes.pointers {
            // SAFETY: each buffer slot lives until its array is released.
            unsafe { *slot = pointer };
        }
        header.private_data = producer.private_data;
        header.release = Some(producer.release);
        // SAFETY: the header is again exactly what the producer exported.
        unsafe { (producer.release)(array) };
    }

    /// Ready `array`, a batch of the struct type `kind`, for the import:
    /// give each null column no buffer (`null_columns_without_buffers`) and
    /// each buffer the import reads an aligned copy (`align_read_buffers`).
    /// The producer's release callback may read what changed, so the
    /// batch's release is wrapped to restore it first (`release_restored`),
    /// before the first change: a refusal, or a panic, part way through
    /// leaves a batch whose release restores what changed. A batch that
    /// needs no change gets its producer's release back.
    fn prepare(array: &mut FFI_ArrowArray, kind: &DataType) -> Result<(), ArrowError> {
        let header = std::ptr::from_mut(array).cast::<ArrayHeader>();
        // SAFETY: `FFI_ArrowArray` is the interface's `struct ArrowArray`
        // (`repr(C)`), which `ArrayHeader` spells field by field, and this
        // consumer owns the live array the producer just wrote.
        let header = unsafe { &mut *header };
        let Some(release) = header.release else {
            return Ok(());
        };
        let producer = Box::into_raw(Box::new(Producer {
            release,
            private_data: header.private_data,
            changes: Changes::default(),
        }));
        header.private_data = producer.cast::<c_void>();
        header.release = Some(release_restored);
        // SAFETY: the box lives until `release_restored` frees it, which
        // runs when the batch is released, after this returns or unwinds.
        let changes = unsafe { &mut (*producer).changes };
        if let DataType::Struct(fields) = kind {
            null_columns_without_buffers(header, fields, changes);
        }
        align_read_buffers(header, kind, changes)?;
        if changes.counts.is_empty() && changes.pointers.is_empty() {
            // SAFETY: as above; nothing else holds the box.
            let producer = unsafe { Box::from_raw(producer) };
            header.private_data = producer.private_data;
            header.release = Some(producer.release);
        }
        Ok(())
    }

    /// The live child headers of `header`, one for each of `count` types,
    /// or none when the header does not hold that many.
    fn children(header: &ArrayHeader, count: usize) -> Vec<*mut ArrayHeader> {
        if header.children.is_null() || usize::try_from(header.n_children) != Ok(count) {
            return Vec::new();
        }
        // SAFETY: a live array holds `n_children` child pointers.
        (0..count)
            .map(|index| unsafe { *header.children.add(index) })
            .collect()
    }

    /// Give each column of the null type in `header`, a batch of `fields`,
    /// no buffer for the import to read. The null type has none, but a
    /// producer may export one, as Polars exports an absent validity
    /// buffer, and the import refuses any buffer it does not expect.
    fn null_columns_without_buffers(
        header: &ArrayHeader,
        fields: &arrow_schema::Fields,
        changes: &mut Changes,
    ) {
        for (field, child) in fields.iter().zip(children(header, fields.len())) {
            if field.data_type() != &DataType::Null {
                continue;
            }
            // SAFETY: a non-NULL child pointer is the producer's live child
            // header, owned with its parent.
            if let Some(header) = unsafe { child.as_mut() } {
                if header.n_buffers > 0 {
                    changes.counts.push((child, header.n_buffers));
                    header.n_buffers = 0;
                }
            }
        }
    }

    /// Replace each buffer the import reads in `header`, an array of
    /// `kind`, and below it, by an aligned copy where the producer's is
    /// under-aligned: the import reads a text or binary array's last offset
    /// and a view array's buffer lengths to size its other buffers. Its
    /// depth is the schema's, which `nests_within` bounds.
    fn align_read_buffers(
        header: &mut ArrayHeader,
        kind: &DataType,
        changes: &mut Changes,
    ) -> Result<(), ArrowError> {
        let count = || {
            let length = usize::try_from(header.length).ok()?;
            length
                .checked_add(usize::try_from(header.offset).ok()?)?
                .checked_add(1)
        };
        let read = match kind {
            DataType::Utf8 | DataType::Binary => count().map(|count| (1, 4, count)),
            DataType::LargeUtf8 | DataType::LargeBinary => count().map(|count| (1, 8, count)),
            // A view array's buffers are its validity, its views, its data
            // buffers and, last, an `i64` length for each data buffer.
            DataType::Utf8View | DataType::BinaryView => usize::try_from(header.n_buffers)
                .ok()
                .filter(|&buffers| buffers >= 3)
                .map(|buffers| (buffers - 1, 8, buffers - 3)),
            _ => None,
        };
        if let Some((index, width, count)) = read {
            align_buffer(header, index, width, count, changes)?;
        }
        let kinds: Vec<&DataType> = match kind {
            DataType::Struct(fields) => fields.iter().map(|field| field.data_type()).collect(),
            DataType::Union(fields, _) => {
                fields.iter().map(|(_, field)| field.data_type()).collect()
            }
            DataType::List(field)
            | DataType::LargeList(field)
            | DataType::ListView(field)
            | DataType::LargeListView(field)
            | DataType::FixedSizeList(field, _)
            | DataType::Map(field, _) => vec![field.data_type()],
            DataType::RunEndEncoded(ends, values) => vec![ends.data_type(), values.data_type()],
            _ => Vec::new(),
        };
        for (kind, child) in kinds.iter().zip(children(header, kinds.len())) {
            // SAFETY: as in `null_columns_without_buffers`.
            if let Some(child) = unsafe { child.as_mut() } {
                align_read_buffers(child, kind, changes)?;
            }
        }
        if let DataType::Dictionary(_, values) = kind {
            // SAFETY: a non-NULL dictionary pointer is the producer's live
            // dictionary header, owned with its array.
            if let Some(dictionary) = unsafe { header.dictionary.as_mut() } {
                align_read_buffers(dictionary, values, changes)?;
            }
        }
        Ok(())
    }

    /// Replace buffer `index` of `header`, `count` elements of `width`
    /// bytes, by an aligned copy when the producer's is not aligned to
    /// `width`. A copy no allocation can hold refuses the batch.
    fn align_buffer(
        header: &mut ArrayHeader,
        index: usize,
        width: usize,
        count: usize,
        changes: &mut Changes,
    ) -> Result<(), ArrowError> {
        if header.buffers.is_null() || !usize::try_from(header.n_buffers).is_ok_and(|n| n > index) {
            return Ok(());
        }
        // SAFETY: a live array holds `n_buffers` buffer pointers.
        let slot = unsafe { header.buffers.add(index) };
        let pointer = unsafe { *slot };
        if pointer.is_null() || pointer.cast::<u8>().align_offset(width) == 0 {
            return Ok(());
        }
        let refused = || {
            ArrowError::CDataInterface(format!(
                "input buffer of {count} elements of {width} bytes cannot be copied aligned"
            ))
        };
        let bytes = count.checked_mul(width).ok_or_else(refused)?;
        let mut copy = Vec::new();
        copy.try_reserve_exact(bytes.div_ceil(8))
            .map_err(|_| refused())?;
        copy.resize(bytes.div_ceil(8), 0u64);
        // SAFETY: the producer's buffer holds `count` elements of `width`
        // bytes, and the copy, aligned to eight bytes, holds at least as
        // many bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(pointer.cast::<u8>(), copy.as_mut_ptr().cast(), bytes);
        }
        // Moving the vector into `changes` leaves its elements in place.
        let aligned = copy.as_ptr();
        changes.copies.push(copy);
        changes.pointers.push((slot, pointer));
        // SAFETY: the slot is the live array's; the change is recorded, so
        // the batch's release restores the producer's pointer.
        unsafe { *slot = aligned.cast() };
        Ok(())
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

/// Writer properties capping each row group at `rows` rows.
#[cfg(all(test, feature = "parquet"))]
pub(crate) fn writer_properties_max_rows(
    rows: usize,
) -> parquet::file::properties::WriterProperties {
    parquet::file::properties::WriterProperties::builder()
        .set_max_row_group_size(rows)
        .build()
}
