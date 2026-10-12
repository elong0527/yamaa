//! Typed ownership of named stored-input or completed-output donor relations.
use super::*;

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
    pub fn source_schema<'a>(&'a self, sources: &'a [SecondarySource]) -> Option<&'a TableSchema> {
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
            .filter(|&column| self.source.admits_column(column))
    }
}

fn named_read(read: &Read, items: &[Intermediate]) -> bool {
    matches!(read, Read::Intermediate { index, .. } if items.get(*index).is_some_and(|item| item.source.is_self()))
}
fn reads(expression: &Expression, items: &[Intermediate]) -> bool {
    match expression {
        Expression::Intermediate { index, .. } => items.get(*index).is_some_and(|item| item.source.is_self()),
        Expression::SourceBinding { source, .. } | Expression::StrContains { source, .. }
        | Expression::StrCase { source, .. } | Expression::StrPad { source, .. }
        | Expression::StrExtract { source, .. } | Expression::Round { source, .. } => reads(&source.expression, items),
        Expression::FirstAvailable(selection) => selection.sources().iter().any(|source| matches!(&source.read, SelectionRead::Expression(source) if reads(&source.expression, items))),
        Expression::Extreme { operands, .. } | Expression::StrConcat { operands, .. }
        | Expression::StrTemplate { operands, .. } | Expression::Temporal { operands, .. }
        | Expression::Imputation { operands, .. } => operands.iter().any(|source| reads(&source.expression, items)),
        Expression::Compute(expression) => expression.bindings().iter().any(|binding| named_read(&binding.read, items)),
        Expression::Flag(flag) => flag.bindings().iter().any(|binding| matches!(&binding.read, FlagRead::Read(read) if named_read(read, items))),
        Expression::Function(function) => function.arguments().iter().any(|argument| matches!(&argument.input, FunctionInput::Read(read) if named_read(read, items))),
        Expression::ProjectFunction(function) => function.arguments().iter().any(|argument| matches!(&argument.input, FunctionInput::Read(read) if named_read(read, items))),
        Expression::Case(branches) => branches.iter().any(|branch| branch.when.as_ref().is_some_and(|when| when.bindings().iter().any(|binding| named_read(&binding.read, items))) || reads(&branch.assignment.expression, items)),
        _ => false,
    }
}
pub(super) fn validate(items: &[Intermediate], templates: &[RowTemplate]) -> Result<(), PlanError> {
    for item in items {
        if let IntermediateSource::SelfRows { columns, .. } = &item.source {
            if templates.iter().any(|template| {
                matches!(template.mode.local_mode(), RowMode::Keys)
                    || columns.iter().any(|&column| {
                        !template
                            .assignments
                            .iter()
                            .any(|assignment| assignment.column == column)
                    })
            }) {
                return Err(PlanError::InvalidIntermediate);
            }
        }
    }
    if templates[0]
        .assignments
        .iter()
        .any(|assignment| reads(&assignment.expression, items))
    {
        return Err(PlanError::InvalidIntermediate);
    }
    Ok(())
}
