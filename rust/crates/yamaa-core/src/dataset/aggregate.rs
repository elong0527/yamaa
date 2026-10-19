//! Immutable aggregate relation and field bindings, admitted without study cells.
use super::*;
use crate::expression::CompiledExpression;
use crate::value::ColumnType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateSource {
    Primary,
    Secondary(usize),
}

impl DatasetPlan {
    /// Retain authored grouped-row identities for later column diagnostics.
    /// These facts grant no new source reads or expression bindings.
    pub(crate) fn with_row_contexts(mut self, contexts: Vec<Option<AggregateRowContext>>) -> Self {
        self.row_contexts = contexts;
        self
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
    expression: CompiledExpression,
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
        expression: CompiledExpression,
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
    pub fn expression(&self) -> &CompiledExpression {
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
}
