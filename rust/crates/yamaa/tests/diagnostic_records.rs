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
    engine_with_columns(
        "  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.MISSING}\n",
        "[ID, VALUE]",
    )
}

/// One in-memory specification with the given `columns` block and output
/// column list, over a frame with `ID`, `VALUE` and a `ZERO` column of zeros.
fn engine_with_columns(columns: &str, output: &str) -> (Engine, CancellationToken) {
    let resolver = Arc::new(MemoryResolver::new());
    let specification = format!(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv, types: {{VALUE: float, ZERO: float}}}}}}\noutput: {{path: out.csv, columns: {output}}}\ncolumns:\n{columns}"
    );
    resolver.insert(
        "/",
        "spec.yaml",
        "spec:1",
        Arc::from(specification.as_bytes()),
    );
    let schema = Arc::new(Schema::new(vec![
        Field::new("ID", DataType::Int64, true),
        Field::new("VALUE", DataType::Float64, true),
        Field::new("ZERO", DataType::Float64, true),
    ]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![1, 2])),
            Arc::new(Float64Array::from(vec![2.5, 3.5])),
            Arc::new(Float64Array::from(vec![0.0, 0.0])),
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
        // A definition finding records no geometry: absent, not zero.
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
    }
}

#[test]
fn evaluation_failures_carry_recorded_geometry() {
    let (engine, token) = engine_with_columns(
        "  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n  - {name: ZERO, type: float, derivation: SRC.ZERO}\n  - {name: RATIO, type: float, derivation: {compute: {expr: 'VALUE / ZERO'}}}\n",
        "[ID, VALUE, ZERO, RATIO]",
    );
    let build = engine
        .build(&Request::new("spec.yaml"), &token)
        .expect("adjudicated outcomes return an owned build");
    let report = build.report();
    assert!(!report.is_empty());
    if cfg!(feature = "yaml") {
        assert_eq!(report.len(), 1);
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.phase, "derivation");
        assert_eq!(diagnostic.code, "division_by_zero");
        assert_eq!(diagnostic.requirement.as_deref(), Some("REQ-0430"));
        assert_eq!(diagnostic.spec_path, ["columns.RATIO.derivation.compute"]);
        assert_eq!(
            diagnostic.context.get("expr"),
            Some(&ContextValue::Text("VALUE / ZERO".into()))
        );
        // The failing division is the root node of the expression: its byte
        // span covers the whole expression text and its route is recorded
        // as present and empty, which differs from the absent route of a
        // definition finding.
        let span = diagnostic
            .source_span
            .expect("evaluation failures record their span");
        assert_eq!((span.start, span.end), (0, "VALUE / ZERO".len()));
        assert_eq!(diagnostic.operand_route.as_deref(), Some(&[][..]));
    }
}

#[test]
fn integer_tokens_beyond_sixty_four_bits_stay_exact_numbers() {
    let (engine, token) = engine_with_columns(
        "  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {literal: 340282366920938463463374607431768211456}}\n",
        "[ID, VALUE]",
    );
    let report = engine.check(&Request::new("spec.yaml"), &token);
    assert!(!report.is_empty());
    if cfg!(feature = "yaml") {
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.phase, "validation");
        assert_eq!(diagnostic.code, "integer_overflow");
        assert_eq!(
            diagnostic.context.get("actual"),
            Some(&ContextValue::Wide(
                "340282366920938463463374607431768211456".into()
            ))
        );
        assert_eq!(
            diagnostic.context.get("minimum"),
            Some(&ContextValue::Int(i64::MIN))
        );
        assert_eq!(
            diagnostic.context.get("maximum"),
            Some(&ContextValue::Int(i64::MAX))
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
    }
}
