use serde_json::{json, Value};
use yamaa_adapters::aggregate_transport::{analyze_aggregate, TransportError, MAX_REQUEST_BYTES};

/// Exercise the real bounded transport rather than reproducing the serializer.
fn outcome(expression: &str) -> Value {
    let request = json!({"protocol":"aggregate-syntax/1","expression":expression}).to_string();
    let response: Value = serde_json::from_str(&analyze_aggregate(&request).unwrap()).unwrap();
    assert_eq!(response["protocol"], "aggregate-syntax/1");
    response["outcome"].clone()
}

/// Independent AST truth pins spelling, canonical operators and relation-only reads.
#[test]
fn exact_ast_and_metadata() {
    assert_eq!(
        outcome(" count( SRC.* ) "),
        json!({"status":"parsed","ast":{"kind":"reduction","name":"COUNT","argument":{"kind":"star","dataset":"SRC"},"text":"count( SRC.* )"},"identifiers":[],"star_datasets":["SRC"],"ungrouped_identifiers":[]})
    );
    assert_eq!(
        outcome("(SUM(A)) + A")["ungrouped_identifiers"],
        json!(["A"])
    );
    assert_eq!(
        outcome("0001e+9999")["ast"],
        json!({"kind":"number","type":"float","value":"0001e+9999"})
    );
}

/// Failure positions count Unicode scalars separately from bytes; contexts are exact.
#[test]
fn portable_diagnostics_and_resources() {
    assert_eq!(
        outcome("\u{2003}SUM(MIN(A))"),
        json!({"status":"invalid","condition":"nested_reduction","requirement":"REQ-0502","position":{"byte":3,"character":1},"context":{"outer":"SUM","inner":"MIN"}})
    );
    assert_eq!(
        outcome("AbS(1,2)")["context"],
        json!({"function":"AbS","argument_count":2})
    );
    assert_eq!(
        outcome("unknown(1)")["context"],
        json!({"function":"unknown"})
    );
    assert_eq!(
        outcome("SUM(A) OR B")["context"],
        json!({"construct":"boolean"})
    );
    assert_eq!(outcome(&"A".repeat(65537))["resource"], "bytes");
    assert_eq!(
        outcome(&format!("{}A{}", "(".repeat(70), ")".repeat(70)))["resource"],
        "depth"
    );
    // A failed request does not poison the next independent parse.
    assert_eq!(outcome("SUM(A)")["status"], "parsed");
}

/// Closed metadata refuses coercion, duplicates, extensions and raised budgets.
#[test]
fn malformed_transport_has_no_language_outcome() {
    for request in [
        r#"{}"#,
        r#"{"protocol":"aggregate-syntax/1","expression":null}"#,
        r#"{"protocol":"aggregate-syntax/1","expression":1}"#,
        r#"{"protocol":"aggregate-syntax/1","expression":"A","extra":1}"#,
        r#"{"protocol":"aggregate-syntax/1","expression":"A","expression":"B"}"#,
        r#"{"protocol":"aggregate-syntax/1","expression":"A","limits":{}}"#,
    ] {
        assert_eq!(
            analyze_aggregate(request),
            Err(TransportError::InvalidRequest),
            "{request}"
        );
    }
    assert_eq!(
        analyze_aggregate(r#"{"protocol":"unknown","expression":"A"}"#),
        Err(TransportError::UnsupportedProtocol)
    );
    assert_eq!(
        analyze_aggregate(&" ".repeat(MAX_REQUEST_BYTES + 1)),
        Err(TransportError::RequestLimit)
    );
}

/// Replay independently authored portable expectations also used by installed hosts.
#[test]
fn shared_truth_replays_exact_bytes() {
    for line in include_str!("fixtures/aggregate_syntax.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(
            analyze_aggregate(fields[1]).unwrap(),
            fields[2],
            "{}",
            fields[0]
        );
    }
}
