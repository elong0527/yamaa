//! Pure dataset declarations and admission. No study data or host effects.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordMatchKey {
    pub source_column: usize,
    pub driver_column: usize,
    pub identifier: String,
}
/// Immutable normalized source schemas, independent of any host buffers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSchemas {
    pub primary: TableSchema,
    pub secondary: Vec<SecondarySource>,
}

/// A named secondary record selector with literal absence handling.
#[derive(Clone, Debug, PartialEq)]
pub struct Intermediate {
    pub identifier: String,
    pub path: String,
    pub source: usize,
    pub keys: Vec<MatchKey>,
    pub record_keys: Vec<RecordMatchKey>,
    pub filter: Option<BoundPredicate>,
    pub selection: Option<SourceSelection>,
    /// `Some(Missing)` handles absence; `None` leaves it as a join condition.
    pub no_match: Option<Value>,
}

/// Bind declarations globally; completed-value availability is checked at each read.
pub(super) fn validate(
    items: &[Intermediate],
    sources: &[SecondarySource],
    primary: &TableSchema,
    output: &TableSchema,
) -> Result<(), PlanError> {
    for (index, item) in items.iter().enumerate() {
        if item.identifier.is_empty()
            || item.path.is_empty()
            || items[..index]
                .iter()
                .any(|other| other.identifier == item.identifier || other.path == item.path)
        {
            return Err(PlanError::InvalidIntermediate);
        }
        validate_read(
            item,
            0,
            primary,
            sources,
            &vec![true; output.columns().len()],
            output,
        )?;
        if !item.keys.is_empty() {
            lookup::validate(
                &Lookup {
                    source: item.source,
                    column: 0,
                    keys: item.keys.clone(),
                },
                sources,
                &vec![true; output.columns().len()],
                output,
            )?;
        }
        let width = sources[item.source].schema.columns().len();
        if let Some(filter) = &item.filter {
            filter
                .validate(width, &[], false)
                .map_err(PlanError::Filter)?;
        }
        if item.selection.as_ref().is_some_and(|selection| {
            selection.order_by.is_empty()
                || selection.order_by.iter().any(|term| term.column >= width)
        }) {
            return Err(PlanError::InvalidSourceOrder);
        }
    }
    Ok(())
}

pub(super) fn validate_read(
    item: &Intermediate,
    column: usize,
    primary: &TableSchema,
    sources: &[SecondarySource],
    available: &[bool],
    output: &TableSchema,
) -> Result<(), PlanError> {
    let source = sources
        .get(item.source)
        .ok_or(PlanError::InvalidIntermediate)?;
    if column >= source.schema.columns().len()
        || item.keys.is_empty() && item.record_keys.is_empty()
        || item.record_keys.len() + item.keys.len() > 64
    {
        return Err(PlanError::InvalidIntermediate);
    }
    if !item.keys.is_empty() {
        lookup::validate(
            &Lookup {
                source: item.source,
                column,
                keys: item.keys.clone(),
            },
            sources,
            available,
            output,
        )?;
    }
    for (index, key) in item.record_keys.iter().enumerate() {
        if key.identifier.is_empty() || key.identifier.len() > 65_536 {
            return Err(PlanError::InvalidIntermediate);
        }
        let left = primary
            .columns()
            .get(key.driver_column)
            .ok_or(PlanError::InvalidIntermediate)?;
        let right = source
            .schema
            .columns()
            .get(key.source_column)
            .ok_or(PlanError::InvalidIntermediate)?;
        if left.kind != right.kind
            || item
                .keys
                .iter()
                .any(|prior| prior.source_column == key.source_column)
            || item.record_keys[..index]
                .iter()
                .any(|prior| prior.source_column == key.source_column)
        {
            return Err(PlanError::InvalidIntermediate);
        }
    }
    Ok(())
}
