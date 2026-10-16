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
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
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
    let date = Date::new(2024, 2, 29, DatePrecision::Year).unwrap();
    let datetime = DateTime::new(date, 12, 34, 56, DateTimePrecision::Day).unwrap();
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
    assert_eq!(value.collected_precision(), DatePrecision::Year);
    let ValueRef::DateTime(value) = table.cell(0, 1).unwrap() else {
        panic!()
    };
    assert_eq!(value.collected_precision(), DateTimePrecision::Day);
}
