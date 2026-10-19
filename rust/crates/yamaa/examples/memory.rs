//! Run one specification entirely from memory: specification bytes and one
//! Arrow input table enter through [`MemoryResolver`](yamaa::MemoryResolver),
//! Arrow result tables and typed diagnostics come out, and no filesystem path
//! is touched.
//!
//! Run it with `cargo run -p yamaa --example memory`. The gate in
//! `tests/memory.rs` runs this same entry point, so the example cannot rot.
//! Every assertion is lane-aware: builds with any combination of the `yaml`,
//! `parquet` and `readstat` features run it, and lanes without document
//! admission exercise the reported-diagnostics path instead.

use std::sync::Arc;

use arrow_array::RecordBatchIterator;
use arrow_schema::Schema;
use yamaa::{
    CancellationToken, DataType, Engine, EngineOptions, FFI_ArrowArrayStream, Field, Float64Array,
    HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch, Request, SaveOutcome,
};

/// Specification bytes served from memory. The input table arrives as an
/// Arrow frame below, so the example needs no CSV, Parquet or SAS reader.
const SPECIFICATION: &[u8] = b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n";

/// Build the Arrow input frame the host owns: two rows over `ID` and `VALUE`.
fn input_frame() -> InputFrame {
    let schema = Arc::new(Schema::new(vec![
        Field::new("ID", DataType::Int64, true),
        Field::new("VALUE", DataType::Float64, true),
    ]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![1, 2])),
            Arc::new(Float64Array::from(vec![2.5, 3.5])),
        ],
    )
    .unwrap();
    let stream =
        FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema)));
    InputFrame::import(stream).unwrap()
}

/// Run the in-memory specification end to end.
///
/// Public so the `tests/memory.rs` gate runs the exact example code instead
/// of a copy.
pub fn main() {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert("/", "spec.yaml", "spec:1", Arc::from(SPECIFICATION));
    let frame = input_frame();
    resolver.insert_frame("/spec.yaml", "input.csv", "data:1", &frame);
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "memory-example".into(), "0".into())
            .with_resolver(resolver.clone()),
    );
    let token = CancellationToken::new();
    let request = Request::new("spec.yaml");

    let report = engine.check(&request, &token).unwrap();
    let mut build = engine.build(&request, &token).unwrap();
    if cfg!(feature = "yaml") {
        assert!(report.is_empty());
        assert!(build.report().is_empty());
        let output = build.output().unwrap();
        assert_eq!(output.rows(), 2);
        assert_eq!(output.columns()[0].name(), "ID");
        assert_eq!(output.columns()[1].name(), "VALUE");
        let ids = output.batches()[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!((ids.value(0), ids.value(1)), (1, 2));
        assert!(!output.ipc().unwrap().is_empty());
        assert_eq!(build.save().unwrap(), SaveOutcome::Saved);
        assert!(resolver.published("memory:/out.csv").is_some());
        assert_eq!(resolver.staged_count(), 0);
        println!("memory example: 2 output rows, save published memory:/out.csv");
    } else {
        // Without document admission the same calls report typed diagnostics.
        assert!(!report.is_empty());
        assert!(build.output().is_none());
        assert!(build.save().is_err());
        println!("memory example: unavailable without the yaml feature, as reported");
    }

    // Typed diagnostics out: a missing entry reports records, never text.
    let missing = engine.check(&Request::new("missing.csv"), &token).unwrap();
    assert!(!missing.is_empty());
    assert!(!missing.diagnostics[0].code.is_empty());
    println!(
        "memory example: missing entry reports code '{}'",
        missing.diagnostics[0].code
    );
}
