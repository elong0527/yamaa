//! Bound scalar selection retains operand provenance and converts only its result.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum SelectionRead {
    Expression(std::boxed::Box<Assignment>),
    Column(usize),
    Source(usize),
    Collect {
        column: usize,
        identifier: String,
        filter: Option<std::boxed::Box<BoundPredicate>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionSource {
    pub path: String,
    pub read: SelectionRead,
}

/// `missing` is the declared value, or missing when none is declared; sources
/// that are all missing are counted at the operation's `missing` path either
/// way (EXPR-9).
#[derive(Clone, Debug, PartialEq)]
pub struct FirstAvailable {
    path: String,
    sources: Vec<SelectionSource>,
    missing: Value,
}
impl FirstAvailable {
    pub fn new(path: String, sources: Vec<SelectionSource>, missing: Value) -> Self {
        Self {
            path,
            sources,
            missing,
        }
    }
    pub fn sources(&self) -> &[SelectionSource] {
        &self.sources
    }
    pub(super) fn sources_mut(&mut self) -> &mut [SelectionSource] {
        &mut self.sources
    }
    pub fn missing(&self) -> &Value {
        &self.missing
    }
    pub fn missing_path(&self) -> String {
        std::format!("{}.missing", self.path)
    }
}
