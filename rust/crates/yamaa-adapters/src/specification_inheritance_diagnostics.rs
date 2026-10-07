//! Format shared inheritance outcomes without consuming opaque host failures.
use super::capture_failure;
use crate::{
    inheritance_transport::{self, Failure},
    schema_transport::{errors, wire::Tree},
    specification_source::{InheritanceError, SourceFailure},
};
use serde_json::{json, Value};
use yamaa_engine::inheritance_preparation::Error as E;

fn source<E>(error: SourceFailure<E>) -> Result<String, E> {
    match error {
        SourceFailure::Host(error) => Err(error),
        SourceFailure::Admission(error) => Ok(capture_failure(&error)),
    }
}
fn rejected(code: &str) -> Value {
    json!({"status":"rejected","stage":"inheritance","code":code})
}
/// This prototype preserves existing schema/traversal diagnostics and resource outcomes.
/// A qualified public frontend must still reconcile these with complete run observations.
pub fn failure<T>(error: InheritanceError<T>) -> Result<String, T> {
    let error = match error {
        InheritanceError::Entry(error) => return Ok(capture_failure(&error)),
        InheritanceError::Preparation { error, .. } => *error,
    };
    let outcome = match error {
        E::Path(error) => return source(error),
        E::Traversal(error) => match inheritance_transport::traversal_outcome(error) {
            Ok(value) => value,
            Err(Failure::Host(error)) => return source(error),
            Err(Failure::Transport(_)) => rejected("transport"),
        },
        E::ParentsRequired => rejected("parents_required"),
        E::PathBytes => rejected("path_byte_limit"),
        E::Document(error) => errors::document(error).unwrap_or_else(|_| rejected("document")),
        E::Normalize(error) => {
            errors::normalization(error).unwrap_or_else(|_| rejected("normalization"))
        }
        E::Validation(error) => {
            errors::validation(error).unwrap_or_else(|_| rejected("validation_policy"))
        }
        E::Dependencies(error) => errors::inheritance_dependencies(error)
            .unwrap_or_else(|_| rejected("dependency_policy")),
        E::Model(findings) => {
            errors::normalization(yamaa_core::schema::NormalizationError::Invalid(findings))
                .unwrap_or_else(|_| rejected("invalid_model"))
        }
        E::Composition(error) => {
            let mut outcome =
                errors::normalization(error.error).unwrap_or_else(|_| rejected("composition"));
            if let Some(document) = error.context_document {
                outcome["context_document"] = json!(Tree::from_core(&document));
            }
            outcome
        }
    };
    let response = json!({"protocol":"specification/prototype","outcome":outcome}).to_string();
    Ok(if response.len() > 16_777_216 {
        json!({"protocol":"specification/prototype","outcome":rejected("response_limit")})
            .to_string()
    } else {
        response
    })
}
