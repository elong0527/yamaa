//! Foreign Arrow C streams through both imports behind the hosts' tables:
//! the result table a host restores (`import_result_stream`, behind
//! `Table::import`) and the input frame a host passes (`import_public_stream`).
//! Each input builds a stream that keeps the producer contract the C Data
//! interface leaves to the producer, every buffer as long as its type and
//! length need and every offset inside its values, while the input chooses
//! everything a consumer must judge itself: the schema, its names and its
//! list nesting past `NESTING_DEPTH`, the batches and their lengths, slice
//! offsets, validity at every level including the batch's struct, values
//! including text whose bytes are not UTF-8, and a producer that lacks a
//! callback, loses `get_next`, or fails with or without a message. Every
//! buffer is then also moved one, two or four bytes past its alignment, which
//! the interface only recommends: each import must read the displaced stream
//! as it reads the aligned one.
#![no_main]

use std::{
    collections::VecDeque,
    ffi::{c_char, c_int, c_void},
    sync::Arc,
};

use arbitrary::{Result, Unstructured};
use arrow_array::{
    builder::BooleanBuilder,
    ffi::{FFI_ArrowArray, FFI_ArrowSchema},
    make_array, Array, ArrayRef, BinaryArray, BinaryViewArray, Date32Array, Float64Array,
    Int64Array, LargeBinaryArray, ListArray, NullArray, StructArray, Time32SecondArray,
    Time64MicrosecondArray, TimestampMillisecondArray, TimestampSecondArray,
};
use arrow_buffer::{Buffer, MutableBuffer, NullBuffer, OffsetBuffer};
use arrow_data::ArrayData;
use arrow_schema::{DataType, Field, Fields, Schema};
use libfuzzer_sys::fuzz_target;
use yamaa_adapters::arrow_bridge::{self, FFI_ArrowArrayStream, NESTING_DEPTH};

/// A column type: each one an import reads, and some it refuses.
fn kind(input: &mut Unstructured<'_>, depth: usize) -> Result<DataType> {
    // Past the bound every type is a list again, so the input reaches the
    // refused depth without choosing a list at every level.
    if depth > 0 && depth <= NESTING_DEPTH + 1 && input.ratio(7, 8)? {
        let item = kind(input, depth + 1)?;
        return Ok(DataType::List(Arc::new(Field::new("item", item, true))));
    }
    Ok(match input.int_in_range(0..=12)? {
        0 => DataType::Int64,
        1 => DataType::Float64,
        2 => DataType::Date32,
        3 => DataType::Timestamp(arrow_schema::TimeUnit::Second, None),
        4 => DataType::Timestamp(arrow_schema::TimeUnit::Millisecond, None),
        5 => DataType::Time32(arrow_schema::TimeUnit::Second),
        6 => DataType::Time64(arrow_schema::TimeUnit::Microsecond),
        7 => DataType::Utf8,
        8 => DataType::LargeUtf8,
        9 => DataType::Utf8View,
        10 => DataType::Null,
        11 => DataType::Boolean,
        _ => {
            let item = kind(input, depth + 1)?;
            DataType::List(Arc::new(Field::new("item", item, true)))
        }
    })
}

/// A validity buffer of `len` records, or none.
fn nulls(input: &mut Unstructured<'_>, len: usize) -> Result<Option<NullBuffer>> {
    if !input.arbitrary::<bool>()? {
        return Ok(None);
    }
    let valid = (0..len)
        .map(|_| input.arbitrary::<bool>())
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(NullBuffer::from(valid)))
}

fn values<T: for<'a> arbitrary::Arbitrary<'a>>(
    input: &mut Unstructured<'_>,
    len: usize,
) -> Result<Vec<T>> {
    (0..len).map(|_| input.arbitrary()).collect()
}

/// Text as bytes the input chooses, UTF-8 or not, as the `text` type spells
/// it. Only that type changes; the offsets or views stay the binary array's.
fn text(input: &mut Unstructured<'_>, len: usize, kind: &DataType) -> Result<ArrayRef> {
    let mut cells = Vec::with_capacity(len);
    for _ in 0..len {
        let size = input.int_in_range(0..=16)?;
        cells.push(input.arbitrary::<bool>()?.then_some(input.bytes(size)?));
    }
    let binary: ArrayRef = match kind {
        DataType::Utf8 => Arc::new(BinaryArray::from(cells)),
        DataType::LargeUtf8 => Arc::new(LargeBinaryArray::from(cells)),
        _ => Arc::new(BinaryViewArray::from(cells)),
    };
    let data = binary.to_data().into_builder().data_type(kind.clone());
    // SAFETY: the binary array's offsets or views lie inside its buffers;
    // only the text type's UTF-8 is left unchecked, which the imports judge.
    Ok(make_array(unsafe { data.build_unchecked() }))
}

/// An array of `kind` with `len` records, sliced past some leading ones.
fn array(input: &mut Unstructured<'_>, kind: &DataType, len: usize) -> Result<ArrayRef> {
    let skip = input.int_in_range(0..=3)?;
    let whole = skip + len;
    let built: ArrayRef = match kind {
        DataType::Int64 => Arc::new(Int64Array::new(
            values(input, whole)?.into(),
            nulls(input, whole)?,
        )),
        DataType::Float64 => Arc::new(Float64Array::new(
            values::<u64>(input, whole)?
                .into_iter()
                .map(f64::from_bits)
                .collect(),
            nulls(input, whole)?,
        )),
        DataType::Date32 => Arc::new(Date32Array::new(
            values(input, whole)?.into(),
            nulls(input, whole)?,
        )),
        DataType::Timestamp(arrow_schema::TimeUnit::Second, _) => Arc::new(
            TimestampSecondArray::new(values(input, whole)?.into(), nulls(input, whole)?),
        ),
        DataType::Timestamp(..) => Arc::new(TimestampMillisecondArray::new(
            values(input, whole)?.into(),
            nulls(input, whole)?,
        )),
        DataType::Time32(_) => Arc::new(Time32SecondArray::new(
            values(input, whole)?.into(),
            nulls(input, whole)?,
        )),
        DataType::Time64(_) => Arc::new(Time64MicrosecondArray::new(
            values(input, whole)?.into(),
            nulls(input, whole)?,
        )),
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => text(input, whole, kind)?,
        DataType::Null => Arc::new(NullArray::new(whole)),
        DataType::Boolean => {
            let mut builder = BooleanBuilder::with_capacity(whole);
            for value in values::<Option<bool>>(input, whole)? {
                builder.append_option(value);
            }
            Arc::new(builder.finish())
        }
        DataType::List(field) => {
            let lengths = (0..whole)
                .map(|_| input.int_in_range(0..=3))
                .collect::<Result<Vec<usize>>>()?;
            let items = array(input, field.data_type(), lengths.iter().sum())?;
            let nulls = nulls(input, whole)?;
            Arc::new(ListArray::new(
                field.clone(),
                OffsetBuffer::from_lengths(lengths),
                items,
                nulls,
            ))
        }
        _ => unreachable!("kind chooses no other type"),
    };
    Ok(built.slice(skip, len))
}

/// `data` with every buffer, its own and its children's, moved `offset`
/// bytes past a 64-byte boundary.
fn displaced(data: &ArrayData, offset: usize) -> ArrayData {
    let buffers = data
        .buffers()
        .iter()
        .map(|buffer| {
            let mut moved = MutableBuffer::from_len_zeroed(offset + buffer.len());
            moved.as_slice_mut()[offset..].copy_from_slice(buffer.as_slice());
            Buffer::from(moved).slice_with_length(offset, buffer.len())
        })
        .collect();
    let children = data
        .child_data()
        .iter()
        .map(|child| displaced(child, offset))
        .collect();
    let builder = data
        .clone()
        .into_builder()
        .buffers(buffers)
        .child_data(children);
    // SAFETY: the same bytes at other addresses; only their alignment
    // changes, which the interface leaves to the producer.
    unsafe { builder.build_unchecked() }
}

/// How the producer behaves beside the batches it yields.
#[derive(Clone, Copy)]
struct Behaviour {
    /// `get_next` fails once this many batches were taken.
    fail_after: Option<usize>,
    /// A failure's `get_last_error` returns a message rather than NULL.
    message: bool,
    /// `get_schema` clears `get_next`, as a producer that loses it does.
    drop_next: bool,
    /// The callbacks the stream lacks.
    no_schema: bool,
    no_next: bool,
    no_last_error: bool,
}

struct Producer {
    schema: Schema,
    batches: VecDeque<FFI_ArrowArray>,
    behaviour: Behaviour,
}

/// Status codes a producer returns: errno values, as the interface asks.
const EINVAL: c_int = 22;
const EIO: c_int = 5;

unsafe extern "C" fn get_schema(
    stream: *mut FFI_ArrowArrayStream,
    out: *mut FFI_ArrowSchema,
) -> c_int {
    // SAFETY: the stream's private data is the producer `stream` boxed.
    let producer = unsafe { &*(*stream).private_data.cast::<Producer>() };
    // A schema arrow cannot export fails the call, as a producer's would.
    let Ok(schema) = FFI_ArrowSchema::try_from(&producer.schema) else {
        return EINVAL;
    };
    // SAFETY: `out` is the consumer's empty schema, which it now owns.
    unsafe { std::ptr::write(out, schema) };
    if producer.behaviour.drop_next {
        // SAFETY: the live stream this callback was called on.
        unsafe { (*stream).get_next = None };
    }
    0
}

unsafe extern "C" fn get_next(
    stream: *mut FFI_ArrowArrayStream,
    out: *mut FFI_ArrowArray,
) -> c_int {
    // SAFETY: as in `get_schema`.
    let producer = unsafe { &mut *(*stream).private_data.cast::<Producer>() };
    match &mut producer.behaviour.fail_after {
        Some(0) => return EIO,
        Some(left) => *left -= 1,
        None => {}
    }
    let next = producer
        .batches
        .pop_front()
        .unwrap_or_else(FFI_ArrowArray::empty);
    // SAFETY: `out` is the consumer's empty array, which it now owns.
    unsafe { std::ptr::write(out, next) };
    0
}

unsafe extern "C" fn get_last_error(stream: *mut FFI_ArrowArrayStream) -> *const c_char {
    // SAFETY: as in `get_schema`.
    let producer = unsafe { &*(*stream).private_data.cast::<Producer>() };
    if producer.behaviour.message {
        c"the producer failed".as_ptr()
    } else {
        std::ptr::null()
    }
}

unsafe extern "C" fn release(stream: *mut FFI_ArrowArrayStream) {
    // SAFETY: the box `stream` made, released once: release is then unset.
    unsafe {
        drop(Box::from_raw((*stream).private_data.cast::<Producer>()));
        (*stream).release = None;
    }
}

/// A stream of `batches` under `schema` that behaves as `behaviour` says.
fn stream(schema: &Schema, batches: &[ArrayData], behaviour: Behaviour) -> FFI_ArrowArrayStream {
    let producer = Producer {
        schema: schema.clone(),
        batches: batches.iter().map(FFI_ArrowArray::new).collect(),
        behaviour,
    };
    let mut stream = FFI_ArrowArrayStream::empty();
    stream.get_schema = (!behaviour.no_schema).then_some(get_schema);
    stream.get_next = (!behaviour.no_next).then_some(get_next);
    stream.get_last_error = (!behaviour.no_last_error).then_some(get_last_error);
    stream.release = Some(release);
    stream.private_data = Box::into_raw(Box::new(producer)).cast::<c_void>();
    stream
}

/// The schema, the batches as struct arrays and the producer's behaviour.
fn build(input: &mut Unstructured<'_>) -> Result<(Schema, Vec<ArrayData>, Behaviour)> {
    let count = input.int_in_range(0..=4)?;
    let fields: Fields = (0..count)
        .map(|_| {
            // A C string holds no NUL, and arrow's export panics on one.
            let name = input.arbitrary::<String>()?.replace('\0', "");
            Ok(Field::new(name, kind(input, 0)?, true))
        })
        .collect::<Result<Vec<_>>>()?
        .into();
    let mut batches = Vec::new();
    for _ in 0..input.int_in_range(0..=3)? {
        let rows = input.int_in_range(0..=8)?;
        let columns = fields
            .iter()
            .map(|field| array(input, field.data_type(), rows))
            .collect::<Result<Vec<_>>>()?;
        let nulls = nulls(input, rows)?;
        let batch = StructArray::try_new_with_length(fields.clone(), columns, nulls, rows)
            .expect("columns of the fields' types and the batch's length");
        batches.push(batch.to_data());
    }
    let behaviour = Behaviour {
        fail_after: input
            .ratio(1, 4)?
            .then(|| input.int_in_range(0..=3))
            .transpose()?,
        message: input.arbitrary()?,
        drop_next: input.ratio(1, 16)?,
        no_schema: input.ratio(1, 32)?,
        no_next: input.ratio(1, 32)?,
        no_last_error: input.ratio(1, 16)?,
    };
    Ok((Schema::new(fields), batches, behaviour))
}

fuzz_target!(|data: &[u8]| {
    let mut input = Unstructured::new(data);
    let Ok((schema, batches, behaviour)) = build(&mut input) else {
        return;
    };
    let offset = [1, 2, 4][usize::from(data.first().copied().unwrap_or(0) % 3)];
    let moved: Vec<_> = batches
        .iter()
        .map(|batch| displaced(batch, offset))
        .collect();
    let result = |batches: &[ArrayData]| {
        format!(
            "{:?}",
            arrow_bridge::import_result_stream(stream(&schema, batches, behaviour))
        )
    };
    assert_eq!(result(&batches), result(&moved), "a restored result table");
    let frame = |batches: &[ArrayData]| {
        format!(
            "{:?}",
            arrow_bridge::import_public_stream(stream(&schema, batches, behaviour))
        )
    };
    assert_eq!(frame(&batches), frame(&moved), "an input frame");
});
