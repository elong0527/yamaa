//! Typed ownership of named stored-input or completed-output donor relations.
use super::*;
use crate::value::ColumnType;

/// SELF owns schema metadata only. Its values are borrowed at each phase boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
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
}
impl IntermediateSource {
    pub fn is_self(&self) -> bool {
        matches!(self, Self::SelfRows { .. })
    }
    pub fn input_index(&self) -> Option<usize> {
        match self {
            Self::Input(index) => Some(*index),
            Self::Primary { .. } | Self::SelfRows { .. } => None,
        }
    }
    pub fn admits_column(&self, column: usize) -> bool {
        match self {
            Self::Input(_) | Self::Primary { .. } => true,
            Self::SelfRows { columns, .. } => columns.contains(&column),
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
