//! Bound many-to-one secondary-source reads over immutable normalized snapshots.
use super::*;
use core::cmp::Ordering;

/// Count all matching records before reading a value or applying output conversion.
pub(super) fn read<E>(
    table: &dyn TableAccess<Error = E>,
    lookup: &Lookup,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
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
        budget,
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
    budget: &mut Budget,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let source_row = context.candidate.members[0];
    let mut keys = Vec::with_capacity(lookup.keys.len());
    for key in &lookup.keys {
        budget.work(1, 1)?;
        let value = driver
            .cell(source_row, key.driver_column)
            .map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: context.assignment.path.clone(),
                    source_row,
                    error,
                })
            })?;
        if let ValueRef::Str(text) = value {
            budget.scalar_text(text.len())?;
        }
        keys.push((key.source_column, value));
    }
    read_matching(
        table,
        lookup.source,
        lookup.column,
        &keys,
        context,
        budget,
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
    budget: &mut Budget,
    filter: Option<&yamaa_core::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    mut handlers: Option<&mut HandlerCounter>,
    donor_name: Option<&str>,
) -> Result<Value, Box<ExecutionError<E>>> {
    // Complete the authored eligibility expression before inspecting match keys.
    let mut eligible = Vec::new();
    if let Some(filter) = filter {
        budget.work(table.row_count(), 1)?;
        eligible
            .try_reserve_exact(table.row_count())
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        for source_row in 0..table.row_count() {
            let truth = crate::dataset_predicate::evaluate(
                filter,
                table,
                source_row,
                &[],
                budget.predicate(),
            )
            .map_err(|error| match error.kind {
                yamaa_core::predicate::ErrorKind::Limit(limit) => Box::new(predicate_limit(limit)),
                _ => Box::new(ExecutionError::Predicate { source_row, error }),
            })?;
            if truth == yamaa_core::predicate::Truth::True {
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
            budget.work(1, 1)?;
            let value = cell(table, source, context.assignment, source_row, *column)?;
            if let ValueRef::Str(text) = value {
                budget.scalar_text(text.len())?;
            }
            if yamaa_core::value::compare_present_refs(value, *current) != Ok(Ordering::Equal) {
                matches = false;
                break;
            }
        }
        if matches {
            count += 1;
            chosen = source_row;
            if selection.is_some() {
                budget.work(1, 1)?;
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
            budget,
        )?;
        if let Some(counter) = handlers.as_mut() {
            counter
                .record(
                    &alloc::format!("{}.multiple_matches", context.assignment.path),
                    HandlerKind::MultipleMatches,
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
        }
    } else if count > 1 {
        budget.work(1, keys.len())?;
        // Admit diagnostic copies before owning any raw driver text.
        budget.identity_refs(keys.iter().map(|(_, value)| *value))?;
        return Err(Box::new(ExecutionError::MultipleMatches {
            path: context.assignment.path.clone(),
            dataset: donor_name
                .map(String::from)
                .unwrap_or_else(|| context.plan.secondary()[source].name.clone()),
            intermediate: alloc::format!(
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
            identity: failure_identity(
                context.candidate,
                context.plan.keys(),
                context.row,
                budget,
            )?,
        }));
    }
    budget.work(1, 1)?;
    let value = cell(table, source, context.assignment, chosen, column)?;
    if let ValueRef::Str(text) = value {
        budget.scalar_text(text.len())?;
    }
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
    filter: Option<&yamaa_core::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    context: &Context<'_>,
    budget: &mut Budget,
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
        budget,
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
    budget: &mut Budget,
) -> Result<usize, Box<ExecutionError<E>>> {
    let mut best = matching[0];
    for &row in &matching[1..] {
        let mut order = Ordering::Equal;
        for term in &selection.order_by {
            budget.work(1, 2)?;
            let mut read = |row| -> Result<ValueRef<'_>, Box<ExecutionError<E>>> {
                let value = cell(table, source, assignment, row, term.column)?;
                if let ValueRef::Str(text) = value {
                    budget.scalar_text(text.len())?;
                }
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
pub(super) fn read_joined<E>(
    table: &dyn TableAccess<Error = E>,
    source: usize,
    joined: &yamaa_core::dataset::JoinedRead,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
    filter: Option<&yamaa_core::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<E>>> {
    let keys = joined
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
        source,
        joined.column,
        &keys,
        &Context {
            assignment,
            candidate,
            plan,
            row,
        },
        budget,
        filter,
        selection,
        Some(handlers),
        Some(&joined.name),
    )
}
