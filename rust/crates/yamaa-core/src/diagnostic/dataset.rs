//! Classified dataset failures, borrowed without replaying table or host effects.
use super::{ConditionCode as C, Context, Diagnostic, ProjectionError};
use crate::json::Value as JsonValue;
use crate::{
    reduction::NumericReducer,
    value::{Value, ValueType},
};

/// Facts retained by a failed dataset attempt. Identity, matched keys and
/// partition observations remain attached to that attempt outside this finding.
#[derive(Clone, Copy, Debug)]
pub enum DatasetFinding<'a> {
    MultipleValues {
        path: &'a str,
        identifier: &'a str,
        count: usize,
    },
    UnmatchedKey {
        path: &'a str,
        dataset: &'a str,
        intermediate: &'a str,
    },
    MultipleMatches {
        path: &'a str,
        dataset: &'a str,
        intermediate: &'a str,
        count: usize,
    },
    BaselineAmbiguity {
        path: &'a str,
        column: &'a str,
        date: &'a Value,
        count: usize,
    },
    ReductionInputType {
        path: &'a str,
        expression: &'a str,
        reducer: NumericReducer,
        source: &'a str,
        actual: ValueType,
    },
}

fn text(value: &str) -> JsonValue {
    JsonValue::String(value.into())
}
fn count(value: usize) -> JsonValue {
    match i64::try_from(value) {
        Ok(value) => JsonValue::from(value),
        Err(_) => JsonValue::from(value),
    }
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

/// Static admission and raw runtime key comparisons retain the same authored cause.
pub(crate) fn lookup_key_type(
    path: &str,
    intermediate: &str,
    source: &str,
    expected: crate::result_type::ResultType,
    actual: crate::result_type::ResultType,
) -> Diagnostic {
    Diagnostic {
        code: C::LookupKeyType,
        spec_paths: vec![path.into()],
        context: [
            ("intermediate".into(), text(intermediate)),
            ("source".into(), text(source)),
            ("expected".into(), text(&expected.name())),
            ("actual".into(), text(&actual.name())),
        ]
        .into(),
        source_span: None,
        operand_route: None,
    }
}
impl DatasetFinding<'_> {
    /// Preserve complete facts and refuse contradictory classified causes.
    pub fn diagnostic(self) -> Result<Diagnostic, ProjectionError> {
        let (path, code) = match self {
            Self::MultipleValues { path, count, .. } => {
                if count < 2 {
                    return Err(ProjectionError::InvalidContext);
                }
                (path, C::DatasetMultipleValues)
            }
            Self::UnmatchedKey { path, .. } => (path, C::DatasetUnmatchedKey),
            Self::MultipleMatches { path, count, .. } => {
                if count < 2 {
                    return Err(ProjectionError::InvalidContext);
                }
                (path, C::DatasetMultipleMatches)
            }
            Self::BaselineAmbiguity {
                path, date, count, ..
            } => {
                if count < 2 || !matches!(date, Value::Date(_) | Value::DateTime(_)) {
                    return Err(ProjectionError::InvalidContext);
                }
                (path, C::DatasetBaselineAmbiguity)
            }
            Self::ReductionInputType { path, actual, .. } => {
                if matches!(actual, ValueType::Int | ValueType::Float) {
                    return Err(ProjectionError::InvalidContext);
                }
                (path, C::DatasetReductionInputType)
            }
        };
        let context: Context = match self {
            Self::MultipleValues {
                identifier,
                count: number,
                ..
            } => [
                ("identifier".into(), text(identifier)),
                ("value_count".into(), count(number)),
            ]
            .into(),
            Self::UnmatchedKey {
                dataset,
                intermediate,
                ..
            } => [
                ("dataset".into(), text(dataset)),
                ("intermediate".into(), text(intermediate)),
            ]
            .into(),
            Self::MultipleMatches {
                dataset,
                intermediate,
                count: number,
                ..
            } => [
                ("dataset".into(), text(dataset)),
                ("intermediate".into(), text(intermediate)),
                ("match_count".into(), count(number)),
            ]
            .into(),
            Self::BaselineAmbiguity {
                column,
                date,
                count: number,
                ..
            } => {
                let date = match date {
                    Value::Date(value) => value.to_string(),
                    Value::DateTime(value) => value.to_string(),
                    _ => return Err(ProjectionError::InvalidContext),
                };
                [
                    ("column".into(), text(column)),
                    ("operation".into(), text("baseline_flag")),
                    ("date".into(), JsonValue::String(date)),
                    ("match_count".into(), count(number)),
                ]
                .into()
            }
            Self::ReductionInputType {
                expression,
                reducer,
                source,
                actual,
                ..
            } => [
                ("expr".into(), text(expression)),
                ("reducer".into(), text(reducer.name())),
                ("source".into(), text(source)),
                ("expected".into(), text("numeric")),
                ("actual".into(), text(type_name(actual))),
            ]
            .into(),
        };
        Ok(Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        })
    }
}
