//! Destination-free scalar values for named matching, never output assignments.
use super::*;
use crate::{
    key_relation::ComparableType,
    value::{ColumnType, ValueType},
};

/// The current candidate owns these reads, independently of the donor record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchRead {
    Output {
        column: usize,
        identifier: String,
    },
    Record {
        column: usize,
        identifier: String,
    },
    /// Retained authored source ownership, before entering a donor current scope.
    SecondaryRecord {
        source: usize,
        column: usize,
        identifier: String,
    },
}
impl MatchRead {
    pub fn identifier(&self) -> &str {
        match self {
            Self::Output { identifier, .. }
            | Self::Record { identifier, .. }
            | Self::SecondaryRecord { identifier, .. } => identifier,
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
        secondary: &[SecondarySource],
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
            Self::SecondaryRecord { source, column, .. } => secondary
                .get(*source)
                .and_then(|source| source.schema.columns().get(*column)),
        };
        column
            .map(|column| column.kind)
            .ok_or(PlanError::InvalidIntermediate)
    }
}

/// Authored current operands whose actual donor scope is not known yet.
/// A bound executable read never contains this declaration metadata.
#[derive(Clone, Debug, PartialEq)]
pub enum ContextualMatch {
    Variable(String),
    Source(String),
    StrPad {
        identifier: String,
        operation: Option<StringPad>,
    },
}
impl ContextualMatch {
    pub fn identifier(&self) -> &str {
        match self {
            Self::Variable(name)
            | Self::Source(name)
            | Self::StrPad {
                identifier: name, ..
            } => name,
        }
    }
    pub fn bind(&self, read: MatchRead) -> MatchValue {
        match self {
            Self::Variable(_) => MatchValue::Variable(read),
            Self::Source(_) => MatchValue::Source(read),
            Self::StrPad { operation, .. } => MatchValue::StrPad {
                read,
                operation: operation.clone(),
            },
        }
    }
}

/// Padding has no output type conversion. None retains a deferred declaration
/// defect during binding only; DatasetPlan admission refuses it before execution.
#[derive(Clone, Debug, PartialEq)]
pub enum MatchValue {
    Contextual(ContextualMatch),
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
        !matches!(
            self,
            Self::Variable(_) | Self::Contextual(ContextualMatch::Variable(_))
        )
    }
    pub fn read(&self) -> Option<&MatchRead> {
        match self {
            Self::Variable(read) | Self::Source(read) | Self::StrPad { read, .. } => Some(read),
            Self::Literal(_) | Self::Contextual(_) => None,
        }
    }
    pub fn contextual(&self) -> Option<ContextualMatch> {
        match self {
            Self::Contextual(declaration) => Some(declaration.clone()),
            Self::Variable(read) => Some(ContextualMatch::Variable(read.identifier().into())),
            Self::Source(read) => Some(ContextualMatch::Source(read.identifier().into())),
            Self::StrPad { read, operation } => Some(ContextualMatch::StrPad {
                identifier: read.identifier().into(),
                operation: operation.clone(),
            }),
            Self::Literal(_) => None,
        }
    }
    pub fn result_type(&self, read_kind: Option<ColumnType>) -> Option<ComparableType> {
        match self {
            Self::Variable(_) | Self::Source(_) => read_kind.map(Into::into),
            Self::StrPad { .. } | Self::Contextual(ContextualMatch::StrPad { .. }) => {
                Some(ColumnType::Str.into())
            }
            Self::Contextual(_) => None,
            Self::Literal(value) => value.value_type().map(|kind| match kind {
                ValueType::Str => ColumnType::Str.into(),
                ValueType::Int => ColumnType::Int.into(),
                ValueType::Float => ColumnType::Float.into(),
                ValueType::Bool => ComparableType::Boolean,
                ValueType::Date => ColumnType::Date.into(),
                ValueType::DateTime => ColumnType::DateTime.into(),
            }),
        }
    }
    fn kind(
        &self,
        primary: &TableSchema,
        secondary: &[SecondarySource],
        output: &TableSchema,
        available: &[bool],
    ) -> Result<Option<ComparableType>, PlanError> {
        let read_kind = self
            .read()
            .map(|read| read.kind(primary, secondary, output, available))
            .transpose()?;
        if matches!(
            self,
            Self::StrPad {
                operation: None,
                ..
            } | Self::Contextual(ContextualMatch::StrPad {
                operation: None,
                ..
            })
        ) {
            return Err(PlanError::InvalidIntermediate);
        }
        if let Self::Contextual(declaration) = self {
            if declaration.identifier().is_empty() || declaration.identifier().len() > 65_536 {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        Ok(self.result_type(read_kind))
    }
}

/// One authored donor field and its current scalar match value, in written order.
#[derive(Clone, Debug, PartialEq)]
pub struct IntermediateKey {
    pub source_column: usize,
    pub path: String,
    pub value: MatchValue,
}

pub(super) fn validate_declaration(
    keys: &[IntermediateKey],
    primary: &TableSchema,
    secondary: &[SecondarySource],
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
            .kind(primary, secondary, output, available)?
            .is_some_and(|kind| !crate::key_relation::comparable(kind, donor.kind.into()))
        {
            return Err(PlanError::InvalidLookup);
        }
    }
    Ok(())
}

pub(super) fn validate(
    keys: &[IntermediateKey],
    primary: &TableSchema,
    secondary: &[SecondarySource],
    donor: &TableSchema,
    output: &TableSchema,
    available: &[bool],
) -> Result<(), PlanError> {
    if keys
        .iter()
        .any(|key| matches!(key.value, MatchValue::Contextual(_)))
    {
        return Err(PlanError::InvalidIntermediate);
    }
    validate_declaration(keys, primary, secondary, donor, output, available)
}
