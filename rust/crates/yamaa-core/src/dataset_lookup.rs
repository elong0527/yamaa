//! Pure dataset declarations and admission. No study data or host effects.
use super::*;
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

/// Admit raw driver equality in record scope or on available group keys.
pub(super) fn validate_row(
    lookup: &RowLookup,
    driver: &TableSchema,
    sources: &[SecondarySource],
    mode: &RowMode,
) -> Result<(), PlanError> {
    let source = sources.get(lookup.source).ok_or(PlanError::InvalidLookup)?;
    if matches!(mode, RowMode::Keys)
        || lookup.column >= source.schema.columns().len()
        || lookup.keys.is_empty()
    {
        return Err(PlanError::InvalidLookup);
    }
    for (index, key) in lookup.keys.iter().enumerate() {
        let left = driver
            .columns()
            .get(key.driver_column)
            .ok_or(PlanError::InvalidLookup)?;
        let right = source
            .schema
            .columns()
            .get(key.source_column)
            .ok_or(PlanError::InvalidLookup)?;
        if left.kind != right.kind
            || matches!(mode, RowMode::Groups(keys) if !keys.contains(&key.driver_column))
            || lookup.keys[..index]
                .iter()
                .any(|prior| prior.source_column == key.source_column)
        {
            return Err(PlanError::InvalidLookup);
        }
    }
    Ok(())
}
