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
    pub filter: Option<RelationPredicate>,
    pub selection: Option<SourceSelection>,
    pub range: Option<RelationRange>,
    /// Visible selected fields; donor keys, filters and order still see every slot.
    pub columns: Option<Vec<usize>>,
    /// `Some(Missing)` handles absence; `None` leaves it as a join condition.
    pub no_match: Option<Value>,
}

impl Intermediate {
    pub fn current_fields(&self) -> impl Iterator<Item = &CurrentField> + '_ {
        self.filter
            .iter()
            .flat_map(|filter| filter.current())
            .chain(self.range.iter().map(|range| range.current()))
    }
    pub fn admits_column(&self, column: usize) -> bool {
        self.source.admits_column(column)
            && self
                .columns
                .as_ref()
                .is_none_or(|columns| columns.contains(&column))
    }
    pub fn output_dependencies(&self) -> impl Iterator<Item = usize> + '_ {
        self.keys
            .iter()
            .filter_map(|key| key.value.read().and_then(MatchRead::output_column))
            .chain(
                self.filter
                    .iter()
                    .flat_map(|filter| filter.current())
                    .filter_map(|field| match field.read {
                        CurrentRead::Output(column) => Some(column),
                        _ => None,
                    }),
            )
            .chain(
                self.range
                    .iter()
                    .filter_map(|range| match range.current().read {
                        CurrentRead::Output(column) => Some(column),
                        _ => None,
                    }),
            )
    }
    pub fn has_expression_keys(&self) -> bool {
        self.keys.iter().any(|key| key.value.is_expression())
    }
}

/// Kept current-side reads carry the dependencies of the earlier selector too.
/// The lookup borrows either a complete plan or still-binding optional declarations.
pub fn visit_output_dependencies<'a>(
    index: usize,
    lookup: impl Fn(usize) -> Option<&'a Intermediate>,
    mut visitor: impl FnMut(usize),
) -> Result<(), PlanError> {
    fn visit<'a>(
        index: usize,
        lookup: &impl Fn(usize) -> Option<&'a Intermediate>,
        visitor: &mut impl FnMut(usize),
        depth: usize,
        work: &mut usize,
    ) -> Result<(), PlanError> {
        if depth > 64 || *work == 0 {
            return Err(PlanError::InvalidIntermediate);
        }
        *work -= 1;
        let item = lookup(index).ok_or(PlanError::InvalidIntermediate)?;
        for column in item.output_dependencies() {
            if *work == 0 {
                return Err(PlanError::InvalidIntermediate);
            }
            *work -= 1;
            visitor(column);
        }
        for field in item.current_fields() {
            if let CurrentRead::Intermediate { index: child, .. } = field.read {
                if child >= index {
                    return Err(PlanError::InvalidIntermediate);
                }
                visit(child, lookup, visitor, depth + 1, work)?;
            }
        }
        Ok(())
    }
    visit(index, &lookup, &mut visitor, 0, &mut 16_777_216)
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
        let donor = item
            .source_schema(sources)
            .ok_or(PlanError::InvalidIntermediate)?;
        if matches!(&item.source,IntermediateSource::SelfRows {columns,..} if columns.is_empty()) {
            return Err(PlanError::InvalidIntermediate);
        }
        match_values::validate(
            &item.keys,
            primary,
            donor,
            output,
            &vec![true; output.columns().len()],
        )?;
        let width = item
            .source_schema(sources)
            .ok_or(PlanError::InvalidIntermediate)?
            .columns()
            .len();
        if item.columns.as_ref().is_some_and(|columns| {
            columns.iter().enumerate().any(|(position, &column)| {
                column >= width
                    || !item.source.admits_column(column)
                    || columns[..position].contains(&column)
            })
        }) {
            return Err(PlanError::InvalidIntermediate);
        }
        if let Some(range) = &item.range {
            range
                .validate_source(
                    item.source_schema(sources)
                        .ok_or(PlanError::InvalidIntermediate)?,
                )
                .map_err(|_| PlanError::InvalidIntermediate)?;
            range
                .current()
                .validate(primary, sources, output, items, index)?;
            let current_kind = match range.current().read {
                CurrentRead::Record { source: 0, column } => primary.columns()[column].kind,
                CurrentRead::Record { source, column } => {
                    sources[source - 1].schema.columns()[column].kind
                }
                CurrentRead::Output(column) => output.columns()[column].kind,
                CurrentRead::Intermediate { index, column } => {
                    items[index]
                        .source_schema(sources)
                        .ok_or(PlanError::InvalidIntermediate)?
                        .columns()[column]
                        .kind
                }
            };
            if range.value_kind() != Some(current_kind.into()) {
                return Err(PlanError::InvalidIntermediate);
            }
            if [range.lower(), range.upper()]
                .into_iter()
                .flatten()
                .any(|bound| !item.source.admits_column(bound.column))
            {
                return Err(PlanError::InvalidIntermediate);
            }
        }
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
                || item.filter.as_ref().is_some_and(|filter| filter.bindings().iter().any(|binding| matches!(binding.read, Read::Source(column) if !item.source.admits_column(column))))
                || item.selection.as_ref().is_some_and(|selection| selection.order_by.iter().any(|term| !item.source.admits_column(term.column))) {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        if let Some(filter) = &item.filter {
            filter.validate(width, primary, sources, output, items, index)?;
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
    if column >= source.columns().len() || !item.admits_column(column) {
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
