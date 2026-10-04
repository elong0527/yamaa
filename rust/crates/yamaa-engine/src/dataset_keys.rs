//! Standalone key combinations and their complete feeding-record memberships.
use super::*;
use crate::table_grouping::Key;
use alloc::collections::BTreeSet;

/// Evaluate every key lifecycle before non-key derivation, then retain first occurrences.
pub(super) fn construct<T: TableAccess + ?Sized>(
    plan: &DatasetPlan,
    table: &T,
    limits: Limits,
    budget: &mut Budget,
    handlers: &mut HandlerCounter,
) -> Result<Vec<Candidate>, Box<ExecutionError<T::Error>>> {
    let rows = table.row_count();
    if rows
        .checked_mul(plan.keys.len())
        .is_none_or(|cells| cells > limits.key_cells)
    {
        return Err(Box::new(ExecutionError::Grouping(
            GroupingError::KeyCellLimit {
                limit: limits.key_cells,
            },
        )));
    }
    // Finish the entire source filter before any key conversion can fail.
    let mut retained = Vec::new();
    retained
        .try_reserve_exact(rows)
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    budget.work(rows, 1)?;
    for row in 0..rows {
        if let Some(filter) = &plan.templates[0].filter {
            let truth = filter
                .evaluate(table, row, &[], budget.predicate())
                .map_err(|error| match error.kind {
                    yamaa_core::predicate::ErrorKind::Limit(limit) => {
                        Box::new(predicate_limit(limit))
                    }
                    _ => Box::new(ExecutionError::Predicate {
                        source_row: row,
                        error,
                    }),
                })?;
            if truth != yamaa_core::predicate::Truth::True {
                continue;
            }
        }
        retained.push(row);
    }
    let rows = retained.len();
    let mut key_table = Dataset {
        schema: TableSchema::new(
            plan.keys
                .iter()
                .map(|&column| plan.output.columns()[column].clone())
                .collect(),
        )
        .expect("admitted distinct output keys form a valid schema"),
        rows: Vec::new(),
    };
    key_table
        .rows
        .try_reserve_exact(rows)
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for &row in &retained {
        let mut probe = Candidate {
            members: vec![row],
            values: vec![Value::Missing; plan.output.columns().len()],
            completed: vec![false; plan.output.columns().len()],
        };
        for assignment in &plan.templates[0].assignments {
            probe.values[assignment.column] = evaluate(
                table,
                assignment,
                &probe,
                plan,
                row,
                limits,
                &mut EvaluationState {
                    functions: &mut functions::UnavailableFunctions(core::marker::PhantomData),
                    budget,
                    handlers,
                    secondary: &[],
                    intermediates: &mut intermediates::Run::default(),
                },
            )?;
            probe.completed[assignment.column] = true;
        }
        key_table.rows.push(
            plan.keys
                .iter()
                .map(|&column| core::mem::replace(&mut probe.values[column], Value::Missing))
                .collect(),
        );
    }
    let key_columns: Vec<_> = (0..plan.keys.len()).collect();
    budget.work(rows, plan.keys.len())?;
    let partitions = partition(
        &key_table,
        &key_columns,
        limits.source_rows,
        limits.key_cells,
    )
    .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
    budget.work(partitions.len(), plan.keys.len())?;
    budget.work(rows, 1)?;
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(rows)
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for members in partitions {
        if key_table.rows[members[0]]
            .iter()
            .any(|value| matches!(value, Value::Missing))
        {
            // Each missing-key record survives independently for the output gate.
            // A structural split cannot collide with any user-supplied key value.
            groups.extend(members.into_iter().map(|member| vec![member]));
        } else {
            groups.push(members);
        }
    }
    groups.sort_unstable_by_key(|members| members[0]);
    plan.check_capacity(0, groups.len(), limits)?;
    let mut candidates = Vec::new();
    candidates
        .try_reserve_exact(groups.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for members in groups {
        let keys = core::mem::take(&mut key_table.rows[members[0]]);
        for &duplicate in &members[1..] {
            budget.discard_candidate(&key_table.rows[duplicate]);
            key_table.rows[duplicate].clear();
        }
        let mut candidate = Candidate {
            members: members.into_iter().map(|member| retained[member]).collect(),
            values: vec![Value::Missing; plan.output.columns().len()],
            completed: vec![false; plan.output.columns().len()],
        };
        for (&column, value) in plan.keys.iter().zip(keys) {
            candidate.values[column] = value;
            candidate.completed[column] = true;
        }
        candidates.push(candidate);
    }
    Ok(candidates)
}

/// Count every distinct present raw reading before output conversion or choosing a value.
pub(super) fn collect<T: TableAccess + ?Sized>(
    table: &T,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let Expression::Collect {
        column,
        identifier,
        filter,
        selection,
    } = &assignment.expression
    else {
        unreachable!("only collected-source assignments use this service")
    };
    // Complete eligibility before reading values, including later predicate failures.
    let mut retained = Vec::new();
    let members = if let Some(filter) = filter {
        retained
            .try_reserve_exact(candidate.members.len())
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        budget.work(candidate.members.len(), 1)?;
        for &source_row in &candidate.members {
            let truth = filter
                .evaluate(table, source_row, &[], budget.predicate())
                .map_err(|error| match error.kind {
                    yamaa_core::predicate::ErrorKind::Limit(limit) => {
                        Box::new(predicate_limit(limit))
                    }
                    _ => Box::new(ExecutionError::Predicate { source_row, error }),
                })?;
            if truth == yamaa_core::predicate::Truth::True {
                retained.push(source_row);
            }
        }
        &retained
    } else {
        &candidate.members
    };
    let mut carrying = Vec::new();
    if selection.is_some() {
        carrying
            .try_reserve_exact(members.len())
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
    }
    let mut distinct = BTreeSet::new();
    let mut first = None;
    for &source_row in members {
        let value = table.cell(source_row, *column).map_err(|error| {
            Box::new(ExecutionError::Cell {
                path: assignment.path.clone(),
                source_row,
                error,
            })
        })?;
        if matches!(value, ValueRef::Missing) {
            continue;
        }
        if let ValueRef::Str(text) = value {
            budget.scalar_text(text.len())?;
        }
        distinct.insert(Key::from(value));
        first.get_or_insert(value);
        if selection.is_some() {
            budget.work(1, 1)?;
            carrying.push(source_row);
        }
    }
    if distinct.len() > 1 {
        if let Some(selection) = selection {
            let chosen = select(table, assignment, &carrying, selection, budget)?;
            budget.work(1, 1)?;
            let value = table.cell(chosen, *column).map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: assignment.path.clone(),
                    source_row: chosen,
                    error,
                })
            })?;
            if let ValueRef::Str(text) = value {
                budget.scalar_text(text.len())?;
            }
            handlers
                .record(
                    &alloc::format!("{}.multiple_matches", assignment.path),
                    HandlerKind::MultipleMatches,
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            return Ok(own(value));
        }
        return Err(Box::new(ExecutionError::MultipleValues {
            path: assignment.path.clone(),
            identifier: identifier.clone(),
            value_count: distinct.len(),
            identity: failure_identity(candidate, &plan.keys, row, budget)?,
        }));
    }
    Ok(first.map_or(Value::Missing, own))
}

/// Select a stable extremum without sorting or cloning donor payloads.
fn select<T: TableAccess + ?Sized>(
    table: &T,
    assignment: &Assignment,
    carrying: &[usize],
    selection: &SourceSelection,
    budget: &mut Budget,
) -> Result<usize, Box<ExecutionError<T::Error>>> {
    use core::cmp::Ordering;
    let mut best = carrying[0];
    for &row in &carrying[1..] {
        let mut order = Ordering::Equal;
        for term in &selection.order_by {
            budget.work(1, 2)?;
            let mut read = |source_row| -> Result<ValueRef<'_>, Box<ExecutionError<T::Error>>> {
                let value = table.cell(source_row, term.column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: assignment.path.clone(),
                        source_row,
                        error,
                    })
                })?;
                if let ValueRef::Str(text) = value {
                    budget.scalar_text(text.len())?;
                }
                Ok(value)
            };
            let left = read(row)?;
            let right = read(best)?;
            order = windows::compare(left, right, term);
            if order != Ordering::Equal {
                break;
            }
        }
        order = order.then_with(|| row.cmp(&best));
        if (selection.keep == Keep::First && order == Ordering::Less)
            || (selection.keep == Keep::Last && order == Ordering::Greater)
        {
            best = row;
        }
    }
    Ok(best)
}
