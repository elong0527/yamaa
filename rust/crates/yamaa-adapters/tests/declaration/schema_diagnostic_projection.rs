use serde_json::{json, Value};
use yamaa_adapters::{
    specification_diagnostics::capture_failure,
    specification_source::{CapturedSchema, Error, Source},
};

const BIG: &str = "922337203685477580812345678901234567890";
fn source(identity: &str, text: &str) -> Source {
    Source {
        identity: identity.into(),
        bytes: text.as_bytes().to_vec(),
    }
}

#[test]
fn captured_structural_projection_preserves_complete_ordered_constraint_outcome_and_authority() {
    let schema = CapturedSchema::admit_root(vec![source("schema.yaml", &format!(
        "version: '1.0'\npayload:\n  - schema_version: {{type: str}}\n  - alpha: {{type: str, values: [z, a, z], pattern: '^OK$', min_length: {BIG}}}\n  - items: {{type: 'list[int]', size: {BIG}}}\n"))],
        0, "payload", Default::default()).unwrap();
    let raw = "schema_version: '1.0'\nalpha: \"bad\\\"\\n\\u03BB\"\nitems: [1]\n";
    let error = schema
        .prepare_structural(source("held/entry.yaml", raw))
        .unwrap_err();
    drop(schema);
    let Error::Findings(captured) = &error else {
        panic!("held semantic report: {}", capture_failure(&error))
    };
    assert_eq!(captured.source().identity, "held/entry.yaml");
    assert_eq!(captured.source().bytes, raw.as_bytes());
    let held_bytes = captured.source().bytes.as_ptr();
    let held_nodes = captured.document().unwrap().nodes().as_ptr();
    let held_findings = captured.findings().as_ptr();
    let expected: Value = serde_json::from_str(&format!(r#"{{"protocol":"specification/prototype","outcome":{{"status":"invalid","diagnostics":[
        {{"phase":"validation","condition":"value_not_permitted","requirement":"REQ-0287","spec_paths":["alpha"],"context":{{"value":"bad\"\nλ","permitted":["z","a","z"]}}}},
        {{"phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["alpha"],"context":{{"value":"bad\"\nλ","pattern":"^OK$"}}}},
        {{"phase":"validation","condition":"minimum_length","requirement":"REQ-0287","spec_paths":["alpha"],"context":{{"minimum":{BIG}}}}},
        {{"phase":"validation","condition":"invalid_size","requirement":"REQ-0287","spec_paths":["items"],"context":{{"size":{BIG}}}}}
    ]}}}}"#)).unwrap();
    for _ in 0..2 {
        let encoded = capture_failure(&error);
        assert_eq!(serde_json::from_str::<Value>(&encoded).unwrap(), expected);
        assert!(encoded.contains(&format!("\"minimum\":{BIG}")));
        assert!(encoded.contains(&format!("\"size\":{BIG}")));
        assert_eq!(captured.source().bytes.as_ptr(), held_bytes);
        assert_eq!(captured.document().unwrap().nodes().as_ptr(), held_nodes);
        assert_eq!(captured.findings().as_ptr(), held_findings);
    }
}

#[test]
fn a_late_oversized_finding_refuses_the_whole_report_and_retains_the_original() {
    let schema = CapturedSchema::admit_root(vec![source("schema.yaml",
        "version: '1.0'\npayload:\n  - schema_version: {type: str}\n  - first: {type: str, values: [OK]}\n  - last: {type: str, values: [OK]}\n")],
        0, "payload", Default::default()).unwrap();
    let raw = format!(
        "schema_version: '1.0'\nfirst: '{}'\nlast: '{}'\n",
        "x".repeat(1_400_000),
        "y".repeat(1_400_000)
    );
    let error = schema
        .prepare_structural(source("held/entry.yaml", &raw))
        .unwrap_err();
    let Error::Findings(captured) = &error else {
        panic!(
            "both language findings remain held: {}",
            capture_failure(&error)
        )
    };
    assert_eq!(captured.findings().len(), 2);
    assert_eq!(captured.findings()[0].path, "first");
    assert_eq!(captured.findings()[1].path, "last");
    let original = captured.document().unwrap().nodes().as_ptr();
    for _ in 0..2 {
        assert_eq!(
            serde_json::from_str::<Value>(&capture_failure(&error)).unwrap(),
            json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"capture","code":"diagnostic_context_limit"}})
        );
        assert_eq!(captured.document().unwrap().nodes().as_ptr(), original);
        assert_eq!(captured.source().bytes, raw.as_bytes());
        assert_eq!(captured.findings().len(), 2);
    }
}
