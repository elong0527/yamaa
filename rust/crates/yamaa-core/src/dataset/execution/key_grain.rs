//! Standalone key combinations and their complete feeding-record memberships.
use super::*;
use crate::table_grouping::Key;
use std::collections::BTreeSet;

/// Evaluate every key lifecycle before non-key derivation, then retain first occurrences.
pub(super) fn construct<T: TableAccess + ?Sized>(
    plan: &DatasetPlan,
    table: &T,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Vec<Candidate>, Box<ExecutionError<T::Error>>> {
    let rows = table.row_count();
    Limits::table(rows, plan.keys().len())
        .map_err(|error| Box::new(ExecutionError::Grouping(GroupingError::Limit(error))))?;
    // Finish the entire source filter before any key conversion can fail.
    let mut retained = Vec::new();
    retained
        .try_reserve_exact(rows)
        .map_err(|_| Box::new(ExecutionError::Allocation))?;

    for row in 0..rows {
        if let Some(filter) = &plan.templates()[0].filter {
            let truth = crate::dataset::execution::predicates::evaluate(filter, table, row, &[])
                .map_err(|error| {
                    Box::new(ExecutionError::Predicate {
                        source_row: row,
                        error,
                    })
                })?;
            if truth != crate::predicate::Truth::True {
                continue;
            }
        }
        retained.push(row);
    }
    let rows = retained.len();
    let mut key_table = Dataset {
        schema: TableSchema::new(
            plan.keys()
                .iter()
                .map(|&column| plan.output().columns()[column].clone())
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
            template: None,
            selection_identity: SelectionIdentity::KeyCandidate(row),
            members: vec![row],
            values: vec![Value::Missing; plan.output().columns().len()],
            completed: vec![false; plan.output().columns().len()],
        };
        for assignment in &plan.templates()[0].assignments {
            probe.values[assignment.column] =
                evaluate(table, assignment, &probe, plan, row, state)?;
            probe.completed[assignment.column] = true;
        }
        key_table.rows.push(
            plan.keys()
                .iter()
                .map(|&column| std::mem::replace(&mut probe.values[column], Value::Missing))
                .collect(),
        );
    }
    let key_columns: Vec<_> = (0..plan.keys().len()).collect();

    let partitions = partition(&key_table, &key_columns)
        .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;

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
    plan.check_capacity(0, groups.len())?;
    let mut candidates = Vec::new();
    candidates
        .try_reserve_exact(groups.len())
        .map_err(|_| Box::new(ExecutionError::Allocation))?;
    for members in groups {
        let keys = std::mem::take(&mut key_table.rows[members[0]]);
        for &duplicate in &members[1..] {
            key_table.rows[duplicate].clear();
        }
        let mut candidate = Candidate {
            template: None,
            selection_identity: SelectionIdentity::Candidate(candidates.len()),
            members: members.into_iter().map(|member| retained[member]).collect(),
            values: vec![Value::Missing; plan.output().columns().len()],
            completed: vec![false; plan.output().columns().len()],
        };
        for (&column, value) in plan.keys().iter().zip(keys) {
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
    collect_bound(
        table,
        Collection {
            column: *column,
            identifier,
            filter: filter.as_ref(),
            selection: selection.as_ref(),
        },
        CollectionContext {
            path: &assignment.path,
            candidate,
            plan,
            row,
        },
        handlers,
    )
}

/// Borrow one admitted source read; function arguments use no selection policy.
pub(super) struct Collection<'a> {
    pub column: usize,
    pub identifier: &'a str,
    pub filter: Option<&'a BoundPredicate>,
    pub selection: Option<&'a SourceSelection>,
}

/// Carry diagnostic provenance without constructing a synthetic output assignment.
pub(super) struct CollectionContext<'a> {
    pub path: &'a str,
    pub candidate: &'a Candidate,
    pub plan: &'a DatasetPlan,
    pub row: usize,
}

/// Reuse exact distinct-reading semantics for assignments and collected callback arguments.
/// The caller charges all potential member reads before entering this service.
pub(super) fn collect_bound<T: TableAccess + ?Sized>(
    table: &T,
    read: Collection<'_>,
    context: CollectionContext<'_>,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let Collection {
        column,
        identifier,
        filter,
        selection,
    } = read;
    let CollectionContext {
        path,
        candidate,
        plan,
        row,
    } = context;
    // Complete eligibility before reading values, including later predicate failures.
    let mut retained = Vec::new();
    let members = if let Some(filter) = filter {
        retained
            .try_reserve_exact(candidate.members.len())
            .map_err(|_| Box::new(ExecutionError::Allocation))?;

        for &source_row in &candidate.members {
            let truth =
                crate::dataset::execution::predicates::evaluate(filter, table, source_row, &[])
                    .map_err(|error| Box::new(ExecutionError::Predicate { source_row, error }))?;
            if truth == crate::predicate::Truth::True {
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
        let value = table.cell(source_row, column).map_err(|error| {
            Box::new(ExecutionError::Cell {
                path: path.into(),
                source_row,
                error,
            })
        })?;
        if matches!(value, ValueRef::Missing) {
            continue;
        }
        distinct.insert(Key::from(value));
        first.get_or_insert(value);
        if selection.is_some() {
            carrying.push(source_row);
        }
    }
    if distinct.len() > 1 {
        if let Some(selection) = selection {
            let chosen = select(table, path, &carrying, selection)?;

            let value = table.cell(chosen, column).map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: path.into(),
                    source_row: chosen,
                    error,
                })
            })?;
            handlers
                .record(
                    &std::format!("{}.multiple_matches", path),
                    HandlerKind::MultipleMatches,
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            return Ok(own(value));
        }
        return Err(Box::new(ExecutionError::MultipleValues {
            path: path.into(),
            identifier: identifier.into(),
            value_count: distinct.len(),
            identity: failure_identity(candidate, plan.keys(), row)?,
        }));
    }
    Ok(first.map_or(Value::Missing, own))
}

/// Select a stable extremum without sorting or cloning donor payloads.
fn select<T: TableAccess + ?Sized>(
    table: &T,
    path: &str,
    carrying: &[usize],
    selection: &SourceSelection,
) -> Result<usize, Box<ExecutionError<T::Error>>> {
    use std::cmp::Ordering;
    let mut best = carrying[0];
    for &row in &carrying[1..] {
        let mut order = Ordering::Equal;
        for term in &selection.order_by {
            let read = |source_row| -> Result<ValueRef<'_>, Box<ExecutionError<T::Error>>> {
                let value = table.cell(source_row, term.column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: path.into(),
                        source_row,
                        error,
                    })
                })?;
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
