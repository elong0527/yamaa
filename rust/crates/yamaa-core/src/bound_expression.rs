//! Immutable numeric/predicate bindings for the shared compiled specification.
//! Admission inspects names and column availability, never table cells or hosts.

use crate::{numeric_compiler::CompiledNumeric, predicate};
use alloc::{collections::BTreeSet, string::String, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    Source(usize),
    Column(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub read: Read,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingError {
    EmptyPath,
    DuplicateName,
    MissingName,
    UnusedName,
    UnavailableColumn,
    InvalidSource,
    GroupedSource,
}

/// Only scope failures are possible after the immutable name map is admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeError {
    UnavailableColumn,
    InvalidSource,
    GroupedSource,
}

impl From<ScopeError> for BindingError {
    fn from(error: ScopeError) -> Self {
        match error {
            ScopeError::UnavailableColumn => Self::UnavailableColumn,
            ScopeError::InvalidSource => Self::InvalidSource,
            ScopeError::GroupedSource => Self::GroupedSource,
        }
    }
}

fn bind_names<'a>(
    identifiers: impl IntoIterator<Item = &'a str>,
    mut bindings: Vec<Binding>,
) -> Result<Vec<Binding>, BindingError> {
    let identifiers: BTreeSet<_> = identifiers.into_iter().collect();
    bindings.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    for (index, binding) in bindings.iter().enumerate() {
        if index > 0 && bindings[index - 1].name == binding.name {
            return Err(BindingError::DuplicateName);
        }
        if !identifiers.contains(binding.name.as_str()) {
            return Err(BindingError::UnusedName);
        }
    }
    if identifiers.iter().any(|name| {
        bindings
            .binary_search_by(|binding| binding.name.as_str().cmp(name))
            .is_err()
    }) {
        return Err(BindingError::MissingName);
    }
    Ok(bindings)
}

fn validate_scope(
    bindings: &[Binding],
    source_width: usize,
    available: &[bool],
    grouped: bool,
) -> Result<(), ScopeError> {
    for binding in bindings {
        match binding.read {
            Read::Source(_) if grouped => return Err(ScopeError::GroupedSource),
            Read::Source(index) if index >= source_width => return Err(ScopeError::InvalidSource),
            Read::Column(index) if !available.get(index).copied().unwrap_or(false) => {
                return Err(ScopeError::UnavailableColumn)
            }
            _ => {}
        }
    }
    Ok(())
}

/// A predicate owns its plan and complete, unique name-to-column map.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundPredicate {
    plan: predicate::Plan,
    bindings: Vec<Binding>,
}

impl BoundPredicate {
    /// Admit every occurrence, including those behind a short-circuiting node.
    pub fn new(plan: predicate::Plan, bindings: Vec<Binding>) -> Result<Self, BindingError> {
        if plan.spec_path().is_empty() {
            return Err(BindingError::EmptyPath);
        }
        let bindings = bind_names(plan.identifiers(), bindings)?;
        Ok(Self { plan, bindings })
    }
    pub fn plan(&self) -> &predicate::Plan {
        &self.plan
    }
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// A grouped filter sees completed output columns, never source records.
    pub fn validate(
        &self,
        source_width: usize,
        available: &[bool],
        grouped: bool,
    ) -> Result<(), BindingError> {
        validate_scope(&self.bindings, source_width, available, grouped).map_err(Into::into)
    }
}

/// A numeric expression owns the same closed binding representation as predicates.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundNumeric {
    expression: CompiledNumeric,
    bindings: Vec<Binding>,
}

impl BoundNumeric {
    /// Bind all names without resolving cells, converting literals or folding arithmetic.
    pub fn new(expression: CompiledNumeric, bindings: Vec<Binding>) -> Result<Self, BindingError> {
        if expression.spec_path().is_empty() {
            return Err(BindingError::EmptyPath);
        }
        let bindings = bind_names(expression.identifiers(), bindings)?;
        Ok(Self {
            expression,
            bindings,
        })
    }
    pub fn expression(&self) -> &CompiledNumeric {
        &self.expression
    }
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// A non-key key-grain computation cannot choose a single feeding source row.
    pub fn reads_source(&self) -> bool {
        self.bindings
            .iter()
            .any(|binding| matches!(binding.read, Read::Source(_)))
    }
    pub fn validate(
        &self,
        source_width: usize,
        available: &[bool],
        grouped: bool,
    ) -> Result<(), ScopeError> {
        validate_scope(&self.bindings, source_width, available, grouped)
    }
}
