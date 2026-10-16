//! Real registry diagnostics end to end through the public [`yamaa::Engine`] API.
//!
//! The conversion unit tests in `src/diagnostic.rs` deliberately use an
//! unregistered probe condition (the `check_diagnostics` ratchet forbids
//! registered condition/requirement literals in crate sources). This suite
//! pins real registry diagnostics -- codes, requirements, ordered spec paths
//! and structured context -- through `Engine::check` on an in-memory
//! specification, where such literals are allowed.

use std::sync::Arc;

use arrow_array::RecordBatchIterator;
use arrow_schema::Schema;
use yamaa::{
    CancellationToken, ContextValue, DataType, Engine, EngineOptions, FFI_ArrowArrayStream, Field,
    Float64Array, HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch, Request,
    Severity,
};

fn engine_with_bad_derivation() -> (Engine, CancellationToken) {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert(
        "/",
        "spec.yaml",
        "spec:1",
        Arc::from(
            &b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.MISSING}\n"[..],
        ),
    );
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
    let frame = InputFrame::import(FFI_ArrowArrayStream::new(Box::new(
        RecordBatchIterator::new(vec![Ok(batch)], schema),
    )))
    .unwrap();
    resolver.insert_frame("/spec.yaml", "input.csv", "data:1", &frame);
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "records-test".into(), "0".into())
            .with_resolver(resolver),
    );
    (engine, CancellationToken::new())
}

#[test]
fn unknown_source_column_arrives_as_a_typed_record() {
    let (engine, token) = engine_with_bad_derivation();
    let build = engine
        .build(&Request::new("spec.yaml"), &token)
        .expect("adjudicated outcomes return an owned build");
    let report = build.report();
    assert!(!report.is_empty());
    if cfg!(feature = "yaml") {
        assert_eq!(report.len(), 1);
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.phase, "validation");
        assert_eq!(diagnostic.code, "unknown_field");
        assert_eq!(diagnostic.requirement.as_deref(), Some("REQ-0103"));
        // Ordered spec-path segments, not a joined string.
        assert_eq!(diagnostic.spec_path, ["columns.VALUE.derivation.source"]);
        // Structured context, never a formatted string.
        assert_eq!(
            diagnostic.context.get("identifier"),
            Some(&ContextValue::Text("SRC.MISSING".into()))
        );
        assert_eq!(
            diagnostic.context.to_json(),
            r#"{"identifier":"SRC.MISSING"}"#
        );
    }
}
