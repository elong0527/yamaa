//! Immutable aggregate relation and field bindings, admitted without study cells.
use super::*;
use crate::aggregate_compiler::CompiledAggregate;
use crate::value::ColumnType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateSource {
    Primary,
    Secondary(usize),
}

impl DatasetPlan {
    /// Retain authored grouped-row identities for later column diagnostics.
    /// These facts grant no new source reads or expression bindings.
    pub fn with_row_contexts(
        mut self,
        contexts: Vec<Option<AggregateRowContext>>,
    ) -> Result<Self, PlanError> {
        if contexts.len() != self.templates.len() {
            return Err(PlanError::InvalidColumns);
        }
        for (template, context) in self.templates.iter().zip(&contexts) {
            if let Some(context) = context {
                let RowMode::Groups(groups) = template.mode.local_mode() else {
                    return Err(PlanError::UngroupedReduction);
                };
                if context.row.is_empty()
                    || context.groups.len() != groups.len()
                    || context
                        .groups
                        .iter()
                        .enumerate()
                        .any(|(index, (name, column))| {
                            name.is_empty()
                                || *column != groups[index]
                                || context.groups[..index]
                                    .iter()
                                    .any(|(prior, _)| prior == name)
                        })
                {
                    return Err(PlanError::InvalidSource);
                }
            }
        }
        self.row_contexts = contexts;
        Ok(self)
    }
    pub fn row_context(&self, template: usize) -> Option<&AggregateRowContext> {
        self.row_contexts.get(template).and_then(Option::as_ref)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AggregateRelation {
    /// Only the enclosing grouped row's original driver membership.
    CurrentGroup,
    /// Qualified aggregates reduce a donor relation before joining to this row.
    Matched {
        source: AggregateSource,
        keys: Vec<IntermediateKey>,
        group_by: Vec<usize>,
    },
    /// Completed constructed output rows, broadcast within these columns.
    Output { group_by: Vec<usize> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregateBinding {
    pub name: String,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AggregateBindingError {
    #[error("dataset aggregate empty path")]
    EmptyPath,
    #[error("dataset aggregate duplicate name")]
    DuplicateName,
    #[error("dataset aggregate missing name")]
    MissingName,
    #[error("dataset aggregate unused name")]
    UnusedName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregatePhase {
    RowConstruction,
    Derivation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregateRowContext {
    pub row: String,
    pub groups: Vec<(String, usize)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundAggregate {
    expression: CompiledAggregate,
    relation: AggregateRelation,
    bindings: Vec<AggregateBinding>,
    filter: Option<BoundPredicate>,
    phase: AggregatePhase,
    row_context: Option<AggregateRowContext>,
    operation_path: String,
    range: Option<RelationRange>,
    derivations: Vec<AggregateDerivation>,
    derived_schema: Option<TableSchema>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AggregateDerivation {
    pub assignment: Assignment,
    pub kind: ColumnType,
    pub handler: Option<crate::conversion::LiteralHandler>,
    pub path: String,
}
impl BoundAggregate {
    pub fn new(
        expression: CompiledAggregate,
        relation: AggregateRelation,
        mut bindings: Vec<AggregateBinding>,
        filter: Option<BoundPredicate>,
        phase: AggregatePhase,
        row_context: Option<AggregateRowContext>,
        operation_path: &str,
    ) -> Result<Self, AggregateBindingError> {
        use AggregateBindingError as E;
        if expression.spec_path().is_empty() || operation_path.is_empty() {
            return Err(E::EmptyPath);
        }
        bindings.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        for (index, binding) in bindings.iter().enumerate() {
            if index > 0 && bindings[index - 1].name == binding.name {
                return Err(E::DuplicateName);
            }
            if !expression.identifiers().any(|name| name == binding.name) {
                return Err(E::UnusedName);
            }
        }
        if expression.identifiers().any(|name| {
            bindings
                .binary_search_by(|b| b.name.as_str().cmp(name))
                .is_err()
        }) {
            return Err(E::MissingName);
        }
        Ok(Self {
            expression,
            relation,
            bindings,
            filter,
            phase,
            row_context,
            operation_path: operation_path.into(),
            range: None,
            derivations: Vec::new(),
            derived_schema: None,
        })
    }
    pub fn with_record_operations(
        mut self,
        range: Option<RelationRange>,
        derivations: Vec<AggregateDerivation>,
        derived_schema: Option<TableSchema>,
    ) -> Self {
        self.range = range;
        self.derivations = derivations;
        self.derived_schema = derived_schema;
        self
    }
    pub fn range(&self) -> Option<&RelationRange> {
        self.range.as_ref()
    }
    pub fn derivations(&self) -> &[AggregateDerivation] {
        &self.derivations
    }
    pub fn derived_schema(&self) -> Option<&TableSchema> {
        self.derived_schema.as_ref()
    }
    pub fn expression(&self) -> &CompiledAggregate {
        &self.expression
    }
    pub fn relation(&self) -> &AggregateRelation {
        &self.relation
    }
    pub fn bindings(&self) -> &[AggregateBinding] {
        &self.bindings
    }
    pub fn filter(&self) -> Option<&BoundPredicate> {
        self.filter.as_ref()
    }
    pub fn phase(&self) -> AggregatePhase {
        self.phase
    }
    pub fn row_context(&self) -> Option<&AggregateRowContext> {
        self.row_context.as_ref()
    }
    pub fn operation_path(&self) -> &str {
        &self.operation_path
    }
    pub fn column(&self, name: &str) -> usize {
        self.bindings[self
            .bindings
            .binary_search_by(|b| b.name.as_str().cmp(name))
            .expect("admitted aggregate field")]
        .column
    }
    pub(super) fn validate(
        &self,
        primary: &TableSchema,
        secondary: &[SecondarySource],
        intermediates: &[Intermediate],
        output: &TableSchema,
        available: &[bool],
        mode: &RowMode,
    ) -> Result<(), PlanError> {
        let (schema, groups) = match &self.relation {
            AggregateRelation::CurrentGroup => {
                let RowMode::Groups(groups) = mode.local_mode() else {
                    return Err(PlanError::UngroupedReduction);
                };
                (primary, Some(groups.as_slice()))
            }
            AggregateRelation::Matched {
                source,
                keys,
                group_by,
            } => {
                let donor = match source {
                    AggregateSource::Primary => primary,
                    AggregateSource::Secondary(index) => {
                        &secondary
                            .get(*index)
                            .ok_or(PlanError::InvalidSource)?
                            .schema
                    }
                };
                match_values::validate(
                    keys,
                    primary,
                    secondary,
                    donor,
                    &|column| donor.columns().get(column).map(|column| column.kind),
                    output,
                    available,
                )?;
                if group_by
                    .iter()
                    .any(|&column| column >= donor.columns().len())
                {
                    return Err(PlanError::InvalidSource);
                }
                // REQ-0470: every group_by column of a qualified aggregate
                // must also be one of the keys it joins on. The Matched
                // executor matches on keys only, so any other group_by
                // column would silently merge distinct donor groups.
                if group_by
                    .iter()
                    .any(|column| keys.iter().all(|key| key.source_column != *column))
                {
                    return Err(PlanError::InvalidSource);
                }
                (donor, Some(group_by.as_slice()))
            }
            AggregateRelation::Output { group_by } => {
                if group_by.is_empty()
                    || group_by
                        .iter()
                        .any(|&column| !available.get(column).copied().unwrap_or(false))
                {
                    return Err(PlanError::UnavailableColumn);
                }
                (output, Some(group_by.as_slice()))
            }
        };
        if matches!(self.relation, AggregateRelation::CurrentGroup)
            && self.phase != AggregatePhase::RowConstruction
            || matches!(self.relation, AggregateRelation::Output { .. })
                && self.phase != AggregatePhase::Derivation
        {
            return Err(PlanError::InvalidSource);
        }
        if let Some(context) = &self.row_context {
            let RowMode::Groups(groups) = mode.local_mode() else {
                return Err(PlanError::UngroupedReduction);
            };
            if context.row.is_empty()
                || context.groups.len() != groups.len()
                || context
                    .groups
                    .iter()
                    .enumerate()
                    .any(|(index, (name, column))| name.is_empty() || *column != groups[index])
            {
                return Err(PlanError::InvalidSource);
            }
        }
        if let Some(range) = &self.range {
            if !matches!(self.relation, AggregateRelation::Matched { .. }) {
                return Err(PlanError::InvalidSource);
            }
            range
                .validate_source(schema, &|column| {
                    schema
                        .columns()
                        .get(column)
                        .map(|column| column.kind.into())
                })
                .map_err(|_| PlanError::InvalidSource)?;
            if let CurrentRead::Output(column) = range.current().read {
                if !available.get(column).copied().unwrap_or(false) {
                    return Err(PlanError::UnavailableColumn);
                }
            }
            let current_kind = match range.current().read {
                CurrentRead::Output(column) => {
                    output.columns().get(column).map(|field| Some(field.kind))
                }
                CurrentRead::Record { source: 0, column } => {
                    primary.columns().get(column).map(|field| Some(field.kind))
                }
                // A range narrows a reduction for completed rows. A qualified
                // value reads the primary record that built the row (REQ-0085);
                // another input has no record at that row's position.
                CurrentRead::Record { .. } => None,
                CurrentRead::Intermediate { index, column } => {
                    intermediates.get(index).and_then(|item| {
                        item.source_schema(secondary)?.columns().get(column)?;
                        Some(item.source_kind(secondary, column))
                    })
                }
            }
            .ok_or(PlanError::InvalidSource)?;
            if range
                .value_kind()
                .is_some_and(|kind| current_kind.is_some_and(|current| kind != current.into()))
            {
                return Err(PlanError::InvalidSource);
            }
        }
        let evaluation_schema = self.derived_schema.as_ref().unwrap_or(schema);
        if self.derived_schema.is_some()
            && !matches!(self.relation, AggregateRelation::Matched { .. })
        {
            return Err(PlanError::InvalidSource);
        }
        if let Some(derived) = &self.derived_schema {
            if derived.columns().len() != schema.columns().len() + self.derivations.len()
                || derived.columns()[..schema.columns().len()] != *schema.columns()
            {
                return Err(PlanError::InvalidSource);
            }
            let mut derived_output = output.columns().to_vec();
            derived_output.extend(derived.columns()[schema.columns().len()..].iter().cloned());
            let derived_output =
                TableSchema::new(derived_output).map_err(|_| PlanError::InvalidColumns)?;
            let mut complete = vec![false; derived_output.columns().len()];
            complete[..output.columns().len()].copy_from_slice(available);
            for (index, derivation) in self.derivations.iter().enumerate() {
                if derivation.path.is_empty()
                    || derivation
                        .handler
                        .as_ref()
                        .is_some_and(|handler| handler.spec_path.is_empty())
                {
                    return Err(PlanError::EmptyPath);
                }
                if derivation.assignment.column != output.columns().len() + index
                    || derivation.kind != derived.columns()[schema.columns().len() + index].kind
                {
                    return Err(PlanError::InvalidColumns);
                }
                validate_record_assignment(
                    &derivation.assignment,
                    &mut complete,
                    schema,
                    primary,
                    secondary,
                    intermediates,
                    &derived_output,
                )?;
                complete[derivation.assignment.column] = true;
            }
        }
        for binding in &self.bindings {
            if binding.column >= evaluation_schema.columns().len() {
                return Err(PlanError::InvalidSource);
            }
            if matches!(self.relation, AggregateRelation::Output { .. })
                && !available.get(binding.column).copied().unwrap_or(false)
            {
                return Err(PlanError::UnavailableColumn);
            }
        }
        if let Some(groups) = groups {
            for name in self.expression.ungrouped_identifiers() {
                if !groups.contains(&self.column(name)) {
                    return Err(PlanError::NonGroupSource);
                }
            }
        }
        if let Some(filter) = &self.filter {
            filter
                .validate(schema.columns().len(), &[], false)
                .map_err(PlanError::Filter)?;
        }
        Ok(())
    }
}

/// Visit the range of every aggregate an expression evaluates, including an
/// aggregate in a case branch, an operand or a derive binding.
pub(crate) fn visit_ranges(expression: &Expression, visitor: &mut impl FnMut(&RelationRange)) {
    match expression {
        Expression::Aggregate(aggregate) => {
            if let Some(range) = aggregate.range() {
                visitor(range);
            }
            for binding in aggregate.derivations() {
                visit_ranges(&binding.assignment.expression, visitor);
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                visit_ranges(&branch.assignment.expression, visitor);
            }
        }
        Expression::SourceBinding { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::StrContains { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
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
        | Expression::StrConcat { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::Imputation { operands, .. } => {
            for source in operands {
                visit_ranges(&source.expression, visitor);
            }
        }
        // These hold bound reads, never a nested expression.
        Expression::Joined(_)
        | Expression::JoinedSource { .. }
        | Expression::Odm(_)
        | Expression::Literal(_)
        | Expression::SourceLookup { .. }
        | Expression::Flag(_)
        | Expression::Compute(_)
        | Expression::Function(_)
        | Expression::ProjectFunction(_)
        | Expression::Window(_)
        | Expression::ContextWindow(_)
        | Expression::Source(_)
        | Expression::Lookup(_)
        | Expression::RowLookup(_)
        | Expression::Intermediate { .. }
        | Expression::Collect { .. }
        | Expression::Column(_)
        | Expression::Reduce { .. }
        | Expression::Count { .. } => (),
    }
}
