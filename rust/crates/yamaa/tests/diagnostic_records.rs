//! Real registry diagnostics end to end through the public [`yamaa::Engine`] API.
//!
//! The conversion unit tests in `src/diagnostic.rs` deliberately use an
//! unregistered probe condition (the `check_diagnostics` ratchet forbids
//! registered condition/rule literals in crate sources). This suite pins real
//! registry diagnostics -- codes, requirements, ordered spec paths and
//! structured context -- through `SpecRequest::check` and `SpecRequest::run`
//! on an in-memory specification, where such literals are allowed.
#![cfg(feature = "arrow")]

use std::sync::Arc;

use arrow_array::RecordBatchIterator;
use arrow_schema::Schema;
use yamaa::{
    CancellationToken, ContextValue, DataType, Engine, EngineOptions, FFI_ArrowArrayStream, Field,
    Float64Array, HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch, Severity,
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
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv, types: {{VALUE: float, ZERO: float}}}}}}\noutput: {{path: out.csv, columns: {output}}}\ncolumns:\n{columns}"
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
    let run = engine
        .spec("spec.yaml")
        .cancel_with(&token)
        .run()
        .expect("adjudicated outcomes return a run");
    let diagnostics = run.diagnostics();
    assert!(!diagnostics.is_empty());
    if cfg!(feature = "yaml") {
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.phase.as_str(), "validation");
        assert_eq!(diagnostic.code.as_str(), "unknown_field");
        assert_eq!(diagnostic.rule.as_deref(), Some("NAME-3"));
        // Ordered spec-path segments, not a joined string.
        assert_eq!(diagnostic.spec_path, ["columns.VALUE.derivation.source"]);
        // The message renders the registry's template, which names the
        // identifier every cause of the condition records.
        assert_eq!(
            diagnostic.message,
            "SRC.MISSING is not a known field or reference at columns.VALUE.derivation.source"
        );
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
        "  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n  - {name: ZERO, type: float, derivation: SRC.ZERO}\n  - {name: RATIO, type: float, derivation: 'VALUE / ZERO'}\n",
        "[ID, VALUE, ZERO, RATIO]",
    );
    let run = engine
        .spec("spec.yaml")
        .cancel_with(&token)
        .run()
        .expect("adjudicated outcomes return a run");
    let diagnostics = run.diagnostics();
    assert!(!diagnostics.is_empty());
    if cfg!(feature = "yaml") {
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.phase.as_str(), "derivation");
        assert_eq!(diagnostic.code.as_str(), "division_by_zero");
        assert_eq!(diagnostic.rule.as_deref(), Some("FORMULA-15"));
        assert_eq!(diagnostic.message, "VALUE / ZERO divides by zero");
        assert_eq!(diagnostic.spec_path, ["columns.RATIO.derivation"]);
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

/// A literal outside the int range is refused as `integer_overflow`
/// (VALUE-5), and its context keeps the literal as an exact number: an
/// integer that `i64` does not hold is a `Wide` spelling, whether `u64`
/// holds it or not. The context's keys are the four the scalar literal's
/// finding writes: `actual`, `value`, `minimum` and `maximum`. The seam is
/// the facade's typed `ContextValue`, which the host fixtures' readers,
/// reading JSON, cannot see.
#[test]
fn integer_tokens_outside_the_signed_range_stay_exact_numbers() {
    for literal in [
        "9223372036854775808",
        "340282366920938463463374607431768211456",
    ] {
        let (engine, token) = engine_with_columns(
            &format!(
                "  - {{name: ID, type: int, derivation: SRC.ID}}\n  - {{name: VALUE, type: int, derivation: {literal}}}\n"
            ),
            "[ID, VALUE]",
        );
        let diagnostics = engine
            .spec("spec.yaml")
            .cancel_with(&token)
            .check()
            .unwrap()
            .diagnostics();
        assert!(!diagnostics.is_empty(), "{literal}");
        if cfg!(feature = "yaml") {
            let diagnostic = &diagnostics[0];
            let context = diagnostic
                .context
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone()))
                .collect::<Vec<_>>();
            assert_eq!(
                (
                    diagnostic.phase.as_str(),
                    diagnostic.code.as_str(),
                    diagnostic.rule.as_deref(),
                    diagnostic.message.clone(),
                    context,
                    diagnostic.source_span,
                    diagnostic.operand_route.clone(),
                ),
                (
                    "validation",
                    "integer_overflow",
                    Some("VALUE-5"),
                    format!(
                        "{literal} is outside the int range -9223372036854775808 to \
                         9223372036854775807"
                    ),
                    vec![
                        ("actual", ContextValue::Wide(literal.into())),
                        ("maximum", ContextValue::Int(i64::MAX)),
                        ("minimum", ContextValue::Int(i64::MIN)),
                        ("value", ContextValue::Text(literal.into())),
                    ],
                    None,
                    None,
                ),
                "{literal}"
            );
        }
    }
}

/// `check` takes the host language from the engine options, never from the
/// runtime name: an embedder may name its runtime anything (#2838). The
/// project holds only an R lock, which is no packaging lock, so only an R
/// engine reads it and reports it at the field that holds it (FUNC-3,
/// PROJECT-2), and a check and a run agree on it.
#[test]
fn check_takes_the_host_language_from_the_options_not_the_runtime_name() {
    const SPECIFICATION: &[u8] = b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n";
    const PROJECT: &[u8] = b"yamaa: '1.0'\ntype: project\nenvironments: {r: {lock: renv.lock}}\n";
    const LOCK: &[u8] = b"no packaging lock\n";
    for (host, opened) in [(HostLanguage::R, true), (HostLanguage::Python, false)] {
        let resolver = Arc::new(MemoryResolver::new());
        resolver.insert("/", "spec.yaml", "spec:1", Arc::from(SPECIFICATION));
        resolver.insert("/", "project.yaml", "project:1", Arc::from(PROJECT));
        resolver.insert("/project.yaml", "renv.lock", "lock:1", Arc::from(LOCK));
        let engine = Engine::new(
            EngineOptions::new(host, "embedder".into(), "0".into()).with_resolver(resolver),
        );
        let request = engine.spec("spec.yaml").project("project.yaml");
        let checked = request.check().unwrap().diagnostics();
        let ran = request
            .run()
            .expect("adjudicated outcomes return a run")
            .diagnostics();
        for (route, diagnostics) in [("check", &checked), ("run", &ran)] {
            let reported = diagnostics.iter().any(|diagnostic| {
                diagnostic.code.as_str() == "function_file_invalid"
                    && diagnostic.spec_path == ["environments.r.lock"]
            });
            assert_eq!(
                reported,
                opened && cfg!(feature = "yaml"),
                "{host:?} {route}: {diagnostics:?}"
            );
        }
    }
}
