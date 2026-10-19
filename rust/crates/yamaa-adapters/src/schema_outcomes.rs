//! Resource outcomes for typed schema preparation. Semantic findings use issue_rows.
#[derive(Debug)]
pub(crate) enum Error {
    Internal,
    InvalidQuery,
    InvalidDocument,
}
// Stable outcomes keep language findings, policy refusal and transport defects separate.

use crate::json;
use yamaa_core::json::Value;
use yamaa_core::{portable_regex, schema::*};

pub(crate) fn limit(phase: &str, resource: &str, limit: usize) -> Value {
    json!({"status":"resource_limit","phase":phase,"resource":resource,"limit":limit})
}
fn regex_resource(resource: portable_regex::Resource) -> &'static str {
    use portable_regex::Resource::*;
    match resource {
        Depth => "semantic_depth",
        Size => "regex_size",
    }
}
fn document_resource(resource: DocumentResource) -> &'static str {
    match resource {
        DocumentResource::Depth => "yaml_depth",
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
                TypeResource::Depth => "semantic_depth",
            },
            maximum,
        ),
        ValidationError::InvalidDescriptor => return Err(Error::InvalidQuery),
        ValidationError::Depth { limit: maximum } => limit("validation", "semantic_depth", maximum),
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
                NormalizationResource::Depth => "semantic_depth",
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
pub(crate) fn inheritance_dependencies(
    error: yamaa_core::resolution::Error,
) -> Result<Value, Error> {
    use yamaa_core::expression::{CompileError, Context, Resource};
    use yamaa_core::resolution::Error as E;
    Ok(match error {
        E::Invalid(issues) => {
            let findings = yamaa_core::diagnostic::inheritance_dependency_diagnostics(&issues);
            let translated = findings
                .into_iter()
                .map(crate::specification_diagnostics::portable_diagnostic)
                .collect::<Vec<_>>();
            json!({"status":"invalid","diagnostics":translated})
        }
        // An expression field's own boundary, or its regular-expression literal's.
        E::Limit {
            context,
            error:
                CompileError::Limit {
                    resource,
                    limit: maximum,
                    ..
                },
        } => {
            let field = match context {
                Context::Compute => "numeric",
                Context::Aggregate => "aggregate",
                Context::Predicate => "predicate",
            };
            match resource {
                Resource::Depth => {
                    limit(&format!("inheritance_{field}"), "semantic_depth", maximum)
                }
                Resource::Regex(resource) => limit(
                    &format!("inheritance_{field}_regex"),
                    regex_resource(resource),
                    maximum,
                ),
            }
        }
        E::Limit { .. } => return Err(Error::Internal),
        E::Document(error) => return normalization(NormalizationError::Document(error)),
    })
}
