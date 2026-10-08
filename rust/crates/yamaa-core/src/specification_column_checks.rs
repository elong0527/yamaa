//! Static column-check payload admission with findings deferred to column execution.
use super::*;
use crate::{
    conversion::convert,
    dataset::Check,
    value::{compare_present, Value},
};

pub(super) fn prepare(
    d: &Document,
    op: &str,
    payload: usize,
    path: &str,
    kind: ColumnType,
) -> Result<Check, PrepareError> {
    let invalid = |requirement, reason: String| Check::InvalidDeclaration {
        condition: "invalid_declaration",
        requirement,
        reason,
    };
    let kind_name = match kind {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    };
    let wrong_type = || {
        invalid(
            "REQ-0404",
            format!("a {kind_name} column does not admit this verification"),
        )
    };
    match op {
        "not_missing" => Ok(Check::NotMissing),
        "allowed_values" => {
            let entries = sequence(d, field(d, payload, "values")?)?;
            if entries.len() > 64 {
                return Err(PrepareError::Limit("verification_values"));
            }
            if entries.is_empty() {
                return Ok(invalid("REQ-0397", "allowed_values requires values".into()));
            }
            let mut accepted = Vec::new();
            let mut finding = None;
            let mut quote_budget = crate::schema::ValidationBudget::new(Default::default());
            for (index, &id) in entries.iter().enumerate() {
                let item_path = format!("{path}.values[{index}]");
                let value = literal(d, id, &item_path)?;
                match convert(&value, kind) {
                    Ok(value) if !matches!(value, Value::Missing) => accepted.push(value),
                    _ => {
                        let label = match &d.nodes()[id] {
                            N::Text(text) => {
                                crate::schema::quoted_diagnostic_text(text, &mut quote_budget)
                                    .map_err(|_| PrepareError::Limit("verification_diagnostic"))?
                            }
                            node => {
                                crate::schema::scalar_diagnostic_label(node).ok_or_else(|| {
                                    PrepareError::Unsupported(vec![UnsupportedFeature {
                                        operation: "verification_literal_diagnostic".into(),
                                        path: item_path.clone(),
                                    }])
                                })?
                            }
                        };
                        if finding.is_none() {
                            finding = Some(invalid(
                                "REQ-0404",
                                format!("listed value {label} is not a {kind_name} value"),
                            ));
                        }
                    }
                }
            }
            Ok(finding.unwrap_or(Check::AllowedValues(accepted)))
        }
        "range" => {
            // Scan both raw bounds even if the column kind or earlier bound is invalid.
            let bound = |name| -> Result<Option<Value>, PrepareError> {
                let Some(id) = d
                    .field(payload, name)
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                else {
                    return Ok(None);
                };
                let value = literal(d, id, &format!("{path}.{name}"))?;
                if !matches!(value, Value::Int(_) | Value::Float(_)) {
                    return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
                        operation: "verification_numeric_bound".into(),
                        path: format!("{path}.{name}"),
                    }]));
                }
                Ok(Some(value))
            };
            let min = bound("min")?;
            let max = bound("max")?;
            if !matches!(kind, ColumnType::Int | ColumnType::Float) {
                return Ok(wrong_type());
            }
            if min.is_none() && max.is_none() {
                return Ok(invalid("REQ-0399", "range requires one bound".into()));
            }
            if min.as_ref().zip(max.as_ref()).is_some_and(|(min, max)| {
                compare_present(min, max) == Ok(core::cmp::Ordering::Greater)
            }) {
                return Ok(invalid("REQ-0399", "range min exceeds max".into()));
            }
            Ok(Check::Range { min, max })
        }
        "max_length" => {
            let max = literal(d, field(d, payload, "max")?, &format!("{path}.max"))?;
            if kind != ColumnType::Str {
                return Ok(wrong_type());
            }
            let Value::Int(max) = max else {
                return Err(PrepareError::Internal);
            };
            if max < 1 {
                return Ok(invalid("REQ-0400", "max_length max is at least one".into()));
            }
            Ok(Check::MaxLength(
                usize::try_from(max).map_err(|_| PrepareError::Limit("verification_length"))?,
            ))
        }
        _ => Err(PrepareError::Internal),
    }
}
