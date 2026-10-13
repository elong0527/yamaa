//! Resolve shared inherited findings while preserving opaque host failures.
use super::{capture_failure, captured_schema};
use crate::{
    schema_outcomes as errors,
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
fn traversal_finding<E>(error: &graph::Error<E>) -> Result<Option<Value>, captured_schema::Error> {
    let finding = error
        .diagnostic_with_limit(16_777_216)
        .map_err(|error| match error {
            yamaa_core::diagnostic::ProjectionError::Limit => captured_schema::Error::Limit,
            yamaa_core::diagnostic::ProjectionError::InvalidContext => {
                captured_schema::Error::InvalidContext
            }
        })?;
    let Some(finding) = finding else {
        return Ok(None);
    };
    Ok(Some(
        json!({"status":"invalid", "diagnostics":[super::portable_diagnostic(finding).ok_or(captured_schema::Error::InvalidContext)?]}),
    ))
}
// The engine classifies the typed error. This boundary only serializes its
// already-owned fields; no source request or JSON reply crosses a host callback.
fn traversal_outcome<E>(schema: &SchemaStructure, error: graph::Error<E>) -> Result<Value, E> {
    use yamaa_engine::inheritance_outcome::{self, Outcome};
    let error = match inheritance_outcome::traversal_outcome(error, 16_777_216) {
        Ok(Outcome::Diagnostic(finding)) => {
            return Ok(match super::portable_diagnostic(finding) {
                Some(finding) => json!({"status":"invalid", "diagnostics":[finding]}),
                None => rejected("diagnostic_context"),
            });
        }
        Ok(Outcome::Original(error)) => error,
        Err(_) => return Ok(rejected("diagnostic_context_limit")),
    };
    Ok(match error {
        graph::Error::Host(error) => return Err(error),
        graph::Error::Resource(error) => {
            let resource = match error.resource {
                graph::Resource::Layers => "layers",
                graph::Resource::ParentVisits => "parent_visits",
                graph::Resource::Depth => "depth",
                graph::Resource::PathBytes => "path_bytes",
                graph::Resource::InputNodes => "input_nodes",
                graph::Resource::InputTextBytes => "input_text_bytes",
                graph::Resource::Work => "work",
            };
            json!({"status":"resource_limit", "phase":"inheritance", "resource":resource, "limit":error.limit})
        }
        graph::Error::UnsupportedControlField { source, field } => {
            json!({"status":"unsupported", "feature":"inheritance_control_field", "source":source, "field":field})
        }
        graph::Error::Layer {
            source,
            entry,
            input,
            error,
        } => normalized(
            schema,
            Some(&input),
            error,
            &[("source", &source), ("entry", &entry)],
        ),
        _ => rejected("transport"),
    })
}
fn dictionary_failure_ref<'a, T>(
    document: &crate::specification_source::PreparedDocument,
    error: &'a yamaa_engine::dictionary_preparation::Error<
        T,
        crate::specification_source::Error,
        crate::yaml_decode::DecodedYaml,
    >,
) -> Result<String, &'a T> {
    use yamaa_engine::dictionary_preparation::Cause;
    let outcome = match &error.cause {
        Cause::Capture {
            reference,
            error,
            failure,
        } => {
            if let Some(failure) = failure {
                let finding = failure.field_diagnostic(reference.path(), reference.written());
                json!({"status":"invalid","diagnostics":[super::portable_diagnostic(finding).expect("portable resource facts")]})
            } else {
                match error {
                    graph::SourceError::Raised(error) => return Err(error),
                    graph::SourceError::Unavailable => {
                        json!({"status":"rejected","stage":"dictionary","code":"resource_unavailable"})
                    }
                }
            }
        }
        Cause::Decode {
            reference, error, ..
        } => {
            let crate::specification_source::Error::Decode { error, .. } = error else {
                return Ok(capture_failure(error));
            };
            if let Some(mut rows) = crate::yaml_diagnostics::semantic_diagnostics(error) {
                for row in &mut rows {
                    if matches!(error, crate::yaml_decode::DecodeFailure::NonAscii(_)) {
                        row["context"]["path"] = json!(reference.written());
                    }
                }
                json!({"status":"invalid","diagnostics":rows})
            } else {
                json!({"status":"rejected","stage":"dictionary","code":"yaml_decode"})
            }
        }
        Cause::Admission {
            arena,
            error: normalization,
            ..
        } => normalized(
            document.schema().structure(),
            document.dictionary_input(error, *arena),
            normalization.clone(),
            &[],
        ),
        Cause::Materialize(error) => {
            normalized(document.schema().structure(), None, error.clone(), &[])
        }
        Cause::Limit(error) => return Ok(capture_failure(error)),
        Cause::MissingOrigin(_) => {
            json!({"status":"rejected","stage":"dictionary","code":"declaring_origin"})
        }
    };
    Ok(crate::yaml_diagnostics::encode_outcome("specification/prototype",outcome,16_777_216)
        .unwrap_or_else(|_|json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"dictionary","code":"response_limit"}}).to_string()))
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
        InheritanceError::Dictionary { document, error } => {
            return dictionary_failure_ref(document, error)
        }
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
            match traversal_finding(error) {
                Ok(Some(outcome)) => outcome,
                Err(error) => context_failure(error),
                Ok(None) => {
                    let error: graph::Error<()> = match error {
                        graph::Error::Resource(error) => graph::Error::Resource(*error),
                        graph::Error::UnsupportedControlField { source, field } => {
                            graph::Error::UnsupportedControlField {
                                source: source.clone(),
                                field,
                            }
                        }
                        // Classified findings were handled above. A contradictory
                        // cycle carrier remains a transport refusal.
                        _ => graph::Error::InvalidIdentity,
                    };
                    match traversal_outcome(schema, error) {
                        Ok(value) => literal(value),
                        Err(_) => rejected("transport"),
                    }
                }
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
        crate::yaml_diagnostics::encode_outcome("specification/prototype", outcome, 16_777_216)
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
        InheritanceError::Dictionary { document, error } => {
            match dictionary_failure_ref(&document, &error) {
                Ok(value) => return Ok(value),
                Err(_) => match error.cause {
                    yamaa_engine::dictionary_preparation::Cause::Capture {
                        error: graph::SourceError::Raised(error),
                        ..
                    } => return Err(error),
                    _ => unreachable!("only original raised capture failures are opaque"),
                },
            }
        }
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
        E::Traversal(error) => match traversal_finding(&error) {
            Ok(Some(outcome)) => outcome,
            Err(error) => context_failure(error),
            Ok(None) => match traversal_outcome(schema, error) {
                Ok(value) => literal(value),
                Err(error) => return source(error),
            },
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
        crate::yaml_diagnostics::encode_outcome("specification/prototype", outcome, 16_777_216)
            .unwrap_or_else(|_| {
                json!({"protocol":"specification/prototype","outcome":rejected("response_limit")})
                    .to_string()
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_context_limit_preserves_original_whole_failure() {
        let error = graph::Error::<()>::Unavailable {
            declaring: "entry".into(),
            path: "x".repeat(3_000_000),
        };
        for _ in 0..2 {
            assert!(matches!(
                traversal_finding(&error),
                Err(captured_schema::Error::Limit)
            ));
        }
        assert_eq!(
            context_failure(captured_schema::Error::Limit),
            json!({"status":"rejected","stage":"inheritance","code":"diagnostic_context_limit"})
        );
        let graph::Error::Unavailable { path, .. } = error else {
            panic!("failure replaced")
        };
        assert_eq!(path.len(), 3_000_000);
    }
}
