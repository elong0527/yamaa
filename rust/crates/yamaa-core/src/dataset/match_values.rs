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
/// defect during binding only; the compiler refuses it before a plan exists.
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
}

/// One authored donor field and its current scalar match value, in written order.
#[derive(Clone, Debug, PartialEq)]
pub struct IntermediateKey {
    pub source_column: usize,
    pub path: String,
    pub value: MatchValue,
}
