//! Donor metadata preserves unknown derived kinds without inventing a schema type.
use crate::value::{ColumnType, ValueType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationFieldOrigin {
    Stored(usize),
    Derived(usize),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationField {
    pub identifier: String,
    pub kind: Option<ValueType>,
    pub origin: RelationFieldOrigin,
}
impl RelationField {
    pub fn stored(column: usize, identifier: String, kind: ColumnType) -> Self {
        Self {
            identifier,
            kind: Some(kind.into()),
            origin: RelationFieldOrigin::Stored(column),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationShapeError {
    #[error("dataset relation boundary: {0}")]
    Limit(crate::limits::LimitExceeded),
    #[error("dataset relation shape empty identifier")]
    EmptyIdentifier,
    #[error("dataset relation shape duplicate identifier")]
    DuplicateIdentifier,
    #[error("dataset relation shape invalid origin")]
    InvalidOrigin,
    #[error("dataset relation shape invalid kind")]
    InvalidKind,
    #[error("dataset relation shape invalid projection")]
    InvalidProjection,
}
/// Projection restricts readable fields; all donor slots remain available to key,
/// filter, ordering and verification admission. No stored values are retained here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationShape {
    fields: Vec<RelationField>,
    projection: Option<Vec<usize>>,
}
impl RelationShape {
    pub fn new(
        fields: Vec<RelationField>,
        projection: Option<Vec<usize>>,
    ) -> Result<Self, RelationShapeError> {
        crate::limits::Limits::columns(fields.len()).map_err(RelationShapeError::Limit)?;
        let mut stored = 0;
        let mut derived = 0;
        for (index, field) in fields.iter().enumerate() {
            if field.identifier.is_empty() {
                return Err(RelationShapeError::EmptyIdentifier);
            }
            if fields[..index]
                .iter()
                .any(|other| other.identifier == field.identifier)
            {
                return Err(RelationShapeError::DuplicateIdentifier);
            }
            if matches!(field.origin, RelationFieldOrigin::Stored(_))
                && (field.kind.is_none() || field.kind == Some(ValueType::Bool))
            {
                return Err(RelationShapeError::InvalidKind);
            }
            match field.origin {
                RelationFieldOrigin::Stored(column) if derived == 0 && column == stored => {
                    stored += 1
                }
                RelationFieldOrigin::Derived(column) if column == derived => derived += 1,
                _ => return Err(RelationShapeError::InvalidOrigin),
            }
        }
        if let Some(columns) = &projection {
            if columns.iter().enumerate().any(|(index, &column)| {
                column >= fields.len() || columns[..index].contains(&column)
            }) {
                return Err(RelationShapeError::InvalidProjection);
            }
        }
        Ok(Self { fields, projection })
    }
    pub fn fields(&self) -> &[RelationField] {
        &self.fields
    }
    pub fn index(&self, identifier: &str) -> Option<usize> {
        self.fields
            .iter()
            .position(|field| field.identifier == identifier)
    }
    pub fn visible_index(&self, identifier: &str) -> Option<usize> {
        self.index(identifier).filter(|column| {
            self.projection
                .as_ref()
                .is_none_or(|projection| projection.contains(column))
        })
    }
    pub fn projection(&self) -> Option<&[usize]> {
        self.projection.as_deref()
    }
}
