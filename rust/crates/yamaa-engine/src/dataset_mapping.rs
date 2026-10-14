//! Native mapping/cut selection with reached handlers and retained failure identity.
use super::*;
use yamaa_core::mapping_cut::{MappingCut, SelectedValue};

#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    assignment: &Assignment,
    source: &Assignment,
    operation: &MappingCut,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
    retained: bool,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let value = evaluate_raw(table, source, candidate, plan, row, limits, state)?;
    state
        .budget
        .work(operation.comparison_work(ValueRef::from(&value)), 1)?;
    // Admit every possible copied diagnostic before selection can allocate it.
    state.budget.scalar_text(
        operation
            .source()
            .len()
            .saturating_add(operation.path().len())
            .saturating_add(match &value {
                Value::Str(text) => text.len(),
                _ => 0,
            })
            .saturating_add(1024),
    )?;
    let selected = operation
        .select(ValueRef::from(&value))
        .map_err(|diagnostic| {
            retained_diagnostic(diagnostic, true, candidate, plan, row, state.budget)
        })?;
    let result = match selected.value {
        SelectedValue::Literal(value) => {
            if let Value::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            value.clone()
        }
        SelectedValue::Label(text) => {
            state.budget.scalar_text(text.len())?;
            Value::Str(text.into())
        }
    };
    if let Some(handler) = selected.handler {
        let kind = match handler {
            "missing" => HandlerKind::Missing,
            "unmapped" => HandlerKind::Unmapped,
            _ => unreachable!("closed mapping/cut handler"),
        };
        state
            .handlers
            .record(&alloc::format!("{}.{}", operation.path(), handler), kind)
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
        // A reached literal has no eager conversion. At the completed result site,
        // retain its original handler alongside a fatal conversion context.
        if retained && plan.conversion_handler(&assignment.path).is_none() {
            if let Err(error) = convert(&result, plan.output().columns()[assignment.column].kind) {
                let path = alloc::format!(
                    "columns.{}",
                    plan.output().columns()[assignment.column].name
                );
                let mut diagnostic = error.into_diagnostic(path);
                diagnostic
                    .spec_paths
                    .push(alloc::format!("{}.{}", operation.path(), handler));
                diagnostic.context.insert(
                    "variable".into(),
                    yamaa_core::diagnostic::ContextValue::Scalar(Value::Str(
                        operation.source().into(),
                    )),
                );
                diagnostic.context.insert(
                    "handler".into(),
                    yamaa_core::diagnostic::ContextValue::Scalar(Value::Str(handler.into())),
                );
                return Err(retained_diagnostic(
                    diagnostic,
                    true,
                    candidate,
                    plan,
                    row,
                    state.budget,
                ));
            }
        }
    }
    Ok(result)
}
