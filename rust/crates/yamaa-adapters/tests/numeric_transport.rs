use serde_json::{json, Value};
use yamaa_adapters::numeric_transport::{evaluate_numeric, NumericTransportError};

/// Independent lifecycle and boundary truth is shared with both installed hosts.
#[test]
fn shared_numeric_transport_vectors() {
    for row in include_str!("fixtures/numeric_transport.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 3);
        let actual = match evaluate_numeric(fields[1]) {
            Ok(value) => value,
            Err(error) => format!("error:{error}"),
        };
        assert_eq!(actual, fields[2], "{}", fields[0]);
    }
}

/// Construct normalized requests without asking either evaluator for expected truth.
fn request(expression: &str) -> Value {
    json!({"protocol":"numeric/1","expression":expression,"column_path":"columns.A","target":"int","bindings":[]})
}

/// Exercise parser and transport resource limits before any identifier resolution.
#[test]
fn limits_are_separate_from_language_failure_and_have_no_effects() {
    let too_long = "A".repeat(65_537);
    let outcome: Value =
        serde_json::from_str(&evaluate_numeric(&request(&too_long).to_string()).unwrap()).unwrap();
    assert_eq!(outcome["outcome"]["status"], "limit");
    assert_eq!(outcome["outcome"]["resource"], "bytes");
    assert_eq!(outcome["outcome"]["limit"], "65536");
    assert_eq!(
        outcome["outcome"]["spec_path"],
        "columns.A.derivation.value.compute"
    );
    assert_eq!(outcome["outcome"]["expression"], too_long);
    assert_eq!(outcome["handler_counts"], json!([]));
    assert_eq!(outcome["resolutions"], json!([]));
    let deep = format!("{}A{}", "(".repeat(70), ")".repeat(70));
    let result: Value =
        serde_json::from_str(&evaluate_numeric(&request(&deep).to_string()).unwrap()).unwrap();
    assert_eq!(result["outcome"]["status"], "limit");
    assert_eq!(result["outcome"]["resource"], "depth");
    assert_eq!(result["resolutions"], json!([]));
    assert_eq!(
        evaluate_numeric(&" ".repeat(1_048_577)),
        Err(NumericTransportError::RequestLimit)
    );
    let mut many = request("1");
    many["bindings"] = json!((0..4097)
        .map(|n| json!({"name":format!("A{n}"),"value":{"int":"1"}}))
        .collect::<Vec<_>>());
    assert_eq!(
        evaluate_numeric(&many.to_string()),
        Err(NumericTransportError::RequestLimit)
    );
}

/// Oversized diagnostics remain exact text and every call owns fresh state.
#[test]
fn long_diagnostic_and_repeated_calls() {
    let digits = "9".repeat(5000);
    let input = request(&digits).to_string();
    let output = evaluate_numeric(&input).unwrap();
    drop(input);
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        parsed["outcome"]["diagnostic"]["context"]["value"],
        json!({"str": digits})
    );
    let handled = json!({"protocol":"numeric/1","expression":"1.5","column_path":"columns.A","target":"int","bindings":[],"unconvertible":{"value":{"int":"7"}}}).to_string();
    for _ in 0..100 {
        let result: Value = serde_json::from_str(&evaluate_numeric(&handled).unwrap()).unwrap();
        assert_eq!(
            result["outcome"],
            json!({"status":"value","value":{"int":"7"}})
        );
        assert_eq!(result["handler_counts"][0]["count"], "1");
    }
    assert_eq!(
        parsed["outcome"]["diagnostic"]["context"]["value"],
        json!({"str": digits})
    );
}

/// Strict request decoding cannot silently select one duplicate or erase a null handler.
#[test]
fn malformed_fields_do_not_change_request_meaning() {
    for malformed in [
        r#"{"protocol":"numeric/1","expression":"1","expression":"2","column_path":"columns.A","target":"int","bindings":[]}"#,
        r#"{"protocol":"numeric/1","expression":"1","column_path":"columns.A","target":"int","bindings":[],"unconvertible":null}"#,
        r#"{"protocol":"numeric/1","expression":"1","column_path":"columns.A","target":"int","bindings":[],"unconvertible":{"value":{"missing":null}},"unconvertible":{"value":{"int":"7"}}}"#,
        r#"{"protocol":"numeric/1","expression":"A","column_path":"columns.A","target":"int","bindings":[{"name":"A","name":"B","value":{"int":"1"}}]}"#,
    ] {
        assert_eq!(
            evaluate_numeric(malformed),
            Err(NumericTransportError::InvalidRequest)
        );
    }
    let valid = evaluate_numeric(&request("1").to_string()).unwrap();
    let outcome: Value = serde_json::from_str(&valid).unwrap();
    assert_eq!(
        outcome["outcome"],
        json!({"status":"value","value":{"int":"1"}})
    );
}
