//! Predicate evaluation facts and stable structural routes without host formatting.
use super::{ConditionCode as C, Context, Diagnostic};
use crate::json::Value as V;
use crate::{
    predicate::{Condition, ErrorKind, EvaluationError, Route},
    value::ValueType,
};

/// Native resolver failures remain borrowed and never acquire an invented
/// predicate condition when diagnostic inspection cannot represent them.
#[derive(Debug)]
pub enum PredicateDiagnosticError<'a, E> {
    Resolution(&'a E),
}
fn text(s: &str) -> V {
    V::String(s.into())
}
fn kind(t: ValueType) -> &'static str {
    match t {
        ValueType::Str => "str",
        ValueType::Int => "int",
        ValueType::Float => "float",
        ValueType::Bool => "bool",
        ValueType::Date => "date",
        ValueType::DateTime => "datetime",
    }
}
impl Condition {
    pub fn diagnostic_code(&self) -> C {
        match self {
            Self::UnknownField { .. } => C::PredicateUnknownField,
            Self::IncompatiblePair { .. } => C::PredicateIncompatiblePair,
            Self::ExpectedText { .. } => C::PredicateExpectedText,
            Self::ContainsExpectedText { .. } => C::PredicateContainsExpectedText,
            Self::DanglingEscape => C::PredicateDanglingEscape,
        }
    }
}
impl Route {
    /// These historical route spellings are explicit portable vocabulary, not
    /// Rust Debug output. Indexed membership preserves the full usize width.
    fn diagnostic_label(self) -> String {
        match self {
            Self::Child => "Child".into(),
            Self::Left => "Left".into(),
            Self::Right => "Right".into(),
            Self::Value => "Value".into(),
            Self::Item(n) => format!("Item({n})"),
            Self::Lower => "Lower".into(),
            Self::RepeatedValue => "RepeatedValue".into(),
            Self::Upper => "Upper".into(),
            Self::Pattern => "Pattern".into(),
        }
    }
}
impl<E> EvaluationError<E> {
    /// Retain existing typed context and ordered structural geometry. Original
    /// expression text remains held by this error; no source span is fabricated.
    pub fn diagnostic(&self) -> Result<Diagnostic, PredicateDiagnosticError<'_, E>> {
        let condition = match &self.kind {
            ErrorKind::Resolution { error, .. } => {
                return Err(PredicateDiagnosticError::Resolution(error))
            }
            ErrorKind::Condition(condition) => condition,
        };
        Ok(condition.static_diagnostic(&self.spec_path, &self.route))
    }
}

impl Condition {
    /// Project a retained condition and structural route without running a predicate.
    pub(crate) fn static_diagnostic(&self, spec_path: &str, route: &[Route]) -> Diagnostic {
        let condition = self;
        let fields = match condition {
            Condition::UnknownField { identifier } => [
                (Some("identifier"), Some(identifier.as_str())),
                (None, None),
            ],
            Condition::IncompatiblePair { left, right } => [
                (Some("left_type"), Some(kind(*left))),
                (Some("right_type"), Some(kind(*right))),
            ],
            Condition::ExpectedText { actual } | Condition::ContainsExpectedText { actual } => [
                (Some("expected"), Some("str")),
                (Some("actual"), Some(kind(*actual))),
            ],
            Condition::DanglingEscape => [
                (Some("reason"), Some("LIKE pattern has a dangling escape")),
                (None, None),
            ],
        };
        let context: Context = fields
            .into_iter()
            .filter_map(|(key, value)| Some((key?.into(), text(value?))))
            .collect();
        Diagnostic {
            code: condition.diagnostic_code(),
            spec_paths: vec![spec_path.into()],
            context,
            source_span: None,
            operand_route: Some(route.iter().map(|r| r.diagnostic_label()).collect()),
        }
    }
}

impl<E: std::fmt::Debug> std::fmt::Display for PredicateDiagnosticError<'_, E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "diagnostic projection: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for PredicateDiagnosticError<'_, E> {}
