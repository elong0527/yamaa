//! ODM scope selection, source-only eligibility, and strict record uniqueness.
use super::*;
use yamaa_core::{
    dataset::{BoundOdm, OdmScope},
    odm_source::{FIELDS, HIERARCHY, IDENTIFYING},
};

fn cell<'a, T: TableAccess + ?Sized>(
    table: &'a T,
    bound: &BoundOdm,
    row: usize,
    column: usize,
    budget: &mut Budget,
) -> Result<ValueRef<'a>, Box<ExecutionError<T::Error>>> {
    budget.work(1, 1)?;
    table.cell(row, column).map_err(|error| {
        Box::new(ExecutionError::Cell {
            path: bound.read().path.clone(),
            source_row: row,
            error,
        })
    })
}

pub(super) fn evaluate<T: TableAccess + ?Sized>(
    bound: &BoundOdm,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let read = bound.read();
    let mut scope = Vec::new();
    let fields: &[usize] = match &read.scope {
        OdmScope::Keys => {
            for &column in plan.keys() {
                let value = &candidate.values[column];
                state
                    .budget
                    .identity_refs(core::iter::once(ValueRef::from(value)))?;
                scope.push((plan.output().columns()[column].name.clone(), value.clone()));
            }
            &[]
        }
        OdmScope::Records => &HIERARCHY,
        OdmScope::Groups(fields) => fields,
    };
    if !fields.is_empty() {
        let representative = *candidate
            .members
            .first()
            .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
        for &column in fields {
            let value = cell(table, bound, representative, column, state.budget)?;
            if let ValueRef::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            scope.push((FIELDS[column].into(), own(value)));
        }
    }
    let capacity = if matches!(read.scope, OdmScope::Keys) {
        candidate.members.len()
    } else {
        table.row_count()
    };
    state.budget.work(capacity, 1)?;
    let mut selected = Vec::with_capacity(capacity);
    for position in 0..capacity {
        let source_row = if matches!(read.scope, OdmScope::Keys) {
            candidate.members[position]
        } else {
            position
        };
        let mut matching = true;
        for (&column, (_, expected)) in fields.iter().zip(&scope) {
            if cell(table, bound, source_row, column, state.budget)? != ValueRef::from(expected) {
                matching = false;
                break;
            }
        }
        if !matching
            || cell(table, bound, source_row, 9, state.budget)? != ValueRef::Str(&read.item_oid)
        {
            continue;
        }
        for (column, wanted) in [3, 5, 7].into_iter().zip(&read.selectors) {
            if let Some(wanted) = wanted {
                let value = cell(table, bound, source_row, column, state.budget)?;
                if !wanted.iter().any(|wanted| value == ValueRef::Str(wanted)) {
                    matching = false;
                    break;
                }
            }
        }
        if !matching {
            continue;
        }
        if let Some(filter) = &read.filter {
            let truth = crate::dataset_predicate::evaluate(
                filter,
                table,
                source_row,
                &[],
                state.budget.predicate(),
            )
            .map_err(|error| match error.kind {
                yamaa_core::predicate::ErrorKind::Limit(limit) => Box::new(predicate_limit(limit)),
                _ => Box::new(ExecutionError::Predicate { source_row, error }),
            })?;
            if truth != yamaa_core::predicate::Truth::True {
                continue;
            }
        }
        selected.push(source_row);
    }
    match selected.as_slice() {
        [] => Ok(Value::Missing),
        [source_row] => {
            let value = cell(table, bound, *source_row, 10, state.budget)?;
            if let ValueRef::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            Ok(own(value))
        }
        _ => {
            state.budget.work(selected.len(), IDENTIFYING.len())?;
            let mut differ = Vec::new();
            for column in IDENTIFYING {
                let mut values = Vec::new();
                for &source_row in &selected {
                    let value = cell(table, bound, source_row, column, state.budget)?;
                    if !values
                        .iter()
                        .any(|existing| value == ValueRef::from(existing))
                    {
                        if let ValueRef::Str(text) = value {
                            state.budget.scalar_text(text.len())?;
                        }
                        values.push(own(value));
                    }
                }
                if values.len() > 1 {
                    differ.push((FIELDS[column].into(), values));
                }
            }
            let diagnostic = yamaa_core::diagnostic::OdmFinding::NotUnique {
                path: &read.path,
                item: &read.item,
                row: read.row.as_deref(),
                phase: read.phase,
                scope: &scope,
                differ: &differ,
                records: selected.len(),
            }
            .diagnostic()
            .map_err(|_| Box::new(ExecutionError::Capacity))?;
            let identity = if read.phase == AggregatePhase::Derivation {
                failure_identity(candidate, plan.keys(), row, state.budget)?
            } else {
                None
            };
            Err(Box::new(ExecutionError::SemanticDiagnostic {
                diagnostic,
                identity,
            }))
        }
    }
}
