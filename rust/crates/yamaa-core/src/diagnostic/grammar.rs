//! Numeric, aggregate and predicate grammar findings retain original spelling in core.
use super::{ConditionCode as C, Context, ContextValue as V, Diagnostic};
use crate::{
    aggregate_parser::GrammarFailure as A, numeric_parser::GrammarFailure as N,
    predicate_parser::GrammarFailure as P, value::Value,
};
use alloc::{string::ToString, vec};

fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
fn finding(code: C, path: &str, expression: &str, extra: Context) -> Diagnostic {
    let mut context = extra;
    context.insert("expr".into(), text(expression));
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context,
        source_span: None,
        operand_route: None,
    }
}
fn function(
    expression: &str,
    name: crate::numeric_parser::SourceSpan,
    count: Option<usize>,
) -> Option<Context> {
    let mut context = Context::new();
    context.insert(
        "function".into(),
        text(expression.get(name.start..name.end)?),
    );
    if let Some(count) = count {
        context.insert("argument_count".into(), V::Integer(count.to_string()));
    }
    Some(context)
}
impl N {
    pub(crate) fn code(&self) -> C {
        match self {
            Self::InvalidExpression => C::NumericInvalidExpression,
            Self::ProhibitedConstruct { .. } => C::NumericProhibitedConstruct,
            Self::ProhibitedFunction { .. } => C::NumericProhibitedFunction,
        }
    }
    /// None denotes an inconsistent caller-supplied name span, never a language finding.
    pub fn diagnostic(&self, path: &str, expression: &str) -> Option<Diagnostic> {
        let extra = match self {
            Self::InvalidExpression => Context::new(),
            Self::ProhibitedConstruct { construct } => {
                [("construct".into(), text(construct))].into()
            }
            Self::ProhibitedFunction {
                name,
                argument_count,
            } => function(expression, *name, *argument_count)?,
        };
        Some(finding(self.code(), path, expression, extra))
    }
}
impl A {
    pub(crate) fn code(&self) -> C {
        match self {
            Self::InvalidExpression => C::AggregateInvalidExpression,
            Self::ProhibitedConstruct { .. } => C::AggregateProhibitedConstruct,
            Self::ProhibitedFunction { .. } => C::AggregateProhibitedFunction,
            Self::NestedReduction { .. } => C::AggregateNestedReduction,
        }
    }
    pub fn diagnostic(&self, path: &str, expression: &str) -> Option<Diagnostic> {
        let extra = match self {
            Self::InvalidExpression => Context::new(),
            Self::ProhibitedConstruct { construct } => {
                [("construct".into(), text(construct))].into()
            }
            Self::ProhibitedFunction {
                name,
                argument_count,
            } => function(expression, *name, *argument_count)?,
            Self::NestedReduction { outer, inner } => [
                ("outer".into(), text(outer.name())),
                ("inner".into(), text(inner.name())),
            ]
            .into(),
        };
        Some(finding(self.code(), path, expression, extra))
    }
}

/// REQ-0162 fixes the standard rewrite for a predicate that stops at an
/// arithmetic operator after an operand: compute the value as its own column
/// first, then compare that column.
const ARITHMETIC_SUGGESTION: &str =
    "derive the value as a column with compute, then compare that column";

impl P {
    /// The suggestion applies only when the parser stopped at an arithmetic
    /// operator (`+ - * /`) that directly follows an operand; a failure at
    /// any other character keeps the context unchanged.
    fn arithmetic_suggestion(&self, predicate: &str, position: usize) -> Option<&'static str> {
        if !matches!(self, Self::InvalidExpression) {
            return None;
        }
        let operator = predicate.chars().nth(position)?;
        if !matches!(operator, '+' | '-' | '*' | '/') {
            return None;
        }
        let operand_end = predicate
            .chars()
            .take(position)
            .filter(|character| !character.is_whitespace())
            .last()?;
        (operand_end.is_alphanumeric() || matches!(operand_end, '_' | '.' | '\'' | '"' | ')'))
            .then_some(ARITHMETIC_SUGGESTION)
    }
    pub fn diagnostic_code(&self) -> C {
        match self {
            Self::InvalidExpression | Self::InvalidTemporal { .. } => C::PredicateInvalidExpression,
            Self::IntegerOverflow => C::PredicateIntegerOverflow,
            Self::InvalidEscape => C::PredicateInvalidEscape,
            Self::InvalidRegex { .. } => C::PredicateInvalidRegex,
        }
    }
    /// Original predicate syntax owns character positions and temporal/regex detail.
    pub fn diagnostic(&self, path: &str, predicate: &str, position: usize) -> Diagnostic {
        let mut context = self.syntax_diagnostic().context;
        context.insert("predicate".into(), text(predicate));
        context.insert("position".into(), V::Integer(position.to_string()));
        if let Some(suggestion) = self.arithmetic_suggestion(predicate, position) {
            context.insert("suggestion".into(), text(suggestion));
        }
        Diagnostic {
            code: self.diagnostic_code(),
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        }
    }
    /// Original document findings retain the reference predicate/position
    /// context. Parser-only detail stays on the immutable grammar failure and
    /// remains available through `syntax_diagnostic`.
    pub fn original_diagnostic(
        &self,
        path: &str,
        predicate: &str,
        position: usize,
    ) -> Result<Diagnostic, super::PredicateGrammarDiagnosticError> {
        self.original_diagnostic_with_limit(
            path,
            predicate,
            position,
            super::MAX_PREDICATE_DIAGNOSTIC_BYTES,
        )
    }
    /// Whole-envelope admission precedes every authored string copy. Character
    /// positions preserve their complete unsigned width; no byte span is added.
    pub fn original_diagnostic_with_limit(
        &self,
        path: &str,
        predicate: &str,
        position: usize,
        maximum: usize,
    ) -> Result<Diagnostic, super::PredicateGrammarDiagnosticError> {
        use super::PredicateGrammarDiagnosticError::OutputLimit;
        let mut remaining = maximum.min(super::MAX_PREDICATE_DIAGNOSTIC_BYTES);
        let definition = self.diagnostic_code().definition();
        let suggestion = self.arithmetic_suggestion(predicate, position);
        let mut accounted = vec![
            "phase",
            definition.phase,
            "condition",
            definition.condition,
            "requirement",
            definition.requirement.unwrap_or(""),
            "spec_paths",
            path,
            "context",
            "predicate",
            predicate,
            "position",
            "01234567890123456789",
        ];
        if let Some(suggestion) = suggestion {
            accounted.push("suggestion");
            accounted.push(suggestion);
        }
        for value in accounted {
            let required = value
                .len()
                .checked_mul(12)
                .and_then(|n| n.checked_add(128))
                .ok_or(OutputLimit)?;
            remaining = remaining.checked_sub(required).ok_or(OutputLimit)?;
        }
        let mut context: Context = [
            ("predicate".into(), text(predicate)),
            ("position".into(), V::Integer(position.to_string())),
        ]
        .into();
        if let Some(suggestion) = suggestion {
            context.insert("suggestion".into(), text(suggestion));
        }
        Ok(Diagnostic {
            code: self.diagnostic_code(),
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        })
    }
    /// Standalone syntax retains parser-only details without a fabricated
    /// specification path, operand route or expression field.
    pub fn syntax_diagnostic(&self) -> Diagnostic {
        let mut context = Context::new();
        match self {
            Self::InvalidExpression | Self::InvalidEscape | Self::IntegerOverflow => {}
            Self::InvalidRegex { byte, reason } => {
                context.insert("pattern_byte".into(), V::Integer(byte.to_string()));
                context.insert("reason".into(), text(reason));
            }
            Self::InvalidTemporal { kind, error } => {
                use crate::{predicate_parser::TemporalKind, temporal::TemporalError};
                context.insert(
                    "literal_type".into(),
                    text(match kind {
                        TemporalKind::Date => "date",
                        TemporalKind::DateTime => "datetime",
                    }),
                );
                context.insert(
                    "temporal_error".into(),
                    text(match error {
                        TemporalError::InvalidForm => "invalid_form",
                        TemporalError::InvalidDate => "invalid_date",
                        TemporalError::InvalidTime => "invalid_time",
                    }),
                );
            }
        }
        Diagnostic {
            code: self.diagnostic_code(),
            spec_paths: vec![],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
