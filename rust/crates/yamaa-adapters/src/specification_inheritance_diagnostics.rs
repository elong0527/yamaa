//! Resolve shared inherited findings while preserving opaque host failures.
use super::{capture_failure, captured_schema};
use crate::{
    inheritance_transport::{self, Failure},
    schema_transport::errors,
    specification_source::{InheritanceError, SourceFailure},
};
use serde_json::{json, Value};
use yamaa_core::schema::{
    Document, InheritanceDependencyError, InheritanceReferenceError, NormalizationError,
    SchemaStructure,
};
use yamaa_engine::{inheritance as graph, inheritance_preparation::Error as E};

fn source<E>(error: SourceFailure<E>) -> Result<String, E> {
    match error {
        SourceFailure::Host(error) => Err(error),
        SourceFailure::Admission(error) => Ok(capture_failure(&error)),
    }
}
fn rejected(code: &str) -> Value {
    json!({"status":"rejected","stage":"inheritance","code":code})
}
fn context_failure(error: captured_schema::Error) -> Value {
    rejected(match error {
        captured_schema::Error::Limit => "diagnostic_context_limit",
        captured_schema::Error::InvalidContext => "diagnostic_context",
    })
}
fn normalized(
    schema: &SchemaStructure,
    input: Option<&Document>,
    error: NormalizationError,
    source_context: &[(&str, &str)],
) -> Value {
    match error {
        NormalizationError::Invalid(findings) => {
            match captured_schema::schema_findings(schema, input, &findings, source_context) {
                Ok(diagnostics) => json!({"status":"invalid","diagnostics":diagnostics}),
                Err(error) => context_failure(error),
            }
        }
        error => errors::normalization(error).unwrap_or_else(|_| rejected("normalization")),
    }
}
fn literal(outcome: Value) -> Value {
    captured_schema::literal_outcome(outcome).unwrap_or_else(context_failure)
}
/// Borrow rejected preparation so a public carrier can retain its exact sources
/// and opaque resource payload beside the complete projected findings.
pub fn failure_ref<T>(error: &InheritanceError<T>) -> Result<String, &T> {
    let source_ref = |error: &SourceFailure<T>| match error {
        SourceFailure::Host(_) => None,
        SourceFailure::Admission(error) => Some(capture_failure(error)),
    };
    let (schema, error) = match error {
        InheritanceError::Entry(error) => return Ok(capture_failure(error)),
        InheritanceError::Preparation { schema, error, .. } => (schema.structure(), error.as_ref()),
    };
    let outcome = match error {
        E::Path(error) | E::Traversal(graph::Error::Host(error)) => {
            if let SourceFailure::Host(error) = error {
                return Err(error);
            }
            return Ok(source_ref(error).expect("admission or host"));
        }
        E::Traversal(graph::Error::Layer {
            source,
            entry,
            input,
            error,
        }) => normalized(
            schema,
            Some(input),
            error.clone(),
            &[("source", source), ("entry", entry)],
        ),
        E::Traversal(error) => {
            let error: graph::Error<()> = match error {
                graph::Error::Resource(error) => graph::Error::Resource(*error),
                graph::Error::Unavailable { declaring, path } => graph::Error::Unavailable {
                    declaring: declaring.clone(),
                    path: path.clone(),
                },
                graph::Error::InvalidParent { declaring, path } => graph::Error::InvalidParent {
                    declaring: declaring.clone(),
                    path: path.clone(),
                },
                graph::Error::Cycle {
                    path,
                    returns_to_entry,
                } => graph::Error::Cycle {
                    path: path.clone(),
                    returns_to_entry: *returns_to_entry,
                },
                graph::Error::Version {
                    source,
                    entry,
                    expected,
                    actual,
                    is_entry,
                } => graph::Error::Version {
                    source: source.clone(),
                    entry: entry.clone(),
                    expected: expected.clone(),
                    actual: actual.clone(),
                    is_entry: *is_entry,
                },
                graph::Error::UnsupportedControlField { source, field } => {
                    graph::Error::UnsupportedControlField {
                        source: source.clone(),
                        field,
                    }
                }
                graph::Error::InvalidIdentity => graph::Error::InvalidIdentity,
                _ => unreachable!("host and layer handled above"),
            };
            match inheritance_transport::traversal_outcome(error) {
                Ok(value) => literal(value),
                Err(_) => rejected("transport"),
            }
        }
        E::ParentsRequired => rejected("parents_required"),
        E::PathBytes => rejected("path_byte_limit"),
        E::Document(error) => errors::document(*error).unwrap_or_else(|_| rejected("document")),
        E::Normalize(failure) => {
            normalized(schema, failure.input.as_ref(), failure.error.clone(), &[])
        }
        E::Validation(error) => {
            errors::validation(*error).unwrap_or_else(|_| rejected("validation_policy"))
        }
        E::Dependencies(failure) => match &failure.error {
            InheritanceDependencyError::Normalization(error)
            | InheritanceDependencyError::Reference(InheritanceReferenceError::Normalization(
                error,
            )) => normalized(schema, failure.input.as_ref(), error.clone(), &[]),
            error => literal(
                errors::inheritance_dependencies(error.clone())
                    .unwrap_or_else(|_| rejected("dependency_policy")),
            ),
        },
        E::Model(findings) => normalized(
            schema,
            None,
            NormalizationError::Invalid(findings.clone()),
            &[],
        ),
        E::Composition(error) => normalized(
            schema,
            error.context_document.as_ref(),
            error.error.clone(),
            &[],
        ),
    };
    Ok(
        crate::yaml_transport::encode_outcome("specification/prototype", outcome, 16_777_216)
            .unwrap_or_else(|_| {
                json!({"protocol":"specification/prototype","outcome":rejected("response_limit")})
                    .to_string()
            }),
    )
}
/// Both document forms return portable issue records, with source provenance
/// resolved by the shared adapter. No host interprets context occurrence IDs.
pub fn failure<T>(error: InheritanceError<T>) -> Result<String, T> {
    let (schema, error) = match error {
        InheritanceError::Entry(error) => return Ok(capture_failure(&error)),
        InheritanceError::Preparation { schema, error, .. } => (schema, *error),
    };
    let schema = schema.structure();
    let outcome = match error {
        E::Path(error) => return source(error),
        E::Traversal(graph::Error::Layer {
            source,
            entry,
            input,
            error,
        }) => normalized(
            schema,
            Some(&input),
            error,
            &[("source", &source), ("entry", &entry)],
        ),
        E::Traversal(error) => match inheritance_transport::traversal_outcome(error) {
            Ok(value) => literal(value),
            Err(Failure::Host(error)) => return source(error),
            Err(Failure::Transport(_)) => rejected("transport"),
        },
        E::ParentsRequired => rejected("parents_required"),
        E::PathBytes => rejected("path_byte_limit"),
        E::Document(error) => errors::document(error).unwrap_or_else(|_| rejected("document")),
        E::Normalize(failure) => normalized(schema, failure.input.as_ref(), failure.error, &[]),
        E::Validation(error) => {
            errors::validation(error).unwrap_or_else(|_| rejected("validation_policy"))
        }
        E::Dependencies(failure) => match failure.error {
            InheritanceDependencyError::Normalization(error)
            | InheritanceDependencyError::Reference(InheritanceReferenceError::Normalization(
                error,
            )) => normalized(schema, failure.input.as_ref(), error, &[]),
            error => literal(
                errors::inheritance_dependencies(error)
                    .unwrap_or_else(|_| rejected("dependency_policy")),
            ),
        },
        E::Model(findings) => normalized(schema, None, NormalizationError::Invalid(findings), &[]),
        E::Composition(error) => {
            normalized(schema, error.context_document.as_ref(), error.error, &[])
        }
    };
    Ok(
        crate::yaml_transport::encode_outcome("specification/prototype", outcome, 16_777_216)
            .unwrap_or_else(|_| {
                json!({"protocol":"specification/prototype","outcome":rejected("response_limit")})
                    .to_string()
            }),
    )
}
