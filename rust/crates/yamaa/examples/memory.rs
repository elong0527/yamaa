//! Run one specification entirely from memory: specification bytes and one
//! Arrow input table enter through [`MemoryResolver`](yamaa::MemoryResolver),
//! Arrow result tables and typed diagnostics come out, and no filesystem path
//! is touched. One request, as [`Engine::spec`](yamaa::Engine::spec) gives
//! it, checks the specification and runs it.
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
    HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch,
};

/// Specification bytes served from memory. The input table arrives as an
/// Arrow frame below, so the example needs no CSV, Parquet or SAS reader.
const SPECIFICATION: &[u8] = b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n";

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

    // The request reads nothing until an operation runs, and each operation
    // reads the specification afresh, as a host's specification object does.
    let spec = engine.spec("spec.yaml").cancel_with(&token);
    let check = spec.check().unwrap();
    let mut run = spec.run().unwrap();
    if cfg!(feature = "yaml") {
        assert_eq!(check.issues().rows(), 0);
        assert_eq!(run.issues().rows(), 0);
        let output = run.output().unwrap();
        assert_eq!(output.rows(), 2);
        assert_eq!(output.columns()[0].name(), "ID");
        assert_eq!(output.columns()[1].name(), "VALUE");
        let ids = output.batches()[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!((ids.value(0), ids.value(1)), (1, 2));
        // A save returns what it wrote; a caller's resolver names each file
        // by the reference its target reply gave.
        assert_eq!(
            run.save(&CancellationToken::new()).unwrap(),
            ["memory:/out.csv"]
        );
        assert!(resolver.published("memory:/out.csv").is_some());
        assert_eq!(resolver.staged_count(), 0);
        println!("memory example: 2 output rows, save published memory:/out.csv");
    } else {
        // Without document admission the same calls report typed diagnostics.
        assert!(check.issues().rows() > 0);
        assert!(run.output().is_none());
        assert!(run
            .save(&CancellationToken::new())
            .unwrap_err()
            .paths
            .is_empty());
        println!("memory example: unavailable without the yaml feature, as reported");
    }

    // Typed diagnostics out: a missing entry reports records, never text,
    // each with its phase and its registered condition. A request given no
    // token polls one that nothing cancels.
    let missing = engine.spec("missing.csv").check().unwrap().diagnostics();
    let first = missing.first().expect("a missing entry reports an issue");
    println!(
        "memory example: missing entry reports {} in the {} phase",
        first.code, first.phase
    );
}
