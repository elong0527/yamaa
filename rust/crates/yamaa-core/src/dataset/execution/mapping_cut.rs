//! Native mapping/cut selection with reached handlers and retained failure identity.
//! A missing source is counted under `missing` whether its handler is declared
//! or its default substitution gives missing.
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
    state: &mut EvaluationState<'_, T::Error>,
    retained: bool,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let value = evaluate_raw(table, source, candidate, plan, row, state)?;

    let selected = match operation.select(ValueRef::from(&value)) {
        Ok(selected) => selected,
        Err(diagnostic) => {
            return Err(retained_diagnostic(diagnostic, true, candidate, plan, row));
        }
    };
    let result = match selected.value {
        SelectedValue::Literal(value) => value.clone(),
        SelectedValue::Label(text) => Value::Str(text.into()),
        SelectedValue::Missing => Value::Missing,
    };
    if let Some(handler) = selected.handler {
        let kind = match handler {
            "missing" => HandlerKind::Missing,
            "unmapped" => HandlerKind::Unmapped,
            _ => unreachable!("closed mapping/cut handler"),
        };
        state
            .handlers
            .record(&std::format!("{}.{}", operation.path(), handler), kind)
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
        // A reached literal has no eager conversion. At the completed result site,
        // retain its original handler alongside a fatal conversion context.
        if retained && plan.conversion_handler(&assignment.path).is_none() {
            if let Err(error) = convert(&result, plan.output().columns()[assignment.column].kind) {
                let path = std::format!(
                    "columns.{}",
                    plan.output().columns()[assignment.column].name
                );
                let mut diagnostic = error.into_diagnostic(path);
                diagnostic
                    .spec_paths
                    .push(std::format!("{}.{}", operation.path(), handler));
                diagnostic.context.insert(
                    "variable".into(),
                    crate::json::Value::String(operation.source().into()),
                );
                diagnostic
                    .context
                    .insert("handler".into(), crate::json::Value::String(handler.into()));
                return Err(retained_diagnostic(diagnostic, true, candidate, plan, row));
            }
        }
    }
    Ok(result)
}
