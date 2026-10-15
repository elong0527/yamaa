//! Classified dataset failures, borrowed without replaying table or host effects.
use super::{
    ConditionCode as C, Context, ContextValue, Diagnostic, ProjectionBudget, ProjectionError,
    MAX_PROJECTION_BYTES,
};
use crate::{
    reduction::NumericReducer,
    value::{Value, ValueType},
};
use alloc::{string::ToString, vec};

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

fn text(value: &str) -> ContextValue {
    ContextValue::Scalar(Value::Str(value.into()))
}
fn count(value: usize) -> ContextValue {
    match i64::try_from(value) {
        Ok(value) => ContextValue::Scalar(Value::Int(value)),
        Err(_) => ContextValue::Integer(value.to_string()),
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
impl DatasetFinding<'_> {
    pub fn diagnostic(self) -> Option<Diagnostic> {
        self.diagnostic_with_limit(MAX_PROJECTION_BYTES).ok()
    }

    /// Admit the complete path and context before allocating any owned fact.
    /// Invalid typed causes and resource refusal preserve the original attempt.
    pub fn diagnostic_with_limit(self, maximum: usize) -> Result<Diagnostic, ProjectionError> {
        let mut budget = ProjectionBudget::new(maximum)?;
        let (path, code) = match self {
            Self::MultipleValues {
                path,
                identifier,
                count,
            } => {
                if count < 2 {
                    return Err(ProjectionError::InvalidContext);
                }
                budget.text(path)?;
                budget.field("identifier", identifier)?;
                budget.text("value_count")?;
                budget.charge(32)?;
                (path, C::DatasetMultipleValues)
            }
            Self::UnmatchedKey {
                path,
                dataset,
                intermediate,
            } => {
                budget.text(path)?;
                budget.field("dataset", dataset)?;
                budget.field("intermediate", intermediate)?;
                (path, C::DatasetUnmatchedKey)
            }
            Self::MultipleMatches {
                path,
                dataset,
                intermediate,
                count,
            } => {
                if count < 2 {
                    return Err(ProjectionError::InvalidContext);
                }
                budget.text(path)?;
                budget.field("dataset", dataset)?;
                budget.field("intermediate", intermediate)?;
                budget.text("match_count")?;
                budget.charge(32)?;
                (path, C::DatasetMultipleMatches)
            }
            Self::BaselineAmbiguity {
                path,
                column,
                date,
                count,
            } => {
                if count < 2 || !matches!(date, Value::Date(_) | Value::DateTime(_)) {
                    return Err(ProjectionError::InvalidContext);
                }
                budget.text(path)?;
                budget.field("column", column)?;
                budget.field("operation", "baseline_flag")?;
                budget.text("date")?;
                // Every admitted temporal spelling fits this fixed capacity;
                // formatting remains after the whole-context admission.
                budget.charge(128)?;
                budget.text("match_count")?;
                budget.charge(32)?;
                (path, C::DatasetBaselineAmbiguity)
            }
            Self::ReductionInputType {
                path,
                expression,
                reducer,
                source,
                actual,
            } => {
                if matches!(actual, ValueType::Int | ValueType::Float) {
                    return Err(ProjectionError::InvalidContext);
                }
                budget.text(path)?;
                budget.field("expr", expression)?;
                budget.field("reducer", reducer.name())?;
                budget.field("source", source)?;
                budget.field("expected", "numeric")?;
                budget.field("actual", type_name(actual))?;
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
                    ("date".into(), ContextValue::Scalar(Value::Str(date))),
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
