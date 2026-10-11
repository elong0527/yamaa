//! Represent core-admitted schema findings without reparsing source YAML.
use crate::specification_source::CapturedFindings;
use serde_json::{json, Map, Number, Value};
pub(crate) use yamaa_core::schema::SchemaDiagnosticError as Error;
use yamaa_core::schema::{
    admit_schema_diagnostics, Document, SchemaDiagnostic, SchemaDiagnosticBudget,
    SchemaDiagnosticValue as V, SchemaStructure,
};

pub(crate) struct Budget(SchemaDiagnosticBudget);
impl Budget {
    pub(crate) fn new() -> Self {
        Self(SchemaDiagnosticBudget::new())
    }
    fn charge(&mut self, bytes: usize) -> Result<(), Error> {
        self.0.charge(bytes)
    }
    fn text(&mut self, text: &str) -> Result<Value, Error> {
        self.0.text(text)?;
        Ok(json!(text))
    }
}
fn value(value: V<'_>) -> Value {
    match value {
        V::Text(text) => json!(text),
        V::Count(count) => json!(count),
        V::Null => Value::Null,
        V::Boolean(value) => json!(value),
        V::Float(value) => json!(value),
        V::Integer(text) => Value::Number(
            text.parse::<Number>()
                .expect("admitted canonical schema integer"),
        ),
        V::TextList(values) => json!(values),
    }
}
pub(super) fn findings(captured: &CapturedFindings) -> Result<Vec<Value>, Error> {
    schema_findings(
        captured.schema().structure(),
        captured.document(),
        captured.findings(),
        &[],
    )
}
pub(super) fn schema_findings(
    schema: &SchemaStructure,
    document: Option<&Document>,
    findings: &[SchemaDiagnostic],
    source_context: &[(&str, &str)],
) -> Result<Vec<Value>, Error> {
    schema_findings_with_budget(
        schema,
        document,
        findings,
        source_context,
        &mut Budget::new(),
    )
}
pub(super) fn schema_findings_with_budget(
    schema: &SchemaStructure,
    document: Option<&Document>,
    findings: &[SchemaDiagnostic],
    source_context: &[(&str, &str)],
    budget: &mut Budget,
) -> Result<Vec<Value>, Error> {
    let report =
        admit_schema_diagnostics(schema, document, findings, source_context, &mut budget.0)?;
    Ok(report.iter().map(|finding| {
        let context = finding.context.map(|(name, field)| (name.into(), value(field))).collect::<Map<_, _>>();
        json!({"phase":finding.phase, "condition":finding.condition, "requirement":finding.requirement,
            "spec_paths":[finding.path], "context":context})
    }).collect())
}

/// Convert already-rendered traversal/dependency findings with literal context.
/// Schema-node/descriptor references must use schema_findings with their held arena.
pub(super) fn literal_outcome(mut outcome: Value) -> Result<Value, Error> {
    if outcome["status"] != "invalid" {
        return Ok(outcome);
    }
    let mut budget = Budget::new();
    let Value::Array(findings) = outcome["diagnostics"].take() else {
        return Err(Error::InvalidContext);
    };
    let mut records = Vec::new();
    for mut finding in findings {
        budget.charge(128)?;
        let condition = budget.text(finding["condition"].as_str().ok_or(Error::InvalidContext)?)?;
        let path = budget.text(finding["path"].as_str().ok_or(Error::InvalidContext)?)?;
        let requirement = match &finding["requirement"] {
            Value::Null => Value::Null,
            Value::String(text) => budget.text(text)?,
            _ => return Err(Error::InvalidContext),
        };
        let Value::Array(context) = finding["context"].take() else {
            return Err(Error::InvalidContext);
        };
        let mut resolved = Map::new();
        for item in context {
            let name = item["name"].as_str().ok_or(Error::InvalidContext)?;
            budget.text(name)?;
            let reference = &item["value"];
            let value = match reference["kind"].as_str() {
                Some("text") => {
                    budget.text(reference["value"].as_str().ok_or(Error::InvalidContext)?)?
                }
                Some("count") => {
                    budget.charge(20)?;
                    json!(reference["value"].as_u64().ok_or(Error::InvalidContext)?)
                }
                Some("null") => Value::Null,
                Some("text_list") => {
                    let values = reference["value"].as_array().ok_or(Error::InvalidContext)?;
                    budget.charge(values.len().saturating_add(2))?;
                    Value::Array(
                        values
                            .iter()
                            .map(|v| budget.text(v.as_str().ok_or(Error::InvalidContext)?))
                            .collect::<Result<_, _>>()?,
                    )
                }
                _ => return Err(Error::InvalidContext),
            };
            if resolved.insert(name.into(), value).is_some() {
                return Err(Error::InvalidContext);
            }
        }
        for name in ["source", "entry", "parent"] {
            if let Some(value) = outcome.get(name) {
                budget.text(name)?;
                let value = match value {
                    Value::Null => Value::Null,
                    Value::String(text) => budget.text(text)?,
                    _ => return Err(Error::InvalidContext),
                };
                if resolved.insert(name.into(), value).is_some() {
                    return Err(Error::InvalidContext);
                }
            }
        }
        records.push(json!({"phase":"validation","condition":condition,"requirement":requirement,"spec_paths":[path],"context":resolved}));
    }
    Ok(json!({"status":"invalid","diagnostics":records}))
}
