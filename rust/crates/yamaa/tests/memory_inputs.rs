//! Shared memory-input truth, exercised through the facade before the hosts.
#![cfg(all(feature = "yaml", feature = "arrow"))]
use crate::support::Scratch;
use arrow_array::{Int64Array, RecordBatchIterator, StringArray};
use std::sync::{Arc, LazyLock};
use yamaa::{
    CancellationToken, DataType, Engine, EngineOptions, FFI_ArrowArrayStream, Field, InputFrame,
    Plans, RecordBatch, Run, Status,
};
use yamaa_core::json::Value;

static TRUTH: LazyLock<Value> = LazyLock::new(|| {
    yamaa_adapters::json_wire::decode(include_str!("../../../tests/host-fixtures/cases.json"))
        .unwrap()["families"]["memory-inputs"]
        .clone()
});
fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}
fn study() -> (Scratch, String) {
    let root = Scratch::new("memory-inputs");
    for (name, value) in TRUTH["files"].as_object().unwrap() {
        root.write(name, text(value));
    }
    let path = root
        .join(text(&TRUTH["entry"]))
        .to_str()
        .unwrap()
        .to_owned();
    (root, path)
}
fn frame(rows: &Value) -> InputFrame {
    let names = TRUTH["columns"].as_array().unwrap();
    let fields = names
        .iter()
        .map(|name| Field::new(text(name), DataType::Int64, true))
        .collect::<Vec<_>>();
    let schema = Arc::new(arrow_schema::Schema::new(fields));
    let columns = (0..names.len())
        .map(|index| {
            Arc::new(Int64Array::from(
                rows.as_array()
                    .unwrap()
                    .iter()
                    .map(|row| row[index].as_i64().unwrap())
                    .collect::<Vec<_>>(),
            )) as arrow_array::ArrayRef
        })
        .collect();
    let batch = RecordBatch::try_new(schema.clone(), columns).unwrap();
    InputFrame::import(FFI_ArrowArrayStream::new(Box::new(
        RecordBatchIterator::new(vec![Ok(batch)], schema),
    )))
    .unwrap()
}
fn saved(root: &Scratch, run: &mut Run) -> String {
    assert_eq!(
        (run.status(), run.diagnostics()),
        (Status::Succeeded, vec![])
    );
    run.save(&CancellationToken::new()).unwrap();
    std::fs::read_to_string(root.join("out.csv")).unwrap()
}
fn memory_details(run: &Run) -> Value {
    let report = run.report().unwrap();
    for batch in report.batches() {
        let names = batch.schema();
        let kind = batch
            .column(names.index_of("KIND").unwrap())
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let id = batch
            .column(names.index_of("NAME").unwrap())
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let details = batch
            .column(names.index_of("DETAILS").unwrap())
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for row in 0..batch.num_rows() {
            if kind.value(row) == "input" && id.value(row) == text(&TRUTH["alias"]) {
                return yamaa_adapters::json_wire::decode(details.value(row)).unwrap();
            }
        }
    }
    panic!("memory input report row");
}
#[test]
fn alias_overrides_admit_declared_types_and_keep_file_fallback_per_attempt() {
    let (root, path) = study();
    let engine = Engine::new(EngineOptions::neutral(
        "memory-input-test".into(),
        "0".into(),
    ));
    let plans = Plans::new();
    for case in TRUTH["cases"].as_array().unwrap() {
        let mut run = engine
            .spec(&path)
            .reusing(&plans)
            .inputs([(text(&TRUTH["alias"]).into(), frame(&case["rows"]))])
            .run()
            .unwrap();
        assert_eq!(
            saved(&root, &mut run),
            text(&case["csv"]),
            "{}",
            text(&case["name"])
        );
        assert_eq!(memory_details(&run), TRUTH["memory_details"]);
    }
    let mut run = engine.spec(&path).reusing(&plans).run().unwrap();
    assert_eq!(saved(&root, &mut run), text(&TRUTH["file_csv"]));
}
#[test]
fn deferred_stream_owns_its_frames_without_retaining_them_in_the_plan() {
    let (root, path) = study();
    let engine = Engine::new(EngineOptions::neutral(
        "memory-input-test".into(),
        "0".into(),
    ));
    let plans = Plans::new();
    let case = &TRUTH["cases"][0];
    let stream = engine
        .spec(&path)
        .reusing(&plans)
        .inputs([(text(&TRUTH["alias"]).into(), frame(&case["rows"]))])
        .stream()
        .unwrap();
    let rows: usize = stream.reader().map(|batch| batch.unwrap().num_rows()).sum();
    assert_eq!(rows, case["rows"].as_array().unwrap().len());
    let mut run = stream.run().unwrap();
    assert_eq!(memory_details(&run), TRUTH["memory_details"]);
    assert_eq!(saved(&root, &mut run), text(&case["csv"]));
}
#[test]
fn unknown_alias_is_a_complete_authored_refusal_before_study_access() {
    let (root, path) = study();
    std::fs::remove_file(root.join("data.csv")).unwrap();
    let expected = &TRUTH["unknown"];
    let engine = Engine::new(EngineOptions::neutral(
        "memory-input-test".into(),
        "0".into(),
    ));
    let run = engine
        .spec(&path)
        .inputs([(
            text(&expected["alias"]).into(),
            frame(&TRUTH["cases"][0]["rows"]),
        )])
        .run()
        .unwrap();
    assert_issue(&run, expected);
}
#[test]
fn overrides_keep_the_custom_resolver_for_other_inputs_and_publication() {
    // Seam: an alias overlay must preserve an embedder's resource authority.
    let resolver = Arc::new(yamaa::MemoryResolver::new());
    resolver.insert(
        "/",
        "spec.yaml",
        "/spec.yaml",
        Arc::from(text(&TRUTH["files"]["parent.yaml"]).as_bytes()),
    );
    resolver.insert(
        "/spec.yaml",
        "data.csv",
        "/data.csv",
        Arc::from(text(&TRUTH["files"]["data.csv"]).as_bytes()),
    );
    let engine = Engine::new(
        EngineOptions::neutral("memory-input-test".into(), "0".into())
            .with_resolver(resolver.clone()),
    );
    let case = &TRUTH["cases"][0];
    let mut run = engine
        .spec("spec.yaml")
        .inputs([("SRC".into(), frame(&case["rows"]))])
        .run()
        .unwrap();
    assert_eq!(
        (run.status(), run.diagnostics()),
        (Status::Succeeded, vec![])
    );
    let paths = run.save(&CancellationToken::new()).unwrap();
    assert_eq!(paths, ["memory:/out.csv"]);
    assert_eq!(
        resolver.published(&paths[0]).as_deref(),
        Some(text(&case["csv"]).as_bytes())
    );
}

fn assert_issue(run: &Run, expected: &Value) {
    let issues = run.diagnostics();
    assert_eq!(issues.len(), 1);
    let issue = &issues[0];
    assert_eq!(
        (
            issue.phase.as_str(),
            issue.code.as_str(),
            issue.rule.as_deref(),
            issue.spec_path.clone(),
            issue.context.to_json().to_owned(),
            issue.message.as_str(),
            issue.source_span.is_none(),
            issue.operand_route.is_none()
        ),
        (
            text(&expected["phase"]),
            text(&expected["condition"]),
            Some(text(&expected["rule"])),
            vec![text(&expected["spec_path"][0]).to_owned()],
            yamaa_adapters::json_wire::encode(&expected["context"]),
            text(&expected["message"]),
            true,
            true
        )
    );
}

#[test]
fn overriding_both_aliases_never_inspects_the_missing_shared_file() {
    let (root, path) = study();
    std::fs::remove_file(root.join("data.csv")).unwrap();
    let engine = Engine::new(EngineOptions::neutral(
        "memory-input-test".into(),
        "0".into(),
    ));
    let mut run = engine
        .spec(&path)
        .inputs([
            ("SRC".into(), frame(&TRUTH["cases"][0]["rows"])),
            ("AUX".into(), frame(&TRUTH["cases"][1]["rows"])),
        ])
        .run()
        .unwrap();
    assert_eq!(saved(&root, &mut run), text(&TRUTH["all_memory_csv"]));
}
#[test]
fn declared_input_types_are_checked_against_the_override_schema() {
    let (root, path) = study();
    let expected = &TRUTH["missing_type"];
    root.write(
        "parent.yaml",
        text(&TRUTH["files"]["parent.yaml"]).replace(
            text(&expected["declaration_from"]),
            text(&expected["declaration_to"]),
        ),
    );
    let engine = Engine::new(EngineOptions::neutral(
        "memory-input-test".into(),
        "0".into(),
    ));
    let run = engine
        .spec(&path)
        .inputs([("SRC".into(), frame(&TRUTH["cases"][0]["rows"]))])
        .run()
        .unwrap();
    assert_issue(&run, expected);
}
