//! Immutable numeric/predicate bindings for the shared compiled specification.
//! Admission inspects names and column availability, never table cells or hosts.

use crate::{numeric_compiler::CompiledNumeric, predicate};
use alloc::{boxed::Box, collections::BTreeSet, string::String, vec::Vec};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    Source(usize),
    Column(usize),
    Intermediate { index: usize, column: usize },
    Joined(Box<crate::dataset::JoinedRead>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub read: Read,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingError {
    EmptyPath,
    MetadataLimit,
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
            Read::Joined(ref read)
                if read
                    .keys
                    .iter()
                    .any(|key| !available.get(key.output_column).copied().unwrap_or(false)) =>
            {
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
    plan: Box<predicate::Plan>,
    bindings: Vec<Binding>,
    collected: Vec<String>,
}

impl BoundPredicate {
    /// Admit every occurrence, including those behind a short-circuiting node.
    pub fn new(plan: predicate::Plan, bindings: Vec<Binding>) -> Result<Self, BindingError> {
        if bindings
            .iter()
            .any(|binding| matches!(binding.read, Read::Intermediate { .. } | Read::Joined(_)))
        {
            return Err(BindingError::InvalidSource);
        }
        if plan.spec_path().is_empty() {
            return Err(BindingError::EmptyPath);
        }
        let bindings = bind_names(plan.identifiers(), bindings)?;
        Ok(Self {
            plan: Box::new(plan),
            bindings,
            collected: Vec::new(),
        })
    }
    /// Case predicates may read a kept record or lazily reduce a feeding key
    /// group. Generic donor/output filters retain their narrower constructor.
    /// Collection ownership is by authored name, not by an invented current row.
    pub fn new_contextual(
        plan: predicate::Plan,
        bindings: Vec<Binding>,
        mut collected: Vec<String>,
    ) -> Result<Self, BindingError> {
        let bytes = plan
            .expression()
            .len()
            .checked_add(plan.spec_path().len())
            .and_then(|bytes| {
                bindings.iter().try_fold(bytes, |bytes, binding| {
                    bytes.checked_add(binding.name.len())?.checked_add(128)
                })
            })
            .and_then(|bytes| {
                collected
                    .iter()
                    .try_fold(bytes, |bytes, name| bytes.checked_add(name.len()))
            })
            .and_then(|bytes| bytes.checked_mul(8));
        if bytes.is_none_or(|bytes| bytes > 16_777_216)
            || bindings.len() > 65_536
            || collected.len() > bindings.len()
        {
            return Err(BindingError::MetadataLimit);
        }
        if plan.spec_path().is_empty() {
            return Err(BindingError::EmptyPath);
        }
        let bindings = bind_names(plan.identifiers(), bindings)?;
        collected.sort_unstable();
        for (index, name) in collected.iter().enumerate() {
            if index > 0 && collected[index - 1] == *name {
                return Err(BindingError::DuplicateName);
            }
            if !bindings
                .iter()
                .any(|binding| binding.name == *name && matches!(binding.read, Read::Source(_)))
            {
                return Err(BindingError::InvalidSource);
            }
        }
        Ok(Self {
            plan: Box::new(plan),
            bindings,
            collected,
        })
    }
    pub fn collected(&self) -> &[String] {
        &self.collected
    }
    pub fn collects(&self, name: &str) -> bool {
        self.collected
            .binary_search_by(|candidate| candidate.as_str().cmp(name))
            .is_ok()
    }
    /// Case owners additionally admit kept-record coordinates against their
    /// named relation schemas; generic filters cannot acquire those coordinates.
    pub(crate) fn validate_contextual(
        &self,
        source_width: usize,
        available: &[bool],
    ) -> Result<(), BindingError> {
        validate_scope(&self.bindings, source_width, available, false).map_err(Into::into)
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
        if !self.collected.is_empty()
            || self
                .bindings
                .iter()
                .any(|binding| matches!(binding.read, Read::Intermediate { .. } | Read::Joined(_)))
        {
            return Err(BindingError::InvalidSource);
        }
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
