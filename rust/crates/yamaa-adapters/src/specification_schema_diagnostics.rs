//! Resolve schema findings against held pass inputs, without reparsing source YAML.
use crate::specification_source::CapturedFindings;
use serde_json::{json, Map, Number, Value};
use yamaa_core::schema::{
    Document, DocumentNode as N, SchemaContext as C, SchemaDiagnostic, SchemaStructure,
};

const MAX_BYTES: usize = 16_777_216;
#[derive(Debug)]
pub(super) enum Error {
    Limit,
    InvalidContext,
}
struct Budget(usize);
impl Budget {
    fn charge(&mut self, bytes: usize) -> Result<(), Error> {
        self.0 = self
            .0
            .checked_add(bytes)
            .filter(|&n| n <= MAX_BYTES)
            .ok_or(Error::Limit)?;
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<Value, Error> {
        // Each source byte can expand to at most six JSON bytes; charge before copying.
        self.charge(
            text.len()
                .checked_mul(6)
                .and_then(|n| n.checked_add(2))
                .ok_or(Error::Limit)?,
        )?;
        Ok(json!(text))
    }
    fn integer(&mut self, text: &str) -> Result<Value, Error> {
        self.charge(text.len())?;
        // Schema integers have mathematical identity, including values outside i64.
        Ok(Value::Number(
            text.parse::<Number>().map_err(|_| Error::InvalidContext)?,
        ))
    }
}
fn context(
    schema: &SchemaStructure,
    document: Option<&Document>,
    value: &C,
    budget: &mut Budget,
) -> Result<Value, Error> {
    Ok(match value {
        C::Text(text) => budget.text(text)?,
        C::ValueKind(_) | C::LayerKind(_) => budget
            .text(crate::schema_diagnostic_kinds::label(value).ok_or(Error::InvalidContext)?)?,
        C::Count(value) => {
            budget.charge(20)?;
            json!(value)
        }
        C::Null => {
            budget.charge(4)?;
            Value::Null
        }
        C::InputValue(node) => {
            let node = document
                .and_then(|d| d.nodes().get(*node))
                .ok_or(Error::InvalidContext)?;
            match node {
                N::Null => {
                    budget.charge(4)?;
                    Value::Null
                }
                N::Boolean(value) => {
                    budget.charge(5)?;
                    json!(value)
                }
                N::Integer(text) => budget.integer(text)?,
                N::Float(value) => {
                    budget.charge(32)?;
                    json!(value)
                }
                N::Text(text) => budget.text(text)?,
                // Core schema findings refer to scalar values; collections use actual_type.
                N::Sequence(_) | N::Mapping(_) => return Err(Error::InvalidContext),
            }
        }
        C::DescriptorValues(id)
        | C::DescriptorPattern(id)
        | C::DescriptorMinimum(id)
        | C::DescriptorSize(id) => {
            let descriptor = &schema
                .descriptors()
                .get(*id)
                .ok_or(Error::InvalidContext)?
                .descriptor;
            match value {
                C::DescriptorValues(_) => {
                    let values = descriptor.permitted().ok_or(Error::InvalidContext)?;
                    budget.charge(values.len().saturating_add(2))?;
                    Value::Array(
                        values
                            .iter()
                            .map(|text| budget.text(text))
                            .collect::<Result<_, _>>()?,
                    )
                }
                C::DescriptorPattern(_) => {
                    budget.text(descriptor.pattern().ok_or(Error::InvalidContext)?.0)?
                }
                C::DescriptorMinimum(_) => {
                    budget.integer(descriptor.minimum().ok_or(Error::InvalidContext)?)?
                }
                C::DescriptorSize(_) => {
                    budget.integer(descriptor.size().ok_or(Error::InvalidContext)?)?
                }
                _ => unreachable!(),
            }
        }
    })
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
    let mut budget = Budget(128);
    findings.iter().map(|finding| {
        budget.charge(128)?;
        let path = budget.text(&finding.path)?;
        let condition = budget.text(finding.condition)?;
        let requirement = finding.requirement.map(|text| budget.text(text)).transpose()?;
        let mut resolved = Map::new();
        for (name, value) in &finding.context {
            budget.text(name)?;
            budget.charge(2)?;
            let value = context(schema, document, value, &mut budget)?;
            if resolved.insert((*name).into(), value).is_some() { return Err(Error::InvalidContext); }
        }
        for (name, value) in source_context {
            budget.text(name)?;
            let value = budget.text(value)?;
            if resolved.insert((*name).into(), value).is_some() { return Err(Error::InvalidContext); }
        }
        Ok(json!({"phase":"validation", "condition":condition,"requirement":requirement,"spec_paths":[path],"context":resolved}))
    }).collect()
}

/// Convert already-rendered traversal/dependency findings with literal context.
/// Schema-node/descriptor references must use schema_findings with their held arena.
pub(super) fn literal_outcome(mut outcome: Value) -> Result<Value, Error> {
    if outcome["status"] != "invalid" {
        return Ok(outcome);
    }
    let mut budget = Budget(128);
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
