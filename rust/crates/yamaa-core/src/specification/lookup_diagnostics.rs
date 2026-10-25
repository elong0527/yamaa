//! Typed canonical causes for named-selection binding and predicate findings.
use super::*;
use crate::json::Value as JsonValue;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    expression::{Context, Failure},
};

#[derive(Clone, Copy, Debug)]
pub(super) enum ReferenceCause {
    Selection,
    DonorKey,
    OutputKey,
    Value,
}
impl ReferenceCause {
    fn code(self) -> C {
        match self {
            Self::Selection => C::LookupSelectionReference,
            Self::DonorKey => C::LookupDonorKeyReference,
            Self::OutputKey => C::LookupMatchValueReference,
            Self::Value => C::LookupValueReference,
        }
    }
}

#[derive(Debug)]
pub struct LookupFinding(Diagnostic);
impl LookupFinding {
    pub(super) fn from_diagnostic(diagnostic: Diagnostic) -> Self {
        Self(diagnostic)
    }
    pub(super) fn new(path: &str, code: C) -> Self {
        Self(Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context: crate::json::Map::new(),
            source_span: None,
            operand_route: None,
        })
    }
    /// Add the further authored sites the finding names, in order.
    pub(super) fn extend_paths(&mut self, paths: impl IntoIterator<Item = String>) {
        self.0.spec_paths.extend(paths);
    }
    pub(super) fn text(&mut self, key: &str, value: &str) {
        self.0
            .context
            .insert(key.into(), JsonValue::String(value.into()));
    }
    /// A key the cause records on every path, null where the site has no
    /// value for it.
    pub(super) fn null(&mut self, key: &str) {
        self.0.context.insert(key.into(), JsonValue::Null);
    }
    pub(super) fn texts(&mut self, key: &str, values: &[&str]) {
        self.0.context.insert(
            key.into(),
            JsonValue::Array(
                values
                    .iter()
                    .map(|value| JsonValue::String((*value).into()))
                    .collect(),
            ),
        );
    }
    pub fn diagnostic(&self) -> Diagnostic {
        self.0.clone()
    }
    pub fn definition(&self) -> crate::diagnostic::Definition {
        self.0.definition()
    }
    pub(super) fn reference(
        path: &str,
        name: &str,
        intermediate: Option<&str>,
        cause: ReferenceCause,
        suggestion: Option<String>,
    ) -> Self {
        let mut finding = Self::new(path, cause.code());
        finding.text("identifier", name);
        if let Some(name) = intermediate {
            finding.text("intermediate", name);
        }
        if let Some(name) = suggestion {
            finding.text("suggestion", &name);
        }
        finding
    }
    pub(super) fn key_type(
        path: &str,
        intermediate: &str,
        source: &str,
        expected: crate::result_type::ResultType,
        actual: ColumnType,
    ) -> Self {
        Self(crate::diagnostic::lookup::key_type(
            path,
            crate::diagnostic::lookup::Owner::Intermediate(intermediate),
            source,
            expected,
            actual.into(),
        ))
    }
    pub(super) fn grammar(path: &str, text: &str, position: usize, error: &Failure) -> Self {
        Self(error.diagnostic(Context::Predicate, path, text, position))
    }
}
