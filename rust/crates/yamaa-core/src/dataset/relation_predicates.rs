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
    /// A filter field or range value deferred to a reading donor record
    /// (LOOKUP-16), bare or qualified by that record's dataset. Binding
    /// refuses a row's read of an unresolved name (LOOKUP-7, LOOKUP-9),
    /// so only a rebased donor plan evaluates it.
    Contextual,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentField {
    pub read: CurrentRead,
    pub identifier: String,
}

/// Source bindings read donors. Column bindings index the current field metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct RelationPredicate {
    predicate: BoundPredicate,
    current: Vec<CurrentField>,
    deferred_type_findings: Vec<crate::diagnostic::Diagnostic>,
}
impl From<BoundPredicate> for RelationPredicate {
    fn from(predicate: BoundPredicate) -> Self {
        Self {
            predicate,
            current: Vec::new(),
            deferred_type_findings: Vec::new(),
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
        Ok(Self {
            predicate,
            current,
            deferred_type_findings: Vec::new(),
        })
    }
    pub(crate) fn with_deferred_type_findings(
        mut self,
        findings: Vec<crate::diagnostic::Diagnostic>,
    ) -> Self {
        self.deferred_type_findings = findings;
        self
    }
    /// Known operand types retained until the current-key dependency checkpoint.
    pub fn deferred_type_findings(&self) -> &[crate::diagnostic::Diagnostic] {
        &self.deferred_type_findings
    }
    pub fn predicate(&self) -> &BoundPredicate {
        &self.predicate
    }
    pub fn plan(&self) -> &crate::expression::CompiledExpression {
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
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrSubstr { source, .. }
        | Expression::StrLength { source, .. }
        | Expression::StrDecimal { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::Round { source, .. } => visit(&source.expression, visitor),
        Expression::FirstAvailable(selection) => {
            for source in selection.sources() {
                if let SelectionRead::Expression(source) = &source.read {
                    visit(&source.expression, visitor);
                }
            }
        }
        Expression::Extreme { operands, .. }
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
        // A range value and a derive binding read a selection for the current row.
        Expression::Aggregate(aggregate) => {
            if let Some(CurrentRead::Intermediate { index, .. }) =
                aggregate.range().map(|range| range.current().read)
            {
                visitor(index);
            }
            for binding in aggregate.derivations() {
                visit(&binding.assignment.expression, visitor);
            }
        }
        // An implicit join's range value may read a selection for the row.
        Expression::SourceLookup { range, .. } | Expression::JoinedSource { range, .. } => {
            if let Some(CurrentRead::Intermediate { index, .. }) =
                range.as_ref().map(|range| range.current().read)
            {
                visitor(index);
            }
        }
        // The aggregate relation/source/key IR has no named selector.
        Expression::Joined(_)
        | Expression::Odm(_)
        | Expression::Literal(_)
        | Expression::Window(_)
        | Expression::ContextWindow(_)
        | Expression::Source(_)
        | Expression::Collect { .. }
        | Expression::Column(_) => (),
    }
}
/// Visit the range of every lookup the current row evaluates directly, a
/// qualified aggregate's included, through lazy and nested alternatives. A
/// named intermediate's range is reached through its reads, and a derive
/// binding evaluates for a donor record instead.
pub(crate) fn visit_ranges(expression: &Expression, visitor: &mut impl FnMut(&RelationRange)) {
    match expression {
        Expression::Aggregate(aggregate) => {
            if let Some(range) = aggregate.range() {
                visitor(range);
            }
        }
        Expression::SourceLookup { range, .. } | Expression::JoinedSource { range, .. } => {
            if let Some(range) = range {
                visitor(range);
            }
        }
        Expression::SourceBinding { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrSubstr { source, .. }
        | Expression::StrLength { source, .. }
        | Expression::StrDecimal { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::Round { source, .. } => visit_ranges(&source.expression, visitor),
        Expression::FirstAvailable(selection) => {
            for source in selection.sources() {
                if let SelectionRead::Expression(source) = &source.read {
                    visit_ranges(&source.expression, visitor);
                }
            }
        }
        Expression::Extreme { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::Imputation { operands, .. } => {
            for source in operands {
                visit_ranges(&source.expression, visitor);
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                visit_ranges(&branch.assignment.expression, visitor);
            }
        }
        Expression::Intermediate { .. }
        | Expression::Compute(_)
        | Expression::Flag(_)
        | Expression::ProjectFunction(_)
        | Expression::Joined(_)
        | Expression::Odm(_)
        | Expression::Literal(_)
        | Expression::Window(_)
        | Expression::ContextWindow(_)
        | Expression::Source(_)
        | Expression::Collect { .. }
        | Expression::Column(_) => (),
    }
}
