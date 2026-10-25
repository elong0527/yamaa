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
//! Logical timestamp types are struct variants on 55, with the time unit a
//! union of empty structs (later releases wrap them in tuple variants with
//! constructors), so the writer's construction of one also lives here.
//!
//! Importing a C stream also lives here (`StreamReader`): 55's
//! `ArrowArrayStreamReader` unwraps the producer's callbacks and its
//! last-error message, which the C stream interface lets be NULL, and builds
//! each batch with `RecordBatch::from`, which panics when the producer's
//! struct array has a null. 55's import also keeps buffers where the producer
//! placed them, while the interface only recommends alignment, so the reader
//! copies any under-aligned buffer before typed arrays read it (later
//! releases align on import).
#[cfg(feature = "parquet")]
use parquet::basic::{LogicalType, TimeUnit};
#[cfg(feature = "parquet")]
use std::sync::Arc;

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
            // Arrow 55 converts no decimal field of a big-endian schema.
            if !little && field.type_type() == arrow_ipc::Type::Decimal {
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

#[cfg(feature = "parquet")]
pub(crate) fn timestamp_micros(utc: bool) -> LogicalType {
    LogicalType::Timestamp {
        is_adjusted_to_u_t_c: utc,
        unit: TimeUnit::MICROS(Default::default()),
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
