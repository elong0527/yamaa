//! Construct diagnostic context at the arithmetic and conversion boundary.

use super::{ConditionCode, Context, Diagnostic};
use crate::json::Value as JsonValue;
use crate::{
    conversion::{ConversionError, ConversionReason},
    numeric::{ArithmeticError, ArithmeticErrorKind},
    value::{ColumnType, Value, ValueType},
};

fn scalar(value: Value) -> JsonValue {
    JsonValue::from(value)
}
fn text(value: impl Into<String>) -> JsonValue {
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
            ("value".into(), self.value),
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
            Self::InvalidPower { .. } => ConditionCode::InvalidPower,
        }
    }
}

impl ArithmeticError {
    /// Reductions use the same context as compiled numeric execution, without
    /// inventing a source span for an already-bound arithmetic operation.
    pub fn into_diagnostic(self, spec_path: String) -> Diagnostic {
        Diagnostic {
            code: self.kind.diagnostic_code(),
            spec_paths: vec![spec_path],
            context: crate::expression::Condition::Arithmetic(self.kind)
                .diagnostic_context(&self.expression),
            source_span: None,
            operand_route: None,
        }
    }
}
