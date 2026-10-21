//! Resource refusals of typed schema preparation. Semantic findings stay
//! diagnostic records; a stage is the caller's.
#[derive(Debug)]
pub(crate) enum Error {
    Internal,
    InvalidQuery,
    InvalidDocument,
}
// Stable outcomes keep language findings, policy refusal and transport defects separate.

use crate::specification_diagnostics::{bound, fault, Rejection};
use yamaa_core::application_issue::{Fault, Step};
use yamaa_core::{portable_regex, schema::*};

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
fn limit(step: Step, phase: &str, resource: &str, limit: usize) -> Rejection {
    bound(step, phase, resource, limit, None)
}
pub(crate) fn document(error: DocumentError, step: Step) -> Result<Rejection, Error> {
    match error {
        DocumentError::Limit {
            resource,
            limit: maximum,
        } => Ok(limit(
            step,
            "decoded_document",
            document_resource(resource),
            maximum,
        )),
        _ => Err(Error::InvalidDocument),
    }
}
pub(crate) fn validation(error: ValidationError, step: Step) -> Result<Rejection, Error> {
    Ok(match error {
        ValidationError::Type(TypeError::Invalid { .. }) => return Err(Error::InvalidQuery),
        ValidationError::Type(TypeError::Limit {
            resource,
            limit: maximum,
        }) => limit(
            step,
            "type",
            match resource {
                TypeResource::Depth => "semantic_depth",
            },
            maximum,
        ),
        ValidationError::InvalidDescriptor => return Err(Error::InvalidQuery),
        ValidationError::Depth { limit: maximum } => {
            limit(step, "validation", "semantic_depth", maximum)
        }
    })
}
pub(crate) fn normalization(error: NormalizationError, step: Step) -> Result<Rejection, Error> {
    Ok(match error {
        NormalizationError::Validation(error) => return validation(error, step),
        NormalizationError::Invalid(_) | NormalizationError::Defaults(_) => {
            return Err(Error::Internal)
        }
        NormalizationError::Limit {
            resource,
            limit: maximum,
        } => limit(
            step,
            "normalization",
            match resource {
                NormalizationResource::Depth => "semantic_depth",
            },
            maximum,
        ),
        NormalizationError::Document(DocumentError::Limit {
            resource,
            limit: maximum,
        }) => limit(
            step,
            "normalized_document",
            document_resource(resource),
            maximum,
        ),
        // A mapping key that is not a scalar reports the projection fault of
        // the preparation stage it has always reported.
        NormalizationError::Document(DocumentError::NonScalarKey) => {
            fault(Step::Prepare, Fault::IssueProjection)
        }
        NormalizationError::Document(_) => return Err(Error::Internal),
    })
}
/// Dependency discovery never turns a resource refusal into a missing edge.
pub(crate) fn inheritance_dependencies(
    error: yamaa_core::resolution::Error,
    step: Step,
) -> Result<Rejection, Error> {
    use yamaa_core::expression::{CompileError, Context, Resource};
    use yamaa_core::resolution::Error as E;
    Ok(match error {
        E::Invalid(issues) => Rejection::Invalid(
            yamaa_core::diagnostic::inheritance_dependency_diagnostics(&issues),
        ),
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
                Context::Derivation => "derivation",
            };
            match resource {
                Resource::Depth => limit(
                    step,
                    &format!("inheritance_{field}"),
                    "semantic_depth",
                    maximum,
                ),
                Resource::Regex(resource) => limit(
                    step,
                    &format!("inheritance_{field}_regex"),
                    regex_resource(resource),
                    maximum,
                ),
            }
        }
        E::Limit { .. } => return Err(Error::Internal),
        E::Document(error) => return normalization(NormalizationError::Document(error), step),
    })
}
