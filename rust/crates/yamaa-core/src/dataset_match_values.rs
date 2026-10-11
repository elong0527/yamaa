//! Destination-free scalar values for named matching, never output assignments.
use super::*;
use crate::{
    key_relation::ComparableType,
    value::{ColumnType, ValueType},
};

/// The current candidate owns these reads, independently of the donor record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchRead {
    Output { column: usize, identifier: String },
    Record { column: usize, identifier: String },
}
impl MatchRead {
    pub fn identifier(&self) -> &str {
        match self {
            Self::Output { identifier, .. } | Self::Record { identifier, .. } => identifier,
        }
    }
    pub fn output_column(&self) -> Option<usize> {
        match self {
            Self::Output { column, .. } => Some(*column),
            _ => None,
        }
    }
    fn kind(
        &self,
        primary: &TableSchema,
        output: &TableSchema,
        available: &[bool],
    ) -> Result<ColumnType, PlanError> {
        if self.identifier().is_empty() || self.identifier().len() > 65_536 {
            return Err(PlanError::InvalidIntermediate);
        }
        let column = match self {
            Self::Output { column, .. } => {
                if !available.get(*column).copied().unwrap_or(false) {
                    return Err(PlanError::InvalidLookup);
                }
                output.columns().get(*column)
            }
            Self::Record { column, .. } => primary.columns().get(*column),
        };
        column
            .map(|column| column.kind)
            .ok_or(PlanError::InvalidIntermediate)
    }
}

/// Padding has no output type conversion. None retains a deferred declaration
/// defect during binding only; DatasetPlan admission refuses it before execution.
#[derive(Clone, Debug, PartialEq)]
pub enum MatchValue {
    Variable(MatchRead),
    Source(MatchRead),
    Literal(Value),
    StrPad {
        read: MatchRead,
        operation: Option<StringPad>,
    },
}
impl MatchValue {
    pub fn is_expression(&self) -> bool {
        !matches!(self, Self::Variable(_))
    }
    pub fn read(&self) -> Option<&MatchRead> {
        match self {
            Self::Variable(read) | Self::Source(read) | Self::StrPad { read, .. } => Some(read),
            Self::Literal(_) => None,
        }
    }
    fn kind(
        &self,
        primary: &TableSchema,
        output: &TableSchema,
        available: &[bool],
    ) -> Result<Option<ComparableType>, PlanError> {
        if let Some(read) = self.read() {
            read.kind(primary, output, available)?;
        }
        Ok(match self {
            Self::Variable(read) | Self::Source(read) => {
                Some(read.kind(primary, output, available)?.into())
            }
            Self::StrPad { operation, .. } => {
                if operation.is_none() {
                    return Err(PlanError::InvalidIntermediate);
                }
                Some(ColumnType::Str.into())
            }
            Self::Literal(value) => value.value_type().map(|kind| match kind {
                ValueType::Str => ColumnType::Str.into(),
                ValueType::Int => ColumnType::Int.into(),
                ValueType::Float => ColumnType::Float.into(),
                ValueType::Bool => ComparableType::Boolean,
                ValueType::Date => ColumnType::Date.into(),
                ValueType::DateTime => ColumnType::DateTime.into(),
            }),
        })
    }
}

/// One authored donor field and its current scalar match value, in written order.
#[derive(Clone, Debug, PartialEq)]
pub struct IntermediateKey {
    pub source_column: usize,
    pub path: String,
    pub value: MatchValue,
}

pub(super) fn validate(
    keys: &[IntermediateKey],
    primary: &TableSchema,
    donor: &TableSchema,
    output: &TableSchema,
    available: &[bool],
) -> Result<(), PlanError> {
    if keys.is_empty() || keys.len() > 64 {
        return Err(PlanError::InvalidIntermediate);
    }
    for (index, key) in keys.iter().enumerate() {
        if key.path.is_empty()
            || key.path.len() > 65_536
            || keys[..index]
                .iter()
                .any(|prior| prior.source_column == key.source_column)
        {
            return Err(PlanError::InvalidIntermediate);
        }
        let donor = donor
            .columns()
            .get(key.source_column)
            .ok_or(PlanError::InvalidIntermediate)?;
        if let Some(read) = key.value.read() {
            if keys[..index]
                .iter()
                .filter_map(|prior| prior.value.read())
                .any(|prior| prior.identifier() == read.identifier() && prior != read)
            {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        if key
            .value
            .kind(primary, output, available)?
            .is_some_and(|kind| !crate::key_relation::comparable(kind, donor.kind.into()))
        {
            return Err(PlanError::InvalidLookup);
        }
    }
    Ok(())
}
