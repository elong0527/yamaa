//! Predicate evaluation facts and stable structural routes without host formatting.
use super::{ConditionCode as C, Context, ContextValue as V, Diagnostic};
use crate::{
    predicate::{Condition, ErrorKind, EvaluationError, LimitError, Route},
    value::{Value, ValueType},
};
use alloc::{format, string::String, vec};

pub const MAX_PREDICATE_DIAGNOSTIC_BYTES: usize = 16_777_216;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredicateGrammarDiagnosticError {
    OutputLimit,
}
/// Native resolver/resource failures remain borrowed and never acquire an
/// invented predicate condition when diagnostic inspection cannot represent them.
#[derive(Debug)]
pub enum PredicateDiagnosticError<'a, E> {
    Resolution(&'a E),
    Resource(&'a LimitError),
    OutputLimit,
}
struct Budget(usize);
impl Budget {
    fn text<'a, E>(&mut self, s: &str) -> Result<(), PredicateDiagnosticError<'a, E>> {
        let amount = s
            .len()
            .checked_mul(12)
            .and_then(|n| n.checked_add(128))
            .ok_or(PredicateDiagnosticError::OutputLimit)?;
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or(PredicateDiagnosticError::OutputLimit)?;
        Ok(())
    }
}
fn text(s: &str) -> V {
    V::Scalar(Value::Str(s.into()))
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
        self.diagnostic_with_limit(MAX_PREDICATE_DIAGNOSTIC_BYTES)
    }
    /// Admit every borrowed path, condition fact and route before cloning the
    /// first authored string. A late quota refusal produces no partial finding.
    pub fn diagnostic_with_limit(
        &self,
        maximum: usize,
    ) -> Result<Diagnostic, PredicateDiagnosticError<'_, E>> {
        let condition = match &self.kind {
            ErrorKind::Resolution { error, .. } => {
                return Err(PredicateDiagnosticError::Resolution(error))
            }
            ErrorKind::Limit(limit) => return Err(PredicateDiagnosticError::Resource(limit)),
            ErrorKind::Condition(condition) => condition,
        };
        let mut budget = Budget(maximum.min(MAX_PREDICATE_DIAGNOSTIC_BYTES));
        let d = condition.diagnostic_code().definition();
        for s in [
            "phase",
            d.phase,
            "condition",
            d.condition,
            "requirement",
            d.requirement.unwrap_or(""),
            "spec_paths",
            self.spec_path.as_str(),
            "context",
            "operand_route",
        ] {
            budget.text(s)?;
        }
        // A route label contains at most twenty usize digits plus fixed text.
        for _ in &self.route {
            budget.text("01234567890123456789012345678901")?;
        }
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
        for (key, value) in fields {
            if let (Some(key), Some(value)) = (key, value) {
                budget.text(key)?;
                budget.text(value)?;
            }
        }
        let context: Context = fields
            .into_iter()
            .filter_map(|(key, value)| Some((key?.into(), text(value?))))
            .collect();
        Ok(Diagnostic {
            code: condition.diagnostic_code(),
            spec_paths: vec![self.spec_path.clone()],
            context,
            source_span: None,
            operand_route: Some(self.route.iter().map(|r| r.diagnostic_label()).collect()),
        })
    }
}
