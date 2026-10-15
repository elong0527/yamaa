//! Declarative rounding retains the source field rather than inventing formula text.
use crate::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    numeric::{self, Number},
    table::ValueRef,
    value::Value,
};
use alloc::{format, string::String, vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidDigits;

/// An integer digits declaration and the original operand's diagnostic geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rounding {
    source: String,
    path: String,
    digits: i64,
}
impl Rounding {
    /// Admit integer spelling before any source resolution. Arbitrary-width
    /// digits retain the same binary64 result as the corresponding i64 extreme;
    /// only the arithmetic bound is saturated, never an input value or identity.
    pub fn new(source: &str, path: &str, digits: &str) -> Result<Self, InvalidDigits> {
        let magnitude = digits
            .strip_prefix('-')
            .or_else(|| digits.strip_prefix('+'))
            .unwrap_or(digits);
        if magnitude.is_empty() || !magnitude.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InvalidDigits);
        }
        let digits = digits.parse().unwrap_or_else(|_| {
            if digits.starts_with('-') {
                i64::MIN
            } else {
                i64::MAX
            }
        });
        Ok(Self {
            source: source.into(),
            path: path.into(),
            digits,
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub(crate) fn input_diagnostic(source: &str, path: &str, actual: &str) -> Diagnostic {
        Diagnostic {
            code: ConditionCode::DeclarativeRoundingInput,
            spec_paths: vec![format!("{path}.source")],
            context: [
                ("source", source),
                ("expected", "numeric"),
                ("actual", actual),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), ContextValue::Scalar(Value::Str(value.into()))))
            .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    /// Missing precedes source-type checking. Present values are never converted
    /// implicitly; the enclosing assignment converts only the rounded result.
    pub fn apply(&self, value: ValueRef<'_>) -> Result<Value, Diagnostic> {
        let number = match value {
            ValueRef::Missing => Number::Missing,
            ValueRef::Int(value) => Number::Int(value),
            ValueRef::Float(value) => Number::Float(value),
            other => {
                let actual = match other {
                    ValueRef::Str(_) => "str",
                    ValueRef::Bool(_) => "bool",
                    ValueRef::Date(_) => "date",
                    ValueRef::DateTime(_) => "datetime",
                    _ => unreachable!("numeric and missing values handled above"),
                };
                return Err(Self::input_diagnostic(&self.source, &self.path, actual));
            }
        };
        Ok(Value::from(numeric::round_integer_places(
            number,
            self.digits,
        )))
    }
}
