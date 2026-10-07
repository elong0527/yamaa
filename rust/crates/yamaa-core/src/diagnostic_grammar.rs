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

impl P {
    pub fn diagnostic_code(&self) -> C {
        match self {
            Self::InvalidExpression | Self::InvalidTemporal { .. } => C::PredicateInvalidExpression,
            Self::InvalidEscape => C::PredicateInvalidEscape,
            Self::InvalidRegex { .. } => C::PredicateInvalidRegex,
        }
    }
    /// Original predicate syntax owns character positions and temporal/regex detail.
    pub fn diagnostic(&self, path: &str, predicate: &str, position: usize) -> Diagnostic {
        let mut context = Context::new();
        context.insert("predicate".into(), text(predicate));
        context.insert("position".into(), V::Integer(position.to_string()));
        match self {
            Self::InvalidExpression | Self::InvalidEscape => {}
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
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
