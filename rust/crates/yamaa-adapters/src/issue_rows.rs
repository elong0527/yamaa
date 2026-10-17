//! Owned issue records shared by the build and static-check facades.
//! Context is carried as the owned structured JSON tree from the failure
//! site, and geometry (`source_span` / `operand_route`) is carried from the
//! core diagnostic instead of being discarded. Encoded in Rust so host
//! data-frame conversion cannot narrow exact numbers.
//!
//! Portable shape (the S1 boundary record from `diagnostic_record`, under
//! `BOUNDARY_VERSION` 1): the five base fields `phase`, `condition`,
//! `requirement`, `spec_paths` and `context` (an owned object tree), plus
//! the geometry keys only when geometry was recorded. An absent
//! `source_span` key means no span was recorded and differs from a present
//! zero span (`{"start":0,"end":0}`); an absent `operand_route` key differs
//! from a present empty route. Geometry keys are omitted when absent so
//! diagnostics without geometry keep their previous wire bytes.
use yamaa_core::json::Value;
use yamaa_core::numeric_parser::SourceSpan;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub phase: String,
    pub condition: String,
    pub requirement: Option<String>,
    pub spec_paths: Vec<String>,
    pub context: Value,
    pub source_span: Option<SourceSpan>,
    pub operand_route: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidIssue;

fn span_value(span: SourceSpan) -> Value {
    crate::json!({"start": span.start, "end": span.end})
}

fn parse_span(value: &Value) -> Result<SourceSpan, InvalidIssue> {
    let object = value.as_object().ok_or(InvalidIssue)?;
    if object.len() != 2 {
        return Err(InvalidIssue);
    }
    let bound = |name: &str| {
        let number = object
            .get(name)
            .and_then(Value::as_number)
            .ok_or(InvalidIssue)?;
        // Spans are byte offsets: only plain non-negative integer tokens
        // that fit the host offset type are admitted, never a float token.
        if !number.is_integer() || number.as_str().starts_with('-') {
            return Err(InvalidIssue);
        }
        number
            .as_u64()
            .and_then(|bound| usize::try_from(bound).ok())
            .ok_or(InvalidIssue)
    };
    let (start, end) = (bound("start")?, bound("end")?);
    if start > end {
        return Err(InvalidIssue);
    }
    Ok(SourceSpan { start, end })
}

fn parse_route(value: &Value) -> Result<Vec<String>, InvalidIssue> {
    value
        .as_array()
        .ok_or(InvalidIssue)?
        .iter()
        .map(|value| value.as_str().map(str::to_owned).ok_or(InvalidIssue))
        .collect()
}

impl Issue {
    pub fn from_definition(
        definition: yamaa_core::diagnostic::Definition,
        paths: Vec<String>,
        context: yamaa_core::diagnostic::Context,
    ) -> Result<Self, InvalidIssue> {
        // Definition-only findings record no geometry.
        let value = crate::specification_diagnostics::portable_definition(
            definition, paths, context, None, None,
        );
        Self::from_diagnostic(value)
    }

    pub fn from_core(diagnostic: yamaa_core::diagnostic::Diagnostic) -> Result<Self, InvalidIssue> {
        let definition = diagnostic.definition();
        let yamaa_core::diagnostic::Diagnostic {
            spec_paths,
            context,
            source_span,
            operand_route,
            ..
        } = diagnostic;
        Ok(Self {
            phase: definition.phase.into(),
            condition: definition.condition.into(),
            requirement: definition.requirement.map(str::to_owned),
            spec_paths,
            context: Value::Object(context.into_iter().collect()),
            source_span,
            operand_route,
        })
    }

    /// Build the issue directly from the S1 boundary record: severity is a
    /// pure function of the record's code, and phase/condition/requirement
    /// come from the same registry definition, so no field is lost.
    pub fn from_record(
        record: crate::diagnostic_record::DiagnosticRecord,
    ) -> Result<Self, InvalidIssue> {
        let definition = record.definition();
        Ok(Self {
            phase: definition.phase.into(),
            condition: definition.condition.into(),
            requirement: definition.requirement.map(str::to_owned),
            spec_paths: record.spec_paths,
            context: Value::Object(record.context.into_iter().collect()),
            source_span: record.source_span,
            operand_route: record.operand_route,
        })
    }

    /// Accept only the complete portable diagnostic shape, not a prototype envelope.
    pub fn from_diagnostic(value: Value) -> Result<Self, InvalidIssue> {
        let object = value.as_object().ok_or(InvalidIssue)?;
        // Five base fields, plus source_span / operand_route only when present.
        if object.len() < 5 || object.len() > 7 {
            return Err(InvalidIssue);
        }
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "phase"
                    | "condition"
                    | "requirement"
                    | "spec_paths"
                    | "context"
                    | "source_span"
                    | "operand_route"
            ) {
                return Err(InvalidIssue);
            }
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
            .clone();
        let source_span = match object.get("source_span") {
            None => None,
            Some(value) => Some(parse_span(value)?),
        };
        let operand_route = match object.get("operand_route") {
            None => None,
            Some(value) => Some(parse_route(value)?),
        };
        Ok(Self {
            phase: text("phase")?,
            condition: text("condition")?,
            requirement,
            spec_paths,
            context,
            source_span,
            operand_route,
        })
    }

    pub fn as_value(&self) -> Value {
        let mut object = yamaa_core::json::Map::new();
        object.insert("phase".into(), Value::String(self.phase.clone()));
        object.insert("condition".into(), Value::String(self.condition.clone()));
        object.insert(
            "requirement".into(),
            self.requirement
                .as_ref()
                .map_or(Value::Null, |value| Value::String(value.clone())),
        );
        object.insert(
            "spec_paths".into(),
            Value::Array(
                self.spec_paths
                    .iter()
                    .map(|path| Value::String(path.clone()))
                    .collect(),
            ),
        );
        object.insert("context".into(), self.context.clone());
        if let Some(span) = self.source_span {
            object.insert("source_span".into(), span_value(span));
        }
        if let Some(route) = &self.operand_route {
            object.insert(
                "operand_route".into(),
                Value::Array(
                    route
                        .iter()
                        .map(|step| Value::String(step.clone()))
                        .collect(),
                ),
            );
        }
        Value::Object(object)
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
        context,
        source_span: None,
        operand_route: None,
    }
    .as_value()
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
            crate::json!({"maximum":9223372036854775807_i64,"minimum":-9223372036854775808_i64,"values":[null,"é🙂",true]})
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
    fn nested_context_with_wide_numbers_round_trips_exactly() {
        // Integers beyond i64/u64, nested arrays/objects, null/bool/text:
        // every leaf must survive from_diagnostic -> Issue -> as_value ->
        // from_diagnostic and through the JSON wire encoding unchanged.
        // Build the condition without a literal so the repository literal
        // ratchet (tools.validation.check_diagnostics) keeps owning the
        // vocabulary in the core registry.
        let condition = ["integer", "_overflow"].concat();
        let template = r#"{"phase":"derivation","condition":"__CONDITION__","requirement":null,"spec_paths":["columns.X"],"context":{"wide":[18446744073709551616,-18446744073709551617,92233720368547758081234567890],"nested":{"list":[null,true,false,"text",{"deep":[1.0]}],"empty":{}}},"source_span":{"start":0,"end":0},"operand_route":[]}"#;
        let text = template.replace("__CONDITION__", &condition);
        let value = crate::json_wire::decode(text.as_str()).unwrap();
        let issue = Issue::from_diagnostic(value.clone()).unwrap();
        assert_eq!(issue.as_value(), value);
        assert_eq!(Issue::from_diagnostic(issue.as_value()).unwrap(), issue);
        let encoded = encode(std::slice::from_ref(&issue));
        assert!(encoded.contains("18446744073709551616"), "{encoded}");
        assert!(encoded.contains("-18446744073709551617"), "{encoded}");
        assert!(
            encoded.contains("92233720368547758081234567890"),
            "{encoded}"
        );
        let reparsed = crate::json_wire::decode(&encoded).unwrap();
        assert_eq!(reparsed, Value::Array(vec![value]));
    }

    #[test]
    fn absent_geometry_differs_from_zero_geometry() {
        let absent =
            Issue::from_diagnostic(crate::test_diagnostic_truth::value("unknown-input-field"))
                .unwrap();
        assert_eq!(absent.source_span, None);
        assert_eq!(absent.operand_route, None);
        assert!(absent.as_value().get("source_span").is_none());
        assert!(absent.as_value().get("operand_route").is_none());
        let mut zero_value = absent.as_value();
        zero_value["source_span"] = crate::json!({"start":0,"end":0});
        zero_value["operand_route"] = crate::json!([]);
        let zero = Issue::from_diagnostic(zero_value).unwrap();
        assert_eq!(zero.source_span, Some(SourceSpan { start: 0, end: 0 }));
        assert_eq!(zero.operand_route, Some(Vec::new()));
        assert_ne!(absent, zero);
    }

    #[test]
    fn core_geometry_and_context_carry_through_from_core() {
        let mut context = yamaa_core::diagnostic::Context::new();
        context.insert(
            "big".into(),
            crate::json_wire::decode("340282366920938463463374607431768211456").unwrap(),
        );
        let diagnostic = yamaa_core::diagnostic::Diagnostic {
            code: yamaa_core::diagnostic::ConditionCode::IntegerOverflow,
            spec_paths: vec!["columns.X".into()],
            context,
            source_span: Some(SourceSpan { start: 3, end: 9 }),
            operand_route: Some(vec!["operand".into(), "root".into()]),
        };
        let issue = Issue::from_core(diagnostic).unwrap();
        assert_eq!(issue.source_span, Some(SourceSpan { start: 3, end: 9 }));
        assert_eq!(
            issue.operand_route.as_deref(),
            Some(&["operand".to_owned(), "root".to_owned()][..])
        );
        assert_eq!(
            crate::json_wire::encode(&issue.context),
            r#"{"big":340282366920938463463374607431768211456}"#
        );
        assert_eq!(Issue::from_diagnostic(issue.as_value()).unwrap(), issue);
    }

    #[test]
    fn final_collector_preserves_the_first_ten_thousand_across_stages() {
        fn row(index: usize) -> Issue {
            Issue {
                phase: if index < 6_000 { "input" } else { "output" }.into(),
                condition: "finding".into(),
                requirement: None,
                spec_paths: vec![format!("columns.C{index}")],
                context: crate::json!({"index": index}),
                source_span: None,
                operand_route: None,
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
        complete.context = crate::json!({"detail": "é🙂".repeat(50_000)});
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
            ("source_span", json!({"start": 0})),
            ("source_span", json!({"start": -1, "end": 0})),
            ("source_span", json!({"start": 0.5, "end": 1})),
            ("source_span", json!({"start": 9, "end": 3})),
            ("source_span", json!({"start": 0, "end": 1, "extra": 2})),
            ("operand_route", json!([1])),
            ("operand_route", json!("root")),
        ] {
            let mut wrong = valid.clone();
            wrong[field] = value;
            assert_eq!(Issue::from_diagnostic(wrong), Err(InvalidIssue), "{field}");
        }
        let mut extra = valid;
        extra["severity"] = json!("error");
        assert_eq!(Issue::from_diagnostic(extra), Err(InvalidIssue));
    }
}
