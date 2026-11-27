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
/// Seam: a frame without columns that claims records it does not hold, as a
/// host's data frame of no columns and any number of rows exports them. The
/// shared truth states its frames by their rows, which a test would hold.
///
/// Nothing bounds that count but the memory a run asks for, so each
/// reservation it makes for those records is refused before one is built
/// (LIMIT-2): the generated `ordinal` field at its path, and without one the
/// driver's records, which no path owns.
#[test]
fn a_frame_without_columns_that_claims_records_no_memory_holds_refuses_the_run() {
    // More records than any memory holds: the most one batch claims.
    let claimed = usize::try_from(i64::MAX).unwrap();
    let frame = || {
        let schema = Arc::new(arrow_schema::Schema::empty());
        let options = arrow_array::RecordBatchOptions::new().with_row_count(Some(claimed));
        let batch = RecordBatch::try_new_with_options(schema.clone(), Vec::new(), &options);
        InputFrame::import(FFI_ArrowArrayStream::new(Box::new(
            RecordBatchIterator::new(vec![Ok(batch.unwrap())], schema),
        )))
        .unwrap()
    };
    let numbered = text(&TRUTH["zero_columns"]["specification"]);
    // No generated field: a row template gives each record of SRC the row 1.
    let constant = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: missing.csv}\noutput: {path: out.csv, columns: [ID]}\ncolumns: [{name: ID, type: int}]\nrows:\n  - id: each\n    dataset: SRC\n    derivations: {ID: 1}\n";
    for (label, specification, spec_path) in [
        ("the ordinal field", numbered, vec!["input.SRC.ordinal"]),
        ("the driver's records", constant, vec![]),
    ] {
        let root = Scratch::new("memory-inputs-claimed");
        root.write("spec.yaml", specification);
        let engine = Engine::new(EngineOptions::neutral(
            "memory-input-test".into(),
            "0".into(),
        ));
        let run = engine
            .spec(root.join("spec.yaml").to_str().unwrap())
            .inputs([("SRC".into(), frame())])
            .run()
            .unwrap();
        let issues: Vec<_> = run
            .diagnostics()
            .into_iter()
            .map(|issue| {
                (
                    issue.phase.as_str().to_owned(),
                    issue.code.as_str().to_owned(),
                    issue.rule,
                    issue.spec_path,
                    issue.context.to_json().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            (run.status(), issues),
            (
                Status::Failed,
                vec![(
                    "engine".to_owned(),
                    "engine_rejected".to_owned(),
                    None,
                    spec_path.into_iter().map(String::from).collect(),
                    r#"{"code":"allocation","step":"execute"}"#.to_owned(),
                )]
            ),
            "{label}"
        );
    }
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
fn failed_attempts_keep_override_provenance_before_capture() {
    for case in TRUTH["report_refusals"].as_array().unwrap() {
        let (root, path) = study();
        match case["fallback_hex"].as_str() {
            None => std::fs::remove_file(root.join("data.csv")).unwrap(),
            Some(hex) => {
                let bytes = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect::<Vec<_>>();
                std::fs::write(root.join("data.csv"), bytes).unwrap();
            }
        }
        let engine = Engine::new(EngineOptions::neutral(
            "memory-input-test".into(),
            "0".into(),
        ));
        let run = engine
            .spec(&path)
            .inputs([("SRC".into(), frame(&TRUTH["cases"][0]["rows"]))])
            .run()
            .unwrap();
        assert_eq!(
            (run.status(), memory_details(&run)),
            (Status::Failed, TRUTH["memory_details"].clone()),
            "{}",
            text(&case["name"])
        );
    }
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
/// A frame of the `frame_types` columns, each of its kind: `missing` is the
/// Arrow null type, as a column of None or NA reaches the hosts.
fn typed_columns(columns: &Value) -> InputFrame {
    let columns = columns.as_array().unwrap();
    let arrays = columns
        .iter()
        .map(|column| {
            let values = column["values"].as_array().unwrap();
            match text(&column["kind"]) {
                "int" => Arc::new(Int64Array::from(
                    values.iter().map(Value::as_i64).collect::<Vec<_>>(),
                )) as arrow_array::ArrayRef,
                "float" => Arc::new(arrow_array::Float64Array::from(
                    values.iter().map(Value::as_f64).collect::<Vec<_>>(),
                )),
                "str" => Arc::new(StringArray::from(
                    values.iter().map(Value::as_str).collect::<Vec<_>>(),
                )),
                "missing" => Arc::new(arrow_array::NullArray::new(values.len())),
                kind => panic!("no frame column of kind {kind}"),
            }
        })
        .collect::<Vec<_>>();
    let fields = columns
        .iter()
        .zip(&arrays)
        .map(|(column, array)| Field::new(text(&column["name"]), array.data_type().clone(), true))
        .collect::<Vec<_>>();
    let schema = Arc::new(arrow_schema::Schema::new(fields));
    let batch = RecordBatch::try_new(schema.clone(), arrays).unwrap();
    InputFrame::import(FFI_ArrowArrayStream::new(Box::new(
        RecordBatchIterator::new(vec![Ok(batch)], schema),
    )))
    .unwrap()
}
#[test]
fn each_frame_is_held_to_the_types_and_fields_its_input_declares() {
    // The facade's own reading of the frame_types truth both hosts read, so
    // the frame admission of `source_decoding::decode_frame` has Rust proof.
    let truth = &TRUTH["frame_types"];
    let engine = Engine::new(EngineOptions::neutral(
        "memory-input-test".into(),
        "0".into(),
    ));
    for case in truth["cases"].as_array().unwrap() {
        let name = text(&case["name"]);
        let root = Scratch::new("frame-types");
        let specification = case.get("specification").unwrap_or(&truth["specification"]);
        let path = root.write("frame-types.yaml", text(specification));
        let mut run = engine
            .spec(path.to_str().unwrap())
            .inputs([("SRC".into(), typed_columns(&case["columns"]))])
            .run()
            .unwrap();
        match case.get("issue") {
            Some(expected) => {
                assert_eq!(run.status(), Status::Failed, "{name}");
                assert_issue(&run, expected);
            }
            None => {
                assert_eq!(
                    (run.status(), run.diagnostics()),
                    (Status::Succeeded, vec![]),
                    "{name}"
                );
                run.save(&CancellationToken::new()).unwrap();
                assert_eq!(
                    std::fs::read_to_string(root.join("frame-types.csv")).unwrap(),
                    text(&case["csv"]),
                    "{name}"
                );
            }
        }
    }
}
#[test]
fn a_read_odm_table_passes_back_as_the_frame_of_its_odm_xml_input() {
    // The facade's reading of the odm_round_trip truth: read_odm's table,
    // whose SourceOrdinal is int as the clinical-item table types it, is the
    // frame decode_frame holds to that table.
    let truth = &TRUTH["odm_round_trip"];
    let root = Scratch::new("odm-round-trip");
    let document = root.write("items.xml", text(&truth["document"]));
    let path = root.write("round-trip.yaml", text(&truth["specification"]));
    let engine = Engine::new(EngineOptions::neutral(
        "memory-input-test".into(),
        "0".into(),
    ));
    let table = engine
        .read_odm(document.to_str().unwrap(), &yamaa::OdmOptions::default())
        .unwrap();
    let frame = InputFrame::import(FFI_ArrowArrayStream::new(Box::new(
        RecordBatchIterator::new(
            table.batches().to_vec().into_iter().map(Ok),
            table.schema().clone(),
        ),
    )))
    .unwrap();
    let mut run = engine
        .spec(path.to_str().unwrap())
        .inputs([("SRC".into(), frame)])
        .run()
        .unwrap();
    assert_eq!(
        (run.status(), run.diagnostics()),
        (Status::Succeeded, vec![])
    );
    run.save(&CancellationToken::new()).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("round-trip.csv")).unwrap(),
        text(&truth["csv"])
    );
}
