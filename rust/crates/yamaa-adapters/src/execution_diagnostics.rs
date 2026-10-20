//! Typed execution causes as diagnostic records.
use crate::dataset_result::DatasetError as Error;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::table::CellError;
pub(crate) fn predicate<E>(
    error: &yamaa_core::expression::EvaluationError<CellError<E>>,
) -> Result<Diagnostic, Error> {
    error.diagnostic().map_err(|_| Error::InvalidObservation)
}
pub(crate) fn numeric(
    error: yamaa_core::expression::EvaluationError<std::convert::Infallible>,
) -> Result<Diagnostic, Error> {
    error.diagnostic().map_err(|_| Error::Internal)
}
pub(crate) fn failure<E>(
    error: &yamaa_core::dataset::ExecutionError<E>,
) -> Result<Diagnostic, Error> {
    error
        .diagnostic()
        .map_err(|_| Error::InvalidObservation)?
        .ok_or(Error::UnsupportedOutcome)
}

pub(crate) fn declaration(
    path: String,
    code: yamaa_core::diagnostic::ConditionCode,
    reason: String,
) -> Result<Diagnostic, Error> {
    yamaa_core::dataset_checks::declaration_diagnostic(path, code, reason).ok_or(Error::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use yamaa_core::dataset::ExecutionError;
    use yamaa_core::json::Value;
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
            projected.context["identifier"].as_str().unwrap().len(),
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
        let value = crate::report_storage::diagnostic_value(&failure(&error).unwrap());
        assert_eq!(value.as_object().unwrap().len(), 5);
        let expected: Value = crate::json_wire::decode(include_str!(
            "../tests/fixtures/retired_probe_diagnostics.json"
        ))
        .unwrap();
        assert_eq!(value["phase"], expected["phase"]);
        assert_eq!(value["condition"], expected["condition"]);
        assert_eq!(value["rule"], expected["rule"]);
        assert_eq!(value["context"], expected["context"]);
        assert_eq!(
            crate::json_wire::encode(&value["context"]["value_count"]),
            usize::MAX.to_string()
        );
    }
}
