//! Pure dataset declarations and admission. No study data or host effects.
use super::*;
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
    pub filter: Option<BoundPredicate>,
    pub selection: Option<SourceSelection>,
    /// `Some(Missing)` handles absence; `None` leaves it as a join condition.
    pub no_match: Option<Value>,
}

/// Bind declarations globally; completed-value availability is checked at each read.
pub(super) fn validate(
    items: &[Intermediate],
    sources: &[SecondarySource],
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
