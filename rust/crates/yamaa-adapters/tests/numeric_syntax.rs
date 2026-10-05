use serde_json::{json, Value};
use yamaa_adapters::numeric_syntax::{analyze_numeric, TransportError, MAX_REQUEST_BYTES};

/// Exercise the same strict wire service exported by both installed hosts.
fn outcome(expression: &str) -> Value {
    let request = json!({"protocol":"numeric-syntax/1","expression":expression}).to_string();
    let response: Value = serde_json::from_str(&analyze_numeric(&request).unwrap()).unwrap();
    assert_eq!(response["protocol"], "numeric-syntax/1");
    response["outcome"].clone()
}

/// Independent portable truth pins exact ASTs, ordered reads and diagnostic ownership.
#[test]
fn shared_truth_replays_exact_bytes() {
    for line in include_str!("fixtures/numeric_syntax.tsv").lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(
            analyze_numeric(fields[1]).unwrap(),
            fields[2],
            "{}",
            fields[0]
        );
    }
}

/// Literals stay text and grammar acceptance cannot grant evaluator capabilities.
#[test]
fn spelling_and_function_policy_remain_separate() {
    assert_eq!(
        outcome("0001e+9999")["ast"],
        json!({"kind":"number","type":"float","value":"0001e+9999"})
    );
    for expression in ["POWER(A,B)", "EXP(A)", "LN(A)"] {
        assert_eq!(outcome(expression)["status"], "parsed");
    }
    assert_eq!(
        outcome("COUNT(D.*)")["condition"],
        "invalid_numeric_expression"
    );
    assert_eq!(outcome("SUM(A)")["condition"], "prohibited_function");
}

/// Each parser policy is distinct from grammar diagnostics and failed calls never poison retry.
#[test]
fn limits_and_retry() {
    for (text, resource) in [
        ("A".repeat(65537), "bytes"),
        (format!("{}A{}", "(".repeat(70), ")".repeat(70)), "depth"),
        ("A+".repeat(4097) + "A", "tokens"),
        // Wide shallow calls exhaust arena nodes without exhausting tree depth.
        (format!("COALESCE({})", vec!["A"; 4096].join(",")), "tokens"),
        (
            format!("COALESCE((A+A),{})", vec!["(A)"; 2046].join(",")),
            "nodes",
        ),
    ] {
        let result = outcome(&text);
        assert_eq!(result["status"], "resource_limit");
        assert_eq!(result["resource"], resource);
        assert_eq!(outcome("A")["identifiers"], json!(["A"]));
    }
}

/// Closed transport rejects coercion, duplicate fields and caller-supplied policy changes.
#[test]
fn malformed_transport_has_no_language_outcome() {
    for request in [
        r#"{}"#,
        r#"{"protocol":"numeric-syntax/1","expression":null}"#,
        r#"{"protocol":"numeric-syntax/1","expression":1}"#,
        r#"{"protocol":"numeric-syntax/1","expression":"A","extra":1}"#,
        r#"{"protocol":"numeric-syntax/1","expression":"A","expression":"B"}"#,
        r#"{"protocol":"numeric-syntax/1","expression":"A","limits":{}}"#,
    ] {
        assert_eq!(
            analyze_numeric(request),
            Err(TransportError::InvalidRequest)
        );
    }
    assert_eq!(
        analyze_numeric(r#"{"protocol":"other","expression":"A"}"#),
        Err(TransportError::UnsupportedProtocol)
    );
    assert_eq!(
        analyze_numeric(&" ".repeat(MAX_REQUEST_BYTES + 1)),
        Err(TransportError::RequestLimit)
    );
}
