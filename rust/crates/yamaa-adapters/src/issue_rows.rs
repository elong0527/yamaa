//! Owned five-field issue records shared by the build and static-check facades.
//! Context is encoded in Rust so host data-frame conversion cannot narrow it.
use yamaa_core::json::Value;

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
    pub fn from_definition(
        definition: yamaa_core::diagnostic::Definition,
        paths: Vec<String>,
        context: yamaa_core::diagnostic::Context,
    ) -> Result<Self, InvalidIssue> {
        let value =
            crate::specification_diagnostics::portable_definition(definition, paths, context);
        Self::from_diagnostic(value)
    }
    pub fn from_core(diagnostic: yamaa_core::diagnostic::Diagnostic) -> Result<Self, InvalidIssue> {
        let value = crate::specification_diagnostics::portable_diagnostic(diagnostic);
        Self::from_diagnostic(value)
    }
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
        let context = crate::json_wire::encode(
            object
                .get("context")
                .filter(|value| value.is_object())
                .ok_or(InvalidIssue)?,
        );
        Ok(Self {
            phase: text("phase")?,
            condition: text("condition")?,
            requirement,
            spec_paths,
            context,
        })
    }

    pub fn as_value(&self) -> Value {
        crate::json_wire::project(self).expect("owned issue fields serialize")
    }

    /// Retained report observations expose admitted context data while installed
    /// issue tables retain JSON text. Both use this one five-field serializer.
    pub(crate) fn report_value(&self) -> Result<Value, InvalidIssue> {
        let context: Value = crate::json_wire::decode(&self.context).map_err(|_| InvalidIssue)?;
        if !context.is_object() {
            return Err(InvalidIssue);
        }
        let mut value = self.as_value();
        value["context"] = context;
        Ok(value)
    }
}

/// Shared representation-only projection of already admitted core facts.
pub(crate) fn report(
    phase: &str,
    condition: &str,
    requirement: Option<&str>,
    paths: Vec<String>,
    context: Value,
) -> Value {
    Issue {
        phase: phase.into(),
        condition: condition.into(),
        requirement: requirement.map(str::to_owned),
        spec_paths: paths,
        context: crate::json_wire::encode(&context),
    }
    .report_value()
    .expect("admitted core diagnostic context")
}

pub fn encode(issues: &[Issue]) -> String {
    crate::json_wire::encode(&Value::Array(issues.iter().map(Issue::as_value).collect()))
}

/// Final public issue-table collection. Raw facts and retained reports remain complete.
/// Taking the prefix here preserves authored stage order across one result.
pub(crate) fn collect(issues: impl IntoIterator<Item = Issue>) -> Vec<Issue> {
    issues
        .into_iter()
        .take(yamaa_core::limits::Limits::FINDINGS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    #[test]
    fn context_retains_full_integer_range_unicode_and_nested_values() {
        let issue = Issue::from_diagnostic(crate::test_diagnostic_truth::value("overflow-context"))
            .unwrap();
        assert_eq!(
            issue.context,
            r#"{"maximum":9223372036854775807,"minimum":-9223372036854775808,"values":[null,"é🙂",true]}"#
        );
        assert_eq!(issue.requirement, None);
        assert_eq!(issue.spec_paths, ["columns.é🙂.derivation"]);
        assert_eq!(encode(&[]), "[]");
        assert_eq!(
            crate::json_wire::decode(&encode(std::slice::from_ref(&issue))).unwrap(),
            Value::Array(vec![issue.as_value()])
        );
    }

    #[test]
    fn final_collector_preserves_the_first_ten_thousand_across_stages() {
        fn row(index: usize) -> Issue {
            Issue {
                phase: if index < 6_000 { "input" } else { "output" }.into(),
                condition: "finding".into(),
                requirement: None,
                spec_paths: vec![format!("columns.C{index}")],
                context: format!("{{\"index\":{index}}}"),
            }
        }
        for length in [0, 9_999, 10_000, 10_001] {
            let actual = collect((0..length).map(row));
            assert_eq!(actual.len(), length.min(10_000));
            for (index, actual) in actual.iter().enumerate() {
                assert_eq!(actual, &row(index));
            }
        }
        let mut complete = row(0);
        complete.context = format!("{{\"detail\":\"{}\"}}", "é🙂".repeat(50_000));
        assert_eq!(collect([complete.clone()]), [complete]);
    }

    #[test]
    fn malformed_diagnostics_cannot_silently_become_partial_issue_rows() {
        let valid = crate::test_diagnostic_truth::value("unknown-input-field");
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
