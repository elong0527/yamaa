//! Represent core-admitted schema findings without reparsing source YAML.
use crate::json;
use crate::specification_source::CapturedFindings;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::json::{Number, Value};
pub use yamaa_core::schema::SchemaDiagnosticError as Error;
use yamaa_core::schema::{
    admit_schema_diagnostics, Document, SchemaDiagnostic, SchemaDiagnosticValue as V,
    SchemaStructure,
};

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
/// The findings of `captured`, naming the file they are in when it is one a
/// referencing field names.
pub(super) fn findings(captured: &CapturedFindings) -> Result<Vec<Diagnostic>, Error> {
    schema_findings(
        captured.schema().structure(),
        captured.document(),
        captured.findings(),
        &captured.source_context(),
    )
}
/// Each finding keeps its registered cause; its context resolves against the
/// exact schema and pass input, then the caller's source provenance.
pub(super) fn schema_findings(
    schema: &SchemaStructure,
    document: Option<&Document>,
    findings: &[SchemaDiagnostic],
    source_context: &[(&str, &str)],
) -> Result<Vec<Diagnostic>, Error> {
    let report = admit_schema_diagnostics(schema, document, findings, source_context)?;
    Ok(report
        .iter()
        .zip(report.findings())
        .map(|(resolved, finding)| Diagnostic {
            code: finding.code,
            spec_paths: vec![resolved.path.to_owned()],
            context: resolved
                .context
                .map(|(name, field)| (name.into(), value(field)))
                .collect(),
            source_span: None,
            operand_route: None,
        })
        .collect())
}
