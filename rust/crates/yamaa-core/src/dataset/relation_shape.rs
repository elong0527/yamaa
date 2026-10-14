//! Bounded donor metadata preserves unknown derived kinds without inventing a schema type.
use super::*;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationShapeLimits {
    pub fields: usize,
    pub metadata_bytes: usize,
    pub validation_work: usize,
}
impl Default for RelationShapeLimits {
    fn default() -> Self {
        Self {
            fields: 4096,
            metadata_bytes: 16_777_216,
            validation_work: 16_777_216,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationShapeError {
    Limit,
    EmptyIdentifier,
    DuplicateIdentifier,
    InvalidOrigin,
    InvalidKind,
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
        limits: RelationShapeLimits,
    ) -> Result<Self, RelationShapeError> {
        let reject = || RelationShapeError::Limit;
        if fields.len() > limits.fields {
            return Err(reject());
        }
        let projection_len = projection.as_ref().map_or(0, Vec::len);
        let identifier_bytes = fields
            .iter()
            .map(|field| field.identifier.len())
            .max()
            .unwrap_or(0)
            .max(1);
        let work = fields
            .len()
            .checked_mul(fields.len())
            .and_then(|n| n.checked_mul(identifier_bytes))
            .and_then(|n| {
                projection_len
                    .checked_mul(projection_len)
                    .and_then(|p| n.checked_add(p))
            })
            .filter(|&n| n <= limits.validation_work)
            .ok_or_else(reject)?;
        let _ = work;
        let bytes = fields
            .capacity()
            .checked_mul(core::mem::size_of::<RelationField>())
            .and_then(|n| {
                projection.as_ref().map_or(Some(n), |p| {
                    p.capacity()
                        .checked_mul(core::mem::size_of::<usize>())
                        .and_then(|p| n.checked_add(p))
                })
            })
            .ok_or_else(reject)?;
        let bytes = fields
            .iter()
            .try_fold(bytes, |n, field| n.checked_add(field.identifier.capacity()))
            .filter(|&n| n <= limits.metadata_bytes)
            .ok_or_else(reject)?;
        let _ = bytes;
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
