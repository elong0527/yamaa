//! Approved #1973 item 7 citation disposition for unchanged private reference captures.
//! Candidate reports never supply expected facts or select the reconciliation.
use serde_json::Value;

pub fn reconciled(text: &str, resolved_output: bool) -> Value {
    let mut expected: Value = serde_json::from_str(text).unwrap();
    reconcile(&mut expected, resolved_output);
    expected
}
fn reason(context: &Value) -> Option<&str> {
    context.get("reason").and_then(Value::as_str).or_else(|| {
        context
            .as_array()?
            .iter()
            .find(|v| v["name"] == "reason")?
            .get("value")?
            .get("value")?
            .as_str()
    })
}
fn reconcile(value: &mut Value, resolved_output: bool) {
    match value {
        Value::Array(values) => {
            for value in values {
                reconcile(value, resolved_output);
            }
        }
        Value::Object(fields) => {
            let diagnostic = fields.contains_key("condition")
                && fields.contains_key("requirement")
                && fields.contains_key("context")
                && (fields.contains_key("path") || fields.contains_key("spec_paths"))
                && fields
                    .get("phase")
                    .is_none_or(|phase| phase == "validation");
            if !diagnostic {
                for key in ["outcome", "results", "diagnostics"] {
                    if let Some(value) = fields.get_mut(key) {
                        reconcile(value, resolved_output);
                    }
                }
                return;
            }
            // Context stays source data even when it resembles a diagnostic.
            if fields.get("requirement").is_some_and(Value::is_null) {
                let requirement = match fields.get("condition").and_then(Value::as_str) {
                    Some("schema_version_mismatch") => Some("REQ-0281"),
                    Some("unknown_field") => Some("REQ-0285"),
                    Some("missing_required_field")
                        if resolved_output
                            && fields.get("path").and_then(Value::as_str) == Some("output") =>
                    {
                        Some("REQ-0657")
                    }
                    Some("missing_required_field") => Some("REQ-0285"),
                    Some("invalid_operation_count" | "unknown_operation") => Some("REQ-0286"),
                    Some("model_contract_mismatch") => {
                        Some(match fields.get("context").and_then(reason) {
                            Some("Field required" | "Extra inputs are not permitted") => "REQ-0285",
                            Some(
                                "Value error, an expression must contain exactly one operation",
                            ) => "REQ-0286",
                            _ => "REQ-0287",
                        })
                    }
                    _ => None,
                };
                if let Some(requirement) = requirement {
                    fields.insert("requirement".into(), Value::String(requirement.into()));
                }
            }
        }
        _ => {}
    }
}

#[test]
fn citation_disposition_never_rewrites_diagnostic_shaped_source_context() {
    let historical = serde_json::json!({"diagnostics": [{
        "condition": "unknown_field", "requirement": null, "path": "extra",
        "context": {"actual": {"condition": "missing_required_field",
            "requirement": null, "path": "spoof", "context": {"reason": "source data"}}}
    }]});
    let reconciled = reconciled(&historical.to_string(), false);
    assert_eq!(reconciled["diagnostics"][0]["requirement"], "REQ-0285");
    assert_eq!(
        reconciled["diagnostics"][0]["context"],
        historical["diagnostics"][0]["context"]
    );
}
