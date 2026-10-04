//! Independent observations for the closed typed-plan/IPC dataset boundary.
use arrow_array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType, Field, Schema};
use serde_json::{json, Value};
use std::sync::Arc;
use yamaa_adapters::{
    dataset_transport::{execute_dataset, DatasetTransportError as Error, PreparedDataset},
    table_transport::table_snapshot,
};

/// Build a canonical immutable input stream independently of the native decoder.
fn source(ids: Vec<Option<i64>>, values: Vec<Option<&str>>) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, true),
        Field::new("x", DataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(ids)) as ArrayRef,
            Arc::new(StringArray::from(values)),
        ],
    )
    .unwrap();
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).unwrap();
    writer.write(&batch).unwrap();
    writer.finish().unwrap();
    drop(writer);
    bytes
}
/// The request is authored as protocol data, not serialized from a core plan.
fn request() -> Value {
    json!({"protocol":"dataset/1", "source":[{"name":"id","kind":"int"},{"name":"x","kind":"str"}], "output":[{"name":"id","kind":"int"},{"name":"x","kind":"int"}], "templates":[{"mode":{"records":null},"assignments":[]}], "columns":[{"column":0,"path":"columns.id.derivation.source","expression":{"source":0}},{"column":1,"path":"columns.x.derivation.source","expression":{"source":1}}], "keys":[0], "verifications":[{"path":"verifications[0].row_count","check":{"row_count":{"min":"2","max":"2"}}}]})
}
/// Inspect the exact bounded JSON outcome returned by the real adapter.
fn outcome(request: &Value, source: &[u8]) -> (Option<Vec<u8>>, Value) {
    let response = execute_dataset(&request.to_string(), source).unwrap();
    (
        response.table,
        serde_json::from_str::<Value>(&response.outcome).unwrap()["outcome"].take(),
    )
}

/// Accepted output round-trips through the existing strict table decoder with exact values.
#[test]
fn checked_dataset_exports_owned_ipc_and_complete_check_counts() {
    let input = source(
        vec![Some(i64::MIN), Some(i64::MAX)],
        vec![Some("0007"), None],
    );
    let prepared = PreparedDataset::parse(&request().to_string()).unwrap();
    for _ in 0..2 {
        let result = prepared.execute(&input).unwrap();
        let observed: Value = serde_json::from_str(&result.outcome).unwrap();
        assert_eq!(
            observed,
            json!({"protocol":"dataset/1","outcome":{"status":"success","verifications":[{"spec_path":"verifications[0].row_count","condition":"row_count_failed","requirement":"REQ-0385","evaluated_count":"1","failed_count":"0","output_rows":"2","offending_rows":[]}]}})
        );
        let values: Value =
            serde_json::from_str(&table_snapshot(&result.table.unwrap()).unwrap()).unwrap();
        assert_eq!(
            values["rows"],
            json!([[{"int":"-9223372036854775808"},{"int":"7"}],[{"int":"9223372036854775807"},{"missing":null}]])
        );
    }
}

/// All plan checks precede IPC parsing, including latent errors on empty inputs.
#[test]
fn invalid_plans_never_reach_bad_source_bytes() {
    let mut invalid = request();
    invalid["columns"][1]["expression"] = json!({"source":99});
    assert!(matches!(
        execute_dataset(&invalid.to_string(), b"invalid IPC"),
        Err(Error::InvalidPlan)
    ));
    let mut unknown = request();
    unknown["unexpected"] = json!(true);
    assert!(matches!(
        execute_dataset(&unknown.to_string(), b"invalid IPC"),
        Err(Error::InvalidRequest)
    ));
    let mut protocol = request();
    protocol["protocol"] = json!("dataset/2");
    assert!(matches!(
        execute_dataset(&protocol.to_string(), b"invalid IPC"),
        Err(Error::UnsupportedProtocol)
    ));
    let mut bound = request();
    bound["verifications"][0]["check"]["row_count"]["min"] = json!("02");
    assert!(matches!(
        PreparedDataset::parse(&bound.to_string()),
        Err(Error::InvalidRequest)
    ));
    assert!(matches!(
        PreparedDataset::parse(&" ".repeat(1_048_577)),
        Err(Error::RequestLimit)
    ));
}

/// Known keys survive a runtime condition; a partial key remains unavailable.
#[test]
fn conversion_failure_has_exact_context_without_any_output_bytes() {
    let input = source(vec![Some(5)], vec![Some("bad")]);
    let (table, result) = outcome(&request(), &input);
    assert!(table.is_none());
    assert_eq!(
        result,
        json!({"status":"condition","diagnostic":{"phase":"convert","condition":"conversion_failed","requirement":"REQ-0013","spec_paths":["columns.x"],"context":{"from":{"str":"str"},"to":{"str":"int"},"value":{"str":"bad"}}},"identity":{"position":"0","keys":[{"int":"5"}]}})
    );
    let mut before_keys = request();
    let assignment = before_keys["columns"].as_array_mut().unwrap().remove(1);
    before_keys["templates"][0]["assignments"] = json!([assignment]);
    let (_, result) = outcome(&before_keys, &input);
    assert!(result["identity"].is_null());
}

/// Failed verification retains all evaluated records and complete offending identities.
#[test]
fn verification_errors_and_identity_errors_withhold_ipc() {
    let input = source(vec![Some(1), Some(2)], vec![Some("7"), Some("7")]);
    let mut req = request();
    req["verifications"].as_array_mut().unwrap().insert(
        0,
        json!({"path":"verifications[1].unique","check":{"unique":[1]}}),
    );
    let (table, result) = outcome(&req, &input);
    assert!(table.is_none());
    assert_eq!(result["phase"], "verification");
    assert_eq!(
        result["verifications"][0],
        json!({"spec_path":"verifications[1].unique","condition":"unique_failed","requirement":"REQ-0381","evaluated_count":"1","failed_count":"1","output_rows":"2","offending_rows":[{"position":"0","keys":[{"int":"1"}]},{"position":"1","keys":[{"int":"2"}]}]})
    );
    assert_eq!(result["verifications"][1]["failed_count"], "0");
    let (table, result) = outcome(&req, &source(vec![None, None], vec![Some("7"), Some("7")]));
    assert!(table.is_none());
    assert_eq!(result["phase"], "output");
    assert_eq!(result["verifications"][0]["condition"], "missing_key");
    assert_eq!(result["verifications"][1]["condition"], "duplicate_key");
}

/// A small request cannot expand retained text indefinitely through row replication.
#[test]
fn output_text_amplification_is_stopped_inside_execution() {
    let count = 1025;
    let input = source((0..count).map(Some).collect(), vec![None; count as usize]);
    let mut req = request();
    req["output"][1]["kind"] = json!("str");
    req["columns"][1]["expression"] = json!({"literal":{"str":"x".repeat(1024)}});
    req["verifications"] = json!([]);
    let (table, result) = outcome(&req, &input);
    assert!(table.is_none());
    assert_eq!(
        result,
        json!({"status":"limit","resource":"output_text_bytes","limit":"1048576","required":"1049600"})
    );
}

/// Shared installed-host truth comes from authored outcomes and committed benchmark CSV.
#[test]
fn shared_dataset_cases_match_independent_values_and_observations() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/datasets/expected.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let input: &[u8] = match case["input"].as_str().unwrap() {
            "adlb.arrow" => include_bytes!("fixtures/datasets/adlb.arrow"),
            "adlb-empty.arrow" => include_bytes!("fixtures/datasets/adlb-empty.arrow"),
            "integer-sum.arrow" => include_bytes!("fixtures/datasets/integer-sum.arrow"),
            "key-grain.arrow" => include_bytes!("fixtures/datasets/key-grain.arrow"),
            "key-grain-empty.arrow" => include_bytes!("fixtures/datasets/key-grain-empty.arrow"),
            "key-grain-missing.arrow" => {
                include_bytes!("fixtures/datasets/key-grain-missing.arrow")
            }
            "numbering.arrow" => include_bytes!("fixtures/datasets/numbering.arrow"),
            "numbering-empty.arrow" => include_bytes!("fixtures/datasets/numbering-empty.arrow"),
            other => panic!("unknown fixture {other}"),
        };
        let response = execute_dataset(&case["request"].to_string(), input).unwrap();
        let actual: Value = serde_json::from_str(&response.outcome).unwrap();
        assert_eq!(actual, case["expected"], "{}", case["case"]);
        match response.table {
            Some(bytes) => assert_eq!(
                serde_json::from_str::<Value>(&table_snapshot(&bytes).unwrap()).unwrap(),
                case["snapshot"],
                "{}",
                case["case"]
            ),
            None => assert!(case["snapshot"].is_null()),
        }
    }
}

/// Authored typed filter runs after row conversion and before later source assignments.
fn filtered_request() -> Value {
    let mut req = request();
    let assignment = req["columns"].as_array_mut().unwrap().remove(1);
    req["templates"][0]["assignments"] = json!([assignment]);
    req["templates"][0]["filter"] = json!({"path":"rows[0].filter","text":"x > 0",
        "nodes":[{"compare":{"operator":"gt","left":{"identifier":"x"},"right":{"literal":{"int":"0"}}}}],
        "root":0,"bindings":[{"name":"x","read":{"column":1}}]});
    req["verifications"] = json!([]);
    req
}
/// Missing and false candidates never reach output identity checks or retained IPC.
#[test]
fn typed_filters_keep_only_true_rows() {
    let input = source(
        vec![Some(8), Some(3), Some(7), Some(1)],
        vec![Some("-1"), Some("2"), None, Some("4")],
    );
    let (bytes, result) = outcome(&filtered_request(), &input);
    assert_eq!(result, json!({"status":"success","verifications":[]}));
    let snapshot: Value = serde_json::from_str(&table_snapshot(&bytes.unwrap()).unwrap()).unwrap();
    assert_eq!(
        snapshot["rows"],
        json!([[{"int":"3"},{"int":"2"}],[{"int":"1"},{"int":"4"}]])
    );
    assert_eq!(
        serde_json::from_str::<Value>(yamaa_adapters::dataset_transport::capabilities()).unwrap(),
        json!({"protocol":"dataset/1","features":["row_filter","predicate_checks","key_grain","window_numbering","window_filter","window_values"]})
    );
}
/// Complete predicate and binding admission wins over invalid IPC decoding.
#[test]
fn invalid_filters_fail_before_source_decoding() {
    let base = filtered_request();
    let mut variants = Vec::new();
    for (field, value, expected) in [
        ("root", json!(99), Error::InvalidPlan),
        ("path", json!(""), Error::InvalidPlan),
        ("text", json!(""), Error::InvalidPlan),
        ("bindings", json!([]), Error::InvalidPlan),
        (
            "bindings",
            json!([{"name":"x","read":{"column":0}}]),
            Error::InvalidPlan,
        ),
        (
            "bindings",
            json!([{"name":"x","read":{"source":99}}]),
            Error::InvalidPlan,
        ),
        ("nodes", json!([{"not":0}]), Error::InvalidPlan),
        ("nodes", json!([{"unknown":0}]), Error::InvalidRequest),
        (
            "nodes",
            json!([{"compare":{"operator":"gt","left":{"identifier":"x"},"right":{"literal":{"int":"01"}}}}]),
            Error::InvalidScalar,
        ),
        (
            "nodes",
            json!([{"like":{"value":{"identifier":"x"},"pattern":{"literal":{"str":"%"}},"escape":"ab","negated":false}}]),
            Error::InvalidPlan,
        ),
    ] {
        let mut req = base.clone();
        req["templates"][0]["filter"][field] = value;
        variants.push((req, expected));
    }
    for (req, expected) in variants {
        assert_eq!(
            execute_dataset(&req.to_string(), b"invalid IPC")
                .err()
                .unwrap(),
            expected,
            "{req}"
        );
    }
}
/// Eager boolean evaluation retains native conditions without an accepted table.
#[test]
fn filter_errors_withhold_ipc_even_when_false_is_known() {
    let mut req = filtered_request();
    req["templates"][0]["filter"]["nodes"] = json!([
        {"boolean":false},
        {"compare":{"operator":"eq","left":{"identifier":"x"},"right":{"literal":{"str":"bad"}}}},
        {"and":[0,1]}]);
    req["templates"][0]["filter"]["root"] = json!(2);
    let (bytes, result) = outcome(&req, &source(vec![Some(1)], vec![Some("2")]));
    assert!(bytes.is_none());
    assert_eq!(result["status"], "condition");
    assert_eq!(
        result["diagnostic"]["spec_paths"],
        json!(["rows[0].filter"])
    );
    assert_eq!(result["diagnostic"]["condition"], "incompatible_input_type");
    assert!(result["identity"].is_null());
    let (bytes, result) = outcome(&req, &source(vec![Some(1)], vec![Some("bad")]));
    assert!(bytes.is_none());
    assert_eq!(result["diagnostic"]["condition"], "conversion_failed");
    let (bytes, result) = outcome(&req, &source(vec![], vec![]));
    assert!(bytes.is_some());
    assert_eq!(result["status"], "success");
}

/// Check bindings use completed output only and are admitted before any IPC read.
#[test]
fn predicate_checks_reject_invalid_bindings_before_source_decoding() {
    let predicate = filtered_request()["templates"][0]["filter"].clone();
    for kind in ["assert", "implies", "predicate_declaration"] {
        for read in [json!({"source": 1}), json!({"column": 99})] {
            let mut invalid = predicate.clone();
            invalid["bindings"][0]["read"] = read;
            let mut req = request();
            let check = if kind == "implies" {
                json!({"implies": {"when": predicate, "then": invalid}})
            } else {
                json!({kind: invalid})
            };
            req["verifications"] = json!([{"path":"verifications[0]","check":check}]);
            assert_eq!(
                execute_dataset(&req.to_string(), b"invalid IPC").err(),
                Some(Error::InvalidPlan),
                "{kind}"
            );
        }
    }
}

/// A compiler checkpoint validates types without observing actual-row dynamic patterns.
#[test]
fn declaration_checkpoint_emits_no_record_and_does_not_evaluate_rows() {
    let mut req = request();
    req["output"][1]["kind"] = json!("str");
    let predicate = json!({"path":"verifications[1].implies.when","text":"'x' LIKE x ESCAPE '1'",
        "nodes":[{"like":{"value":{"literal":{"str":"x"}},"pattern":{"identifier":"x"},"escape":"1","negated":false}}],
        "root":0,"bindings":[{"name":"x","read":{"column":1}}]});
    req["verifications"].as_array_mut().unwrap().push(
        json!({"path":"verifications[1].implies","check":{"predicate_declaration":predicate}}),
    );
    let input = source(vec![Some(1), Some(2)], vec![Some("1"), Some("1")]);
    let (table, result) = outcome(&req, &input);
    assert!(table.is_some());
    assert_eq!(result["status"], "success");
    assert_eq!(result["verifications"].as_array().unwrap().len(), 1);
    req["verifications"][1]["check"] = json!({"assert":predicate});
    let (table, result) = outcome(&req, &input);
    assert!(table.is_none());
    assert_eq!(result["status"], "condition");
    assert_eq!(result["diagnostic"]["condition"], "invalid_predicate");
    assert_eq!(result["verifications"].as_array().unwrap().len(), 1);
}

/// An authored key-only phase precedes collection across complete source memberships.
fn key_request() -> Value {
    let mut req = request();
    let assignment = req["columns"].as_array_mut().unwrap().remove(0);
    req["templates"][0] = json!({"mode":{"keys":null},"assignments":[assignment]});
    req["columns"][0]["expression"] = json!({"collect":{"column":1,"identifier":"SRC.x"}});
    req["verifications"] = json!([]);
    req
}
/// Key and collected-value phase mistakes fail before any input bytes are decoded.
#[test]
fn invalid_key_grain_plans_fail_before_ipc_and_recover() {
    let mut variants = Vec::new();
    let mut req = key_request();
    req["columns"][0]["expression"] = json!({"source":1});
    variants.push(req);
    let mut req = key_request();
    req["templates"][0]["assignments"] = json!([]);
    variants.push(req);
    let mut req = key_request();
    req["templates"][0]["assignments"][0]["expression"] =
        json!({"collect":{"column":0,"identifier":"SRC.id"}});
    variants.push(req);
    let mut req = key_request();
    req["columns"][0]["expression"]["collect"]["column"] = json!(99);
    variants.push(req);
    let mut req = key_request();
    let extra = req["templates"][0].clone();
    req["templates"].as_array_mut().unwrap().push(extra);
    variants.push(req);
    let mut req = key_request();
    req["templates"][0]["filter"] =
        json!({"path":"filter","text":"TRUE","root":0,"nodes":[{"boolean":true}],"bindings":[]});
    variants.push(req);
    for req in variants {
        assert_eq!(
            execute_dataset(&req.to_string(), b"bad IPC").err(),
            Some(Error::InvalidPlan),
            "{req}"
        );
    }
    let prepared = PreparedDataset::parse(&key_request().to_string()).unwrap();
    for _ in 0..2 {
        let result = prepared
            .execute(&source(
                vec![Some(2), Some(1), Some(2)],
                vec![None, Some("8"), Some("7")],
            ))
            .unwrap();
        let snapshot: Value =
            serde_json::from_str(&table_snapshot(&result.table.unwrap()).unwrap()).unwrap();
        assert_eq!(
            snapshot["rows"],
            json!([[{"int":"2"},{"int":"7"}],[{"int":"1"},{"int":"8"}]])
        );
    }
}

/// Malformed numbering declarations fail before IPC, and admitted plans remain reusable.
#[test]
fn numbering_admission_precedes_ipc_and_repeated_execution_is_fresh() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/datasets/expected.json")).unwrap();
    let request = &cases
        .iter()
        .find(|case| case["case"] == "numbering_exact_descending")
        .unwrap()["request"];
    for field in ["source", "unexpected"] {
        let mut invalid = request.clone();
        invalid["columns"][2]["expression"]["number"][field] = json!(null);
        assert!(matches!(
            execute_dataset(&invalid.to_string(), b"invalid ipc"),
            Err(Error::InvalidRequest)
        ));
    }
    for order in [
        json!([]),
        json!([{"column":3,"descending":false,"nulls_first":false}]),
        json!([{"column":99,"descending":false,"nulls_first":false}]),
    ] {
        let mut invalid = request.clone();
        invalid["columns"][2]["expression"]["number"]["order_by"] = order;
        assert!(matches!(
            execute_dataset(&invalid.to_string(), b"invalid ipc"),
            Err(Error::InvalidPlan)
        ));
    }
    let plan = PreparedDataset::parse(&request.to_string()).unwrap();
    for _ in 0..2 {
        let result = plan
            .execute(include_bytes!("fixtures/datasets/numbering.arrow"))
            .unwrap();
        assert!(result.table.is_some());
        assert_eq!(
            serde_json::from_str::<Value>(&result.outcome).unwrap()["outcome"]["status"],
            "success"
        );
    }
}

/// Every filter occurrence is bound before IPC, even for empty or unreachable data.
#[test]
fn window_filter_bindings_cannot_read_source_or_uncompleted_outputs() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/datasets/expected.json")).unwrap();
    let request = &cases
        .iter()
        .find(|case| case["case"] == "window_filter_truth_and_ties")
        .unwrap()["request"];
    for read in [
        json!({"source":2}),
        json!({"column":3}),
        json!({"column":99}),
    ] {
        let mut invalid = request.clone();
        invalid["columns"][2]["expression"]["number"]["filter"]["bindings"][0]["read"] = read;
        assert!(matches!(
            execute_dataset(&invalid.to_string(), b"bad ipc"),
            Err(Error::InvalidPlan)
        ));
    }
}

/// Value-window wire requests preserve canonical signed offsets and complete donor admission.
#[test]
fn value_window_requests_reject_bad_offsets_and_unavailable_donors_before_ipc() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/datasets/expected.json")).unwrap();
    let request = &cases
        .iter()
        .find(|case| case["case"] == "window_values_neighbors_and_carry")
        .unwrap()["request"];
    for offset in ["+1", "01", "-0", "1.0", "9223372036854775808"] {
        let mut invalid = request.clone();
        invalid["columns"][2]["expression"]["window"]["kind"]["row_value"]["offset"] =
            json!(offset);
        assert!(matches!(
            execute_dataset(&invalid.to_string(), b"bad ipc"),
            Err(Error::InvalidRequest)
        ));
    }
    for (column, offset) in [(2, "0"), (3, "-1"), (99, "1")] {
        let mut invalid = request.clone();
        invalid["columns"][2]["expression"]["window"]["kind"] =
            json!({"row_value":{"column":column,"offset":offset}});
        assert!(matches!(
            execute_dataset(&invalid.to_string(), b"bad ipc"),
            Err(Error::InvalidPlan)
        ));
    }
    let mut invalid = request.clone();
    invalid["columns"][2]["expression"] =
        json!({"number":request["columns"][2]["expression"]["window"]});
    assert!(matches!(
        execute_dataset(&invalid.to_string(), b"bad ipc"),
        Err(Error::InvalidPlan)
    ));
}
