//! Declaration-ordered conversion recovery shared by every dataset assignment.
use super::*;
use crate::numeric_lifecycle::{recover_conversion, ConversionRecoveryError};

/// Recover initial conversion once and retain the reached handler.
pub(super) fn recover<E>(
    original: ConversionError,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<E>>> {
    let handler = plan.conversion_handler(&assignment.path);
    let column_path = std::format!(
        "columns.{}",
        plan.output().columns()[assignment.column].name
    );
    match recover_conversion(
        plan.output().columns()[assignment.column].kind,
        &column_path,
        original,
        handler,
        handlers,
    ) {
        Ok(value) => Ok(value),
        Err(error) => match *error {
            ConversionRecoveryError::Accounting { error, .. } => {
                Err(Box::new(ExecutionError::HandlerAccounting(error)))
            }
            ConversionRecoveryError::Conversion { spec_path, error }
            | ConversionRecoveryError::HandlerConversion {
                spec_path,
                replacement: error,
                ..
            } => {
                let identity = failure_identity(candidate, plan.keys(), row)?;
                Err(Box::new(ExecutionError::Conversion {
                    path: spec_path,
                    output_row: row,
                    error,
                    identity,
                }))
            }
        },
    }
}
