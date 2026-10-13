//! Donor predicates carry explicit current-driver coordinates without changing scalar reads.
use super::*;
use crate::bound_expression::Binding;

/// Explicit current-side ownership shared by correlated relation operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrentRead {
    /// Zero identifies the original driver; positive indices identify secondary inputs.
    Record {
        source: usize,
        column: usize,
    },
    Output(usize),
    Intermediate {
        index: usize,
        column: usize,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentField {
    pub read: CurrentRead,
    pub identifier: String,
}
impl CurrentField {
    pub(super) fn validate(
        &self,
        primary: &TableSchema,
        secondary: &[SecondarySource],
        output: &TableSchema,
        items: &[Intermediate],
        item_index: usize,
    ) -> Result<(), PlanError> {
        if self.identifier.is_empty() {
            return Err(PlanError::InvalidIntermediate);
        }
        match self.read {
            CurrentRead::Record { source, column } => {
                let schema = if source == 0 {
                    primary
                } else {
                    &secondary
                        .get(source - 1)
                        .ok_or(PlanError::InvalidIntermediate)?
                        .schema
                };
                if column >= schema.columns().len() {
                    return Err(PlanError::InvalidIntermediate);
                }
            }
            CurrentRead::Output(column) if column >= output.columns().len() => {
                return Err(PlanError::InvalidIntermediate)
            }
            CurrentRead::Intermediate { index, column } => {
                if index >= item_index
                    || !items.get(index).is_some_and(|item| {
                        item.admits_column(column)
                            && item
                                .source_schema(secondary)
                                .is_some_and(|schema| column < schema.columns().len())
                    })
                {
                    return Err(PlanError::InvalidIntermediate);
                }
            }
            _ => (),
        }
        Ok(())
    }
    pub(super) fn validate_context(
        &self,
        source: usize,
        mode: &RowMode,
        available: &[bool],
    ) -> Result<(), PlanError> {
        match self.read {
            CurrentRead::Record { source: driver, .. } if driver != source => {
                return Err(PlanError::InvalidIntermediate)
            }
            CurrentRead::Record { column, .. } if matches!(mode.local_mode(), RowMode::Groups(keys) if !keys.contains(&column)) => {
                return Err(PlanError::NonGroupSource)
            }
            CurrentRead::Output(column) if !available.get(column).copied().unwrap_or(false) => {
                return Err(PlanError::UnavailableColumn)
            }
            _ => (),
        }
        Ok(())
    }
}

/// Source bindings read donors. Column bindings index the current field metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct RelationPredicate {
    predicate: BoundPredicate,
    current: Vec<CurrentField>,
}
impl From<BoundPredicate> for RelationPredicate {
    fn from(predicate: BoundPredicate) -> Self {
        Self {
            predicate,
            current: Vec::new(),
        }
    }
}
impl RelationPredicate {
    pub fn new(
        predicate: BoundPredicate,
        current: Vec<CurrentField>,
    ) -> Result<Self, BindingError> {
        for binding in predicate.bindings() {
            if let Read::Column(index) = binding.read {
                if current
                    .get(index)
                    .is_none_or(|field| field.identifier != binding.name)
                {
                    return Err(BindingError::UnavailableColumn);
                }
            }
        }
        for (index, field) in current.iter().enumerate() {
            if field.identifier.is_empty()
                || !predicate
                    .bindings()
                    .iter()
                    .any(|binding| binding.read == Read::Column(index))
            {
                return Err(BindingError::UnusedName);
            }
        }
        Ok(Self { predicate, current })
    }
    pub fn predicate(&self) -> &BoundPredicate {
        &self.predicate
    }
    pub fn plan(&self) -> &crate::predicate::Plan {
        self.predicate.plan()
    }
    pub fn bindings(&self) -> &[Binding] {
        self.predicate.bindings()
    }
    pub fn current(&self) -> &[CurrentField] {
        &self.current
    }
    pub fn correlated(&self) -> bool {
        !self.current.is_empty()
    }
    pub(super) fn validate(
        &self,
        donor_width: usize,
        primary: &TableSchema,
        secondary: &[SecondarySource],
        output: &TableSchema,
        items: &[Intermediate],
        item_index: usize,
    ) -> Result<(), PlanError> {
        self.predicate
            .validate(donor_width, &vec![true; self.current.len()], false)
            .map_err(PlanError::Filter)?;
        for field in &self.current {
            field.validate(primary, secondary, output, items, item_index)?;
        }
        // From<BoundPredicate> only admits donor-only predicates.
        for binding in self.bindings() {
            if let Read::Column(index) = binding.read {
                if self
                    .current
                    .get(index)
                    .is_none_or(|field| field.identifier != binding.name)
                {
                    return Err(PlanError::Filter(BindingError::UnavailableColumn));
                }
            }
        }
        Ok(())
    }
}

/// Visit every named selection, including lazy and nested alternatives.
pub(crate) fn visit(expression: &Expression, visitor: &mut impl FnMut(usize)) {
    let mut read = |read: &Read| {
        if let Read::Intermediate { index, .. } = read {
            visitor(*index);
        }
    };
    match expression {
        Expression::Intermediate { index, .. } => visitor(*index),
        Expression::SourceBinding { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::StrContains { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::Round { source, .. } => visit(&source.expression, visitor),
        Expression::FirstAvailable(selection) => {
            for source in selection.sources() {
                if let SelectionRead::Expression(source) = &source.read {
                    visit(&source.expression, visitor);
                }
            }
        }
        Expression::Extreme { operands, .. }
        | Expression::StrConcat { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::Imputation { operands, .. } => {
            for source in operands {
                visit(&source.expression, visitor);
            }
        }
        Expression::Compute(expression) => {
            for binding in expression.bindings() {
                read(&binding.read);
            }
        }
        Expression::Flag(flag) => {
            for binding in flag.bindings() {
                if let FlagRead::Read(value) = &binding.read {
                    read(value);
                }
            }
        }
        Expression::Function(function) => {
            for argument in function.arguments() {
                if let FunctionInput::Read(value) = &argument.input {
                    read(value);
                }
            }
        }
        Expression::ProjectFunction(function) => {
            for argument in function.arguments() {
                if let FunctionInput::Read(value) = &argument.input {
                    read(value);
                }
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                if let Some(when) = &branch.when {
                    for binding in when.bindings() {
                        if let Read::Intermediate { index, .. } = binding.read {
                            visitor(index);
                        }
                    }
                }
                visit(&branch.assignment.expression, visitor);
            }
        }
        // The current aggregate relation/source/key IR has no named selector.
        // Extend this visitor when aggregate derive or named relations are added.
        Expression::Joined(_)
        | Expression::JoinedSource { .. }
        | Expression::Aggregate(_)
        | Expression::Odm(_)
        | Expression::Literal(_)
        | Expression::Window(_)
        | Expression::ContextWindow(_)
        | Expression::Source(_)
        | Expression::SourceLookup { .. }
        | Expression::Lookup(_)
        | Expression::RowLookup(_)
        | Expression::Collect { .. }
        | Expression::Column(_)
        | Expression::Reduce { .. }
        | Expression::Count { .. } => (),
    }
}
pub(super) fn validate_reads(
    expression: &Expression,
    items: &[Intermediate],
    source: usize,
    mode: &RowMode,
    available: &[bool],
) -> Result<(), PlanError> {
    fn named(
        index: usize,
        items: &[Intermediate],
        source: usize,
        mode: &RowMode,
        available: &[bool],
        depth: usize,
    ) -> Result<(), PlanError> {
        if depth > 64 {
            return Err(PlanError::InvalidIntermediate);
        }
        let item = items.get(index).ok_or(PlanError::InvalidIntermediate)?;
        for key in &item.keys {
            if let Some(MatchRead::Record { column, .. }) = key.value.read() {
                if source != 0 {
                    return Err(PlanError::InvalidIntermediate);
                }
                if matches!(mode.local_mode(), RowMode::Groups(keys) if !keys.contains(column)) {
                    return Err(PlanError::NonGroupSource);
                }
            }
        }
        for field in item.current_fields() {
            field.validate_context(source, mode, available)?;
            if let CurrentRead::Intermediate {
                index: child,
                column: _,
            } = field.read
            {
                if child >= index {
                    return Err(PlanError::InvalidIntermediate);
                }
                if items[child].keys.iter().filter_map(|key| key.value.read()).any(|read| matches!(read, MatchRead::Output { column, .. } if !available.get(*column).copied().unwrap_or(false))) {
                        return Err(PlanError::InvalidLookup);
                    }
                named(child, items, source, mode, available, depth + 1)?;
            }
        }
        Ok(())
    }
    let mut result = Ok(());
    visit(expression, &mut |index| {
        if result.is_ok() {
            result = named(index, items, source, mode, available, 0);
        }
    });
    result
}
