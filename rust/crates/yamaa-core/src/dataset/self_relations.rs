//! Typed ownership of named stored-input or completed-output donor relations.
use super::*;
use crate::value::ColumnType;

/// SELF owns schema metadata only. Its values are borrowed at each phase boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum IntermediateSource {
    Input(usize),
    /// The original driver relation shares its already captured table.
    Primary {
        schema: TableSchema,
        name: String,
    },
    SelfRows {
        schema: TableSchema,
        columns: Vec<usize>,
    },
    /// The matching pairs of a pair relation's two sides, each a joined
    /// record, materialized once before any row is built (LOOKUP-27).
    Pairs(Box<PairJoin>),
}
impl IntermediateSource {
    pub fn is_self(&self) -> bool {
        matches!(self, Self::SelfRows { .. })
    }
    /// The join of a pair relation, which no reader reads through the
    /// intermediate (LOOKUP-27).
    pub fn pairs(&self) -> Option<&PairJoin> {
        match self {
            Self::Pairs(join) => Some(join),
            _ => None,
        }
    }
    pub fn input_index(&self) -> Option<usize> {
        match self {
            Self::Input(index) => Some(*index),
            Self::Primary { .. } | Self::SelfRows { .. } | Self::Pairs(_) => None,
        }
    }
    pub fn admits_column(&self, column: usize) -> bool {
        match self {
            Self::Input(_) | Self::Primary { .. } => true,
            Self::SelfRows { columns, .. } => columns.contains(&column),
            Self::Pairs(join) => column < join.schema.columns().len(),
        }
    }
}
impl Intermediate {
    /// Schema slots describe layout; only admitted inference describes raw results.
    pub fn source_kind(&self, sources: &[SecondarySource], column: usize) -> Option<ColumnType> {
        if let Some(derived) = &self.derivations {
            return derived.column_kind(column);
        }
        let stored = match &self.source {
            IntermediateSource::Input(index) => &sources.get(*index)?.schema,
            IntermediateSource::Primary { schema, .. }
            | IntermediateSource::SelfRows { schema, .. } => schema,
            IntermediateSource::Pairs(join) => &join.schema,
        };
        stored.columns().get(column).map(|column| column.kind)
    }
    pub fn source_schema<'a>(&'a self, sources: &'a [SecondarySource]) -> Option<&'a TableSchema> {
        if let Some(schema) = &self.pending_schema {
            return Some(schema);
        }
        if let Some(derived) = &self.derivations {
            return Some(derived.plan.output());
        }
        match &self.source {
            IntermediateSource::Input(index) => sources.get(*index).map(|source| &source.schema),
            IntermediateSource::Primary { schema, .. }
            | IntermediateSource::SelfRows { schema, .. } => Some(schema),
            IntermediateSource::Pairs(join) => Some(&join.schema),
        }
    }
    pub fn source_column(&self, sources: &[SecondarySource], name: &str) -> Option<usize> {
        self.source_schema(sources)?
            .columns()
            .iter()
            .position(|column| column.name == name)
            .filter(|&column| self.admits_column(column))
    }
}
