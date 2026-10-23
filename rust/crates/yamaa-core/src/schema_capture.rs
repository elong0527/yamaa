//! Queries of a schema closure. The adapters decode each module of a closure
//! with its positions and admit the decoded modules with
//! [`SchemaStructure::admit`]; include resolution, descriptors and default
//! authority remain in the shared core.
use crate::schema::{Document, SchemaStructure};

/// Discovery grants read authority only to names accepted by the existing core
/// include grammar. Unsafe authored names remain for final bundle diagnostics.
pub fn include_names(input: &Document) -> Vec<&str> {
    use crate::schema::DocumentNode;
    let Some(index) = input.field(input.root(), "includes") else {
        return Vec::new();
    };
    let DocumentNode::Sequence(entries) = &input.nodes()[index] else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|&entry| match &input.nodes()[entry] {
            DocumentNode::Text(value) if crate::schema::schema_include_name(value) => {
                Some(value.as_str())
            }
            _ => None,
        })
        .collect()
}

/// Read-only queries borrow an already admitted closure.
/// Physical codecs, host query carriers and result serialization stay in adapters.
pub fn validate_query(
    schema: &SchemaStructure,
    input: &Document,
    types: &[String],
    fragment: bool,
) -> Result<Vec<crate::schema::SchemaDiagnostic>, crate::schema::ValidationError> {
    let selected = schema.parse_query_types(types)?;
    schema.validate_types(&selected, input, input.root(), "", fragment)
}

pub fn normalize_query(
    schema: &SchemaStructure,
    input: &Document,
    types: &[String],
    fragment: bool,
) -> Result<crate::schema::NormalizedDocument, crate::schema::NormalizationError> {
    let selected = schema.parse_query_types(types)?;
    schema.normalize_types(&selected, input, input.root(), fragment)
}

pub fn default_findings(
    schema: &SchemaStructure,
) -> Result<Vec<crate::schema::DefaultDiagnostics>, crate::schema::ValidationError> {
    schema.validate_defaults()
}
