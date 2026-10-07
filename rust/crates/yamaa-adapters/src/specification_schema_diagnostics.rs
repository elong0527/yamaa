//! Resolve schema findings against held pass inputs, without reparsing source YAML.
use crate::specification_source::CapturedFindings;
use serde_json::{json, Map, Number, Value};
use yamaa_core::schema::{DocumentNode as N, SchemaContext as C};

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
fn context(captured: &CapturedFindings, value: &C, budget: &mut Budget) -> Result<Value, Error> {
    Ok(match value {
        C::Text(text) => budget.text(text)?,
        C::Count(value) => {
            budget.charge(20)?;
            json!(value)
        }
        C::Null => {
            budget.charge(4)?;
            Value::Null
        }
        C::InputValue(node) => {
            let node = captured
                .document()
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
            let descriptor = &captured
                .schema()
                .structure()
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
    let mut budget = Budget(128);
    captured.findings().iter().map(|finding| {
        budget.charge(128)?;
        let path = budget.text(&finding.path)?;
        let condition = budget.text(finding.condition)?;
        let requirement = finding.requirement.map(|text| budget.text(text)).transpose()?;
        let mut resolved = Map::new();
        for (name, value) in &finding.context {
            budget.text(name)?;
            budget.charge(2)?;
            let value = context(captured, value, &mut budget)?;
            if resolved.insert((*name).into(), value).is_some() { return Err(Error::InvalidContext); }
        }
        Ok(json!({"phase":"validation", "condition":condition,"requirement":requirement,"spec_paths":[path],"context":resolved}))
    }).collect()
}
