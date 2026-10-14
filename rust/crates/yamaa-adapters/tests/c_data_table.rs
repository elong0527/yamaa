#![cfg(feature = "arrow-c-data")]
use arrow_array::{
    Array, ArrayRef, Float64Array, Int32Array, Int64Array, Int8Array, LargeStringArray,
    RecordBatch, RecordBatchIterator, RecordBatchReader, StringArray, StringViewArray,
    TimestampMicrosecondArray,
};
use arrow_buffer::ScalarBuffer;
use arrow_schema::{ArrowError, DataType, Field, Schema, SchemaRef};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use yamaa_adapters::{
    c_data_table::{self, Error, FFI_ArrowArrayStream, ImportBudget},
    specification_run::PreparedRun,
    specification_source::Source,
};
use yamaa_core::table::{TableAccess, ValueRef};

fn batch(arrays: Vec<(&str, ArrayRef)>) -> RecordBatch {
    let schema = Arc::new(Schema::new(
        arrays
            .iter()
            .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
            .collect::<Vec<_>>(),
    ));
    RecordBatch::try_new(schema, arrays.into_iter().map(|(_, array)| array).collect()).unwrap()
}
fn stream(batches: Vec<RecordBatch>, schema: SchemaRef) -> FFI_ArrowArrayStream {
    FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(
        batches.into_iter().map(Ok),
        schema,
    )))
}
fn import(batch: RecordBatch) -> Result<yamaa_adapters::arrow_table::ArrowTable, Error> {
    let stream = stream(vec![batch.clone()], batch.schema());
    // SAFETY: arrow-rs exports valid, immutable arrays and owns the callbacks.
    unsafe { ImportBudget::default().import(stream) }
}

#[test]
fn primitive_and_all_string_layouts_share_buffers_and_outlive_the_producer() {
    for strings in [
        Arc::new(StringArray::from(vec![
            Some("long text that lives in a buffer"),
            None,
            Some("β"),
        ])) as ArrayRef,
        Arc::new(LargeStringArray::from(vec![
            Some("long text that lives in a buffer"),
            None,
            Some("β"),
        ])),
        Arc::new(StringViewArray::from(vec![
            Some("long text that lives in a buffer"),
            None,
            Some("β"),
        ])),
    ] {
        let integers = Arc::new(Int64Array::from(vec![Some(i64::MIN), None, Some(i64::MAX)]));
        let integer_pointer = integers.values().as_ptr();
        let source = batch(vec![("S", strings), ("I", integers)]);
        let text_pointer = match source.column(0).data_type() {
            DataType::Utf8 => source
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0)
                .as_ptr(),
            DataType::LargeUtf8 => source
                .column(0)
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .unwrap()
                .value(0)
                .as_ptr(),
            _ => source
                .column(0)
                .as_any()
                .downcast_ref::<StringViewArray>()
                .unwrap()
                .value(0)
                .as_ptr(),
        };
        let table = import(source.slice(0, 2)).unwrap();
        drop(source);
        let ValueRef::Str(text) = table.cell(0, 0).unwrap() else {
            panic!("text")
        };
        assert_eq!(text.as_ptr(), text_pointer);
        assert_eq!(
            table.batches()[0]
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .values()
                .as_ptr(),
            integer_pointer
        );
        assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(i64::MIN));
        assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing);
    }
}

#[test]
fn chunks_empty_batches_and_schema_only_tables_retain_order_and_shape() {
    let first = batch(vec![("I", Arc::new(Int64Array::from(vec![1, 2])))]);
    let second = batch(vec![("I", Arc::new(Int64Array::from(vec![3])))]);
    let input = stream(
        vec![first.slice(0, 0), first.clone(), second],
        first.schema(),
    );
    let table = unsafe { ImportBudget::default().import(input) }.unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.batches().len(), 3);
    assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Int(3));
    let empty = unsafe { ImportBudget::default().import(stream(vec![], first.schema())) }.unwrap();
    assert_eq!(empty.row_count(), 0);
    assert_eq!(empty.schema(), table.schema());
}

#[test]
fn numeric_widening_nonfinite_and_temporal_conversion_preserve_values() {
    let table = import(batch(vec![
        ("I", Arc::new(Int32Array::from(vec![Some(i32::MIN), None]))),
        (
            "F",
            Arc::new(Float64Array::from(vec![Some(-0.0), Some(f64::NAN)])),
        ),
        (
            "T",
            Arc::new(TimestampMicrosecondArray::from(vec![
                Some(-1_000_000),
                None,
            ])),
        ),
    ]))
    .unwrap();
    assert_eq!(
        table.cell(0, 0).unwrap(),
        ValueRef::Int(i64::from(i32::MIN))
    );
    let ValueRef::Float(value) = table.cell(0, 1).unwrap() else {
        panic!("float")
    };
    assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
    assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Missing);
    let ValueRef::DateTime(value) = table.cell(0, 2).unwrap() else {
        panic!("datetime")
    };
    assert_eq!(value.fields(), (1969, 12, 31, 23, 59, 59));
    assert!(matches!(
        import(batch(vec![(
            "T",
            Arc::new(TimestampMicrosecondArray::from(vec![1]))
        )])),
        Err(Error::Value)
    ));
}

struct CountedReader {
    schema: SchemaRef,
    batch: Option<RecordBatch>,
    releases: Arc<AtomicUsize>,
    reads: Arc<AtomicUsize>,
    repeat: bool,
}
impl Iterator for CountedReader {
    type Item = Result<RecordBatch, ArrowError>;
    fn next(&mut self) -> Option<Self::Item> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.repeat {
            self.batch.clone().map(Ok)
        } else {
            self.batch.take().map(Ok)
        }
    }
}
impl RecordBatchReader for CountedReader {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}
impl Drop for CountedReader {
    fn drop(&mut self) {
        self.releases.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn rejection_releases_once_and_empty_streams_cannot_evade_the_batch_limit() {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "BOOL",
        DataType::Boolean,
        true,
    )]));
    let releases = Arc::new(AtomicUsize::new(0));
    let reads = Arc::new(AtomicUsize::new(0));
    let input = FFI_ArrowArrayStream::new(Box::new(CountedReader {
        schema,
        batch: None,
        releases: releases.clone(),
        reads: reads.clone(),
        repeat: false,
    }));
    assert!(matches!(
        unsafe { ImportBudget::default().import(input) },
        Err(Error::Schema)
    ));
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    let empty = batch(vec![("I", Arc::new(Int64Array::from(Vec::<i64>::new())))]);
    let input = FFI_ArrowArrayStream::new(Box::new(CountedReader {
        schema: empty.schema(),
        batch: Some(empty),
        releases: releases.clone(),
        reads: reads.clone(),
        repeat: true,
    }));
    assert!(matches!(
        unsafe { ImportBudget::default().import(input) },
        Err(Error::Limit)
    ));
    assert_eq!(releases.load(Ordering::SeqCst), 2);
    assert_eq!(reads.load(Ordering::SeqCst), 257);
    assert!(matches!(
        unsafe { ImportBudget::default().import(FFI_ArrowArrayStream::empty()) },
        Err(Error::Stream)
    ));
}

#[test]
fn aliased_text_and_multiple_sources_use_cumulative_limits() {
    let values = StringViewArray::from(vec!["x".repeat(128 * 1024)]);
    let (views, buffers, _) = values.into_parts();
    let values = StringViewArray::new(ScalarBuffer::from(vec![views[0]; 129]), buffers, None);
    assert!(matches!(
        import(batch(vec![("S", Arc::new(values))])),
        Err(Error::Limit)
    ));
    let empty = batch(vec![("I", Arc::new(Int64Array::from(Vec::<i64>::new())))]);
    let mut budget = ImportBudget::default();
    unsafe { budget.import(stream(vec![empty.clone(); 128], empty.schema())) }.unwrap();
    assert!(matches!(
        unsafe { budget.import(stream(vec![empty.clone(); 129], empty.schema())) },
        Err(Error::Limit)
    ));
}

#[test]
fn cumulative_rows_and_cells_accept_exact_limits_and_keep_prior_source_ownership() {
    for accepted_rows in [
        c_data_table::LIMITS.max_rows - 1,
        c_data_table::LIMITS.max_rows,
    ] {
        let source = batch(vec![(
            "I",
            Arc::new(Int64Array::from(vec![7; accepted_rows])),
        )]);
        let mut budget = ImportBudget::default();
        let held = unsafe { budget.import(stream(vec![source.clone()], source.schema())) }.unwrap();
        let additional = batch(vec![("I", Arc::new(Int64Array::from(vec![9])))]);
        let accepted =
            unsafe { budget.import(stream(vec![additional.clone()], additional.schema())) };
        assert_eq!(
            accepted.is_ok(),
            accepted_rows < c_data_table::LIMITS.max_rows
        );
        drop(source);
        assert_eq!(held.row_count(), accepted_rows);
        assert_eq!(held.cell(accepted_rows - 1, 0).unwrap(), ValueRef::Int(7));
    }

    // Eight narrow arrays keep input buffers far below the byte ceiling, so
    // the next single cell independently crosses only the logical-cell limit.
    let rows = c_data_table::LIMITS.max_cells / 8;
    let values = Arc::new(Int8Array::from(vec![7; rows])) as ArrayRef;
    let source = batch(
        ["A", "B", "C", "D", "E", "F", "G", "H"]
            .into_iter()
            .map(|name| (name, values.clone()))
            .collect(),
    );
    let mut budget = ImportBudget::default();
    let held = unsafe { budget.import(stream(vec![source.clone()], source.schema())) }.unwrap();
    let additional = batch(vec![("I", Arc::new(Int8Array::from(vec![9])))]);
    assert!(matches!(
        unsafe { budget.import(stream(vec![additional.clone()], additional.schema())) },
        Err(Error::Limit)
    ));
    drop(source);
    assert_eq!(held.row_count(), rows);
    assert_eq!(held.cell(rows - 1, 7).unwrap(), ValueRef::Int(7));
}

fn prepared(spec: &str) -> PreparedRun {
    let document = yamaa_adapters::shipped_schema::capture()
        .unwrap()
        .prepare_standalone(Source {
            identity: "memory.yaml".into(),
            bytes: spec.as_bytes().to_vec(),
        })
        .unwrap();
    PreparedRun::prepare(document).unwrap()
}

#[test]
fn imported_execution_matches_csv_and_preserves_view_strings_with_ordinals() {
    let spec = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput:\n  SRC: {path: src.csv, ordinal: REC}\noutput: {path: out.csv, columns: [ID, VALUE, REC]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: SRC.ID}\n  - {name: VALUE, type: str, label: Value, derivation: SRC.VALUE}\n  - {name: REC, type: int, label: Record, derivation: SRC.REC}\n";
    let run = prepared(spec);
    let source = batch(vec![
        ("ID", Arc::new(StringViewArray::from(vec!["a", "b"]))),
        (
            "VALUE",
            Arc::new(StringViewArray::from(vec![Some(""), Some("β")])),
        ),
    ]);
    let table = import(source).unwrap();
    let actual = c_data_table::execute(&run, vec![table]).unwrap();
    let expected = run.execute_csv("ID,VALUE\na,\nb,β\n".as_bytes()).unwrap();
    assert_eq!(actual.outcome, expected.outcome);
    assert_eq!(actual.table, expected.table);
    assert!(c_data_table::admit_sources(&run, 0).is_err());
}
