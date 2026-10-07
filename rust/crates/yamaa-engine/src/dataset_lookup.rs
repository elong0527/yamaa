//! Bound many-to-one secondary-source reads over immutable normalized snapshots.
use super::*;
use crate::table_grouping::Key;

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
    read_matching(table, lookup.source, lookup.column, &keys, context, budget)
}

/// Count all matching records before donor access, preserving raw match-key evidence.
fn read_matching<E>(
    table: &dyn TableAccess<Error = E>,
    source: usize,
    column: usize,
    keys: &[(usize, ValueRef<'_>)],
    context: &Context<'_>,
    budget: &mut Budget,
) -> Result<Value, Box<ExecutionError<E>>> {
    if keys
        .iter()
        .any(|(_, value)| matches!(value, ValueRef::Missing))
    {
        return Ok(Value::Missing);
    }
    let mut count = 0_usize;
    let mut chosen = 0;
    for source_row in 0..table.row_count() {
        let mut matches = true;
        for (column, current) in keys {
            budget.work(1, 1)?;
            let value = cell(table, source, context.assignment, source_row, *column)?;
            if let ValueRef::Str(text) = value {
                budget.scalar_text(text.len())?;
            }
            if Key::from(value) != Key::from(*current) {
                matches = false;
                break;
            }
        }
        if matches {
            count += 1;
            chosen = source_row;
        }
    }
    if count == 0 {
        return Ok(Value::Missing);
    }
    if count > 1 {
        budget.work(1, keys.len())?;
        // Admit diagnostic copies before owning any raw driver text.
        budget.identity_refs(keys.iter().map(|(_, value)| *value))?;
        return Err(Box::new(ExecutionError::MultipleMatches {
            path: context.assignment.path.clone(),
            dataset: context.plan.secondary()[source].name.clone(),
            intermediate: alloc::format!("intermediate({})", context.plan.secondary()[source].name),
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
        Box::new(ExecutionError::SecondaryCell {
            path: assignment.path.clone(),
            source,
            source_row,
            error,
        })
    })
}
