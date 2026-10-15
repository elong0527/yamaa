//! Representation of typed execution causes in the portable issue shape.
use crate::dataset_result::DatasetError as Error;
use serde_json::Value;
use yamaa_core::{diagnostic::ProjectionError, table::CellError};
pub(crate) type Diagnostic = Value;
pub(crate) fn project(diagnostic: yamaa_core::diagnostic::Diagnostic) -> Result<Box<Value>, Error> {
    crate::specification_diagnostics::portable_diagnostic(diagnostic)
        .map(Box::new)
        .ok_or(Error::Internal)
}
pub(crate) fn predicate<E>(
    error: &yamaa_core::predicate::EvaluationError<CellError<E>>,
) -> Result<Box<Value>, Error> {
    use yamaa_core::diagnostic::PredicateDiagnosticError;
    project(error.diagnostic().map_err(|error| match error {
        PredicateDiagnosticError::OutputLimit => Error::OutputLimit,
        _ => Error::Internal,
    })?)
}
pub(crate) fn numeric(
    error: yamaa_core::numeric_compiler::CompiledEvaluationError<std::convert::Infallible>,
) -> Result<Box<Value>, Error> {
    project(error.diagnostic().ok_or(Error::Internal)?)
}
pub(crate) fn arithmetic(
    error: yamaa_core::numeric::ArithmeticError,
    path: String,
) -> Result<Box<Value>, Error> {
    project(error.into_diagnostic(path))
}
pub(crate) fn conversion(
    error: yamaa_core::conversion::ConversionError,
    path: String,
) -> Result<Box<Value>, Error> {
    project(error.into_diagnostic(path))
}
pub(crate) fn failure<E>(
    error: &yamaa_engine::dataset::ExecutionError<E>,
    maximum: usize,
) -> Result<Box<Value>, Error> {
    project(
        error
            .diagnostic_with_limit(maximum)
            .map_err(|error| match error {
                ProjectionError::Limit => Error::OutputLimit,
                ProjectionError::InvalidContext => Error::Internal,
            })?,
    )
}

pub(crate) fn declaration(
    path: String,
    code: yamaa_core::diagnostic::ConditionCode,
    reason: String,
) -> Result<Box<Value>, Error> {
    project(
        yamaa_core::dataset_checks::declaration_diagnostic(path, code, reason)
            .ok_or(Error::Internal)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use yamaa_engine::dataset::ExecutionError;
    #[test]
    fn projection_refusal_preserves_original_typed_error() {
        let error = ExecutionError::<Infallible>::MultipleValues {
            path: "columns.A.source".into(),
            identifier: "A".repeat(3_000_000),
            value_count: 2,
            identity: None,
        };
        assert!(matches!(failure(&error, 1024), Err(Error::OutputLimit)));
        assert!(
            matches!(&error,ExecutionError::MultipleValues {identifier,value_count:2,..} if identifier.len()==3_000_000)
        );
        assert!(matches!(
            failure(&ExecutionError::<Infallible>::SchemaMismatch, 1024),
            Err(Error::Internal)
        ));
    }
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn full_width_count_uses_the_one_five_field_diagnostic_shape() {
        let error = ExecutionError::<Infallible>::MultipleValues {
            path: "columns.A.source".into(),
            identifier: "SRC.A".into(),
            value_count: usize::MAX,
            identity: None,
        };
        let value = *failure(&error, 4096).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 5);
        let issue = crate::issue_rows::Issue::from_diagnostic(value).unwrap();
        let expected: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/retired_probe_diagnostics.json"
        ))
        .unwrap();
        assert_eq!(issue.phase, expected["phase"].as_str().unwrap());
        assert_eq!(issue.condition, expected["condition"].as_str().unwrap());
        assert_eq!(
            issue.requirement.as_deref(),
            expected["requirement"].as_str()
        );
        let context: Value = serde_json::from_str(&issue.context).unwrap();
        assert_eq!(context, expected["context"]);
        assert_eq!(context["value_count"].to_string(), usize::MAX.to_string());
    }
}
