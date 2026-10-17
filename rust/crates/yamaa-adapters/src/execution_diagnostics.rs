//! Representation of typed execution causes in the portable issue shape.
use crate::dataset_result::DatasetError as Error;
use yamaa_core::json::Value;
use yamaa_core::table::CellError;
pub(crate) type Diagnostic = Value;
pub(crate) fn project(diagnostic: yamaa_core::diagnostic::Diagnostic) -> Result<Box<Value>, Error> {
    Ok(Box::new(
        crate::specification_diagnostics::portable_diagnostic(diagnostic),
    ))
}
pub(crate) fn predicate<E>(
    error: &yamaa_core::predicate::EvaluationError<CellError<E>>,
) -> Result<Box<Value>, Error> {
    project(error.diagnostic().map_err(|_| Error::InvalidObservation)?)
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
    error: &yamaa_core::dataset::ExecutionError<E>,
) -> Result<Box<Value>, Error> {
    project(
        error
            .diagnostic()
            .map_err(|_| Error::InvalidObservation)?
            .ok_or(Error::UnsupportedOutcome)?,
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
    use yamaa_core::dataset::ExecutionError;
    #[test]
    fn complete_diagnostic_keeps_long_context_and_original_typed_error() {
        let error = ExecutionError::<Infallible>::MultipleValues {
            path: "columns.A.source".into(),
            identifier: "A".repeat(3_000_000),
            value_count: 2,
            identity: None,
        };
        let projected = failure(&error).unwrap();
        assert_eq!(
            projected["context"]["identifier"].as_str().unwrap().len(),
            3_000_000
        );
        assert!(
            matches!(&error,ExecutionError::MultipleValues {identifier,value_count:2,..} if identifier.len()==3_000_000)
        );
        assert!(matches!(
            failure(&ExecutionError::<Infallible>::SchemaMismatch),
            Err(Error::UnsupportedOutcome)
        ));
    }
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn full_width_count_keeps_five_fields_when_no_geometry_was_recorded() {
        let error = ExecutionError::<Infallible>::MultipleValues {
            path: "columns.A.source".into(),
            identifier: "SRC.A".into(),
            value_count: usize::MAX,
            identity: None,
        };
        let value = *failure(&error).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 5);
        let issue = crate::issue_rows::Issue::from_diagnostic(value).unwrap();
        let expected: Value = crate::json_wire::decode(include_str!(
            "../tests/fixtures/retired_probe_diagnostics.json"
        ))
        .unwrap();
        assert_eq!(issue.phase, expected["phase"].as_str().unwrap());
        assert_eq!(issue.condition, expected["condition"].as_str().unwrap());
        assert_eq!(
            issue.requirement.as_deref(),
            expected["requirement"].as_str()
        );
        let context: Value = issue.context.clone();
        assert_eq!(context, expected["context"]);
        assert_eq!(
            crate::json_wire::encode(&context["value_count"]),
            usize::MAX.to_string()
        );
    }
}
