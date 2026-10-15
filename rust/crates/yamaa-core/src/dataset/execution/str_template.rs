//! Ordered occurrence reads and complete output admission before materialization.
use super::*;
use crate::text_template::{Part, StringTemplate};
pub(super) fn register(expression: &Expression, counter: &mut HandlerCounter) {
    match expression {
        Expression::StrTemplate {
            operands,
            operation,
        } => {
            if operation.missing().is_some() {
                counter.register(
                    &std::format!("{}.missing", operation.path()),
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
        | Expression::MappingCut { source, .. }
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
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let count = operation
        .parts()
        .iter()
        .filter(|part| matches!(part, Part::Placeholder(_)))
        .count();
    // The plan's admitted occurrence arena bounds transient owned Value slots.
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for part in operation.parts() {
        if let Part::Placeholder(index) = part {
            let value = evaluate_raw(table, &operands[*index], candidate, plan, row, state)?;
            if matches!(value, Value::Missing) {
                if let Some(replacement) = operation.missing() {
                    state
                        .handlers
                        .record(
                            &std::format!("{}.missing", operation.path()),
                            HandlerKind::Missing,
                        )
                        .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
                    return Ok(replacement.clone());
                }

                return Err(retained_diagnostic(
                    operation
                        .missing_diagnostic(*index)
                        .expect("admitted index"),
                    true,
                    candidate,
                    plan,
                    row,
                ));
            }
            if let Some(actual) = StringTemplate::input_kind(ValueRef::from(&value)) {
                return Err(retained_diagnostic(
                    operation
                        .input_diagnostic(*index, actual)
                        .expect("admitted index"),
                    false,
                    candidate,
                    plan,
                    row,
                ));
            }
            values.push(value);
        }
    }
    let value = operation.materialize(&values).map_err(|error| {
        Box::new(match error {
            crate::text_template::MaterializeError::Shape => ExecutionError::SchemaMismatch,
            crate::text_template::MaterializeError::Size => ExecutionError::Capacity,
            crate::text_template::MaterializeError::Allocation => ExecutionError::Allocation,
        })
    })?;
    Ok(Value::Str(value))
}
