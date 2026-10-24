//! Expression findings: the parser's and binder's, static types and evaluation.
//! Each keeps the original spelling: a field's text is its `expr`, and a
//! predicate field also names it under `predicate`, with the character
//! position.
use super::{ConditionCode as C, Context, Diagnostic, ProjectionError};
use crate::expression::{
    Condition, Context as Field, ErrorKind, EvaluationError, Failure, Step, TemporalKind,
    TypeFinding,
};
use crate::json::Value as V;
use crate::numeric::{ArithmeticErrorKind, MathFunction};
use crate::temporal::TemporalError;
use crate::value::{Value, ValueType};

fn text(value: &str) -> V {
    V::String(value.into())
}

fn type_name(kind: ValueType) -> &'static str {
    match kind {
        ValueType::Str => "str",
        ValueType::Int => "int",
        ValueType::Float => "float",
        ValueType::Bool => "bool",
        ValueType::Date => "date",
        ValueType::DateTime => "datetime",
    }
}

/// A written integer literal's value: its sign without `+`, and its digits
/// without leading zeros.
fn canonical_integer(literal: &str) -> String {
    let (sign, digits) = match literal.as_bytes().first() {
        Some(b'-') => ("-", &literal[1..]),
        Some(b'+') => ("", &literal[1..]),
        _ => ("", literal),
    };
    match digits.trim_start_matches('0') {
        "" => "0".into(),
        digits => format!("{sign}{digits}"),
    }
}

impl Failure {
    pub fn diagnostic_code(&self) -> C {
        match self {
            Self::Syntax | Self::InvalidTemporal { .. } => C::ExpressionInvalid,
            Self::InvalidEscape => C::ExpressionInvalidEscape,
            Self::InvalidRegex { .. } => C::ExpressionInvalidRegex,
            Self::PatternNotLiteral => C::ExpressionPatternNotLiteral,
            Self::IntegerOverflow { .. } => C::LiteralIntegerOverflow,
            Self::ProhibitedConstruct { .. } => C::ExpressionProhibitedConstruct,
            Self::ProhibitedFunction { .. } => C::ExpressionProhibitedFunction,
            Self::NestedReduction { .. } => C::ExpressionNestedReduction,
        }
    }
    /// The registered condition, independent of diagnostic rendering.
    pub fn condition(&self) -> &'static str {
        self.diagnostic_code().definition().condition
    }
    /// The rule owning the finding.
    pub fn rule(&self) -> &'static str {
        self.diagnostic_code()
            .definition()
            .rule
            .expect("expression findings have a normative rule")
    }
    /// The construct, function or reducers a finding names.
    fn facts(&self, context: &mut Context) {
        match self {
            Self::ProhibitedConstruct { construct } => {
                context.insert("construct".into(), text(construct.name()));
            }
            Self::ProhibitedFunction {
                function,
                argument_count,
                ..
            } => {
                context.insert("function".into(), text(function));
                if let Some(count) = argument_count {
                    context.insert("argument_count".into(), V::from(*count));
                } else {
                    let hint = match function.to_ascii_uppercase().as_str() {
                        "COALESCE" => Some("use the first_available expression"),
                        "GREATEST" => Some("use the greatest expression"),
                        "LEAST" => Some("use the least expression"),
                        "ROUND" | "ROUND_HALF_AWAY_FROM_ZERO" => {
                            Some("use the round_half_away_from_zero operation")
                        }
                        _ => None,
                    };
                    if let Some(hint) = hint {
                        context.insert("hint".into(), text(hint));
                    }
                }
            }
            Self::NestedReduction { outer, inner } => {
                context.insert("outer".into(), text(outer.name()));
                context.insert("inner".into(), text(inner.name()));
            }
            // The literal and the range it leaves, as a specification's
            // scalar literal reports them (VALUE-5).
            Self::IntegerOverflow { literal } => {
                if let Ok(number) = canonical_integer(literal).parse::<crate::json::Number>() {
                    context.insert("actual".into(), V::Number(number));
                }
                context.insert("value".into(), text(literal));
                context.insert("minimum".into(), V::from(i64::MIN));
                context.insert("maximum".into(), V::from(i64::MAX));
            }
            Self::Syntax
            | Self::InvalidTemporal { .. }
            | Self::InvalidEscape
            | Self::InvalidRegex { .. }
            | Self::PatternNotLiteral => {}
        }
    }
    /// A specification finding: the field's text, a predicate's character
    /// position, and the construct, function or reducers the finding names.
    pub fn diagnostic(
        &self,
        field: Field,
        path: &str,
        expression: &str,
        position: usize,
    ) -> Diagnostic {
        let mut context = Context::new();
        context.insert("expr".into(), text(expression));
        if field == Field::Predicate {
            context.insert("predicate".into(), text(expression));
            context.insert("position".into(), V::from(position));
        }
        self.facts(&mut context);
        Diagnostic {
            code: self.diagnostic_code(),
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        }
    }
    /// Standalone syntax facts, without a specification path or field text:
    /// a rejected regex's byte and reason, or a temporal literal's cause.
    pub fn syntax_diagnostic(&self) -> Diagnostic {
        let mut context = Context::new();
        match self {
            Self::InvalidRegex { byte, reason } => {
                context.insert("pattern_byte".into(), V::from(*byte));
                context.insert("reason".into(), text(reason));
            }
            Self::InvalidTemporal { kind, error } => {
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
            _ => self.facts(&mut context),
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

impl Step {
    /// Truth nodes and arithmetic nodes keep their own route spellings.
    pub fn label(self) -> String {
        match self {
            Self::Child => "Child".into(),
            Self::Left => "Left".into(),
            Self::Right => "Right".into(),
            Self::Value => "Value".into(),
            Self::Item(index) => format!("Item({index})"),
            Self::Lower => "Lower".into(),
            Self::RepeatedValue => "RepeatedValue".into(),
            Self::Upper => "Upper".into(),
            Self::Pattern => "Pattern".into(),
            Self::Unary => "unary".into(),
            Self::LeftOperand => "left".into(),
            Self::RightOperand => "right".into(),
        }
    }
}

fn route(steps: &[Step]) -> Option<Vec<String>> {
    Some(steps.iter().map(|step| step.label()).collect())
}

impl Condition {
    pub fn diagnostic_code(&self) -> C {
        match self {
            Self::UnknownField { .. } => C::PredicateUnknownField,
            Self::IncompatiblePair { .. } => C::PredicateIncompatiblePair,
            Self::ExpectedText { .. } => C::PredicateExpectedText,
            Self::ContainsExpectedText { .. } => C::PredicateContainsExpectedText,
            Self::DanglingEscape => C::PredicateDanglingEscape,
            Self::NumericUnknownField { .. } => C::NumericUnknownField,
            Self::NumericInput { .. } => C::NumericInputType,
            Self::AggregateOperand { .. } => C::AggregateNumericOperand,
            Self::Arithmetic(kind) => kind.diagnostic_code(),
        }
    }
    /// A binding fault is a validation finding; an arithmetic one a derivation.
    pub fn phase(&self) -> &'static str {
        self.diagnostic_code().definition().phase
    }
    pub fn condition(&self) -> &'static str {
        self.diagnostic_code().definition().condition
    }
    pub fn rule(&self) -> &'static str {
        self.diagnostic_code()
            .definition()
            .rule
            .expect("expression conditions have a normative rule")
    }
    /// A comparison's facts name its operands' types; a numeric operand's
    /// facts carry the expression and the failing operation's values.
    pub(crate) fn diagnostic_context(&self, expression: &str) -> Context {
        let mut context = Context::new();
        let mut insert = |key: &str, value: V| {
            context.insert(key.into(), value);
        };
        match self {
            Self::UnknownField { identifier } => insert("identifier", text(identifier)),
            // The right operand's type is the one the comparison does not
            // accept beside the left one's (FORMULA-5).
            Self::IncompatiblePair { left, right } => {
                insert("left_type", text(type_name(*left)));
                insert("right_type", text(type_name(*right)));
                insert("actual", text(type_name(*right)));
            }
            Self::ExpectedText { actual } | Self::ContainsExpectedText { actual } => {
                insert("expected", text("str"));
                insert("actual", text(type_name(*actual)));
            }
            Self::DanglingEscape => {
                insert("expr", text(expression));
                insert("reason", text("LIKE pattern has a dangling escape"));
            }
            Self::NumericUnknownField { identifier } => {
                insert("expr", text(expression));
                insert("identifier", text(identifier));
            }
            Self::NumericInput { identifier, actual }
            | Self::AggregateOperand {
                operand: identifier,
                actual,
            } => {
                insert("expr", text(expression));
                insert("source", text(identifier));
                insert("expected", text("numeric"));
                insert("actual", text(type_name(*actual)));
            }
            Self::Arithmetic(kind) => {
                insert("expr", text(expression));
                match kind {
                    ArithmeticErrorKind::InvalidPower {
                        base_bits,
                        exponent_bits,
                    } => {
                        insert("base", V::from(Value::float(f64::from_bits(*base_bits))));
                        insert(
                            "exponent",
                            V::from(Value::float(f64::from_bits(*exponent_bits))),
                        );
                    }
                    ArithmeticErrorKind::IntegerOverflow { value } => {
                        insert("value", text(&value.to_string()));
                        insert("minimum", V::from(Value::Int(i64::MIN)));
                        insert("maximum", V::from(Value::Int(i64::MAX)));
                    }
                    ArithmeticErrorKind::LnOfNonpositive { function } => {
                        insert("function", text(function));
                    }
                    ArithmeticErrorKind::InvalidProbability { value_bits } => {
                        insert("function", text(MathFunction::NormInv.name()));
                        insert("value", V::from(Value::float(f64::from_bits(*value_bits))));
                    }
                    ArithmeticErrorKind::DivisionByZero | ArithmeticErrorKind::SqrtOfNegative => {}
                }
            }
        }
        context
    }
}

/// A resolver's own failure has no language diagnostic; it stays borrowed.
#[derive(Debug)]
pub enum EvaluationDiagnosticError<'a, E> {
    Resolution(&'a E),
}

impl<E: std::fmt::Debug> std::fmt::Display for EvaluationDiagnosticError<'_, E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "diagnostic projection: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for EvaluationDiagnosticError<'_, E> {}

impl<E> EvaluationError<E> {
    /// Keep the typed facts, the route and any arithmetic source extent.
    pub fn diagnostic(&self) -> Result<Diagnostic, EvaluationDiagnosticError<'_, E>> {
        let condition = match &self.kind {
            ErrorKind::Resolution { error, .. } => {
                return Err(EvaluationDiagnosticError::Resolution(error))
            }
            ErrorKind::Condition(condition) => condition,
        };
        Ok(Diagnostic {
            code: condition.diagnostic_code(),
            spec_paths: vec![self.spec_path.clone()],
            context: condition.diagnostic_context(&self.expression),
            source_span: self.source_span,
            operand_route: route(&self.route),
        })
    }
}

impl TypeFinding {
    /// Project a static fault at its site, before any value is read.
    pub fn diagnostic(&self, path: &str, expression: &str) -> Result<Diagnostic, ProjectionError> {
        let (condition, steps) = match self {
            Self::NumericOperand { source, actual } => (
                Condition::NumericInput {
                    identifier: source.clone(),
                    actual: *actual,
                },
                None,
            ),
            Self::AggregateOperand { source, actual } => (
                Condition::AggregateOperand {
                    operand: source.clone(),
                    actual: *actual,
                },
                None,
            ),
            Self::IncompatiblePair { left, right, route } => (
                Condition::IncompatiblePair {
                    left: *left,
                    right: *right,
                },
                Some(route),
            ),
            Self::ExpectedText { actual, route } => {
                (Condition::ExpectedText { actual: *actual }, Some(route))
            }
            Self::ContainsExpectedText { actual, route } => (
                Condition::ContainsExpectedText { actual: *actual },
                Some(route),
            ),
            Self::ReductionInput {
                reducer,
                source,
                actual,
            } => {
                return super::DatasetFinding::ReductionInputType {
                    path,
                    expression,
                    reducer: *reducer,
                    source,
                    actual: *actual,
                }
                .diagnostic()
            }
        };
        Ok(Diagnostic {
            code: condition.diagnostic_code(),
            spec_paths: vec![path.into()],
            context: condition.diagnostic_context(expression),
            source_span: None,
            operand_route: steps.and_then(|steps| route(steps)),
        })
    }
}
