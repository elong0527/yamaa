//! ODM scope selection, source-only eligibility, and strict record uniqueness.
use super::*;
use crate::{
    dataset::{BoundOdm, OdmScope},
    odm_source::{FIELDS, HIERARCHY, IDENTIFYING},
};

fn cell<'a, T: TableAccess + ?Sized>(
    table: &'a T,
    bound: &BoundOdm,
    row: usize,
    column: usize,
) -> Result<ValueRef<'a>, Box<ExecutionError<T::Error>>> {
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
    let original = match read.source {
        None => row_drivers::Driver::Primary(table),
        Some(AggregateSource::Primary) => row_drivers::Driver::Secondary(state.primary),
        Some(AggregateSource::Secondary(index)) => row_drivers::Driver::Secondary(
            *state
                .secondary
                .get(index)
                .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?,
        ),
    };
    let table = &original;
    let template = if let OdmScope::Templates(contexts) = &read.scope {
        Some(
            contexts
                .get(
                    candidate
                        .template
                        .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?,
                )
                .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?,
        )
    } else {
        None
    };
    let mut scope = Vec::new();
    let fields: &[usize] = match &read.scope {
        OdmScope::Keys => {
            for &column in plan.keys() {
                let value = &candidate.values[column];

                scope.push((plan.output().columns()[column].name.clone(), value.clone()));
            }
            &[]
        }
        OdmScope::Records => &HIERARCHY,
        OdmScope::Groups(fields) => fields,
        OdmScope::Templates(_) => template.unwrap().groups.as_deref().unwrap_or(&HIERARCHY),
    };
    if !fields.is_empty() {
        let representative = *candidate
            .members
            .first()
            .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
        for &column in fields {
            let value = cell(table, bound, representative, column)?;
            scope.push((FIELDS[column].into(), own(value)));
        }
    }
    let capacity = if matches!(read.scope, OdmScope::Keys) {
        candidate.members.len()
    } else {
        table.row_count()
    };

    let mut selected = Vec::with_capacity(capacity);
    for position in 0..capacity {
        let source_row = if matches!(read.scope, OdmScope::Keys) {
            candidate.members[position]
        } else {
            position
        };
        let mut matching = true;
        for (&column, (_, expected)) in fields.iter().zip(&scope) {
            if cell(table, bound, source_row, column)? != ValueRef::from(expected) {
                matching = false;
                break;
            }
        }
        if !matching || cell(table, bound, source_row, 9)? != ValueRef::Str(&read.item_oid) {
            continue;
        }
        for (column, wanted) in [3, 5, 7].into_iter().zip(&read.selectors) {
            if let Some(wanted) = wanted {
                let value = cell(table, bound, source_row, column)?;
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
            let truth =
                crate::dataset::execution::predicates::evaluate(filter, table, source_row, &[])
                    .map_err(|error| Box::new(ExecutionError::Predicate { source_row, error }))?;
            if truth != crate::predicate::Truth::True {
                continue;
            }
        }
        selected.push(source_row);
    }
    match selected.as_slice() {
        [] => Ok(Value::Missing),
        [source_row] => {
            let value = cell(table, bound, *source_row, 10)?;
            Ok(own(value))
        }
        _ => {
            let mut differ = Vec::new();
            for column in IDENTIFYING {
                let mut values = Vec::new();
                for &source_row in &selected {
                    let value = cell(table, bound, source_row, column)?;
                    if !values
                        .iter()
                        .any(|existing| value == ValueRef::from(existing))
                    {
                        values.push(own(value));
                    }
                }
                if values.len() > 1 {
                    differ.push((FIELDS[column].into(), values));
                }
            }
            let diagnostic = crate::diagnostic::OdmFinding::NotUnique {
                path: &read.path,
                item: &read.item,
                row: read
                    .row
                    .as_deref()
                    .or_else(|| template.map(|context| context.row.as_str())),
                phase: read.phase,
                scope: &scope,
                differ: &differ,
                records: selected.len(),
            }
            .diagnostic();
            let identity = if read.phase == AggregatePhase::Derivation {
                failure_identity(candidate, plan.keys(), row)?
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
