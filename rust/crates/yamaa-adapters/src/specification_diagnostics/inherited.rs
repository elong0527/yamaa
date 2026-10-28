//! Resolve shared inherited findings while preserving opaque host failures.
use super::{bound, capture_failure, captured_schema, fault, Rejection};
use crate::{
    schema_outcomes as errors,
    specification_source::{InheritanceError, SourceFailure},
};
use yamaa_core::application_issue::{Fault, Step};
use yamaa_core::inheritance as graph;
use yamaa_core::schema::{Document, NormalizationError, SchemaStructure};

fn rejected(code: Fault) -> Rejection {
    fault(Step::Inheritance, code)
}
fn context_failure(error: captured_schema::Error) -> Rejection {
    rejected(match error {
        captured_schema::Error::InvalidContext => Fault::DiagnosticContext,
    })
}
fn normalized(
    schema: &SchemaStructure,
    input: Option<&Document>,
    error: NormalizationError,
    source_context: &[(&str, &str)],
) -> Rejection {
    match error {
        NormalizationError::Invalid(findings) => {
            match captured_schema::schema_findings(schema, input, &findings, source_context) {
                Ok(diagnostics) => Rejection::Invalid(diagnostics),
                Err(error) => context_failure(error),
            }
        }
        error => errors::normalization(error, Step::Prepare)
            .unwrap_or_else(|_| rejected(Fault::Normalization)),
    }
}
fn traversal_finding<E>(
    error: &graph::Error<E>,
) -> Result<Option<Rejection>, captured_schema::Error> {
    let finding = error
        .diagnostic()
        .map_err(|_| captured_schema::Error::InvalidContext)?;
    Ok(finding.map(|finding| Rejection::Invalid(vec![finding])))
}
// The engine classifies the typed error. This boundary only represents its
// already-owned fields; no source request or reply crosses a host callback.
fn traversal_outcome<E>(schema: &SchemaStructure, error: graph::Error<E>) -> Result<Rejection, E> {
    use yamaa_core::inheritance_outcome::{self, Outcome};
    let error = match inheritance_outcome::traversal_outcome(error) {
        Ok(Outcome::Diagnostic(finding)) => return Ok(Rejection::Invalid(vec![finding])),
        Ok(Outcome::Original(error)) => error,
        Err(_) => return Ok(rejected(Fault::DiagnosticContext)),
    };
    Ok(match error {
        graph::Error::Host(error) => return Err(error),
        graph::Error::Resource(error) => bound(
            Step::Prepare,
            "inheritance",
            error.resource,
            error.limit,
            error.actual,
        ),
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
        _ => rejected(Fault::Transport),
    })
}
fn dictionary_failure_ref<'a, T>(
    document: &crate::specification_source::PreparedDocument,
    error: &'a yamaa_core::dictionary_preparation::Error<
        T,
        crate::specification_source::Error,
        crate::yaml_decode::DecodedYaml,
    >,
) -> Result<Rejection, &'a T> {
    use crate::yaml_decode::DecodeFailure;
    use yamaa_core::dictionary_preparation::Cause;
    Ok(match &error.cause {
        Cause::Capture {
            reference,
            error,
            failure,
        } => {
            if let Some(failure) = failure {
                Rejection::Invalid(vec![
                    failure.field_diagnostic(reference.path(), reference.written())
                ])
            } else {
                match error {
                    graph::SourceError::Raised(error) => return Err(error),
                    graph::SourceError::Unavailable => {
                        fault(Step::Dictionary, Fault::ResourceUnavailable)
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
            if let Some(rows) =
                crate::yaml_diagnostics::semantic_diagnostics(error, reference.written())
            {
                Rejection::Invalid(rows)
            } else if let DecodeFailure::Unavailable = error {
                fault(Step::Dictionary, Fault::YamlReaderUnavailable)
            } else if let DecodeFailure::Limit(limit) = error {
                super::decoder_limit(limit, Step::Dictionary)
            } else {
                fault(Step::Dictionary, Fault::YamlDecode)
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
        Cause::MissingOrigin(_) => fault(Step::Dictionary, Fault::DeclaringOrigin),
    })
}

/// The refusal of a traversal of a specification's layers (LAYER-2, LAYER-4).
fn traversal<'a, T>(
    schema: &SchemaStructure,
    error: &'a graph::Error<SourceFailure<T>>,
) -> Result<Rejection, &'a T> {
    Ok(match error {
        graph::Error::Host(error) => match error {
            SourceFailure::Host(error) => return Err(error),
            SourceFailure::Admission(error) => capture_failure(error),
        },
        graph::Error::Layer {
            source,
            entry,
            input,
            error,
        } => normalized(
            schema,
            Some(input),
            error.clone(),
            &[("source", source), ("entry", entry)],
        ),
        error => match traversal_finding(error) {
            Ok(Some(rejection)) => rejection,
            Err(error) => context_failure(error),
            Ok(None) => {
                let error: graph::Error<()> = match error {
                    graph::Error::Resource(error) => graph::Error::Resource(*error),
                    // Classified findings were handled above. A contradictory
                    // cycle carrier remains a transport refusal.
                    _ => graph::Error::InvalidIdentity,
                };
                traversal_outcome(schema, error).unwrap_or_else(|()| rejected(Fault::Transport))
            }
        },
    })
}

/// The refusal of a specification that declares the 1.x header (#3030 D76),
/// whose entry file is `entry`: a finding in the entry or the resolved
/// specification, a layer's traversal, or a clear finding in the layer that
/// writes it.
fn versioned_failure_ref<'a, T>(
    schema: &SchemaStructure,
    entry: &str,
    error: &'a crate::document::SpecificationError<T>,
) -> Result<Rejection, &'a T> {
    use crate::document::SpecificationError as V;
    Ok(match error {
        V::Entry(error) => capture_failure(error),
        V::Traversal(error) => return traversal(schema, error),
        V::Path(error) => match error {
            SourceFailure::Host(error) => return Err(error),
            SourceFailure::Admission(error) => capture_failure(error),
        },
        V::Clear {
            source,
            document,
            findings,
        }
        | V::Copy {
            source,
            document,
            findings,
        } => normalized(
            schema,
            Some(document),
            NormalizationError::Invalid(findings.clone()),
            &[("source", &source.identity), ("entry", entry)],
        ),
        // A copy entry's producer that was not read reports what its link
        // would (INPUT-10): findings at the link, or the producer's own.
        V::Finding(finding) => Rejection::Invalid(vec![finding.as_ref().clone()]),
        V::Producer(failure) => match failure {
            crate::document::ProducerFailure::Findings(findings) => {
                Rejection::Invalid(findings.clone())
            }
            crate::document::ProducerFailure::Preparation(error) => return failure_ref(error),
            crate::document::ProducerFailure::Host(error) => return Err(error),
        },
    })
}

/// Borrow rejected preparation so a public carrier can retain its exact sources
/// and opaque resource payload beside the complete projected findings. Source
/// provenance is resolved here; no host interprets context occurrence IDs.
pub fn failure_ref<T>(error: &InheritanceError<T>) -> Result<Rejection, &T> {
    match error {
        InheritanceError::Entry(error) => Ok(capture_failure(error)),
        InheritanceError::Dictionary { document, error } => dictionary_failure_ref(document, error),
        InheritanceError::Versioned {
            schema,
            entry,
            error,
        } => versioned_failure_ref(schema.structure(), entry, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_core::json::Value;
    #[test]
    fn traversal_preserves_the_complete_long_failure_and_original_owner() {
        let path = "x".repeat(3_000_000);
        let error = graph::Error::<()>::Unavailable {
            declaring: "entry".into(),
            path,
        };
        for _ in 0..2 {
            let Rejection::Invalid(findings) = traversal_finding(&error).unwrap().unwrap() else {
                panic!("an unavailable parent is a finding");
            };
            assert_eq!(
                findings,
                [yamaa_core::diagnostic::Diagnostic {
                    code: yamaa_core::diagnostic::ConditionCode::InheritanceParentUnavailable,
                    spec_paths: vec!["parent".into()],
                    context: [
                        ("source".into(), Value::String("entry".into())),
                        ("path".into(), Value::String("x".repeat(3_000_000))),
                    ]
                    .into(),
                    source_span: None,
                    operand_route: None,
                }]
            );
        }
        let graph::Error::Unavailable { path, .. } = error else {
            panic!("failure replaced")
        };
        assert_eq!(path.len(), 3_000_000);
    }
}
