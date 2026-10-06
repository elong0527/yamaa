use serde_json::{json, Value};
use yamaa_adapters::regex_transport::{evaluate_regex, TransportError, MAX_REQUEST_BYTES};

/// Construct a caller request without consulting native output for expected truth.
fn request(pattern: &str, operation: Value) -> String {
    json!({"protocol":"regex/1","pattern":pattern,"operation":operation}).to_string()
}

/// Decode an owned outcome and require the explicit transport/grammar versions.
fn outcome(request: &str) -> Value {
    let response: Value = serde_json::from_str(&evaluate_regex(request).unwrap()).unwrap();
    assert_eq!(response["protocol"], "regex/1");
    assert_eq!(response["contract_version"], "2.0.0");
    response["outcome"].clone()
}

/// Both installed hosts replay these authored wire bytes without an engine oracle.
#[test]
fn shared_wire_truth() {
    for line in include_str!("fixtures/regex_transport.tsv").lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(
            evaluate_regex(fields[1]).unwrap(),
            fields[2],
            "{}",
            fields[0]
        );
    }
}

/// Search, full match, empty captures and unentered captures stay separate.
#[test]
fn authored_matching_outcomes() {
    for (source, subject, expected) in [
        (
            r"^(a)?()b$",
            "b",
            json!({"status":"matched","group_count":2,"groups":["b",null,""]}),
        ),
        (
            r"(?<=\1(a))b",
            "aab",
            json!({"status":"matched","group_count":1,"groups":["b","a"]}),
        ),
        (
            r"(?<x>a)\k<x>",
            "aa",
            json!({"status":"matched","group_count":1,"groups":["aa","a"]}),
        ),
        (
            r"a*",
            "b",
            json!({"status":"matched","group_count":0,"groups":[""]}),
        ),
        (r"a+", "b", json!({"status":"no_match","group_count":0})),
        (
            r"(\u{1d400})",
            "\u{1d400}",
            json!({"status":"matched","group_count":1,"groups":["\u{1d400}","\u{1d400}"]}),
        ),
    ] {
        assert_eq!(
            outcome(&request(source, json!({"kind":"search","subject":subject}))),
            expected,
            "{source}"
        );
    }
    assert_eq!(
        outcome(&request("a|ab", json!({"kind":"search","subject":"ab"}))),
        json!({"status":"matched","group_count":0,"groups":["a"]})
    );
    assert_eq!(
        outcome(&request(
            "a|ab",
            json!({"kind":"full_match","subject":"ab"})
        )),
        json!({"status":"matched","group_count":0,"groups":["ab"]})
    );
    assert_eq!(
        outcome(&request("a", json!({"kind":"full_match","subject":"ab"}))),
        json!({"status":"no_match","group_count":0})
    );
    assert_eq!(
        outcome(&request("(a)", json!({"kind":"compile"}))),
        json!({"status":"compiled","group_count":1})
    );
}

/// Closed JSON variants reject duplicate/unknown keys, coercions and unused data.
#[test]
fn malformed_transport_is_not_a_regex_outcome() {
    for value in [
        r#"{}"#,
        r#"null"#,
        r#"[]"#,
        r#"{"protocol":"regex/1","pattern":null,"operation":{"kind":"compile"}}"#,
        r#"{"protocol":"regex/1","pattern":"a","operation":{"kind":"compile","subject":"a"}}"#,
        r#"{"protocol":"regex/1","pattern":"a","extra":1,"operation":{"kind":"compile"}}"#,
        r#"{"protocol":"regex/1","pattern":"a","pattern":"b","operation":{"kind":"compile"}}"#,
        r#"{"protocol":"regex/1","pattern":"a","operation":{"kind":"search"}}"#,
        r#"{"protocol":"regex/1","pattern":"a","operation":{"kind":"search","subject":1}}"#,
        r#"{"protocol":"regex/1","pattern":"a","operation":{"kind":"search","subject":"a","subject":"b"}}"#,
        r#"{"protocol":"regex/1","pattern":"a","operation":{"kind":"search","subject":"a","extra":1}}"#,
        r#"{"protocol":"regex/1","pattern":"a","operation":{"kind":"compile","kind":"search"}}"#,
        r#"{"protocol":"regex/1","pattern":"a","operation":{"kind":"unknown"}}"#,
        r#"{"protocol":"regex/1","pattern":"\ud800","operation":{"kind":"compile"}}"#,
    ] {
        assert_eq!(
            evaluate_regex(value),
            Err(TransportError::InvalidRequest),
            "{value}"
        );
    }
    assert_eq!(
        evaluate_regex(r#"{"protocol":"other","pattern":"a","operation":{"kind":"compile"}}"#),
        Err(TransportError::UnsupportedProtocol)
    );
    assert_eq!(
        evaluate_regex(&" ".repeat(MAX_REQUEST_BYTES + 1)),
        Err(TransportError::RequestLimit)
    );
}

/// Invalid patterns win over subject matching policy and retain both coordinates.
#[test]
fn compilation_precedes_match_limits() {
    let subject = "a".repeat(500_000);
    let invalid = outcome(&request(
        "\u{e9}(a+)(?<=\\1)b",
        json!({"kind":"search","subject":subject}),
    ));
    assert_eq!(
        invalid,
        json!({"status":"invalid","condition":"invalid_regex","requirement":"REQ-0827",
        "position":{"byte":6,"character":5},"reason":"variable-length lookbehind"})
    );
    assert_eq!(
        outcome(&request("a", json!({"kind":"search","subject":subject}))),
        json!({"status":"resource_limit","phase":"match","resource":"state_cells","limit":1_000_000})
    );
    assert_eq!(
        outcome(&request(&"a".repeat(65_537), json!({"kind":"compile"}))),
        json!({"status":"resource_limit","phase":"compile","resource":"pattern_bytes","limit":65_536})
    );
    assert_eq!(
        outcome(&request("a", json!({"kind":"search","subject":"a"}))),
        json!({"status":"matched","group_count":0,"groups":["a"]})
    );
}

/// Repeated captures and JSON escaping cannot allocate an unbounded response.
#[test]
fn response_limit_discards_partial_captures_and_allows_retry() {
    let source = "(".repeat(32) + ".{8192}" + &")".repeat(32);
    let encoded = evaluate_regex(&request(
        &source,
        json!({"kind":"full_match","subject":"\0".repeat(8192)}),
    ))
    .unwrap();
    assert!(encoded.len() < 512);
    assert_eq!(
        serde_json::from_str::<Value>(&encoded).unwrap()["outcome"],
        json!({"status":"resource_limit","phase":"response","resource":"response_bytes","limit":1_048_576})
    );
    assert_eq!(
        outcome(&request("(.)", json!({"kind":"search","subject":"\0"}))),
        json!({"status":"matched","group_count":1,"groups":["\0","\0"]})
    );
}
