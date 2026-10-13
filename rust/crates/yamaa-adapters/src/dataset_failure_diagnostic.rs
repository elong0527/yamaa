//! Representation-only bridge for core-owned classified dataset causes.
use crate::{dataset_transport::DatasetTransportError, numeric_transport::Diagnostic};
use std::boxed::Box;
use yamaa_core::diagnostic::ProjectionError;
use yamaa_engine::dataset::ExecutionError;

pub(super) fn failure<E>(
    error: &ExecutionError<E>,
    maximum: usize,
) -> Result<Box<Diagnostic>, DatasetTransportError> {
    error
        .diagnostic_with_limit(maximum)
        .map(|finding| Box::new(finding.into()))
        .map_err(|error| match error {
            ProjectionError::Limit => DatasetTransportError::OutputLimit,
            ProjectionError::InvalidContext => DatasetTransportError::Internal,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    #[test]
    fn projection_quota_is_a_boundary_refusal_with_original_error_retained() {
        let error = ExecutionError::<Infallible>::MultipleValues {
            path: "columns.A.source".into(),
            identifier: "A".repeat(3_000_000),
            value_count: 2,
            identity: None,
        };
        assert!(matches!(
            failure(&error, 1024),
            Err(DatasetTransportError::OutputLimit)
        ));
        assert!(
            matches!(&error, ExecutionError::MultipleValues { identifier, value_count: 2, .. } if identifier.len() == 3_000_000)
        );
        assert!(matches!(
            failure(&ExecutionError::<Infallible>::SchemaMismatch, 1024),
            Err(DatasetTransportError::Internal)
        ));
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn compatibility_projection_never_narrows_a_full_width_count() {
        let error = ExecutionError::<Infallible>::MultipleValues {
            path: "columns.A.source".into(),
            identifier: "SRC.A".into(),
            value_count: usize::MAX,
            identity: None,
        };
        let finding = failure(&error, 4096).ok().unwrap();
        let value = serde_json::to_value(finding).unwrap();
        assert_eq!(value["phase"], "derivation");
        assert_eq!(
            value["condition"],
            crate::test_diagnostic_truth::value("multiplicity")["condition"]
        );
        assert_eq!(
            value["requirement"],
            crate::test_diagnostic_truth::value("multiplicity")["requirement"]
        );
        assert_eq!(
            value["context"]["value_count"],
            serde_json::json!({"integer":usize::MAX.to_string()})
        );
    }
}
