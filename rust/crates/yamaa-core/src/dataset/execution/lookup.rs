//! Bound many-to-one secondary-source reads over immutable normalized snapshots.
use super::*;
use std::cmp::Ordering;

/// Count all matching records before reading a value or applying output conversion.
pub(super) fn read<E>(
    table: &dyn TableAccess<Error = E>,
    lookup: &Lookup,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
) -> Result<Value, Box<ExecutionError<E>>> {
    let keys = lookup
        .keys
        .iter()
        .map(|key| {
            (
                key.source_column,
                ValueRef::from(&candidate.values[key.output_column]),
            )
        })
        .collect::<Vec<_>>();
    read_matching(
        table,
        lookup.source,
        lookup.column,
        &keys,
        &Context {
            assignment,
            candidate,
            plan,
            row,
        },
        None,
        None,
        None,
        None,
    )
}

/// Keep assignment provenance and optional completed output identity independent of match keys.
pub(super) struct Context<'a> {
    pub assignment: &'a Assignment,
    pub candidate: &'a Candidate,
    pub plan: &'a DatasetPlan,
    pub row: usize,
}

/// Resolve every raw driver match field before the bounded secondary scan.
pub(super) fn read_row<T: TableAccess + ?Sized>(
    driver: &T,
    table: &dyn TableAccess<Error = T::Error>,
    lookup: &RowLookup,
    context: &Context<'_>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let source_row = context.candidate.members[0];
    let mut keys = Vec::with_capacity(lookup.keys.len());
    for key in &lookup.keys {
        let value = driver
            .cell(source_row, key.driver_column)
            .map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: context.assignment.path.clone(),
                    source_row,
                    error,
                })
            })?;
        keys.push((key.source_column, value));
    }
    read_matching(
        table,
        lookup.source,
        lookup.column,
        &keys,
        context,
        None,
        None,
        None,
        None,
    )
}

/// Count all matching records before donor access, preserving raw match-key evidence.
#[allow(clippy::too_many_arguments)]
fn read_matching<E>(
    table: &dyn TableAccess<Error = E>,
    source: usize,
    column: usize,
    keys: &[(usize, ValueRef<'_>)],
    context: &Context<'_>,
    filter: Option<&crate::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    mut handlers: Option<&mut HandlerCounter>,
    donor_name: Option<&str>,
) -> Result<Value, Box<ExecutionError<E>>> {
    // Complete the authored eligibility expression before inspecting match keys.
    let mut eligible = Vec::new();
    if let Some(filter) = filter {
        eligible
            .try_reserve_exact(table.row_count())
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        for source_row in 0..table.row_count() {
            let truth =
                crate::dataset::execution::predicates::evaluate(filter, table, source_row, &[])
                    .map_err(|error| match error.kind {
                        crate::predicate::ErrorKind::Regex(limit) => Box::new(regex_limit(limit)),
                        _ => Box::new(ExecutionError::Predicate { source_row, error }),
                    })?;
            if truth == crate::predicate::Truth::True {
                eligible.push(source_row);
            }
        }
    }
    if keys
        .iter()
        .any(|(_, value)| matches!(value, ValueRef::Missing))
    {
        return Ok(Value::Missing);
    }
    let mut count = 0_usize;
    let mut matching = Vec::new();
    let mut chosen = 0;
    let all_rows = 0..table.row_count();
    let rows: &mut dyn Iterator<Item = usize> = if filter.is_some() {
        &mut eligible.into_iter()
    } else {
        &mut all_rows.into_iter()
    };
    for source_row in rows {
        let mut matches = true;
        for (column, current) in keys {
            let value = cell(table, source, context.assignment, source_row, *column)?;
            if crate::value::compare_present_refs(value, *current) != Ok(Ordering::Equal) {
                matches = false;
                break;
            }
        }
        if matches {
            count += 1;
            chosen = source_row;
            if selection.is_some() {
                matching
                    .try_reserve(1)
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                matching.push(source_row);
            }
        }
    }
    if count == 0 {
        return Ok(Value::Missing);
    }
    if count > 1 && selection.is_some() {
        chosen = select(
            table,
            source,
            context.assignment,
            &matching,
            selection.expect("declared source choice"),
        )?;
        if let Some(counter) = handlers.as_mut() {
            counter
                .record(
                    &std::format!("{}.multiple_matches", context.assignment.path),
                    HandlerKind::MultipleMatches,
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
        }
    } else if count > 1 {
        // Admit diagnostic copies before owning any raw driver text.

        return Err(Box::new(ExecutionError::MultipleMatches {
            path: context.assignment.path.clone(),
            dataset: donor_name
                .map(String::from)
                .unwrap_or_else(|| context.plan.secondary()[source].name.clone()),
            intermediate: std::format!(
                "intermediate({})",
                donor_name.unwrap_or_else(|| context.plan.secondary()[source].name.as_str())
            ),
            match_count: count,
            matched_key: keys
                .iter()
                .map(|(column, value)| {
                    (table.schema().columns()[*column].name.clone(), own(*value))
                })
                .collect(),
            identity: failure_identity(context.candidate, context.plan.keys(), context.row)?,
        }));
    }

    let value = cell(table, source, context.assignment, chosen, column)?;
    Ok(own(value))
}

/// Retain the secondary source and original row coordinates on opaque port failures.
fn cell<'a, E>(
    table: &'a dyn TableAccess<Error = E>,
    source: usize,
    assignment: &Assignment,
    source_row: usize,
    column: usize,
) -> Result<ValueRef<'a>, Box<ExecutionError<E>>> {
    table.cell(source_row, column).map_err(|error| {
        Box::new(if source == usize::MAX {
            ExecutionError::Cell {
                path: assignment.path.clone(),
                source_row,
                error,
            }
        } else {
            ExecutionError::SecondaryCell {
                path: assignment.path.clone(),
                source,
                source_row,
                error,
            }
        })
    })
}

/// Source eligibility precedes cardinality and ordered selection on the implicit join.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_source<E>(
    table: &dyn TableAccess<Error = E>,
    lookup: &Lookup,
    filter: Option<&crate::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    context: &Context<'_>,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<E>>> {
    let keys = lookup
        .keys
        .iter()
        .map(|key| {
            (
                key.source_column,
                ValueRef::from(&context.candidate.values[key.output_column]),
            )
        })
        .collect::<Vec<_>>();
    read_matching(
        table,
        lookup.source,
        lookup.column,
        &keys,
        context,
        filter,
        selection,
        Some(handlers),
        None,
    )
}

/// Select the stable donor extremum without losing secondary-source coordinates.
fn select<E>(
    table: &dyn TableAccess<Error = E>,
    source: usize,
    assignment: &Assignment,
    matching: &[usize],
    selection: &SourceSelection,
) -> Result<usize, Box<ExecutionError<E>>> {
    let mut best = matching[0];
    for &row in &matching[1..] {
        let mut order = Ordering::Equal;
        for term in &selection.order_by {
            let read = |row| -> Result<ValueRef<'_>, Box<ExecutionError<E>>> {
                let value = cell(table, source, assignment, row, term.column)?;
                Ok(value)
            };
            order = windows::compare(read(row)?, read(best)?, term);
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

#[allow(clippy::too_many_arguments)]
pub(super) fn read_joined<T: TableAccess + ?Sized>(
    current: &T,
    table: &dyn TableAccess<Error = T::Error>,
    source: usize,
    joined: &crate::dataset::JoinedRead,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    filter: Option<&crate::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let mut keys = Vec::new();
    for (index, key) in joined.keys.iter().enumerate() {
        let value = if let Some(records) = &joined.record_keys {
            let source_row = candidate.members[0];
            let value = current.cell(source_row, records[index]).map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: assignment.path.clone(),
                    source_row,
                    error,
                })
            })?;
            value
        } else {
            ValueRef::from(&candidate.values[key.output_column])
        };
        keys.push((key.source_column, value));
    }
    read_matching(
        table,
        source,
        joined.column,
        &keys,
        &Context {
            assignment,
            candidate,
            plan,
            row,
        },
        filter,
        selection,
        Some(handlers),
        Some(&joined.name),
    )
}
