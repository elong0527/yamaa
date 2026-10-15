//! Native mapping/cut selection with reached handlers and retained failure identity.
use super::*;
use crate::mapping_cut::{MappingCut, SelectedValue};

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
    // Estimate a copied diagnostic without charging it: only the branches
    // that can actually allocate one (selection error, conversion failure)
    // pay this charge, so successfully selected rows do not consume it.
    let diagnostic_text_bytes = operation
        .source()
        .len()
        .saturating_add(operation.path().len())
        .saturating_add(match &value {
            Value::Str(text) => text.len(),
            _ => 0,
        })
        .saturating_add(1024);
    let selected = match operation.select(ValueRef::from(&value)) {
        Ok(selected) => selected,
        Err(diagnostic) => {
            state.budget.scalar_text(diagnostic_text_bytes)?;
            return Err(retained_diagnostic(
                diagnostic,
                true,
                candidate,
                plan,
                row,
                state.budget,
            ));
        }
    };
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
                state.budget.scalar_text(diagnostic_text_bytes)?;
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
                    crate::diagnostic::ContextValue::Scalar(Value::Str(operation.source().into())),
                );
                diagnostic.context.insert(
                    "handler".into(),
                    crate::diagnostic::ContextValue::Scalar(Value::Str(handler.into())),
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
