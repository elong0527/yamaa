//! Bound many-to-one secondary-source reads over immutable normalized snapshots.
use super::*;
use crate::table_grouping::Key;

/// Validate every equality pair and output dependency without reading any source data.
pub(super) fn validate(
    lookup: &Lookup,
    sources: &[SecondarySource],
    available: &[bool],
    output: &TableSchema,
) -> Result<(), PlanError> {
    let source = sources.get(lookup.source).ok_or(PlanError::InvalidLookup)?;
    if lookup.column >= source.schema.columns().len() || lookup.keys.is_empty() {
        return Err(PlanError::InvalidLookup);
    }
    for (index, key) in lookup.keys.iter().enumerate() {
        let field = source
            .schema
            .columns()
            .get(key.source_column)
            .ok_or(PlanError::InvalidLookup)?;
        if !available.get(key.output_column).copied().unwrap_or(false)
            || field.kind != output.columns()[key.output_column].kind
            || lookup.keys[..index]
                .iter()
                .any(|previous| previous.source_column == key.source_column)
        {
            return Err(PlanError::InvalidLookup);
        }
    }
    Ok(())
}

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
    if lookup
        .keys
        .iter()
        .any(|key| matches!(candidate.values[key.output_column], Value::Missing))
    {
        return Ok(Value::Missing);
    }
    let mut count = 0_usize;
    let mut chosen = 0;
    for source_row in 0..table.row_count() {
        let mut matches = true;
        for key in &lookup.keys {
            budget.work(1, 1)?;
            let value = cell(table, lookup, assignment, source_row, key.source_column)?;
            if let ValueRef::Str(text) = value {
                budget.scalar_text(text.len())?;
            }
            if Key::from(value) != Key::from(ValueRef::from(&candidate.values[key.output_column])) {
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
        budget.work(1, lookup.keys.len())?;
        budget.identity(
            lookup
                .keys
                .iter()
                .map(|key| &candidate.values[key.output_column]),
        )?;
        return Err(Box::new(ExecutionError::MultipleMatches {
            path: assignment.path.clone(),
            dataset: plan.secondary[lookup.source].name.clone(),
            match_count: count,
            matched_key: lookup
                .keys
                .iter()
                .map(|key| {
                    (
                        table.schema().columns()[key.source_column].name.clone(),
                        candidate.values[key.output_column].clone(),
                    )
                })
                .collect(),
            identity: failure_identity(candidate, &plan.keys, row, budget)?,
        }));
    }
    budget.work(1, 1)?;
    let value = cell(table, lookup, assignment, chosen, lookup.column)?;
    if let ValueRef::Str(text) = value {
        budget.scalar_text(text.len())?;
    }
    Ok(own(value))
}

/// Retain the secondary source and original row coordinates on opaque port failures.
fn cell<'a, E>(
    table: &'a dyn TableAccess<Error = E>,
    lookup: &Lookup,
    assignment: &Assignment,
    source_row: usize,
    column: usize,
) -> Result<ValueRef<'a>, Box<ExecutionError<E>>> {
    table.cell(source_row, column).map_err(|error| {
        Box::new(ExecutionError::SecondaryCell {
            path: assignment.path.clone(),
            source: lookup.source,
            source_row,
            error,
        })
    })
}
