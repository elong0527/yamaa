//! Pure column checks over completed values; effect order belongs to the engine.
use crate::{
    dataset::{Check, PlanError},
    diagnostic::{ConditionCode, ContextValue, Definition, Diagnostic},
    value::{compare_present, ColumnType, Value},
};
use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

/// Full offending positions retain row order; hosts may bound their displayed keys.
pub struct NotMissingResult {
    pub offending_rows: Vec<usize>,
}

pub fn not_missing<'a>(values: impl IntoIterator<Item = &'a Value>) -> NotMissingResult {
    NotMissingResult {
        offending_rows: column_offenders(&Check::NotMissing, values),
    }
}

pub fn not_missing_definition() -> Definition {
    column_definition(&Check::NotMissing).expect("presence check metadata")
}

/// Preserve core-owned vocabulary and base context without inventing host keys.
pub fn not_missing_diagnostic(path: String, column: String, failures: usize) -> Option<Diagnostic> {
    column_diagnostic(&Check::NotMissing, path, column, failures)
}

impl NotMissingResult {
    pub fn diagnostic(&self, path: String, column: String) -> Option<Diagnostic> {
        not_missing_diagnostic(path, column, self.offending_rows.len())
    }
}

/// Validate the already compiled column-check payload without reading values.
pub(crate) fn validate(check: &Check, kind: ColumnType) -> Result<(), PlanError> {
    match check {
        Check::AllowedValues(values) => {
            if values.is_empty()
                || values.iter().any(|value| {
                    !matches!(
                        (kind, value),
                        (ColumnType::Str, Value::Str(_))
                            | (ColumnType::Int, Value::Int(_))
                            | (ColumnType::Float, Value::Float(_))
                            | (ColumnType::Date, Value::Date(_))
                            | (ColumnType::DateTime, Value::DateTime(_))
                    )
                })
            {
                return Err(PlanError::InvalidColumns);
            }
        }
        Check::Range { min, max } => {
            if !matches!(kind, ColumnType::Int | ColumnType::Float)
                || (min.is_none() && max.is_none())
                || min
                    .iter()
                    .chain(max)
                    .any(|value| !matches!(value, Value::Int(_) | Value::Float(_)))
                || min.as_ref().zip(max.as_ref()).is_some_and(|(min, max)| {
                    compare_present(min, max) != Ok(core::cmp::Ordering::Less)
                        && compare_present(min, max) != Ok(core::cmp::Ordering::Equal)
                })
            {
                return Err(PlanError::InvalidBounds);
            }
        }
        Check::MaxLength(max) if kind != ColumnType::Str || *max == 0 => {
            return Err(PlanError::InvalidBounds)
        }
        _ => {}
    }
    Ok(())
}

fn column_code(check: &Check) -> Option<ConditionCode> {
    Some(match check {
        Check::NotMissing => ConditionCode::VerificationNotMissingFailed,
        Check::AllowedValues(_) => ConditionCode::VerificationAllowedValuesFailed,
        Check::Range { .. } => ConditionCode::VerificationRangeFailed,
        Check::MaxLength(_) => ConditionCode::VerificationMaxLengthFailed,
        _ => return None,
    })
}

pub fn column_definition(check: &Check) -> Option<Definition> {
    column_code(check).map(ConditionCode::definition)
}

/// Inputs and permitted values are already converted to the declared column kind.
pub fn column_offenders<'a>(
    check: &Check,
    values: impl IntoIterator<Item = &'a Value>,
) -> Vec<usize> {
    values
        .into_iter()
        .enumerate()
        .filter_map(|(row, value)| {
            let failed = match check {
                Check::NotMissing => matches!(value, Value::Missing),
                _ if matches!(value, Value::Missing) => false,
                Check::AllowedValues(accepted) => !accepted.contains(value),
                Check::Range { min, max } => {
                    min.as_ref().is_some_and(|min| {
                        compare_present(value, min) == Ok(core::cmp::Ordering::Less)
                    }) || max.as_ref().is_some_and(|max| {
                        compare_present(value, max) == Ok(core::cmp::Ordering::Greater)
                    })
                }
                Check::MaxLength(max) => {
                    matches!(value,Value::Str(text) if text.chars().count()>*max)
                }
                _ => unreachable!("only admitted column checks reach the value service"),
            };
            failed.then_some(row)
        })
        .collect()
}

pub fn column_diagnostic(
    check: &Check,
    path: String,
    column: String,
    failures: usize,
) -> Option<Diagnostic> {
    if failures == 0 {
        return None;
    }
    let code = column_code(check)?;
    let mut context: crate::diagnostic::Context = [
        ("column".into(), ContextValue::Scalar(Value::Str(column))),
        (
            "failure_count".into(),
            ContextValue::Integer(failures.to_string()),
        ),
    ]
    .into();
    if let Check::MaxLength(max) = check {
        context.insert("max".into(), ContextValue::Integer(max.to_string()));
    }
    Some(Diagnostic {
        code,
        spec_paths: vec![path],
        context,
        source_span: None,
        operand_route: None,
    })
}
