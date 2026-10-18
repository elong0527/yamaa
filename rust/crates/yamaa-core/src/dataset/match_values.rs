//! Destination-free scalar values for named matching, never output assignments.
use super::*;
use crate::{result_type::ResultType, value::ColumnType};

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
    pub(super) fn kind(
        &self,
        primary: &TableSchema,
        secondary: &[SecondarySource],
        output: &TableSchema,
        available: &[bool],
    ) -> Result<ColumnType, PlanError> {
        if self.identifier().is_empty() {
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
    Expression {
        identifier: String,
    },
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
            Self::Expression { identifier: name }
            | Self::Variable(name)
            | Self::Source(name)
            | Self::StrPad {
                identifier: name, ..
            } => name,
        }
    }
    pub fn bind(&self, read: MatchRead) -> MatchValue {
        match self {
            Self::Expression { .. } => MatchValue::Contextual(self.clone()),
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
    Expression(std::boxed::Box<MatchExpression>),
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
            Self::Literal(_) | Self::Contextual(_) | Self::Expression(_) => None,
        }
    }
    pub fn reads(&self) -> impl Iterator<Item = &MatchRead> {
        self.read().into_iter().chain(match self {
            Self::Expression(expression) => expression.reads.iter(),
            _ => [].iter(),
        })
    }
    pub fn reads_mut(&mut self) -> Box<dyn Iterator<Item = &mut MatchRead> + '_> {
        match self {
            Self::Variable(read) | Self::Source(read) | Self::StrPad { read, .. } => {
                Box::new(std::iter::once(read))
            }
            Self::Expression(expression) => Box::new(expression.reads.iter_mut()),
            _ => Box::new(std::iter::empty()),
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
            Self::Literal(_) | Self::Expression(_) => None,
        }
    }
    /// The compiled match value's result type (EXPR-14): an expression's as
    /// compiled, a read's declared type, a literal's, or `str_pad`'s text.
    pub fn result_type(&self, read_kind: Option<ColumnType>) -> ResultType {
        match self {
            Self::Expression(expression) => expression.result_type,
            Self::Variable(_) | Self::Source(_) => ResultType::declared(read_kind),
            Self::StrPad { .. } | Self::Contextual(ContextualMatch::StrPad { .. }) => {
                ResultType::STR
            }
            Self::Contextual(_) => ResultType::UNKNOWN,
            Self::Literal(value) => ResultType::of_value(value),
        }
    }
    fn kind(
        &self,
        primary: &TableSchema,
        secondary: &[SecondarySource],
        output: &TableSchema,
        available: &[bool],
        read_kind: &impl Fn(&MatchRead, ColumnType) -> Option<ColumnType>,
    ) -> Result<ResultType, PlanError> {
        if let Self::Expression(expression) = self {
            expression.validate(primary, secondary, output, available)?;
        }
        let read_kind = self
            .read()
            .map(|read| {
                read.kind(primary, secondary, output, available)
                    .map(|physical| read_kind(read, physical))
            })
            .transpose()?
            .flatten();
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
            if declaration.identifier().is_empty() {
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

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_declaration(
    keys: &[IntermediateKey],
    primary: &TableSchema,
    secondary: &[SecondarySource],
    donor: &TableSchema,
    donor_kind: &impl Fn(usize) -> Option<ColumnType>,
    output: &TableSchema,
    available: &[bool],
    read_kind: &impl Fn(&MatchRead, ColumnType) -> Option<ColumnType>,
) -> Result<(), PlanError> {
    if keys.is_empty() || keys.len() > crate::limits::Limits::KEYS {
        return Err(PlanError::InvalidIntermediate);
    }
    for (index, key) in keys.iter().enumerate() {
        if key.path.is_empty()
            || keys[..index]
                .iter()
                .any(|prior| prior.source_column == key.source_column)
        {
            return Err(PlanError::InvalidIntermediate);
        }
        donor
            .columns()
            .get(key.source_column)
            .ok_or(PlanError::InvalidIntermediate)?;
        for read in key.value.reads() {
            if keys[..index]
                .iter()
                .flat_map(|prior| prior.value.reads())
                .any(|prior| prior.identifier() == read.identifier() && prior != read)
            {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        let kind = key
            .value
            .kind(primary, secondary, output, available, read_kind)?;
        if donor_kind(key.source_column).is_some_and(|donor| !kind.comparable(donor.into())) {
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
    donor_kind: &impl Fn(usize) -> Option<ColumnType>,
    output: &TableSchema,
    available: &[bool],
) -> Result<(), PlanError> {
    if keys
        .iter()
        .any(|key| matches!(key.value, MatchValue::Contextual(_)))
    {
        return Err(PlanError::InvalidIntermediate);
    }
    validate_declaration(
        keys,
        primary,
        secondary,
        donor,
        donor_kind,
        output,
        available,
        &|_, kind| Some(kind),
    )
}
