//! Declaration-ordered conversion recovery shared by every dataset assignment.
use super::*;
use crate::numeric_lifecycle::{recover_conversion, ConversionRecoveryError, LiteralHandler};

/// Recover initial conversion once; only a reached replacement consumes text budget.
pub(super) fn recover<E>(
    original: ConversionError,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<E>>> {
    let handler = plan.conversion_handler(&assignment.path);
    if let Some(LiteralHandler {
        value: Value::Str(text),
        ..
    }) = handler
    {
        budget.scalar_text(text.len())?;
    }
    let column_path = alloc::format!(
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
                let identity = failure_identity(candidate, plan.keys(), row, budget)?;
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
