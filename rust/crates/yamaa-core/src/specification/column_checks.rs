//! Static column-check payload admission with findings deferred to column execution.
use super::*;
use crate::{
    ast::{ColumnCheck, ColumnCheckKind, Constant, Literal},
    conversion::convert,
    dataset::Check,
    diagnostic::ConditionCode,
    value::{compare_present, Value},
};

/// Read one written scalar, as the document's literal leaves are read.
fn constant(value: &Literal, path: &str) -> Result<Value, PrepareError> {
    Ok(match value {
        Literal::Missing => Value::Missing,
        Literal::Text(value) => Value::Str(value.clone()),
        Literal::Bool(value) => Value::Bool(*value),
        Literal::Float(value) => Value::float(*value),
        Literal::Int(value) => Value::Int(value.parse().map_err(|_| {
            PrepareError::Invalid(vec![PreflightFinding::IntegerLiteralOverflow {
                path: path.into(),
                value: value.clone(),
            }])
        })?),
        Literal::Date(_) | Literal::DateTime(_) | Literal::Unsupported => {
            return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "literal".into(),
                path: path.into(),
            }]))
        }
    })
}

/// The label a listed value takes in a finding: quoted text, or the scalar
/// as the schema's diagnostics write it.
fn label(value: &Literal) -> Option<String> {
    let scalar = match value {
        Literal::Text(text) => return Some(crate::schema::quoted_diagnostic_text(text)),
        Literal::Missing => N::Null,
        Literal::Bool(value) => N::Boolean(*value),
        Literal::Int(text) => N::Integer(text.clone()),
        Literal::Float(value) => N::Float(*value),
        Literal::Date(_) | Literal::DateTime(_) | Literal::Unsupported => return None,
    };
    crate::schema::scalar_diagnostic_label(&scalar)
}

/// A range bound as written: none when omitted, `Err` when not a number.
fn range_bound(
    bound: Option<&Constant>,
    path: &str,
) -> Result<Result<Option<Value>, ()>, PrepareError> {
    let Some(bound) = bound else {
        return Ok(Ok(None));
    };
    let value = constant(&bound.value, path)?;
    Ok(if matches!(value, Value::Int(_) | Value::Float(_)) {
        Ok(Some(value))
    } else {
        Err(())
    })
}

pub(super) fn prepare(
    check: &ColumnCheck,
    path: &str,
    kind: ColumnType,
) -> Result<Check, PrepareError> {
    let invalid = |code, reason: String| Check::InvalidDeclaration { code, reason };
    let kind_name = kind.name();
    let wrong_type = || {
        invalid(
            ConditionCode::VerificationColumnType,
            format!("a {kind_name} column does not admit this verification"),
        )
    };
    match &check.kind {
        ColumnCheckKind::NotMissing => Ok(Check::NotMissing),
        ColumnCheckKind::Matches(pattern) => {
            if kind != ColumnType::Str {
                return Ok(wrong_type());
            }
            match crate::portable_regex::Pattern::compile(&pattern.text) {
                Ok(pattern) => Ok(Check::Matches(pattern)),
                Err(crate::portable_regex::CompileError::Invalid { .. }) => Ok(
                    Check::InvalidDiagnostic(crate::dataset_checks::invalid_pattern_diagnostic(
                        format!("{path}.pattern"),
                        pattern.text.clone(),
                    )),
                ),
                Err(crate::portable_regex::CompileError::Limit { resource, limit }) => {
                    Err(PrepareError::limit(
                        compiler_limits::regex_resource(resource),
                        limit,
                        None,
                        Some(&format!("{path}.pattern")),
                    ))
                }
            }
        }
        ColumnCheckKind::AllowedValues(entries) => {
            if entries.is_empty() {
                return Ok(invalid(
                    ConditionCode::VerificationDeclarationRequired,
                    "allowed_values requires values".into(),
                ));
            }
            let mut accepted = Vec::new();
            let mut finding = None;
            for (index, entry) in entries.iter().enumerate() {
                let value = constant(&entry.value, &format!("{path}.values[{index}]"))?;
                match convert(&value, kind) {
                    Ok(value) if !matches!(value, Value::Missing) => accepted.push(value),
                    _ => {
                        // Admitted documents contain finite scalar leaves.
                        // Invalid carriers belong to schema admission, not
                        // an unimplemented verification operation.
                        let label = label(&entry.value).ok_or(PrepareError::Internal)?;
                        if finding.is_none() {
                            finding = Some(invalid(
                                ConditionCode::VerificationColumnType,
                                format!("listed value {label} is not a {kind_name} value"),
                            ));
                        }
                    }
                }
            }
            Ok(finding.unwrap_or(Check::AllowedValues(accepted)))
        }
        ColumnCheckKind::Range { min, max } => {
            // Scan both raw bounds even if the column kind or earlier bound is invalid.
            let min = range_bound(min.as_ref(), &format!("{path}.min"))?;
            let max = range_bound(max.as_ref(), &format!("{path}.max"))?;
            if !matches!(kind, ColumnType::Int | ColumnType::Float) {
                return Ok(wrong_type());
            }
            let (Ok(min), Ok(max)) = (min, max) else {
                return Ok(invalid(
                    ConditionCode::VerificationDeclarationRequired,
                    "a range bound is numeric".into(),
                ));
            };
            if min.is_none() && max.is_none() {
                return Ok(invalid(
                    ConditionCode::VerificationBounds,
                    "range requires one bound".into(),
                ));
            }
            if min.as_ref().zip(max.as_ref()).is_some_and(|(min, max)| {
                compare_present(min, max) == Ok(std::cmp::Ordering::Greater)
            }) {
                return Ok(invalid(
                    ConditionCode::VerificationBounds,
                    "range min exceeds max".into(),
                ));
            }
            Ok(Check::Range { min, max })
        }
        ColumnCheckKind::MaxLength(max) => {
            let max = constant(&max.value, &format!("{path}.max"))?;
            if kind != ColumnType::Str {
                return Ok(wrong_type());
            }
            let Value::Int(max) = max else {
                return Err(PrepareError::Internal);
            };
            if max < 1 {
                return Ok(invalid(
                    ConditionCode::VerificationLengthBounds,
                    "max_length max is at least one".into(),
                ));
            }
            Ok(Check::MaxLength(usize::try_from(max).map_err(|_| {
                PrepareError::limit("verification_length", usize::MAX, None, None)
            })?))
        }
        // A known operation whose payload is outside the language; an
        // unknown one was refused with the declaration vocabulary.
        ColumnCheckKind::Unsupported(_) => Err(PrepareError::Internal),
    }
}
