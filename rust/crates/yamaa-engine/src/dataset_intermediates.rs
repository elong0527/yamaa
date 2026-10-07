//! Lazy source eligibility and named record selection within one dataset attempt.
use super::*;
use crate::table_grouping::Key;
use alloc::collections::BTreeMap;

/// Keep only immutable row coordinates and the inherited handler outcome.
#[derive(Clone, Copy)]
enum Choice {
    Record { row: usize, multiple: bool },
    Absent,
}

/// Fresh for every attempt, with no entries for unused intermediate declarations.
#[derive(Default)]
pub(super) struct Run {
    eligible: BTreeMap<usize, Vec<usize>>,
    choices: BTreeMap<(usize, usize), Choice>,
}

/// Reuse a selected record while recording each reading's own inherited handler site.
pub(super) fn read<E>(
    index: usize,
    column: usize,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, E>,
) -> Result<Value, Box<ExecutionError<E>>> {
    let item = &plan.intermediates()[index];
    let table = state.secondary[item.source];
    let choice = if let Some(choice) = state.intermediates.choices.get(&(row, index)) {
        *choice
    } else {
        if !state.intermediates.eligible.contains_key(&index) {
            // Complete the source-only filter before checking the current match values.
            state.budget.work(table.row_count(), 1)?;
            let mut eligible = Vec::new();
            eligible
                .try_reserve_exact(table.row_count())
                .map_err(|_| Box::new(ExecutionError::Allocation))?;
            for source_row in 0..table.row_count() {
                if let Some(filter) = &item.filter {
                    let truth = crate::dataset_predicate::evaluate(
                        filter,
                        table,
                        source_row,
                        &[],
                        state.budget.predicate(),
                    )
                    .map_err(|error| match error.kind {
                        yamaa_core::predicate::ErrorKind::Limit(limit) => {
                            Box::new(predicate_limit(limit))
                        }
                        _ => Box::new(ExecutionError::Predicate { source_row, error }),
                    })?;
                    if truth != yamaa_core::predicate::Truth::True {
                        continue;
                    }
                }
                eligible.push(source_row);
            }
            state.budget.work(1, 1)?;
            state.intermediates.eligible.insert(index, eligible);
        }
        let eligible = &state.intermediates.eligible[&index];
        let mut count = 0;
        let mut selected = None;
        if !item
            .keys
            .iter()
            .any(|key| matches!(candidate.values[key.output_column], Value::Missing))
        {
            // First count every matching record without reading order or donor fields.
            let mut matching = Vec::new();
            state.budget.work(eligible.len(), 1)?;
            matching
                .try_reserve_exact(eligible.len())
                .map_err(|_| Box::new(ExecutionError::Allocation))?;
            for &source_row in eligible {
                let mut matches = true;
                for key in &item.keys {
                    let value = cell(table, item, source_row, key.source_column, state.budget)?;
                    if Key::from(value)
                        != Key::from(ValueRef::from(&candidate.values[key.output_column]))
                    {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    matching.push(source_row);
                }
            }
            count = matching.len();
            selected = matching.first().copied();
            if count > 1 {
                if let Some(selection) = &item.selection {
                    selected = Some(select(table, item, &matching, selection, state.budget)?);
                } else {
                    return Err(Box::new(ExecutionError::MultipleMatches {
                        path: item.path.clone(),
                        dataset: plan.secondary()[item.source].name.clone(),
                        intermediate: item.identifier.clone(),
                        match_count: count,
                        matched_key: matched_key(item, table, candidate, state.budget)?,
                        identity: failure_identity(candidate, plan.keys(), row, state.budget)?,
                    }));
                }
            }
        }
        let choice = if let Some(row) = selected {
            Choice::Record {
                row,
                multiple: count > 1,
            }
        } else if item.no_match.is_some() {
            Choice::Absent
        } else {
            return Err(Box::new(ExecutionError::UnmatchedKey {
                path: item.path.clone(),
                dataset: plan.secondary()[item.source].name.clone(),
                intermediate: item.identifier.clone(),
                matched_key: matched_key(item, table, candidate, state.budget)?,
                identity: failure_identity(candidate, plan.keys(), row, state.budget)?,
            }));
        };
        state.budget.work(1, 1)?;
        state.intermediates.choices.insert((row, index), choice);
        choice
    };
    let (value, handler) = match choice {
        Choice::Record { row, multiple } => (
            own(cell(table, item, row, column, state.budget)?),
            multiple.then_some(HandlerKind::MultipleMatches),
        ),
        Choice::Absent => {
            let value = item
                .no_match
                .as_ref()
                .expect("absence handler was admitted before caching");
            if let Value::Str(text) = value {
                state.budget.scalar_text(text.len())?;
            }
            (value.clone(), Some(HandlerKind::NoMatch))
        }
    };
    if let Some(handler) = handler {
        state
            .handlers
            .record(
                &alloc::format!("{}.{}", assignment.path, handler.name()),
                handler,
            )
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
    }
    Ok(value)
}

/// Retain exact key values separately from output-row identity in join diagnostics.
fn matched_key<E>(
    item: &Intermediate,
    table: &dyn TableAccess<Error = E>,
    candidate: &Candidate,
    budget: &mut Budget,
) -> Result<Vec<(String, Value)>, Box<ExecutionError<E>>> {
    budget.work(1, item.keys.len())?;
    budget.identity(
        item.keys
            .iter()
            .map(|key| &candidate.values[key.output_column]),
    )?;
    Ok(item
        .keys
        .iter()
        .map(|key| {
            (
                table.schema().columns()[key.source_column].name.clone(),
                candidate.values[key.output_column].clone(),
            )
        })
        .collect())
}

/// Charge every opaque secondary read and preserve its source coordinates.
fn cell<'a, E>(
    table: &'a dyn TableAccess<Error = E>,
    item: &Intermediate,
    row: usize,
    column: usize,
    budget: &mut Budget,
) -> Result<ValueRef<'a>, Box<ExecutionError<E>>> {
    budget.work(1, 1)?;
    let value = table.cell(row, column).map_err(|error| {
        Box::new(ExecutionError::SecondaryCell {
            path: item.path.clone(),
            source: item.source,
            source_row: row,
            error,
        })
    })?;
    if let ValueRef::Str(text) = value {
        budget.scalar_text(text.len())?;
    }
    Ok(value)
}

/// Select one stable extremum, retaining input position as the final tie breaker.
fn select<E>(
    table: &dyn TableAccess<Error = E>,
    item: &Intermediate,
    matching: &[usize],
    selection: &SourceSelection,
    budget: &mut Budget,
) -> Result<usize, Box<ExecutionError<E>>> {
    use core::cmp::Ordering;
    let mut best = matching[0];
    for &row in &matching[1..] {
        let mut order = Ordering::Equal;
        for term in &selection.order_by {
            order = windows::compare(
                cell(table, item, row, term.column, budget)?,
                cell(table, item, best, term.column, budget)?,
                term,
            );
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
