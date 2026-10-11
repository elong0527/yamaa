//! Typed canonical causes for named-selection binding and predicate findings.
use super::*;
use crate::{
    diagnostic::{ConditionCode as C, ContextValue, Diagnostic},
    predicate_parser::GrammarFailure,
    value::Value,
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
            Self::OutputKey => C::LookupOutputKeyReference,
            Self::Value => C::LookupValueReference,
        }
    }
}

#[derive(Debug)]
pub struct LookupFinding(Diagnostic);
impl LookupFinding {
    pub(super) fn new(path: &str, code: C) -> Self {
        Self(Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context: BTreeMap::new(),
            source_span: None,
            operand_route: None,
        })
    }
    pub(super) fn text(&mut self, key: &str, value: &str) {
        self.0
            .context
            .insert(key.into(), ContextValue::Scalar(Value::Str(value.into())));
    }
    pub(super) fn texts(&mut self, key: &str, values: &[&str]) {
        self.0.context.insert(
            key.into(),
            ContextValue::Sequence(
                values
                    .iter()
                    .map(|value| ContextValue::Scalar(Value::Str((*value).into())))
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
        expected: crate::key_relation::ComparableType,
        actual: ColumnType,
    ) -> Self {
        let mut finding = Self::new(path, C::LookupKeyType);
        let name = |kind: crate::key_relation::ComparableType| match kind {
            crate::key_relation::ComparableType::Boolean => "bool",
            crate::key_relation::ComparableType::Column(kind) => match kind {
                ColumnType::Str => "str",
                ColumnType::Int => "int",
                ColumnType::Float => "float",
                ColumnType::Date => "date",
                ColumnType::DateTime => "datetime",
            },
        };
        for (key, value) in [
            ("intermediate", intermediate),
            ("source", source),
            ("expected", name(expected)),
            ("actual", name(actual.into())),
        ] {
            finding.text(key, value);
        }
        finding
    }
    pub(super) fn grammar(
        path: &str,
        text: &str,
        position: usize,
        error: &GrammarFailure,
    ) -> Result<Self, BindError> {
        error
            .original_diagnostic(path, text, position)
            .map(Self)
            .map_err(|_| BindError::PredicateDiagnosticLimit)
    }
}
