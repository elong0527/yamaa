//! Pure column checks over completed values; effect order belongs to the engine.
use crate::json::Value as JsonValue;
use crate::{
    dataset::Check,
    diagnostic::{ConditionCode, Definition, Diagnostic},
    value::{compare_present, Value},
};
use std::vec;

/// Full offending positions retain row order; hosts may bound their displayed keys.
pub struct NotMissingResult {
    pub offending_rows: Vec<usize>,
}

pub fn not_missing<'a>(values: impl IntoIterator<Item = &'a Value>) -> NotMissingResult {
    NotMissingResult {
        offending_rows: column_offenders(&Check::NOT_MISSING, values),
    }
}

/// Preserve core-owned vocabulary and base context without inventing host keys.
pub(crate) fn not_missing_diagnostic(
    path: String,
    column: String,
    failures: usize,
) -> Option<Diagnostic> {
    column_diagnostic(&Check::NOT_MISSING, path, column, failures)
}

impl NotMissingResult {
    pub fn diagnostic(&self, path: String, column: String) -> Option<Diagnostic> {
        not_missing_diagnostic(path, column, self.offending_rows.len())
    }
}

/// The admitted check retains its semantic cause through every engine record.
pub fn column_code(check: &Check) -> Option<ConditionCode> {
    Some(match check {
        Check::NotMissing { .. } => ConditionCode::VerificationNotMissingFailed,
        Check::Mandatory(_) => ConditionCode::SubmissionMandatoryFailed,
        Check::DataType { .. } => ConditionCode::SubmissionDataTypeFailed,
        Check::AllowedValues(_) => ConditionCode::VerificationAllowedValuesFailed,
        Check::Codelist { .. } | Check::ValueCodelists { .. } => {
            ConditionCode::VerificationCodelistFailed
        }
        Check::Inventory { .. } => ConditionCode::SubmissionInventoryFailed,
        Check::Range { .. } => ConditionCode::VerificationRangeFailed,
        Check::MaxLength(_) | Check::WideMaxLength(_) | Check::ValueMaxLength { .. } => {
            ConditionCode::VerificationMaxLengthFailed
        }
        Check::Matches(_) => ConditionCode::VerificationMatchesFailed,
        Check::ValueEntries(_) => ConditionCode::ValueLevelMultipleMatches,
        Check::ValueEntryMatch { .. } => ConditionCode::ValueLevelUnmatched,
        _ => return None,
    })
}

pub fn column_definition(check: &Check) -> Option<Definition> {
    column_code(check).map(ConditionCode::definition)
}

/// Missing values pass; every other value must contain a match.
pub fn matches_offenders<'a>(
    pattern: &crate::portable_regex::Pattern,
    values: impl IntoIterator<Item = &'a Value>,
) -> Vec<usize> {
    values
        .into_iter()
        .enumerate()
        .filter(|(_, value)| matches!(value, Value::Str(text) if !pattern.is_match(text)))
        .map(|(index, _)| index)
        .collect()
}

/// Pattern syntax findings retain only the authored pattern, without parser geometry.
pub fn invalid_pattern_diagnostic(path: String, pattern: String) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::RegexInvalidPattern,
        spec_paths: vec![path],
        context: [("pattern".into(), JsonValue::String(pattern))].into(),
        source_span: None,
        operand_route: None,
    }
}

/// Project only the compiler's closed verification declaration vocabulary.
pub fn declaration_diagnostic(
    path: String,
    code: ConditionCode,
    reason: String,
) -> Option<Diagnostic> {
    if !matches!(
        code,
        ConditionCode::VerificationIdentifierType
            | ConditionCode::VerificationDeclarationRequired
            | ConditionCode::VerificationIdentifierRepeated
            | ConditionCode::VerificationGrouping
            | ConditionCode::VerificationBounds
            | ConditionCode::VerificationLengthBounds
            | ConditionCode::VerificationCardinality
            | ConditionCode::VerificationColumnType
            | ConditionCode::VerificationDeclaredColumnUnknown
    ) {
        return None;
    }
    Some(Diagnostic {
        code,
        spec_paths: vec![path],
        context: [("reason".into(), JsonValue::String(reason))].into(),
        source_span: None,
        operand_route: None,
    })
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
                Check::NotMissing { .. } | Check::Mandatory(_) => matches!(value, Value::Missing),
                _ if matches!(value, Value::Missing) => false,
                Check::DataType { form, .. } => {
                    matches!(value, Value::Str(text) if !form.admits(text))
                }
                Check::AllowedValues(accepted)
                | Check::Inventory {
                    values: accepted, ..
                } => !accepted.contains(value),
                Check::Codelist { values, .. } => !values
                    .iter()
                    .any(|code| compare_present(value, code) == Ok(std::cmp::Ordering::Equal)),
                Check::Range { min, max } => {
                    min.as_ref().is_some_and(|min| {
                        compare_present(value, min) == Ok(std::cmp::Ordering::Less)
                    }) || max.as_ref().is_some_and(|max| {
                        compare_present(value, max) == Ok(std::cmp::Ordering::Greater)
                    })
                }
                Check::ValueMaxLength { max, .. } => matches!(value, Value::Str(text) if max.compare_count(text.chars().count()) == std::cmp::Ordering::Less),
                Check::MaxLength(max) => {
                    matches!(value,Value::Str(text) if text.chars().count()>*max)
                }
                Check::WideMaxLength(max) => {
                    matches!(value, Value::Str(text) if max.compare_count(text.chars().count()) == std::cmp::Ordering::Less)
                }
                _ => unreachable!("only admitted column checks reach the value service"),
            };
            failed.then_some(row)
        })
        .collect()
}

/// Exact fixed-list equality for inherited and per-value bindings alike.
pub fn codelist_accepts(values: &[Value], value: &Value) -> bool {
    matches!(value, Value::Missing)
        || values
            .iter()
            .any(|code| compare_present(value, code) == Ok(std::cmp::Ordering::Equal))
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
        ("column".into(), JsonValue::String(column)),
        ("failure_count".into(), JsonValue::from(failures)),
    ]
    .into();
    if let Check::MaxLength(max) = check {
        context.insert("max".into(), JsonValue::from(*max));
    }
    if let Check::WideMaxLength(max) | Check::ValueMaxLength { max, .. } = check {
        context.insert(
            "max".into(),
            JsonValue::Number((max.decimal()).parse().expect("canonical integer")),
        );
    }
    if let Check::Codelist { id, .. } = check {
        context.insert("codelist".into(), JsonValue::String(id.clone()));
    }
    if let Check::DataType { form, .. } = check {
        context.insert("data_type".into(), JsonValue::String(form.name().into()));
    }
    // An entry that selects no row names itself, as a run's check does (SUB-13).
    if let Check::ValueEntryMatch { id, .. } = check {
        context.insert("entry".into(), JsonValue::String(id.clone()));
    }
    Some(Diagnostic {
        code,
        spec_paths: vec![path],
        context,
        source_span: None,
        operand_route: None,
    })
}
