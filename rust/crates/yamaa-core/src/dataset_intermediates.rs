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
    pub source: IntermediateSource,
    pub keys: Vec<IntermediateKey>,
    pub filter: Option<BoundPredicate>,
    pub selection: Option<SourceSelection>,
    /// `Some(Missing)` handles absence; `None` leaves it as a join condition.
    pub no_match: Option<Value>,
}

impl Intermediate {
    pub fn output_dependencies(&self) -> impl Iterator<Item = usize> + '_ {
        self.keys
            .iter()
            .filter_map(|key| key.value.read().and_then(MatchRead::output_column))
    }
    pub fn has_expression_keys(&self) -> bool {
        self.keys.iter().any(|key| key.value.is_expression())
    }
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
            match &item.source {
                IntermediateSource::Input(_) | IntermediateSource::Primary { .. } => 0,
                IntermediateSource::SelfRows { columns, .. } => columns
                    .first()
                    .copied()
                    .ok_or(PlanError::InvalidIntermediate)?,
            },
            primary,
            sources,
            &vec![true; output.columns().len()],
            output,
        )?;
        let width = item
            .source_schema(sources)
            .ok_or(PlanError::InvalidIntermediate)?
            .columns()
            .len();
        if let IntermediateSource::Primary { schema, name } = &item.source {
            if schema != primary || name.is_empty() {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        if let IntermediateSource::SelfRows { schema, columns } = &item.source {
            if schema != output
                || columns
                    .iter()
                    .enumerate()
                    .any(|(index, &column)| column >= width || columns[..index].contains(&column))
            {
                return Err(PlanError::InvalidIntermediate);
            }
            if item.keys.iter().any(|key| !item.source.admits_column(key.source_column))
                || item.filter.as_ref().is_some_and(|filter| filter.bindings().iter().any(|binding| !matches!(binding.read, Read::Source(column) if item.source.admits_column(column))))
                || item.selection.as_ref().is_some_and(|selection| selection.order_by.iter().any(|term| !item.source.admits_column(term.column))) {
                return Err(PlanError::InvalidIntermediate);
            }
        }
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
    let source = item
        .source_schema(sources)
        .ok_or(PlanError::InvalidIntermediate)?;
    if column >= source.columns().len() || !item.source.admits_column(column) {
        return Err(PlanError::InvalidIntermediate);
    }
    match_values::validate(&item.keys, primary, source, output, available)
}

/// Direct scalar selections use only admitted complete records or completed keys.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate_context_read(
    item: &Intermediate,
    column: usize,
    primary: &TableSchema,
    sources: &[SecondarySource],
    available: &[bool],
    output: &TableSchema,
    mode: &RowMode,
) -> Result<(), PlanError> {
    if !matches!(mode, RowMode::Records | RowMode::Keys) && !item.source.is_self() {
        return Err(PlanError::InvalidIntermediate);
    }
    if let RowMode::Groups(keys) = mode {
        if item
            .keys
            .iter()
            .filter_map(|key| key.value.read())
            .any(|read| matches!(read, MatchRead::Record { column, .. } if !keys.contains(column)))
        {
            return Err(PlanError::NonGroupSource);
        }
    }
    validate_read(item, column, primary, sources, available, output)
}
