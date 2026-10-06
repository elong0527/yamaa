use serde_json::{json, Value};
use yamaa_adapters::predicate_syntax::{analyze_predicate, TransportError, MAX_REQUEST_BYTES};

/// All host bindings share the same explicitly authored wire truth.
#[test]
fn authored_wire_truth() {
    for line in include_str!("fixtures/predicate_syntax.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(
            analyze_predicate(fields[1]).unwrap(),
            fields[2],
            "{}",
            fields[0]
        );
    }
}

/// Decode one real service response without consulting a reference parser.
fn outcome(text: &str) -> Value {
    let request = json!({"protocol":"predicate-syntax/1","expression":text}).to_string();
    let result: Value = serde_json::from_str(&analyze_predicate(&request).unwrap()).unwrap();
    assert_eq!(result["protocol"], "predicate-syntax/1");
    result["outcome"].clone()
}

/// JSON source boundaries never coerce types, duplicate keys or policy overrides.
#[test]
fn strict_transport() {
    for input in [
        "{}",
        r#"{"protocol":"predicate-syntax/1","expression":null}"#,
        r#"{"protocol":"predicate-syntax/1","expression":true}"#,
        r#"{"protocol":"predicate-syntax/1","expression":"TRUE","expression":"FALSE"}"#,
        r#"{"protocol":"predicate-syntax/1","expression":"TRUE","limits":{}}"#,
        r#"{"protocol":"predicate-syntax/1","expression":"\ud800"}"#,
    ] {
        assert_eq!(
            analyze_predicate(input),
            Err(TransportError::InvalidRequest)
        );
    }
    assert_eq!(
        analyze_predicate(r#"{"protocol":"other","expression":"TRUE"}"#),
        Err(TransportError::UnsupportedProtocol)
    );
    assert_eq!(
        analyze_predicate(&" ".repeat(MAX_REQUEST_BYTES + 1)),
        Err(TransportError::RequestLimit)
    );
}

/// Logical parser/regex refusals stay distinct from invalid predicates and retry succeeds.
#[test]
fn limits_and_non_scalar_positions() {
    for (text, resource) in [
        (" ".repeat(65537), "bytes"),
        ("NOT ".repeat(70) + "TRUE", "depth"),
        (format!("A IN ({})", vec!["1"; 5000].join(",")), "tokens"),
    ] {
        let result = outcome(&text);
        assert_eq!(result["status"], "resource_limit");
        assert_eq!(result["phase"], "parse");
        assert_eq!(result["resource"], resource);
        assert_eq!(outcome("TRUE")["status"], "parsed");
    }
    let result = outcome("str_contains(A, 'a{1000001}')");
    assert_eq!(result["phase"], "regex_compile");
    assert_eq!(result["resource"], "repetition");
    let result = outcome("'\u{1f600}' = @");
    assert_eq!(result["position"], json!({"byte":9,"character":6}));
    assert_eq!(
        outcome("'\u{1f600}' = 'e\u{301}'")["ast"]["right"],
        json!({"kind":"literal","type":"str","value":"e\u{301}","position":6})
    );
}
