//! Independently authored C Data/Stream values and ownership controls.
use arrow_array::{
    new_empty_array, Array, ArrayRef, Date32Array, Float64Array, Int64Array, LargeStringArray,
    RecordBatch, RecordBatchOptions, RecordBatchReader, StringArray, StringViewArray,
    TimestampSecondArray,
};
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
    public_table::PublicTable,
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
            Some(-0.0),
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

#[test]
fn all_logical_values_nulls_slices_and_empty_chunks_share_the_original_buffers() {
    let original = truth();
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
    let all_pointers = pointers(&original);
    let chunks = vec![
        original.slice(0, 0),
        original.slice(0, 2),
        original.slice(2, 0),
        original.slice(2, 2),
        original.slice(4, 0),
    ];
    let integer_pointer = original.column(1).to_data().buffers()[0].as_ptr();
    let text_pointer = original.column(0).to_data().buffers()[1].as_ptr();
    let (source, dropped) = stream(original.schema(), chunks.into_iter().map(Ok).collect());
    drop(original);
    let table = arrow_bridge::import_public_stream(source).unwrap();
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(table.row_count(), 4);
    assert_eq!(
        table
            .batches()
            .iter()
            .map(RecordBatch::num_rows)
            .collect::<Vec<_>>(),
        [0, 2, 0, 2, 0]
    );
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
    assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
    let ValueRef::Float(value) = table.cell(2, 2).unwrap() else {
        panic!()
    };
    assert_eq!(value.get().to_bits(), 1);
    let ValueRef::Date(value) = table.cell(0, 3).unwrap() else {
        panic!()
    };
    assert_eq!(value.to_string(), "0001-01-01");
    let ValueRef::DateTime(value) = table.cell(3, 4).unwrap() else {
        panic!()
    };
    assert_eq!(value.to_string(), "9999-12-31T23:59:59");
    let public = PublicTable::from_snapshot(&table).unwrap();
    drop(table);
    for _ in 0..100 {
        let mut reader = ArrowArrayStreamReader::try_new(public.stream()).unwrap();
        let _empty = reader.next().unwrap().unwrap();
        let first = reader.next().unwrap().unwrap();
        // Releasing the reader early leaves imported C Data arrays alive.
        drop(reader);
        assert_eq!(pointers(&first), all_pointers);
        assert_eq!(
            first.column(1).to_data().buffers()[0].as_ptr(),
            integer_pointer
        );
        assert_eq!(
            first.column(0).to_data().buffers()[1].as_ptr(),
            text_pointer
        );
        assert_eq!(
            first
                .column(3)
                .as_any()
                .downcast_ref::<Date32Array>()
                .unwrap()
                .value(0),
            -719_162
        );
        assert!(first.column(3).is_null(1));
    }
}

#[test]
fn large_and_view_utf8_share_payloads_across_foreign_stream_import_and_export() {
    let text = "a\0\u{1f980} long UTF-8 payload that cannot fit in an inline string view";
    for array in [
        Arc::new(LargeStringArray::from(vec![Some(text), None, Some("")])) as ArrayRef,
        Arc::new(StringViewArray::from(vec![Some(text), None, Some("")])) as ArrayRef,
    ] {
        let pointer = if let Some(array) = array.as_any().downcast_ref::<LargeStringArray>() {
            array.value(0).as_ptr()
        } else {
            array
                .as_any()
                .downcast_ref::<StringViewArray>()
                .unwrap()
                .value(0)
                .as_ptr()
        };
        let schema = Arc::new(Schema::new(vec![Field::new(
            "S",
            array.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema.clone(), vec![array]).unwrap();
        let table =
            arrow_bridge::import_public_stream(arrow_bridge::export_stream(schema, vec![batch]))
                .unwrap();
        let ValueRef::Str(value) = table.cell(0, 0).unwrap() else {
            panic!()
        };
        assert_eq!(value, text);
        assert_eq!(value.as_ptr(), pointer);
        assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing);
        assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Str(""));
        let public = PublicTable::from_snapshot(&table).unwrap();
        let imported = arrow_bridge::import_public_stream(public.stream()).unwrap();
        drop(table);
        drop(public);
        let ValueRef::Str(value) = imported.cell(0, 0).unwrap() else {
            panic!()
        };
        assert_eq!(value.as_ptr(), pointer);
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

#[test]
fn incapable_physical_types_are_refused_and_release_the_input() {
    for kind in [
        DataType::Int32,
        DataType::UInt64,
        DataType::Boolean,
        DataType::Timestamp(TimeUnit::Millisecond, None),
        DataType::Timestamp(TimeUnit::Second, Some("UTC".into())),
    ] {
        let schema = Arc::new(Schema::new(vec![Field::new("X", kind.clone(), true)]));
        let batch = RecordBatch::try_new(schema.clone(), vec![new_empty_array(&kind)]).unwrap();
        let (source, dropped) = stream(schema, vec![Ok(batch)]);
        assert!(arrow_bridge::import_public_stream(source).is_err());
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn nonfinite_missingness_and_empty_schema_are_explicit() {
    let values = Arc::new(Float64Array::from(vec![
        Some(f64::NAN),
        Some(f64::INFINITY),
        Some(f64::NEG_INFINITY),
        Some(-0.0),
        None,
    ]));
    let schema = Arc::new(Schema::new(vec![Field::new("F", DataType::Float64, true)]));
    let batch = RecordBatch::try_new(schema.clone(), vec![values]).unwrap();
    let table =
        arrow_bridge::import_public_stream(arrow_bridge::export_stream(schema, vec![batch]))
            .unwrap();
    for row in [0, 1, 2, 4] {
        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
    }
    let ValueRef::Float(value) = table.cell(3, 0).unwrap() else {
        panic!()
    };
    assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
    let schema = Arc::new(Schema::empty());
    let batch = RecordBatch::try_new_with_options(
        schema.clone(),
        Vec::new(),
        &RecordBatchOptions::new().with_row_count(Some(3)),
    )
    .unwrap();
    let table =
        arrow_bridge::import_public_stream(arrow_bridge::export_stream(schema, vec![batch]))
            .unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.schema().columns().len(), 0);
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
    let batch = RecordBatch::try_new(
        yamaa_adapters::arrow_table::physical_schema(&logical),
        vec![
            Arc::new(yamaa_adapters::arrow_table::date_array(&[Some(date), None]).unwrap()),
            Arc::new(yamaa_adapters::arrow_table::datetime_array(&[Some(datetime), None]).unwrap()),
        ],
    )
    .unwrap();
    let table = yamaa_adapters::arrow_table::ArrowTable::try_new(logical, vec![batch]).unwrap();
    let public = PublicTable::from_snapshot(&table).unwrap();
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
