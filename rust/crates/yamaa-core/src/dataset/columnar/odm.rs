//! ODM item reads: scope selection, source-only eligibility, and strict
//! record uniqueness.
use super::buffer::Values;
use super::eval::owned;
use super::{Cx, Fail, Run, Table};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{ExprId, Op};
use crate::dataset::{AggregatePhase, AggregateSource, BoundOdm, OdmScope};
use crate::odm_source::{FIELDS, HIERARCHY, IDENTIFYING};
use crate::table::{TableAccess, ValueRef};
use crate::value::Value;

fn cell<'t, E>(
    table: Table<'t, E>,
    bound: &BoundOdm,
    row: usize,
    column: usize,
) -> Result<ValueRef<'t>, Fail<E>> {
    table.get(row, column).map_err(|error| {
        Box::new(ExecutionError::Cell {
            path: bound.read().path.clone(),
            source_row: row,
            error,
        })
    })
}

/// An ODM item read node over rows.
pub(super) fn odm<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let mut values = Values::missing(0);
    for &row in rows {
        values.push_value(odm_value(run, cx, expr, row)?);
    }
    Ok(values)
}

fn odm_value<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    row: usize,
) -> Result<Value, Fail<E>> {
    let plan = run.plan;
    let Op::Odm(bound) = &plan.expr(expr).op else {
        unreachable!("an ODM node holds its read")
    };
    let read = bound.read();
    let table = match read.source {
        None => cx.table,
        Some(AggregateSource::Primary) => cx.primary,
        Some(AggregateSource::Secondary(index)) => *cx
            .secondary
            .get(index)
            .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?,
    };
    let template = if let OdmScope::Templates(contexts) = &read.scope {
        let index = cx
            .frame
            .template(row)
            .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
        Some(
            contexts
                .get(index)
                .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?,
        )
    } else {
        None
    };
    let mut scope = Vec::new();
    let fields: &[usize] = match &read.scope {
        OdmScope::Keys => {
            let keys: &[_] = match plan.region(cx.region).kind {
                crate::dataset::resolved::RegionKind::Dataset => plan.keys(),
                crate::dataset::resolved::RegionKind::Donor { .. } => &[],
            };
            for &key in keys {
                let column = cx.local(plan, key);
                scope.push((
                    cx.schema.columns()[column].name.clone(),
                    cx.frame.value(row, column),
                ));
            }
            &[]
        }
        OdmScope::Records => &HIERARCHY,
        OdmScope::Groups(fields) => fields,
        OdmScope::Templates(_) => template
            .expect("a template scope")
            .groups
            .as_deref()
            .unwrap_or(&HIERARCHY),
    };
    let members = cx.frame.members(row);
    if !fields.is_empty() {
        let representative = *members
            .first()
            .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
        for &column in fields {
            let value = cell(table, bound, representative, column)?;
            scope.push((FIELDS[column].into(), owned(value)));
        }
    }
    let keyed = matches!(read.scope, OdmScope::Keys);
    let candidates: Vec<usize> = if keyed {
        members.to_vec()
    } else {
        (0..table.row_count()).collect()
    };
    let mut selected = Vec::new();
    for source_row in candidates {
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
                super::predicates::row_truth(filter, table, source_row, |_| ValueRef::Missing)
                    .map_err(|error| {
                        Box::new(ExecutionError::Predicate {
                            source_row,
                            error: *error,
                        })
                    })?;
            if truth != crate::expression::Truth::True {
                continue;
            }
        }
        selected.push(source_row);
    }
    match selected.as_slice() {
        [] => Ok(Value::Missing),
        [source_row] => Ok(owned(cell(table, bound, *source_row, 10)?)),
        _ => {
            let mut differ = Vec::new();
            for column in IDENTIFYING {
                let mut values: Vec<Value> = Vec::new();
                for &source_row in &selected {
                    let value = cell(table, bound, source_row, column)?;
                    if !values
                        .iter()
                        .any(|existing| value == ValueRef::from(existing))
                    {
                        values.push(owned(value));
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
                run.failure_identity(cx, row)
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
