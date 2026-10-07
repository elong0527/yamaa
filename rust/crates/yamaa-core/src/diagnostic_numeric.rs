//! Construct diagnostic context at the numeric/conversion semantic boundary.

use super::{ConditionCode, Context, ContextValue, Diagnostic};
use crate::{
    conversion::{ConversionError, ConversionReason, ConversionValue},
    evaluation::{EvaluationErrorKind, NumericCondition, Operand},
    numeric::{ArithmeticError, ArithmeticErrorKind},
    numeric_compiler::CompiledEvaluationError,
    value::{ColumnType, Value, ValueType},
};
use alloc::{format, string::String, string::ToString, vec};

fn scalar(value: Value) -> ContextValue {
    ContextValue::Scalar(value)
}
fn text(value: impl Into<String>) -> ContextValue {
    scalar(Value::Str(value.into()))
}
fn type_name(value: ValueType) -> &'static str {
    match value {
        ValueType::Str => "str",
        ValueType::Int => "int",
        ValueType::Float => "float",
        ValueType::Bool => "bool",
        ValueType::Date => "date",
        ValueType::DateTime => "datetime",
    }
}
fn target_name(value: ColumnType) -> &'static str {
    match value {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    }
}

impl ConversionError {
    pub fn diagnostic_code(&self) -> ConditionCode {
        match self.reason {
            ConversionReason::IncompatibleOrInvalidText => ConditionCode::ConversionInput,
            ConversionReason::IntegerRangeOrFraction => ConditionCode::ConversionInteger,
            ConversionReason::InvalidTemporalText => ConditionCode::ConversionTemporal,
        }
    }

    /// Preserve parsed source type and wide integers without selecting a handler.
    pub fn into_diagnostic(self, spec_path: String) -> Diagnostic {
        let code = self.diagnostic_code();
        let context = Context::from([
            (
                "from".into(),
                text(self.source_type().map_or("missing", type_name)),
            ),
            ("to".into(), text(target_name(self.target))),
            (
                "value".into(),
                match self.value {
                    ConversionValue::Runtime(value) => scalar(value),
                    ConversionValue::Integer(integer) => ContextValue::Integer(integer),
                },
            ),
        ]);
        Diagnostic {
            code,
            spec_paths: vec![spec_path],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}

impl ArithmeticErrorKind {
    pub fn diagnostic_code(&self) -> ConditionCode {
        match self {
            Self::IntegerOverflow { .. } => ConditionCode::IntegerOverflow,
            Self::DivisionByZero => ConditionCode::DivisionByZero,
            Self::SqrtOfNegative => ConditionCode::SqrtOfNegative,
            Self::LnOfNonpositive => ConditionCode::LnOfNonpositive,
            Self::InvalidRoundingDigits => ConditionCode::RoundingDigitsType,
            Self::InvalidPower { .. } => ConditionCode::InvalidPower,
        }
    }
}

impl NumericCondition {
    pub fn diagnostic_code(&self) -> ConditionCode {
        match self {
            Self::UnknownField { .. } => ConditionCode::NumericUnknownField,
            Self::IncompatibleInput { .. } => ConditionCode::NumericInputType,
            Self::LiteralOverflow { .. } => ConditionCode::IntegerOverflow,
            Self::Arithmetic(kind) => kind.diagnostic_code(),
        }
    }

    fn diagnostic_context(&self, expression: &str) -> Context {
        let mut context = Context::from([("expr".into(), text(expression))]);
        match self {
            Self::UnknownField { identifier } => {
                context.insert("identifier".into(), text(identifier));
            }
            Self::IncompatibleInput { identifier, actual } => {
                context.insert("source".into(), text(identifier));
                context.insert("expected".into(), text("numeric"));
                context.insert("actual".into(), text(type_name(*actual)));
            }
            Self::Arithmetic(ArithmeticErrorKind::InvalidRoundingDigits) => {
                context.insert("expected".into(), text("int"));
                context.insert("actual".into(), text("float"));
            }
            Self::Arithmetic(ArithmeticErrorKind::InvalidPower {
                base_bits,
                exponent_bits,
            }) => {
                context.insert(
                    "base".into(),
                    scalar(Value::float(f64::from_bits(*base_bits))),
                );
                context.insert(
                    "exponent".into(),
                    scalar(Value::float(f64::from_bits(*exponent_bits))),
                );
            }
            Self::Arithmetic(ArithmeticErrorKind::IntegerOverflow { value }) => {
                context.insert("value".into(), text(value.to_string()));
                context.insert("minimum".into(), scalar(Value::Int(i64::MIN)));
                context.insert("maximum".into(), scalar(Value::Int(i64::MAX)));
            }
            Self::LiteralOverflow { value } => {
                context.insert("value".into(), text(value));
                context.insert("minimum".into(), scalar(Value::Int(i64::MIN)));
                context.insert("maximum".into(), scalar(Value::Int(i64::MAX)));
            }
            Self::Arithmetic(
                ArithmeticErrorKind::DivisionByZero
                | ArithmeticErrorKind::SqrtOfNegative
                | ArithmeticErrorKind::LnOfNonpositive,
            ) => {}
        }
        context
    }
}

impl ArithmeticError {
    /// Reductions use the same context as compiled numeric execution, without
    /// inventing a source span for an already-bound arithmetic operation.
    pub fn into_diagnostic(self, spec_path: String) -> Diagnostic {
        Diagnostic {
            code: self.kind.diagnostic_code(),
            spec_paths: vec![spec_path],
            context: NumericCondition::Arithmetic(self.kind).diagnostic_context(&self.expression),
            source_span: None,
            operand_route: None,
        }
    }
}

impl<E> CompiledEvaluationError<E> {
    /// Borrow an opaque resolver error unchanged; only core-owned failures have a
    /// normative diagnostic. A caller must retain its original boundary failure.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        let EvaluationErrorKind::Numeric(condition) = &self.evaluation.kind else {
            return None;
        };
        Some(Diagnostic {
            code: condition.diagnostic_code(),
            spec_paths: vec![self.evaluation.location.spec_path.clone()],
            context: condition.diagnostic_context(&self.evaluation.location.expression),
            source_span: Some(self.source_span),
            operand_route: Some(
                self.evaluation
                    .location
                    .operands
                    .iter()
                    .map(|operand| match operand {
                        Operand::Unary => "unary".into(),
                        Operand::Left => "left".into(),
                        Operand::Right => "right".into(),
                        Operand::Argument(index) => format!("argument:{index}"),
                    })
                    .collect(),
            ),
        })
    }
}
