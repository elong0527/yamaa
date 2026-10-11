//! Ordered occurrence reads and complete output admission before materialization.
use super::*;
use yamaa_core::text_template::{Part, StringTemplate};
pub(super) fn register(expression: &Expression, counter: &mut HandlerCounter) {
    match expression {
        Expression::StrTemplate {
            operands,
            operation,
        } => {
            if operation.missing().is_some() {
                counter.register(
                    &alloc::format!("{}.missing", operation.path()),
                    HandlerKind::Missing,
                );
            }
            for operand in operands {
                register(&operand.expression, counter);
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                register(&branch.assignment.expression, counter);
            }
        }
        Expression::Extreme { operands, .. }
        | Expression::StrConcat { operands, .. }
        | Expression::Imputation { operands, .. }
        | Expression::Temporal { operands, .. } => {
            for operand in operands {
                register(&operand.expression, counter);
            }
        }
        Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrContains { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::Round { source, .. } => register(&source.expression, counter),
        _ => (),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    operands: &[Assignment],
    operation: &StringTemplate,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
    retained_text: bool,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    state.budget.work(operation.parts().len(), 4)?;
    let count = operation
        .parts()
        .iter()
        .filter(|part| matches!(part, Part::Placeholder(_)))
        .count();
    // The plan's admitted occurrence arena bounds transient owned Value slots.
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| {
        Box::new(ExecutionError::Limit {
            resource: Resource::WorkCells,
            limit: limits.work_cells,
            required: None,
        })
    })?;
    for part in operation.parts() {
        if let Part::Placeholder(index) = part {
            let value = evaluate_raw(
                table,
                &operands[*index],
                candidate,
                plan,
                row,
                limits,
                state,
            )?;
            if matches!(value, Value::Missing) {
                if let Some(replacement) = operation.missing() {
                    if let Value::Str(text) = replacement {
                        if retained_text {
                            state.budget.admit_output_text(text.len())?;
                        }
                        state.budget.scalar_text(text.len())?;
                    }
                    state
                        .handlers
                        .record(
                            &alloc::format!("{}.missing", operation.path()),
                            HandlerKind::Missing,
                        )
                        .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
                    return Ok(replacement.clone());
                }
                state
                    .budget
                    .scalar_text(operation.path().len() + operation.identifiers()[*index].len())?;
                return Err(retained_diagnostic(
                    operation
                        .missing_diagnostic(*index)
                        .expect("admitted index"),
                    true,
                    candidate,
                    plan,
                    row,
                    state.budget,
                ));
            }
            if let Some(actual) = StringTemplate::input_kind(ValueRef::from(&value)) {
                state.budget.scalar_text(
                    operation.path().len() + operation.identifiers()[*index].len() + 64,
                )?;
                return Err(retained_diagnostic(
                    operation
                        .input_diagnostic(*index, actual)
                        .expect("admitted index"),
                    false,
                    candidate,
                    plan,
                    row,
                    state.budget,
                ));
            }
            values.push(value);
        }
    }
    let bytes = operation.output_bytes(&values).map_err(|error| {
        Box::new(match error {
            yamaa_core::text_template::MaterializeError::Shape => ExecutionError::SchemaMismatch,
            yamaa_core::text_template::MaterializeError::Limit => ExecutionError::Limit {
                resource: Resource::ScalarTextBytes,
                limit: limits.scalar_text_bytes,
                required: None,
            },
        })
    })?;
    if retained_text {
        state.budget.admit_output_text(bytes)?;
    }
    state.budget.scalar_text(bytes)?;
    state.budget.work(bytes, 1)?;
    let value = operation
        .materialize(&values, limits.scalar_text_bytes)
        .map_err(|_| {
            Box::new(ExecutionError::Limit {
                resource: Resource::ScalarTextBytes,
                limit: limits.scalar_text_bytes,
                required: Some(bytes),
            })
        })?;
    Ok(Value::Str(value))
}
