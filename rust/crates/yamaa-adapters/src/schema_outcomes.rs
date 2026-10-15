//! Resource outcomes for typed schema preparation. Semantic findings use issue_rows.
#[derive(Debug)]
pub(crate) enum Error {
    Internal,
    ResponseLimit,
    InvalidQuery,
    InvalidDocument,
}
// Stable outcomes keep language findings, policy refusal and transport defects separate.

use serde_json::{json, Value};
use yamaa_core::{regex, schema::*};

pub(crate) fn limit(phase: &str, resource: &str, limit: usize) -> Value {
    json!({"status":"resource_limit","phase":phase,"resource":resource,"limit":limit})
}
fn regex_resource(resource: regex::Resource) -> &'static str {
    use regex::Resource::*;
    match resource {
        PatternBytes => "pattern_bytes",
        Nodes => "nodes",
        Groups => "groups",
        Depth => "depth",
        Repetition => "repetition",
        Width => "width",
        WidthWork => "width_work",
        WidthCells => "width_cells",
        SubjectBytes => "subject_bytes",
        Work => "work",
        StateCells => "state_cells",
    }
}
fn document_resource(resource: DocumentResource) -> &'static str {
    match resource {
        DocumentResource::Nodes => "nodes",
        DocumentResource::TextBytes => "text_bytes",
        DocumentResource::Edges => "edges",
        DocumentResource::Depth => "depth",
    }
}
pub(crate) fn document(error: DocumentError) -> Result<Value, Error> {
    match error {
        DocumentError::Limit {
            resource,
            limit: maximum,
        } => Ok(limit(
            "decoded_document",
            document_resource(resource),
            maximum,
        )),
        _ => Err(Error::InvalidDocument),
    }
}
pub(crate) fn validation(error: ValidationError) -> Result<Value, Error> {
    Ok(match error {
        ValidationError::Type(TypeError::Invalid { .. }) => return Err(Error::InvalidQuery),
        ValidationError::Type(TypeError::Limit {
            resource,
            limit: maximum,
        }) => limit(
            "type",
            match resource {
                TypeResource::Bytes => "bytes",
                TypeResource::Nodes => "nodes",
                TypeResource::Depth => "depth",
            },
            maximum,
        ),
        ValidationError::InvalidDescriptor => return Err(Error::InvalidQuery),
        ValidationError::Depth { limit: maximum } => limit("validation", "depth", maximum),
        ValidationError::Diagnostics { limit: maximum } => {
            limit("validation", "diagnostics", maximum)
        }
        ValidationError::DiagnosticText { limit: maximum } => {
            limit("validation", "diagnostic_text_bytes", maximum)
        }
        ValidationError::Constraint(ConstraintError::Work { limit: maximum }) => {
            limit("validation", "work", maximum)
        }
        ValidationError::Constraint(ConstraintError::Regex(error)) => {
            limit("regex_match", regex_resource(error.resource), error.limit)
        }
    })
}
pub(crate) fn normalization(error: NormalizationError) -> Result<Value, Error> {
    Ok(match error {
        NormalizationError::Validation(error) => return validation(error),
        NormalizationError::Invalid(_) | NormalizationError::Defaults(_) => {
            return Err(Error::Internal)
        }
        NormalizationError::Limit {
            resource,
            limit: maximum,
        } => limit(
            "normalization",
            match resource {
                NormalizationResource::Nodes => "nodes",
                NormalizationResource::TextBytes => "text_bytes",
                NormalizationResource::Edges => "edges",
                NormalizationResource::Depth => "depth",
            },
            maximum,
        ),
        NormalizationError::Document(DocumentError::Limit {
            resource,
            limit: maximum,
        }) => limit("normalized_document", document_resource(resource), maximum),
        NormalizationError::Document(DocumentError::NonScalarKey) => {
            json!({"status":"unsupported","feature":"normalized_mapping_key"})
        }
        NormalizationError::Document(_) => return Err(Error::Internal),
    })
}
/// Dependency discovery never turns a resource refusal into a missing edge.
pub(crate) fn inheritance_dependencies(error: InheritanceDependencyError) -> Result<Value, Error> {
    use yamaa_core::{aggregate_parser as a, numeric_parser as n, predicate_parser as p};
    use InheritanceDependencyError as E;
    use InheritanceReferenceError as R;
    fn syntax_limit(phase: &str, resource: n::ParseResource, maximum: usize) -> Value {
        limit(
            phase,
            match resource {
                n::ParseResource::Bytes => "bytes",
                n::ParseResource::Tokens => "tokens",
                n::ParseResource::Nodes => "nodes",
                n::ParseResource::Depth => "depth",
            },
            maximum,
        )
    }
    Ok(match error {
        E::Normalization(error) | E::Reference(R::Normalization(error)) => {
            return normalization(error)
        }
        E::Invalid(issues) => {
            use yamaa_core::diagnostic::{inheritance_dependency_diagnostics, ProjectionError};
            let findings = inheritance_dependency_diagnostics(&issues, 16_777_216).map_err(
                |error| match error {
                    ProjectionError::Limit => Error::ResponseLimit,
                    ProjectionError::InvalidContext => Error::Internal,
                },
            )?;
            let translated = findings
                .into_iter()
                .map(crate::specification_diagnostics::portable_diagnostic)
                .collect::<Option<Vec<_>>>()
                .ok_or(Error::Internal)?;
            json!({"status":"invalid","diagnostics":translated})
        }
        E::Reference(R::Numeric(n::ParseError::Limit {
            resource, limit, ..
        })) => syntax_limit("inheritance_numeric", resource, limit),
        E::Reference(R::Aggregate(a::ParseError::Limit {
            resource, limit, ..
        })) => syntax_limit("inheritance_aggregate", resource, limit),
        E::Reference(R::Predicate(p::ParseError::Limit {
            resource, limit, ..
        })) => syntax_limit("inheritance_predicate", resource, limit),
        E::Reference(R::Predicate(p::ParseError::RegexLimit {
            resource,
            limit: maximum,
            ..
        })) => limit(
            "inheritance_predicate_regex",
            regex_resource(resource),
            maximum,
        ),
        E::Reference(R::Predicate(p::ParseError::UnsupportedRegex {
            feature,
            byte,
            position,
        })) => {
            json!({"status":"unsupported","phase":"inheritance_predicate_regex","feature":feature,"pattern_byte":byte,"position":{"byte":position.byte,"character":position.character}})
        }
        // Grammar failures deliberately contribute no edges, then survive for final validation.
        E::Reference(_) => return Err(Error::Internal),
    })
}
