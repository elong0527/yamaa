#![cfg(feature = "arrow")]
//! Independently authored C Data/Stream values and ownership controls.
//!
//! The seam is the Arrow bridge itself: C streams built in the test, with
//! counted drops, misaligned buffers and failing callbacks, imported and
//! exported through `arrow_bridge`, which no fixture reader produces.
use arrow_array::{
    make_array, new_empty_array, types::Int32Type, Array, ArrayRef, Date32Array, Float64Array,
    Int32Array, Int64Array, LargeStringArray, ListArray, RecordBatch, RecordBatchOptions,
    RecordBatchReader, RunArray, StringArray, StringViewArray, Time32MillisecondArray,
    Time32SecondArray, Time64MicrosecondArray, Time64NanosecondArray, TimestampMicrosecondArray,
    TimestampMillisecondArray, TimestampNanosecondArray, TimestampSecondArray,
};
use arrow_buffer::OffsetBuffer;
use arrow_schema::{ArrowError, DataType, Field, Schema, SchemaRef, TimeUnit};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use yamaa_adapters::{
    arrow_bridge::{self, ArrowArrayStreamReader, FFI_ArrowArrayStream},
    column_table::{Column as Values, ColumnTable, TableError, ValueCause},
    public_table::{PublicColumn, PublicTable, TableError as ResultError},
};
use yamaa_core::{
    table::{Column, TableAccess, TableSchema, ValueRef},
    temporal::{Date, DateTime, Precision},
    value::ColumnType,
};

struct Tracked {
    schema: SchemaRef,
    batches: VecDeque<Result<RecordBatch, ArrowError>>,
    drops: Arc<AtomicUsize>,
}
impl Iterator for Tracked {
    type Item = Result<RecordBatch, ArrowError>;
    fn next(&mut self) -> Option<Self::Item> {
        self.batches.pop_front()
    }
}
impl RecordBatchReader for Tracked {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn stream(
    schema: SchemaRef,
    batches: Vec<Result<RecordBatch, ArrowError>>,
) -> (FFI_ArrowArrayStream, Arc<AtomicUsize>) {
    let drops = Arc::new(AtomicUsize::new(0));
    (
        FFI_ArrowArrayStream::new(Box::new(Tracked {
            schema,
            batches: batches.into(),
            drops: drops.clone(),
        })),
        drops,
    )
}
fn truth() -> RecordBatch {
    truth_with_zero(-0.0)
}
/// The truth table with `zero` in its first float cell.
fn truth_with_zero(zero: f64) -> RecordBatch {
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(vec![
            Some("a\0\u{1f980}"),
            None,
            Some(""),
            Some("\u{03b2}"),
        ])),
        Arc::new(Int64Array::from(vec![
            Some(i64::MIN),
            None,
            Some(i64::MAX),
            Some(9_007_199_254_740_993),
        ])),
        Arc::new(Float64Array::from(vec![
            Some(zero),
            None,
            Some(f64::from_bits(1)),
            Some(1.25),
        ])),
        Arc::new(Date32Array::from(vec![
            Some(-719_162),
            None,
            Some(0),
            Some(2_932_896),
        ])),
        Arc::new(TimestampSecondArray::from(vec![
            Some(-62_135_596_800),
            None,
            Some(-1),
            Some(253_402_300_799),
        ])),
    ];
    let fields = ["S", "I", "F", "D", "T"]
        .iter()
        .zip(&arrays)
        .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
        .collect::<Vec<_>>();
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).unwrap()
}

/// A one-column frame of `array`, through a C stream.
fn imported(name: &str, array: ArrayRef) -> Result<ColumnTable, TableError> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        name,
        array.data_type().clone(),
        true,
    )]));
    let batch = RecordBatch::try_new(schema.clone(), vec![array]).unwrap();
    arrow_bridge::import_public_stream(arrow_bridge::export_stream(schema, vec![batch]))
}

#[test]
fn all_logical_values_nulls_slices_and_empty_chunks_import_in_record_order() {
    let original = truth();
    let chunks = vec![
        original.slice(0, 0),
        original.slice(0, 2),
        original.slice(2, 0),
        original.slice(2, 2),
        original.slice(4, 0),
    ];
    let (source, dropped) = stream(original.schema(), chunks.into_iter().map(Ok).collect());
    drop(original);
    let table = arrow_bridge::import_public_stream(source).unwrap();
    // The import released the stream and owns copies of its values.
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(table.row_count(), 4);
    assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("a\0\u{1f980}"));
    assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Str(""));
    assert_eq!(table.cell(3, 0).unwrap(), ValueRef::Str("\u{03b2}"));
    for column in 0..5 {
        assert_eq!(table.cell(1, column).unwrap(), ValueRef::Missing);
    }
    assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(i64::MIN));
    assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Int(i64::MAX));
    assert_eq!(
        table.cell(3, 1).unwrap(),
        ValueRef::Int(9_007_199_254_740_993)
    );
    let ValueRef::Float(value) = table.cell(0, 2).unwrap() else {
        panic!()
    };
    assert_eq!(value.get().to_bits(), 0.0f64.to_bits());
    let ValueRef::Float(value) = table.cell(2, 2).unwrap() else {
        panic!()
    };
    assert_eq!(value.get().to_bits(), 1);
    let ValueRef::Date(value) = table.cell(0, 3).unwrap() else {
        panic!()
    };
    assert_eq!(value.to_string(), "0001-01-01");
    assert_eq!(value.collected_precision(), Precision::Day);
    let ValueRef::DateTime(value) = table.cell(3, 4).unwrap() else {
        panic!()
    };
    assert_eq!(value.to_string(), "9999-12-31T23:59:59");
    assert_eq!(value.collected_precision(), Precision::Second);
}

#[test]
fn every_export_shares_the_tables_arrow_buffers_and_outlives_its_reader() {
    let table = arrow_bridge::import_public_stream(arrow_bridge::export_stream(
        truth().schema(),
        vec![truth()],
    ))
    .unwrap();
    let public = PublicTable::from_table(&table, &[0, 1, 2, 3, 4]).unwrap();
    drop(table);
    let pointers = |batch: &RecordBatch| {
        batch
            .columns()
            .iter()
            .map(|array| {
                let data = array.to_data();
                (
                    data.buffers()
                        .iter()
                        .map(|buffer| buffer.as_ptr())
                        .collect::<Vec<_>>(),
                    data.nulls().map(|nulls| nulls.buffer().as_ptr()),
                )
            })
            .collect::<Vec<_>>()
    };
    let held = pointers(&public.batches()[0]);
    for _ in 0..100 {
        let mut reader = ArrowArrayStreamReader::try_new(public.stream()).unwrap();
        let first = reader.next().unwrap().unwrap();
        // Releasing the reader early leaves imported C Data arrays alive.
        drop(reader);
        assert_eq!(pointers(&first), held);
        // VALUE-4: the imported -0.0 is held as positive zero.
        assert_eq!(first, truth_with_zero(0.0));
    }
}

#[test]
fn large_and_view_utf8_import_their_values() {
    let text = "a\0\u{1f980} long UTF-8 payload that cannot fit in an inline string view";
    for array in [
        Arc::new(LargeStringArray::from(vec![Some(text), None, Some("")])) as ArrayRef,
        Arc::new(StringViewArray::from(vec![Some(text), None, Some("")])) as ArrayRef,
    ] {
        let kind = array.data_type().clone();
        let table = imported("S", array).unwrap();
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str(text), "{kind}");
        assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing, "{kind}");
        assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Str(""), "{kind}");
        // Public tables carry text as Utf8, whatever layout a frame used.
        let public = PublicTable::from_table(&table, &[0]).unwrap();
        let expected = StringArray::from(vec![Some(text), None, Some("")]);
        assert_eq!(
            public.batches()[0].column(0).as_ref(),
            &expected as &dyn Array
        );
    }
}

#[test]
fn reader_and_buffer_owners_release_once_on_early_drop_and_failures() {
    let original = truth();
    let (source, dropped) = stream(original.schema(), vec![Ok(original.clone())]);
    drop(source);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    let (source, dropped) = stream(original.schema(), vec![Ok(original.clone())]);
    let mut reader = ArrowArrayStreamReader::try_new(source).unwrap();
    let imported = reader.next().unwrap().unwrap();
    drop(original);
    drop(reader);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(
        imported
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0),
        i64::MIN
    );
    let (source, dropped) = stream(
        imported.schema(),
        vec![
            Ok(imported),
            Err(ArrowError::ParseError("independent reader failure".into())),
        ],
    );
    assert!(arrow_bridge::import_public_stream(source).is_err());
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

/// A refused frame column: its name, its Arrow type and a value's one-based
/// record in the stream with its cause.
type Refused = (String, String, Option<(usize, ValueCause)>);

/// The column a frame import refused; any other failure is `None`.
fn refused_column(error: Option<TableError>) -> Option<Refused> {
    match error {
        Some(TableError::FrameColumn {
            column,
            arrow_type,
            record,
        }) => Some((column, arrow_type, record)),
        _ => None,
    }
}

/// A finer-unit time or timestamp type as Arrow C++ spells it.
fn finer_unit(kind: &DataType) -> &'static str {
    match kind {
        DataType::Time32(TimeUnit::Millisecond) => "time32[ms]",
        DataType::Time64(TimeUnit::Microsecond) => "time64[us]",
        DataType::Time64(TimeUnit::Nanosecond) => "time64[ns]",
        DataType::Timestamp(TimeUnit::Millisecond, None) => "timestamp[ms]",
        DataType::Timestamp(TimeUnit::Microsecond, None) => "timestamp[us]",
        DataType::Timestamp(TimeUnit::Nanosecond, None) => "timestamp[ns]",
        other => panic!("no spelling for {other}"),
    }
}

/// A refused type names its column and the type as Arrow C++ spells it.
#[test]
fn incapable_physical_types_are_refused_and_release_the_input() {
    for (kind, spelled) in [
        (DataType::Int32, "int32"),
        (DataType::UInt64, "uint64"),
        (DataType::Boolean, "bool"),
        (
            DataType::Timestamp(TimeUnit::Second, Some("UTC".into())),
            "timestamp[s, tz=UTC]",
        ),
        (
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into())),
            "timestamp[ms, tz=UTC]",
        ),
        (
            DataType::Struct(vec![Field::new("value", DataType::Date32, false)].into()),
            "struct<value: date32[day] not null>",
        ),
    ] {
        let schema = Arc::new(Schema::new(vec![Field::new("X", kind.clone(), true)]));
        let batch = RecordBatch::try_new(schema.clone(), vec![new_empty_array(&kind)]).unwrap();
        let (source, dropped) = stream(schema, vec![Ok(batch)]);
        assert_eq!(
            refused_column(arrow_bridge::import_public_stream(source).err()),
            Some(("X".into(), spelled.into(), None)),
            "{kind}"
        );
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
}

/// A column of the null type holds no value: text missing in every record.
#[test]
fn a_column_of_the_null_type_is_text_missing_in_every_record() {
    let table = imported("N", Arc::new(arrow_array::NullArray::new(2))).unwrap();
    assert_eq!(
        (
            table.schema().columns()[0].kind,
            table.cell(0, 0).unwrap(),
            table.cell(1, 0).unwrap()
        ),
        (ColumnType::Str, ValueRef::Missing, ValueRef::Missing)
    );
}

/// The text column `kind` (`Utf8`, `LargeUtf8` or `Utf8View`) holding
/// `values` as bytes, built without the UTF-8 check a producer may skip.
fn unchecked_text(kind: &DataType, values: &[Option<&[u8]>]) -> ArrayRef {
    let binary = match kind {
        DataType::Utf8 => Arc::new(arrow_array::BinaryArray::from(values.to_vec())) as ArrayRef,
        DataType::LargeUtf8 => Arc::new(arrow_array::LargeBinaryArray::from(values.to_vec())),
        _ => Arc::new(arrow_array::BinaryViewArray::from(values.to_vec())),
    };
    // SAFETY: the binary array is valid; only its bytes may not be UTF-8,
    // which this test supplies on purpose.
    make_array(unsafe {
        binary
            .to_data()
            .into_builder()
            .data_type(kind.clone())
            .build_unchecked()
    })
}

/// A producer may export text whose bytes are not UTF-8 (pyarrow does not
/// validate a Parquet file's text on read). The value is refused at its
/// record in the stream, counted across batches, as text no table holds;
/// a missing value's bytes are not read.
#[test]
fn text_that_is_not_utf8_is_refused_at_its_record_and_nulls_mask_payloads() {
    let latin1: &[u8] = b"caf\xe9";
    for (kind, spelled) in [
        (DataType::Utf8, "string"),
        (DataType::LargeUtf8, "large_string"),
        (DataType::Utf8View, "string_view"),
    ] {
        let schema = Arc::new(Schema::new(vec![Field::new("S", kind.clone(), true)]));
        let batches = [
            vec![Some(b"A".as_slice())],
            vec![None, Some(b"B".as_slice()), Some(latin1)],
        ]
        .map(|values| {
            RecordBatch::try_new(schema.clone(), vec![unchecked_text(&kind, &values)]).unwrap()
        });
        let error = arrow_bridge::import_public_stream(arrow_bridge::export_stream(
            schema.clone(),
            batches.to_vec(),
        ))
        .err();
        assert!(
            matches!(
                &error,
                Some(TableError::FrameText { column, arrow_type, record: 4 })
                    if column == "S" && arrow_type == spelled
            ),
            "{kind}: {error:?}"
        );
        let masked = unchecked_text(&kind, &[Some(b"A".as_slice()), Some(latin1)]);
        let masked = make_array(unsafe {
            // SAFETY: a validity buffer of the array's length over valid
            // offsets; it hides the value that is not UTF-8.
            masked
                .to_data()
                .into_builder()
                .nulls(Some(vec![true, false].into()))
                .build_unchecked()
        });
        let table = imported("S", masked).unwrap();
        assert_eq!(
            (table.cell(0, 0).unwrap(), table.cell(1, 0).unwrap()),
            (ValueRef::Str("A"), ValueRef::Missing),
            "{kind}"
        );
    }
}

/// The null type has no buffer, but Polars 2.0.0 exports one, which the
/// Arrow import refuses: the column still holds no value, and the producer's
/// release callback sees the child header's buffer count as it exported it.
/// The C Data child here comes from an `Int8` array, whose header carries
/// two buffers, under a stream schema of the null type; no fixture reader
/// builds such a header.
#[test]
fn a_null_type_column_exported_with_buffers_is_missing_in_every_record() {
    let fields = vec![
        Field::new("ID", DataType::Int64, true),
        Field::new("N", DataType::Null, true),
    ];
    let parent = arrow_array::StructArray::from(vec![
        (
            Arc::new(fields[0].clone()),
            Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef,
        ),
        (
            Arc::new(Field::new("N", DataType::Int8, true)),
            Arc::new(arrow_array::Int8Array::from(vec![None, None])) as ArrayRef,
        ),
    ])
    .to_data();
    let (array, released) =
        watched::child_buffers(arrow_array::ffi::FFI_ArrowArray::new(&parent), 1);
    let source = raw::stream(Schema::new(fields), raw::Behaviour::Yield(vec![array]));
    let table = arrow_bridge::import_public_stream(source).unwrap();
    assert_eq!(
        (
            table.schema().columns()[1].kind,
            table.cell(0, 1).unwrap(),
            table.cell(1, 1).unwrap(),
            released.buffers.load(Ordering::SeqCst)
        ),
        (ColumnType::Str, ValueRef::Missing, ValueRef::Missing, 2)
    );
}

/// A producer's release callback that records what it sees of a child
/// header before releasing the batch as arrow-rs exported it.
#[allow(unsafe_code)]
mod watched {
    use arrow_array::ffi::FFI_ArrowArray;
    use std::{
        ffi::c_void,
        sync::{
            atomic::{AtomicI64, AtomicUsize, Ordering},
            Arc,
        },
    };

    /// What the release saw of the child: its buffer count, and the address
    /// of its second buffer, a text array's offsets; -1 and 0 until then.
    pub struct Seen {
        pub buffers: AtomicI64,
        pub second: AtomicUsize,
    }

    /// The C Data interface's `struct ArrowArray`.
    #[repr(C)]
    struct Header {
        _length: i64,
        _null_count: i64,
        _offset: i64,
        n_buffers: i64,
        _n_children: i64,
        buffers: *mut *const c_void,
        children: *mut *mut Header,
        _dictionary: *mut Header,
        release: Option<unsafe extern "C" fn(*mut Header)>,
        private_data: *mut c_void,
    }

    struct Watch {
        release: unsafe extern "C" fn(*mut Header),
        private_data: *mut c_void,
        child: usize,
        seen: Arc<Seen>,
    }

    unsafe extern "C" fn release(array: *mut Header) {
        let header = unsafe { &mut *array };
        let watch = unsafe { Box::from_raw(header.private_data.cast::<Watch>()) };
        let child = unsafe { &*(*header.children.add(watch.child)) };
        watch.seen.buffers.store(child.n_buffers, Ordering::SeqCst);
        if child.n_buffers > 1 {
            let second = unsafe { *child.buffers.add(1) };
            watch.seen.second.store(second as usize, Ordering::SeqCst);
        }
        header.private_data = watch.private_data;
        header.release = Some(watch.release);
        unsafe { (watch.release)(array) };
    }

    /// `array`, whose release records what it sees of child `child`.
    pub fn child_buffers(mut array: FFI_ArrowArray, child: usize) -> (FFI_ArrowArray, Arc<Seen>) {
        let seen = Arc::new(Seen {
            buffers: AtomicI64::new(-1),
            second: AtomicUsize::new(0),
        });
        let header = unsafe { &mut *std::ptr::from_mut(&mut array).cast::<Header>() };
        let watch = Watch {
            release: header.release.expect("a live array"),
            private_data: header.private_data,
            child,
            seen: seen.clone(),
        };
        header.private_data = Box::into_raw(Box::new(watch)).cast::<c_void>();
        header.release = Some(release);
        (array, seen)
    }
}

/// Arrow 55 validates a run-end encoded array against its own run ends,
/// not its parent's length (RUSTSEC-2026-0442), so reading one's values can
/// read past them. Both imports refuse the type before reading a value: an
/// input frame by its schema, before any batch, and a result table by each
/// column's type, at the top level or as a list's items.
///
/// The array is the advisory's shape, five rows over run ends that cover
/// three, which Arrow 55's validating builder admits. A read of rows 3 or 4
/// reaches `get_unchecked` past the two values, which a test build's
/// precondition checks abort, so an import that read before refusing fails
/// this test. A patched Arrow refuses the array at `build`.
#[test]
fn run_end_encoded_columns_are_refused_before_any_value_is_read() {
    let covered = RunArray::<Int32Type>::try_new(
        &Int32Array::from(vec![2, 3]),
        &Int64Array::from(vec![7, 8]),
    )
    .unwrap();
    let runs = make_array(
        covered
            .to_data()
            .into_builder()
            .len(5)
            .build()
            .expect("Arrow 55 admits a parent longer than its run ends"),
    );
    let item = Arc::new(Field::new("item", runs.data_type().clone(), true));
    let list: ArrayRef = Arc::new(ListArray::new(
        item,
        OffsetBuffer::from_lengths([5]),
        runs.clone(),
        None,
    ));
    let batch = |array: &ArrayRef| {
        let field = Field::new("X", array.data_type().clone(), true);
        let schema = Arc::new(Schema::new(vec![field]));
        let batch = RecordBatch::try_new(schema.clone(), vec![array.clone()]).unwrap();
        (schema, batch)
    };
    let (schema, input) = batch(&runs);
    let (source, dropped) = stream(schema, vec![Ok(input)]);
    assert_eq!(
        refused_column(arrow_bridge::import_public_stream(source).err()),
        Some((
            "X".into(),
            "run_end_encoded<run_ends: int32, values: int64>".into(),
            None
        )),
        "an input frame's run-end encoded column"
    );
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    for (label, array) in [("a column", &runs), ("a list's items", &list)] {
        let (schema, result) = batch(array);
        let (source, dropped) = stream(schema, vec![Ok(result)]);
        assert_eq!(
            arrow_bridge::import_result_stream(source),
            Err(ResultError::InvalidTable),
            "a result table's run-end encoded {label}"
        );
        assert_eq!(dropped.load(Ordering::SeqCst), 1, "{label}");
    }
}

/// A visible date or datetime outside the years 1 to 9999 is refused at its
/// column and its record in the stream, counted across batches; a null masks
/// whatever its slot holds.
#[test]
fn visible_temporal_values_outside_the_calendar_are_refused_and_nulls_mask_payloads() {
    for (array, row, spelled) in [
        (
            Arc::new(Date32Array::from(vec![0, i32::MIN])) as ArrayRef,
            1,
            "date32[day]",
        ),
        (
            Arc::new(Date32Array::from(vec![2_932_897, 0])),
            0,
            "date32[day]",
        ),
        (
            Arc::new(TimestampSecondArray::from(vec![0, i64::MAX])),
            1,
            "timestamp[s]",
        ),
        (
            Arc::new(TimestampSecondArray::from(vec![-62_135_596_801, 0])),
            0,
            "timestamp[s]",
        ),
    ] {
        let kind = array.data_type().clone();
        let schema = Arc::new(Schema::new(vec![Field::new("D", kind.clone(), true)]));
        // A first batch of the one admitted value: the refused value's
        // record follows it.
        let valid = RecordBatch::try_new(schema.clone(), vec![array.slice(1 - row, 1)]).unwrap();
        let batch = RecordBatch::try_new(schema.clone(), vec![array]).unwrap();
        let error = arrow_bridge::import_public_stream(arrow_bridge::export_stream(
            schema,
            vec![valid, batch],
        ))
        .err();
        assert_eq!(
            refused_column(error),
            Some((
                "D".into(),
                spelled.into(),
                Some((row + 2, ValueCause::Calendar))
            )),
            "{kind}"
        );
    }
    let masked = Date32Array::new(vec![i32::MIN, 0].into(), Some(vec![false, true].into()));
    let table = imported("D", Arc::new(masked)).unwrap();
    assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Missing);
    let ValueRef::Date(value) = table.cell(1, 0).unwrap() else {
        panic!("a date")
    };
    assert_eq!(value.fields(), (1970, 1, 1));
}

/// A time or datetime count in milliseconds, microseconds or nanoseconds
/// imports as the whole seconds it names, with nulls staying missing.
#[test]
fn sub_second_temporal_units_import_as_the_whole_seconds_they_name() {
    // 08:30:15 and 23:59:59, in each unit's counts from midnight.
    for array in [
        Arc::new(Time32SecondArray::from(vec![
            Some(30_615),
            None,
            Some(86_399),
        ])) as ArrayRef,
        Arc::new(Time32MillisecondArray::from(vec![
            Some(30_615_000),
            None,
            Some(86_399_000),
        ])) as ArrayRef,
        Arc::new(Time64MicrosecondArray::from(vec![
            Some(30_615_000_000),
            None,
            Some(86_399_000_000),
        ])) as ArrayRef,
        Arc::new(Time64NanosecondArray::from(vec![
            Some(30_615_000_000_000),
            None,
            Some(86_399_000_000_000),
        ])) as ArrayRef,
    ] {
        let kind = array.data_type().clone();
        let table = imported("TM", array).unwrap();
        assert_eq!(table.row_count(), 3, "{kind}");
        let ValueRef::Time(value) = table.cell(0, 0).unwrap() else {
            panic!("{kind}: a time")
        };
        assert_eq!(value.to_string(), "08:30:15", "{kind}");
        assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing, "{kind}");
        let ValueRef::Time(value) = table.cell(2, 0).unwrap() else {
            panic!("{kind}: a time")
        };
        assert_eq!(value.to_string(), "23:59:59", "{kind}");
    }
    // 2024-02-29T12:34:56 and 1970-01-01T00:00:00, in each unit's counts
    // from 1970-01-01T00:00:00.
    for array in [
        Arc::new(TimestampSecondArray::from(vec![
            Some(1_709_210_096),
            None,
            Some(0),
        ])) as ArrayRef,
        Arc::new(TimestampMillisecondArray::from(vec![
            Some(1_709_210_096_000),
            None,
            Some(0),
        ])) as ArrayRef,
        Arc::new(TimestampMicrosecondArray::from(vec![
            Some(1_709_210_096_000_000),
            None,
            Some(0),
        ])) as ArrayRef,
        Arc::new(TimestampNanosecondArray::from(vec![
            Some(1_709_210_096_000_000_000),
            None,
            Some(0),
        ])) as ArrayRef,
    ] {
        let kind = array.data_type().clone();
        let table = imported("DT", array).unwrap();
        assert_eq!(table.row_count(), 3, "{kind}");
        let ValueRef::DateTime(value) = table.cell(0, 0).unwrap() else {
            panic!("{kind}: a datetime")
        };
        assert_eq!(value.to_string(), "2024-02-29T12:34:56", "{kind}");
        assert_eq!(value.collected_precision(), Precision::Second, "{kind}");
        assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing, "{kind}");
        let ValueRef::DateTime(value) = table.cell(2, 0).unwrap() else {
            panic!("{kind}: a datetime")
        };
        assert_eq!(value.to_string(), "1970-01-01T00:00:00", "{kind}");
    }
    // The largest whole-second nanosecond count reaches 2262-04-11T23:47:16;
    // a nanosecond count cannot reach either end of the calendar.
    let end = Arc::new(TimestampNanosecondArray::from(vec![
        Some(9_223_372_036_000_000_000),
        None,
    ]));
    let table = imported("DT", end).unwrap();
    let ValueRef::DateTime(value) = table.cell(0, 0).unwrap() else {
        panic!("a datetime")
    };
    assert_eq!(value.to_string(), "2262-04-11T23:47:16");
    assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing);
    // One second before 1970-01-01T00:00:00 is a negative count in each unit.
    for array in [
        Arc::new(TimestampMillisecondArray::from(vec![-1_000])) as ArrayRef,
        Arc::new(TimestampMicrosecondArray::from(vec![-1_000_000])) as ArrayRef,
        Arc::new(TimestampNanosecondArray::from(vec![-1_000_000_000])) as ArrayRef,
    ] {
        let kind = array.data_type().clone();
        let table = imported("DT", array).unwrap();
        let ValueRef::DateTime(value) = table.cell(0, 0).unwrap() else {
            panic!("{kind}: a datetime")
        };
        assert_eq!(value.to_string(), "1969-12-31T23:59:59", "{kind}");
    }
}

/// A whole second outside the day, or outside the years 1 to 9999, is
/// refused at its row in a finer unit as it is in seconds.
#[test]
fn whole_seconds_outside_the_day_or_the_calendar_are_refused_in_finer_units() {
    for (array, row, cause) in [
        // 24:00:00, the first second after the day.
        (
            Arc::new(Time32MillisecondArray::from(vec![86_400_000, 0])) as ArrayRef,
            0,
            ValueCause::Day,
        ),
        // One second before midnight.
        (
            Arc::new(Time64MicrosecondArray::from(vec![0, -1_000_000])) as ArrayRef,
            1,
            ValueCause::Day,
        ),
        // 10000-01-01T00:00:00, the second after 9999-12-31T23:59:59, which
        // is 2_932_896 days and 86_399 seconds from 1970-01-01T00:00:00.
        (
            Arc::new(TimestampMillisecondArray::from(vec![
                253_402_300_800_000,
                0,
            ])) as ArrayRef,
            0,
            ValueCause::Calendar,
        ),
        // 0000-12-31T23:59:59, the second before 0001-01-01T00:00:00, which
        // is 719_162 days before 1970-01-01T00:00:00.
        (
            Arc::new(TimestampMicrosecondArray::from(vec![
                0,
                -62_135_596_801_000_000,
            ])) as ArrayRef,
            1,
            ValueCause::Calendar,
        ),
    ] {
        let kind = array.data_type().clone();
        assert_eq!(
            refused_column(imported("T", array).err()),
            Some(("T".into(), finer_unit(&kind).into(), Some((row + 1, cause)))),
            "{kind}"
        );
    }
}

/// A time or datetime count short of a whole second is refused at its row;
/// a null masks whatever its slot holds.
#[test]
fn temporal_counts_short_of_a_whole_second_are_refused_and_nulls_mask_payloads() {
    for (array, row) in [
        (
            Arc::new(Time32MillisecondArray::from(vec![
                Some(30_615_001),
                Some(0),
            ])) as ArrayRef,
            0,
        ),
        (
            Arc::new(Time64MicrosecondArray::from(vec![Some(0), Some(1)])) as ArrayRef,
            1,
        ),
        (
            Arc::new(Time64NanosecondArray::from(vec![Some(-1), Some(0)])) as ArrayRef,
            0,
        ),
        (
            Arc::new(TimestampMillisecondArray::from(vec![
                Some(1_709_210_096_500),
                Some(0),
            ])) as ArrayRef,
            0,
        ),
        (
            Arc::new(TimestampMicrosecondArray::from(vec![
                Some(0),
                Some(500_000),
            ])) as ArrayRef,
            1,
        ),
        (
            Arc::new(TimestampNanosecondArray::from(vec![
                Some(i64::MAX),
                Some(0),
            ])) as ArrayRef,
            0,
        ),
    ] {
        let kind = array.data_type().clone();
        assert_eq!(
            refused_column(imported("T", array).err()),
            Some((
                "T".into(),
                finer_unit(&kind).into(),
                Some((row + 1, ValueCause::Fraction))
            )),
            "{kind}"
        );
    }
    let masked = Time64MicrosecondArray::new(vec![1, 0].into(), Some(vec![false, true].into()));
    let table = imported("TM", Arc::new(masked)).unwrap();
    assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Missing);
    let ValueRef::Time(value) = table.cell(1, 0).unwrap() else {
        panic!("a time")
    };
    assert_eq!(value.to_string(), "00:00:00");
}

#[test]
fn nonfinite_missingness_empty_schema_and_the_row_limit_are_explicit() {
    let values = Arc::new(Float64Array::from(vec![
        Some(f64::NAN),
        Some(f64::INFINITY),
        Some(f64::NEG_INFINITY),
        Some(-0.0),
        None,
    ]));
    let table = imported("F", values).unwrap();
    for row in [0, 1, 2, 4] {
        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
    }
    let ValueRef::Float(value) = table.cell(3, 0).unwrap() else {
        panic!()
    };
    assert_eq!(value.get().to_bits(), 0.0f64.to_bits());
    let records = |rows| {
        let schema = Arc::new(Schema::empty());
        let batch = RecordBatch::try_new_with_options(
            schema.clone(),
            Vec::new(),
            &RecordBatchOptions::new().with_row_count(Some(rows)),
        )
        .unwrap();
        arrow_bridge::import_public_stream(arrow_bridge::export_stream(schema, vec![batch]))
    };
    let table = records(3).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.schema().columns().len(), 0);
    // Past the former 10,000,000-row limit (#4773).
    assert_eq!(records(10_000_001).unwrap().row_count(), 10_000_001);
}

/// A batch without columns claims its records without holding them, so the
/// batches of one stream may claim more together than the platform counts
/// (#4782). The import refuses that stream, where it once saturated the
/// count; it keeps any count the batches do add up to, which nothing is
/// reserved for until a run reads the table.
#[test]
fn a_stream_without_columns_that_claims_more_records_than_are_counted_is_refused() {
    // The most records one batch of a C stream claims: a signed 64-bit
    // length. Two of them are counted, and three are not.
    let most = usize::try_from(i64::MAX).unwrap();
    let schema = Arc::new(Schema::empty());
    let claimed = |batches: usize| {
        let options = RecordBatchOptions::new().with_row_count(Some(most));
        let batch = RecordBatch::try_new_with_options(schema.clone(), Vec::new(), &options);
        let batches = vec![batch.unwrap(); batches];
        arrow_bridge::import_public_stream(arrow_bridge::export_stream(schema.clone(), batches))
            .map(|table| (table.row_count(), table.schema().columns().len()))
    };
    for (batches, records) in [(1, most), (2, most * 2)] {
        assert_eq!(claimed(batches).unwrap(), (records, 0), "{batches}");
    }
    assert!(
        matches!(
            claimed(3),
            Err(TableError::Representation {
                representation: "record count in the platform's unsigned word"
            })
        ),
        "three batches"
    );
}

#[test]
fn public_temporals_preserve_existing_epoch_view_without_mutating_collected_precision() {
    let logical = TableSchema::new(vec![
        Column {
            name: "D".into(),
            kind: ColumnType::Date,
        },
        Column {
            name: "T".into(),
            kind: ColumnType::DateTime,
        },
    ])
    .unwrap();
    let date = Date::new(2024, 2, 29, Precision::Year).unwrap();
    let datetime = DateTime::new(date, 12, 34, 56, Precision::Day).unwrap();
    let table = ColumnTable::new(
        logical,
        vec![
            Values::Date(vec![Some(date), None]),
            Values::DateTime(vec![Some(datetime), None]),
        ],
        2,
    )
    .unwrap();
    let public = PublicTable::from_table(&table, &[0, 1]).unwrap();
    assert_eq!(public.schema().field(0).data_type(), &DataType::Date32);
    assert_eq!(
        public.schema().field(1).data_type(),
        &DataType::Timestamp(TimeUnit::Second, None)
    );
    let mut reader = ArrowArrayStreamReader::try_new(public.stream()).unwrap();
    let batch = reader.next().unwrap().unwrap();
    assert_eq!(
        batch
            .column(0)
            .as_any()
            .downcast_ref::<Date32Array>()
            .unwrap()
            .value(0),
        19_782
    );
    assert_eq!(
        batch
            .column(1)
            .as_any()
            .downcast_ref::<TimestampSecondArray>()
            .unwrap()
            .value(0),
        1_709_210_096
    );
    assert!(batch.column(0).is_null(1));
    assert!(batch.column(1).is_null(1));
    let ValueRef::Date(value) = table.cell(0, 0).unwrap() else {
        panic!()
    };
    assert_eq!(value.collected_precision(), Precision::Year);
    let ValueRef::DateTime(value) = table.cell(0, 1).unwrap() else {
        panic!()
    };
    assert_eq!(value.collected_precision(), Precision::Day);
}

/// A raw C stream producer for inputs that arrow-rs's own producer cannot
/// emit: arrays with under-aligned buffers, a NULL error message, and a
/// stream that drops its `get_next` callback after reporting its schema.
mod raw {
    use arrow_array::{
        ffi::{FFI_ArrowArray, FFI_ArrowSchema},
        ffi_stream::FFI_ArrowArrayStream,
    };
    use arrow_schema::Schema;
    use std::ffi::{c_char, c_int, c_void};

    pub enum Behaviour {
        Yield(Vec<FFI_ArrowArray>),
        Fail,
        DropNext,
        /// Report a schema that counts its children but gives no pointer to
        /// them; the producer's release frees them from its own copy.
        LoseChildren,
    }

    /// The C Data interface's `struct ArrowSchema`, up to its children.
    #[repr(C)]
    struct SchemaHeader {
        _format: *const c_char,
        _name: *const c_char,
        _metadata: *const c_char,
        _flags: i64,
        _n_children: i64,
        children: *mut *mut c_void,
    }

    struct Producer {
        schema: Schema,
        behaviour: Behaviour,
    }

    unsafe extern "C" fn get_schema(
        stream: *mut FFI_ArrowArrayStream,
        out: *mut FFI_ArrowSchema,
    ) -> c_int {
        let producer = unsafe { &*((*stream).private_data as *const Producer) };
        let schema = FFI_ArrowSchema::try_from(&producer.schema).unwrap();
        unsafe { std::ptr::write(out, schema) };
        if matches!(producer.behaviour, Behaviour::DropNext) {
            unsafe { (*stream).get_next = None };
        }
        if matches!(producer.behaviour, Behaviour::LoseChildren) {
            unsafe { (*out.cast::<SchemaHeader>()).children = std::ptr::null_mut() };
        }
        0
    }

    unsafe extern "C" fn get_next(
        stream: *mut FFI_ArrowArrayStream,
        out: *mut FFI_ArrowArray,
    ) -> c_int {
        let producer = unsafe { &mut *((*stream).private_data as *mut Producer) };
        let Behaviour::Yield(arrays) = &mut producer.behaviour else {
            return 5;
        };
        let next = if arrays.is_empty() {
            FFI_ArrowArray::empty()
        } else {
            arrays.remove(0)
        };
        unsafe { std::ptr::write(out, next) };
        0
    }

    unsafe extern "C" fn no_message(_: *mut FFI_ArrowArrayStream) -> *const c_char {
        std::ptr::null()
    }

    unsafe extern "C" fn release(stream: *mut FFI_ArrowArrayStream) {
        drop(unsafe { Box::from_raw((*stream).private_data as *mut Producer) });
        unsafe { (*stream).release = None };
    }

    pub fn stream(schema: Schema, behaviour: Behaviour) -> FFI_ArrowArrayStream {
        let mut stream = FFI_ArrowArrayStream::empty();
        stream.get_schema = Some(get_schema);
        stream.get_next = Some(get_next);
        stream.get_last_error = Some(no_message);
        stream.release = Some(release);
        stream.private_data =
            Box::into_raw(Box::new(Producer { schema, behaviour })) as *mut c_void;
        stream
    }
}

/// Copy `bytes` to an address `offset` bytes past a 64-byte boundary.
fn displaced(bytes: &[u8], offset: usize) -> arrow_buffer::Buffer {
    let mut buffer = arrow_buffer::MutableBuffer::from_len_zeroed(offset + bytes.len());
    assert_eq!(buffer.as_ptr().align_offset(64), 0);
    buffer.as_slice_mut()[offset..].copy_from_slice(bytes);
    arrow_buffer::Buffer::from(buffer).slice_with_length(offset, bytes.len())
}

#[test]
fn under_aligned_producer_buffers_import_unchanged() {
    // The C Data interface only recommends alignment. Int64 values one or four
    // bytes past an 8-byte boundary, and string views eight bytes past a
    // 16-byte boundary, import with their values intact.
    let integers = Int64Array::from(vec![Some(i64::MIN), None, Some(-1), Some(i64::MAX)]);
    let long = "a string view longer than twelve bytes";
    let views = StringViewArray::from(vec![Some("short"), None, Some(long), Some("")]);
    let fields = vec![
        Field::new("I", DataType::Int64, true),
        Field::new("S", DataType::Utf8View, true),
    ];
    for (integer_offset, view_offset) in [(1, 8), (4, 8), (0, 0)] {
        let integers = integers.to_data();
        let integers = unsafe {
            integers
                .clone()
                .into_builder()
                .buffers(vec![displaced(
                    integers.buffers()[0].as_slice(),
                    integer_offset,
                )])
                .build_unchecked()
        };
        let views = views.to_data();
        let mut buffers = views.buffers().to_vec();
        buffers[0] = displaced(views.buffers()[0].as_slice(), view_offset);
        let views = unsafe {
            views
                .clone()
                .into_builder()
                .buffers(buffers)
                .build_unchecked()
        };
        let parent = arrow_array::StructArray::from(vec![
            (
                Arc::new(fields[0].clone()),
                Arc::new(Int64Array::from(vec![0; 4])) as ArrayRef,
            ),
            (
                Arc::new(fields[1].clone()),
                Arc::new(StringViewArray::from(vec![""; 4])) as ArrayRef,
            ),
        ])
        .to_data();
        let parent = unsafe {
            parent
                .into_builder()
                .child_data(vec![integers, views])
                .build_unchecked()
        };
        let source = raw::stream(
            Schema::new(fields.clone()),
            raw::Behaviour::Yield(vec![arrow_array::ffi::FFI_ArrowArray::new(&parent)]),
        );
        let table = arrow_bridge::import_public_stream(source).unwrap();
        assert_eq!(table.row_count(), 4);
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(i64::MIN));
        assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing);
        assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Int(-1));
        assert_eq!(table.cell(3, 0).unwrap(), ValueRef::Int(i64::MAX));
        assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str("short"));
        assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Missing);
        assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Str(long));
        assert_eq!(table.cell(3, 1).unwrap(), ValueRef::Str(""));
    }
}

#[test]
fn a_producer_failure_with_a_null_message_is_an_error() {
    let schema = Schema::new(vec![Field::new("I", DataType::Int64, true)]);
    let error = arrow_bridge::import_public_stream(raw::stream(schema, raw::Behaviour::Fail))
        .err()
        .unwrap();
    let text = format!("{error:?}");
    assert!(text.contains("Error code: 5"), "{text}");
}

#[test]
fn a_stream_that_drops_its_next_callback_is_an_error_not_an_end() {
    // The callback disappears after the schema is reported, so the import
    // must refuse the stream instead of reading it as empty.
    let schema = Schema::new(vec![Field::new("I", DataType::Int64, true)]);
    let error = arrow_bridge::import_public_stream(raw::stream(schema, raw::Behaviour::DropNext))
        .err()
        .unwrap();
    let text = format!("{error:?}");
    assert!(text.contains("lost its get_next callback"), "{text}");
}

/// A one-column result table of `array`, restored through a C stream.
fn restored(array: ArrayRef) -> Result<PublicTable, ResultError> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "S",
        array.data_type().clone(),
        true,
    )]));
    let batch = RecordBatch::try_new(schema.clone(), vec![array]).unwrap();
    arrow_bridge::import_result_stream(arrow_bridge::export_stream(schema, vec![batch]))
}

/// A restored result table holds text whose bytes are UTF-8 only, the input
/// frame's judgement: a value that is not refuses the stream, at the top
/// level or as a list's items, and a missing value's bytes are not read.
#[test]
fn result_imports_refuse_text_that_is_not_utf8_and_nulls_mask_payloads() {
    let latin1: &[u8] = b"caf\xe9";
    let text = unchecked_text(&DataType::Utf8, &[Some(b"A".as_slice()), Some(latin1)]);
    let item = Arc::new(Field::new("item", DataType::Utf8, true));
    let list: ArrayRef = Arc::new(ListArray::new(
        item,
        OffsetBuffer::from_lengths([2]),
        text.clone(),
        None,
    ));
    for (label, array) in [("a column", &text), ("a list's items", &list)] {
        assert_eq!(
            restored(array.clone()),
            Err(ResultError::InvalidTable),
            "text that is not UTF-8 as {label}"
        );
    }
    let masked = make_array(unsafe {
        // SAFETY: a validity buffer of the array's length over valid
        // offsets; it hides the value that is not UTF-8.
        text.to_data()
            .into_builder()
            .nulls(Some(vec![true, false].into()))
            .build_unchecked()
    });
    assert_eq!(
        restored(masked),
        Ok(PublicTable::new(vec![(
            "S".into(),
            PublicColumn::Str([Some("A"), None].into_iter().collect())
        )])
        .unwrap()),
        "a missing value's bytes"
    );
}

/// A result table restored from a foreign stream reads it as an input frame
/// does: a stream without a callback, one that loses `get_next`, and a
/// failure with no message or no `get_last_error` are refused, where Arrow
/// 55's own reader unwrapped each callback.
#[test]
fn result_imports_refuse_streams_without_callbacks_or_error_messages() {
    type Change = fn(&mut FFI_ArrowArrayStream);
    let cases: [(&str, raw::Behaviour, Change); 5] = [
        (
            "no get_schema",
            raw::Behaviour::Yield(Vec::new()),
            |stream| stream.get_schema = None,
        ),
        ("no get_next", raw::Behaviour::Yield(Vec::new()), |stream| {
            stream.get_next = None
        }),
        ("a lost get_next", raw::Behaviour::DropNext, |_| {}),
        (
            "a failure with a null message",
            raw::Behaviour::Fail,
            |_| {},
        ),
        (
            "a failure without get_last_error",
            raw::Behaviour::Fail,
            |stream| stream.get_last_error = None,
        ),
    ];
    for (label, behaviour, change) in cases {
        let schema = Schema::new(vec![Field::new("I", DataType::Int64, true)]);
        let mut source = raw::stream(schema, behaviour);
        change(&mut source);
        assert_eq!(
            arrow_bridge::import_result_stream(source),
            Err(ResultError::InvalidTable),
            "{label}"
        );
    }
}

/// A batch's struct array is no record: Arrow 55's own reader panicked on
/// its validity, and a restored result table takes its columns as they hold
/// (`storage_api::StreamReader`, as for an input frame).
#[test]
fn result_imports_take_the_columns_of_a_batch_whose_struct_has_nulls() {
    let field = Arc::new(Field::new("I", DataType::Int64, true));
    let parent = arrow_array::StructArray::try_new(
        vec![field.clone()].into(),
        vec![Arc::new(Int64Array::from(vec![Some(1), Some(2)])) as ArrayRef],
        Some(vec![true, false].into()),
    )
    .unwrap();
    let source = raw::stream(
        Schema::new(vec![field.as_ref().clone()]),
        raw::Behaviour::Yield(vec![arrow_array::ffi::FFI_ArrowArray::new(
            &parent.to_data(),
        )]),
    );
    assert_eq!(
        arrow_bridge::import_result_stream(source),
        Ok(PublicTable::new(vec![(
            "I".into(),
            PublicColumn::Int([Some(1), Some(2)].into_iter().collect())
        )])
        .unwrap())
    );
}

/// A raw stream of one batch of `columns`, the first buffer of each, its
/// values or its offsets, moved `offset` bytes past a 64-byte boundary.
fn displaced_stream(columns: &[(&str, &dyn Array)], offset: usize) -> FFI_ArrowArrayStream {
    let fields = columns
        .iter()
        .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
        .collect::<Vec<_>>();
    let children = columns
        .iter()
        .map(|(_, array)| {
            let data = array.to_data();
            let mut buffers = data.buffers().to_vec();
            buffers[0] = displaced(buffers[0].as_slice(), offset);
            // SAFETY: the same bytes at another address; only their
            // alignment changes, which this test supplies on purpose.
            unsafe { data.into_builder().buffers(buffers).build_unchecked() }
        })
        .collect::<Vec<_>>();
    let parent = arrow_array::StructArray::from(
        columns
            .iter()
            .zip(&fields)
            .map(|((_, array), field)| (Arc::new(field.clone()), make_array(array.to_data())))
            .collect::<Vec<_>>(),
    )
    .to_data();
    // SAFETY: the children have the parent's length and fields' types.
    let parent = unsafe { parent.into_builder().child_data(children).build_unchecked() };
    raw::stream(
        Schema::new(fields),
        raw::Behaviour::Yield(vec![arrow_array::ffi::FFI_ArrowArray::new(&parent)]),
    )
}

/// The C Data interface only recommends alignment: Int64, Float64 and
/// Date32 values and Utf8 offsets one, two or four bytes past a 64-byte
/// boundary restore a result table with their values intact. Arrow 55's
/// import reads the last offset to size the text, so an under-aligned
/// offsets buffer is copied before the import reads it.
#[test]
fn under_aligned_result_buffers_restore_unchanged() {
    let integers = Int64Array::from(vec![Some(i64::MIN), None, Some(-1), Some(i64::MAX)]);
    let floats = Float64Array::from(vec![Some(-1.5), None, Some(f64::MAX), Some(0.0)]);
    let days = Date32Array::from(vec![Some(-719_162), None, Some(0), Some(2_932_896)]);
    let text = StringArray::from(vec![Some("a"), None, Some("\u{03b2}eta"), Some("")]);
    let expected = PublicTable::new(vec![
        ("I".into(), PublicColumn::Int(integers.iter().collect())),
        ("F".into(), PublicColumn::Float(floats.iter().collect())),
        ("D".into(), PublicColumn::Date(days.iter().collect())),
        ("S".into(), PublicColumn::Str(text.iter().collect())),
    ])
    .unwrap();
    for offset in [1, 2, 4, 0] {
        let columns: [(&str, &dyn Array); 4] =
            [("I", &integers), ("F", &floats), ("D", &days), ("S", &text)];
        assert_eq!(
            arrow_bridge::import_result_stream(displaced_stream(&columns, offset)).as_ref(),
            Ok(&expected),
            "buffers {offset} bytes past a 64-byte boundary"
        );
    }
}

/// An input frame's Utf8 and LargeUtf8 offsets one, two or four bytes past
/// a 64-byte boundary import with their values intact: the import reads
/// each array's last offset, which the reader gives it aligned.
#[test]
fn under_aligned_text_offsets_import_unchanged() {
    let text = StringArray::from(vec![Some("a"), None, Some("\u{03b2}eta")]);
    let large = LargeStringArray::from(vec![Some(""), Some("large"), None]);
    for offset in [1, 2, 4, 0] {
        let columns: [(&str, &dyn Array); 2] = [("S", &text), ("L", &large)];
        let table = arrow_bridge::import_public_stream(displaced_stream(&columns, offset)).unwrap();
        assert_eq!(
            (0..3)
                .map(|row| (table.cell(row, 0).unwrap(), table.cell(row, 1).unwrap()))
                .collect::<Vec<_>>(),
            [
                (ValueRef::Str("a"), ValueRef::Str("")),
                (ValueRef::Missing, ValueRef::Str("large")),
                (ValueRef::Str("\u{03b2}eta"), ValueRef::Missing),
            ],
            "offsets {offset} bytes past a 64-byte boundary"
        );
    }
}

/// A column nests types at most `NESTING_DEPTH` deep, each list's items one
/// level below it: one deeper is refused before any batch is read, by a
/// result table and an input frame alike, so no walk of a schema, Arrow's
/// or the engine's, recurses past it.
#[test]
fn schemas_nested_past_the_documented_depth_are_refused() {
    let nested = |depth| {
        (0..depth).fold(DataType::Int64, |items, _| {
            DataType::List(Arc::new(Field::new("item", items, true)))
        })
    };
    let schema = |depth| Arc::new(Schema::new(vec![Field::new("L", nested(depth), true)]));
    let limit = arrow_bridge::NESTING_DEPTH;
    for (depth, admitted) in [(1, true), (limit, true), (limit + 1, false)] {
        let restored =
            arrow_bridge::import_result_stream(arrow_bridge::export_stream(schema(depth), vec![]));
        assert_eq!(
            restored.map(|table| table.schema().clone()),
            if admitted {
                Ok(schema(depth))
            } else {
                Err(ResultError::InvalidTable)
            },
            "a result table's lists {depth} deep"
        );
    }
    let refusal =
        format!("input schema nests deeper than {limit} below a column or lacks a child it counts");
    let message = |error: Option<TableError>| match error {
        Some(TableError::Arrow(ArrowError::CDataInterface(message))) => Some(message),
        _ => None,
    };
    let deep = arrow_bridge::export_stream(schema(limit + 1), vec![]);
    assert_eq!(
        message(arrow_bridge::import_public_stream(deep).err()),
        Some(refusal.clone()),
        "an input frame's lists {} deep",
        limit + 1
    );
    // A schema that counts a child but gives no pointer to it, which Arrow
    // 55's schema import unwrapped, is refused by the same walk.
    let lost = || {
        let schema = Schema::new(vec![Field::new("I", DataType::Int64, true)]);
        raw::stream(schema, raw::Behaviour::LoseChildren)
    };
    assert_eq!(
        message(arrow_bridge::import_public_stream(lost()).err()),
        Some(refusal),
        "an input frame's schema without its children"
    );
    assert_eq!(
        arrow_bridge::import_result_stream(lost()),
        Err(ResultError::InvalidTable),
        "a result table's schema without its children"
    );
}

/// The import reads an under-aligned copy of a text column's offsets, and
/// the producer's release sees the header as it exported it: the child's
/// offsets pointer is its own misaligned one again.
#[test]
fn a_copied_offsets_buffer_is_the_producers_own_again_at_release() {
    let text = StringArray::from(vec![Some("a"), None, Some("\u{03b2}eta")]);
    let field = Field::new("S", DataType::Utf8, true);
    let data = text.to_data();
    let offsets = displaced(data.buffers()[0].as_slice(), 1);
    let misaligned = offsets.as_ptr() as usize;
    let mut buffers = data.buffers().to_vec();
    buffers[0] = offsets;
    // SAFETY: the same bytes at another address; only their alignment
    // changes, which this test supplies on purpose.
    let child = unsafe { data.into_builder().buffers(buffers).build_unchecked() };
    let parent =
        arrow_array::StructArray::from(vec![(Arc::new(field.clone()), make_array(text.to_data()))])
            .to_data();
    // SAFETY: the child has the parent's length and field's type.
    let parent = unsafe {
        parent
            .into_builder()
            .child_data(vec![child])
            .build_unchecked()
    };
    let (array, seen) = watched::child_buffers(arrow_array::ffi::FFI_ArrowArray::new(&parent), 0);
    let source = raw::stream(Schema::new(vec![field]), raw::Behaviour::Yield(vec![array]));
    let table = arrow_bridge::import_public_stream(source).unwrap();
    assert_eq!(
        (
            (0..3)
                .map(|row| table.cell(row, 0).unwrap())
                .collect::<Vec<_>>(),
            seen.second.load(Ordering::SeqCst),
        ),
        (
            vec![
                ValueRef::Str("a"),
                ValueRef::Missing,
                ValueRef::Str("\u{03b2}eta")
            ],
            misaligned,
        )
    );
}
