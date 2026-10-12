//! Bound scalar selection retains operand provenance and converts only its result.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum SelectionRead {
    Expression(alloc::boxed::Box<Assignment>),
    Column(usize),
    Source(usize),
    Collect {
        column: usize,
        identifier: String,
        filter: Option<alloc::boxed::Box<BoundPredicate>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionSource {
    pub path: String,
    pub read: SelectionRead,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FirstAvailable {
    sources: Vec<SelectionSource>,
    missing: Value,
}
impl FirstAvailable {
    pub fn new(sources: Vec<SelectionSource>, missing: Value) -> Self {
        Self { sources, missing }
    }
    pub fn sources(&self) -> &[SelectionSource] {
        &self.sources
    }
    pub fn missing(&self) -> &Value {
        &self.missing
    }
    pub(super) fn validate(
        &self,
        source: &TableSchema,
        available: &[bool],
        mode: &RowMode,
    ) -> Result<(), PlanError> {
        for operand in &self.sources {
            if operand.path.is_empty() {
                return Err(PlanError::EmptyPath);
            }
            match &operand.read {
                SelectionRead::Source(column) => {
                    if matches!(mode, RowMode::Keys) || *column >= source.columns().len() {
                        return Err(PlanError::InvalidSource);
                    }
                    if let RowMode::Groups(keys) = mode {
                        if !keys.contains(column) {
                            return Err(PlanError::InvalidSource);
                        }
                    }
                }
                SelectionRead::Expression(_) => (),
                SelectionRead::Column(column) => {
                    if !available.get(*column).copied().unwrap_or(false) {
                        return Err(PlanError::UnavailableColumn);
                    }
                }
                SelectionRead::Collect {
                    column,
                    identifier,
                    filter,
                } => {
                    if !matches!(mode, RowMode::Keys) {
                        return Err(PlanError::InvalidKeyMode);
                    }
                    if *column >= source.columns().len() {
                        return Err(PlanError::InvalidSource);
                    }
                    if identifier.is_empty() {
                        return Err(PlanError::InvalidSource);
                    }
                    if let Some(filter) = filter {
                        filter
                            .validate(source.columns().len(), &[], false)
                            .map_err(PlanError::Filter)?;
                    }
                }
            }
        }
        Ok(())
    }
}
