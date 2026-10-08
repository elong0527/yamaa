//! Owned five-field issue records shared by the build and static-check facades.
//! Context is encoded in Rust so host data-frame conversion cannot narrow it.
use serde_json::{json, Value};
use std::io::Write;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Issue {
    pub phase: String,
    pub condition: String,
    pub requirement: Option<String>,
    pub spec_paths: Vec<String>,
    pub context: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidIssue;

impl Issue {
    /// Accept only the complete portable diagnostic shape, not a prototype envelope.
    pub fn from_diagnostic(value: Value) -> Result<Self, InvalidIssue> {
        let object = value.as_object().ok_or(InvalidIssue)?;
        if object.len() != 5 {
            return Err(InvalidIssue);
        }
        let text = |name: &str| {
            object
                .get(name)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(InvalidIssue)
        };
        let requirement = match object.get("requirement") {
            Some(Value::Null) => None,
            Some(Value::String(value)) => Some(value.clone()),
            _ => return Err(InvalidIssue),
        };
        let spec_paths = object
            .get("spec_paths")
            .and_then(Value::as_array)
            .ok_or(InvalidIssue)?
            .iter()
            .map(|value| value.as_str().map(str::to_owned).ok_or(InvalidIssue))
            .collect::<Result<Vec<_>, _>>()?;
        let context = object
            .get("context")
            .filter(|value| value.is_object())
            .ok_or(InvalidIssue)?
            .to_string();
        Ok(Self {
            phase: text("phase")?,
            condition: text("condition")?,
            requirement,
            spec_paths,
            context,
        })
    }

    pub fn as_value(&self) -> Value {
        json!({
            "phase": self.phase,
            "condition": self.condition,
            "requirement": self.requirement,
            "spec_paths": self.spec_paths,
            "context": self.context,
        })
    }
}

pub fn encode(issues: &[Issue]) -> String {
    Value::Array(issues.iter().map(Issue::as_value).collect()).to_string()
}

/// Bound the actual host issue payload, including the JSON-text context escaping.
/// Count serialization bytes without allocating a second copy of the payload.
pub fn within_limit(issues: &[Issue], maximum: usize) -> bool {
    struct Counter {
        remaining: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.remaining = self
                .remaining
                .checked_sub(bytes.len())
                .ok_or_else(|| std::io::Error::other("issue byte limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Counter { remaining: maximum }, issues).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_retains_full_integer_range_unicode_and_nested_values() {
        let issue = Issue::from_diagnostic(json!({
            "phase":"derivation", "condition":"integer_overflow", "requirement":null,
            "spec_paths":["columns.é🙂.derivation"],
            "context":{"minimum":i64::MIN,"maximum":i64::MAX,"values":[null,"é🙂",true]}
        }))
        .unwrap();
        assert_eq!(
            issue.context,
            r#"{"maximum":9223372036854775807,"minimum":-9223372036854775808,"values":[null,"é🙂",true]}"#
        );
        assert_eq!(issue.requirement, None);
        assert_eq!(issue.spec_paths, ["columns.é🙂.derivation"]);
        assert_eq!(encode(&[]), "[]");
        let length = encode(std::slice::from_ref(&issue)).len();
        assert!(within_limit(std::slice::from_ref(&issue), length));
        assert!(!within_limit(std::slice::from_ref(&issue), length - 1));
        assert!(within_limit(&[], 2));
        assert!(!within_limit(&[], 1));
        assert_eq!(
            serde_json::from_str::<Value>(&encode(std::slice::from_ref(&issue))).unwrap(),
            json!([issue.as_value()])
        );
    }

    #[test]
    fn malformed_diagnostics_cannot_silently_become_partial_issue_rows() {
        let valid = json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0103","spec_paths":["input.SRC"],"context":{}});
        for field in ["phase", "condition", "requirement", "spec_paths", "context"] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert_eq!(Issue::from_diagnostic(missing), Err(InvalidIssue));
        }
        for (field, value) in [
            ("phase", json!(null)),
            ("condition", json!(2)),
            ("requirement", json!([])),
            ("spec_paths", json!([false])),
            ("context", json!("{}")),
        ] {
            let mut wrong = valid.clone();
            wrong[field] = value;
            assert_eq!(Issue::from_diagnostic(wrong), Err(InvalidIssue));
        }
        let mut extra = valid;
        extra["severity"] = json!("error");
        assert_eq!(Issue::from_diagnostic(extra), Err(InvalidIssue));
    }
}
