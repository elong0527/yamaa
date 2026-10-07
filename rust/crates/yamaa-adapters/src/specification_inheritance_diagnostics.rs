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
