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
    Ok(report
        .iter()
        .map(|finding| {
            let context = finding
                .context
                .map(|(name, field)| (name.into(), value(field)))
                .collect::<Map<_, _>>();
            crate::issue_rows::report(
                finding.phase,
                finding.condition,
                finding.requirement,
                vec![finding.path.to_owned()],
                context.into(),
            )
        })
        .collect())
}

/// Admit already projected core findings in the same five-field diagnostic shape.
pub(super) fn literal_outcome(outcome: Value) -> Result<Value, Error> {
    if outcome["status"] == "invalid" {
        let findings = outcome["diagnostics"]
            .as_array()
            .ok_or(Error::InvalidContext)?;
        for finding in findings {
            crate::issue_rows::Issue::from_diagnostic(finding.clone())
                .map_err(|_| Error::InvalidContext)?;
        }
        let mut budget = Budget::new();
        budget.charge(outcome.to_string().len())?;
    }
    Ok(outcome)
}
