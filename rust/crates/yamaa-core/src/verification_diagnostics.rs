//! Complete borrowed verification reports from retained engine observations.
//! Formatting never reads a table, rechecks a row, or narrows authored bounds.
use crate::{
    dataset::{Check, CheckRecord, RowIdentity, VerificationSeverity},
    diagnostic::ConditionCode,
    project_diagnostics::{Context, Diagnostic, Fact},
    specification::PreparedSpecification,
};
use std::vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Output,
    Verification,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Output => "output",
            Self::Verification => "verification",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("verification diagnostics invalid observation")]
    InvalidObservation,
    #[error("verification diagnostics unsupported outcome")]
    UnsupportedOutcome,
}
pub struct Projection<'a> {
    pub observations: Vec<Fact<'a>>,
    pub diagnostics: Vec<Diagnostic<'a>>,
}

fn code(record: &CheckRecord, phase: Phase) -> Result<ConditionCode, Error> {
    use ConditionCode::*;
    if matches!(
        record.code,
        OutputMissingKeyFailed
            | OutputDuplicateKeyFailed
            | VerificationUniqueFailed
            | VerificationAllOrNoneFailed
            | VerificationAssertFailed
            | VerificationRowCountFailed
            | VerificationNotMissingFailed
            | SubmissionMandatoryFailed
            | SubmissionDataTypeFailed
            | VerificationAllowedValuesFailed
            | VerificationCodelistFailed
            | SubmissionInventoryFailed
            | VerificationRangeFailed
            | VerificationMaxLengthFailed
            | VerificationMatchesFailed
            | ValueLevelMultipleMatches
            | ValueLevelUnmatched
    ) && record.code.definition().phase == phase.name()
    {
        Ok(record.code)
    } else {
        Err(Error::UnsupportedOutcome)
    }
}

fn keys<'a>(names: &[&'a str], rows: &'a [RowIdentity]) -> Result<Vec<Fact<'a>>, Error> {
    rows.iter()
        .map(|row| {
            if names.len() != row.values.as_slice().len() {
                return Err(Error::InvalidObservation);
            }
            Ok(Fact::Object(
                names
                    .iter()
                    .copied()
                    .zip(row.values.as_slice().iter().map(Fact::Scalar))
                    .collect(),
            ))
        })
        .collect()
}

fn base_context<'a>(
    compiled: &'a PreparedSpecification,
    record: &'a CheckRecord,
    code: ConditionCode,
) -> Result<Context<'a>, Error> {
    use ConditionCode::*;
    let failed = Fact::Unsigned(record.failed_count);
    Ok(match code {
        VerificationRowCountFailed => {
            if let Some(observation) = record.row_count.as_ref() {
                if observation.counts.len() != record.failed_count
                    || observation
                        .denominators
                        .as_deref()
                        .is_some_and(|d| d.len() != record.failed_count)
                {
                    return Err(Error::InvalidObservation);
                }
                // An ungrouped check bounds the one whole-artifact group and
                // names its single count; a grouped check names every offending
                // group's count, even when exactly one group offends.
                let grouped = matches!(
                    compiled
                        .verifications()
                        .iter()
                        .find(|check| check.path == record.path.as_str())
                        .map(|check| &check.check),
                    Some(Check::ScopedRowCount(check)) if check.group_by.is_some()
                );
                let mut context: Context<'a> = [("failure_count", failed)].into();
                if !grouped && observation.counts.len() == 1 {
                    context.insert("count", Fact::Unsigned(observation.counts[0]));
                    if let Some(d) = observation.denominators.as_deref() {
                        context.insert("denominator", Fact::Unsigned(d[0]));
                    }
                } else {
                    context.insert(
                        "counts",
                        Fact::Sequence(
                            observation
                                .counts
                                .iter()
                                .map(|&n| Fact::Unsigned(n))
                                .collect(),
                        ),
                    );
                    if let Some(d) = observation.denominators.as_deref() {
                        context.insert(
                            "denominators",
                            Fact::Sequence(d.iter().map(|&n| Fact::Unsigned(n)).collect()),
                        );
                    }
                }
                context
            } else {
                [
                    ("count", Fact::Unsigned(record.output_rows)),
                    ("failure_count", failed),
                ]
                .into()
            }
        }
        VerificationUniqueFailed => {
            let Some(Check::Unique(columns)) = compiled
                .verifications()
                .iter()
                .find(|check| check.path == record.path.as_str())
                .map(|check| &check.check)
            else {
                return Err(Error::InvalidObservation);
            };
            let columns = columns
                .iter()
                .map(|&column| {
                    compiled
                        .output()
                        .columns()
                        .get(column)
                        .map(|column| Fact::Text(column.name.as_str()))
                        .ok_or(Error::InvalidObservation)
                })
                .collect::<Result<_, _>>()?;
            [
                ("columns", Fact::Sequence(columns)),
                ("failure_count", failed),
            ]
            .into()
        }
        OutputMissingKeyFailed => {
            let position = record
                .path
                .strip_prefix("keys[")
                .and_then(|path| path.strip_suffix(']'))
                .and_then(|position| position.parse::<usize>().ok())
                .ok_or(Error::InvalidObservation)?;
            [
                (
                    "column",
                    Fact::Text(
                        compiled
                            .key_names()
                            .nth(position)
                            .ok_or(Error::InvalidObservation)?,
                    ),
                ),
                ("missing_count", failed),
            ]
            .into()
        }
        OutputDuplicateKeyFailed => [("duplicate_count", failed)].into(),
        VerificationAssertFailed | VerificationAllOrNoneFailed => {
            [("failure_count", failed)].into()
        }
        VerificationNotMissingFailed
        | SubmissionMandatoryFailed
        | SubmissionDataTypeFailed
        | VerificationAllowedValuesFailed
        | VerificationCodelistFailed
        | SubmissionInventoryFailed
        | VerificationRangeFailed
        | VerificationMaxLengthFailed
        | VerificationMatchesFailed => {
            let column = compiled
                .verification_target(record.path.as_str())
                .ok_or(Error::InvalidObservation)?;
            let check = compiled
                .column_check(record.path.as_str())
                .ok_or(Error::InvalidObservation)?;
            let mapping =
                crate::dataset_checks::column_definition(check).ok_or(Error::InvalidObservation)?;
            if mapping != code.definition() {
                return Err(Error::InvalidObservation);
            }
            let mut context: Context<'a> =
                [("column", Fact::Text(column)), ("failure_count", failed)].into();
            if let Check::MaxLength(maximum) = check {
                context.insert("max", Fact::Unsigned(*maximum));
            }
            if let Check::WideMaxLength(maximum) | Check::ValueMaxLength { max: maximum, .. } =
                check
            {
                context.insert("max", Fact::Bound(maximum));
            }
            if let Some(observation) = record.codelist.as_ref() {
                let (id, _) = check
                    .codelist(&observation.id)
                    .ok_or(Error::InvalidObservation)?;
                context.insert("codelist", Fact::Text(id));
            }
            if let Check::DataType { form, .. } = check {
                context.insert("data_type", Fact::Text(form.name()));
            }
            context
        }
        ValueLevelMultipleMatches | ValueLevelUnmatched => {
            let column = compiled
                .verification_target(record.path.as_str())
                .ok_or(Error::InvalidObservation)?;
            let check = compiled
                .column_check(record.path.as_str())
                .ok_or(Error::InvalidObservation)?;
            let mut context: Context<'a> =
                [("column", Fact::Text(column)), ("failure_count", failed)].into();
            match (code, check) {
                (ValueLevelMultipleMatches, Check::ValueEntries(_)) => {
                    let entries = record.entries.as_ref().ok_or(Error::InvalidObservation)?;
                    if entries.len() != record.offending_rows.len()
                        || entries.iter().any(|ids| ids.len() < 2)
                    {
                        return Err(Error::InvalidObservation);
                    }
                    context.insert(
                        "entries",
                        Fact::Sequence(
                            entries
                                .iter()
                                .map(|ids| {
                                    Fact::Sequence(
                                        ids.iter().map(|id| Fact::Text(id.as_str())).collect(),
                                    )
                                })
                                .collect(),
                        ),
                    );
                }
                (ValueLevelUnmatched, Check::ValueEntryMatch { id, .. }) => {
                    context.insert("entry", Fact::Text(id.as_str()));
                }
                _ => return Err(Error::InvalidObservation),
            }
            context
        }
        _ => return Err(Error::UnsupportedOutcome),
    })
}

/// Preserve every check, its declaration identity, complete failure keys and
/// report context. Ordinary issue display uses the existing first-five-key policy;
/// codelist values retain every key. Existing report admission/byte limits remain
/// at the caller, and no new approximation or bound carrier is introduced here.
pub fn project<'a>(
    compiled: &'a PreparedSpecification,
    records: &'a [CheckRecord],
    phase: Phase,
    specification: &'a str,
) -> Result<Projection<'a>, Error> {
    let names: Vec<_> = compiled.key_names().collect();
    let mut observations = Vec::new();
    let mut diagnostics = Vec::new();
    for record in records {
        let code = code(record, phase)?;
        let mapping = code.definition();
        let severity = compiled.verification_severity(record.path.as_str());
        let mut failure = Fact::Null;
        if record.failed_count > 0 {
            let mut key_names = names.clone();
            if code == ConditionCode::VerificationRowCountFailed
                && record.row_count.as_ref().is_some()
            {
                let Some(Check::ScopedRowCount(check)) = compiled
                    .verifications()
                    .iter()
                    .find(|c| c.path == record.path.as_str())
                    .map(|c| &c.check)
                else {
                    return Err(Error::InvalidObservation);
                };
                key_names = check
                    .group_by
                    .iter()
                    .flatten()
                    .map(|&c| compiled.output().columns()[c].name.as_str())
                    .collect();
            }
            let mut keys = keys(&key_names, &record.offending_rows)?;
            // An ungrouped count and an entry that matches no row examine the
            // whole artifact as their one offending unit.
            if code == ConditionCode::VerificationRowCountFailed
                && record.row_count.as_ref().is_none()
                || code == ConditionCode::ValueLevelUnmatched
            {
                keys = vec![Fact::Object(Context::new())];
            }
            let mut context = base_context(compiled, record, code)?;
            if phase == Phase::Verification {
                if let Some(identity) = compiled.verification_identity(record.path.as_str()) {
                    context.insert("verification_id", Fact::Text(identity));
                }
            }
            if let Some(codelist) = record.codelist.as_ref() {
                let Some((id, _)) = compiled
                    .column_check(record.path.as_str())
                    .and_then(|check| check.codelist(codelist.id.as_str()))
                else {
                    return Err(Error::InvalidObservation);
                };
                if id != codelist.id.as_str() {
                    return Err(Error::InvalidObservation);
                }
                context.insert(
                    "values",
                    Fact::Sequence(codelist.values.iter().map(Fact::Scalar).collect()),
                );
            }
            if let Some(values) = record.inventory.as_ref() {
                if code != ConditionCode::SubmissionInventoryFailed
                    || !matches!(
                        compiled.column_check(record.path.as_str()),
                        Some(Check::Inventory { .. })
                    )
                    || values.len() != record.failed_count
                    || record.offending_rows.len() != record.failed_count
                    || record.failed_count > record.evaluated_count
                    || record.failed_count > record.output_rows
                    || values
                        .iter()
                        .any(|value| !matches!(value, crate::value::Value::Str(_)))
                {
                    return Err(Error::InvalidObservation);
                }
                context.insert(
                    "values",
                    Fact::Sequence(values.iter().map(Fact::Scalar).collect()),
                );
            } else if code == ConditionCode::SubmissionInventoryFailed {
                return Err(Error::InvalidObservation);
            }
            let mut log_context = context.clone();
            log_context.insert("keys", Fact::Sequence(keys.clone()));
            let display = if record.codelist.as_ref().is_some() {
                keys.clone()
            } else {
                keys[..keys.len().min(5)].to_vec()
            };
            context.insert("keys", Fact::Sequence(display));
            if record.inventory.as_ref().is_some() {
                if let Some(Fact::Sequence(values)) = context.get_mut("values") {
                    values.truncate(5);
                }
            }
            // Display samples and the complete run report retain aligned group
            // counts and matched entries.
            if (record.row_count.as_ref().is_some() || record.entries.as_ref().is_some())
                && record.failed_count > 5
            {
                for name in ["counts", "denominators", "entries"] {
                    if let Some(Fact::Sequence(values)) = context.get_mut(name) {
                        values.truncate(5);
                    }
                }
            }
            if severity == VerificationSeverity::Error {
                diagnostics.push(Diagnostic {
                    code,
                    spec_path: record.path.as_str().into(),
                    context: context.clone(),
                });
            }
            failure = Fact::Object(
                [
                    ("phase", Fact::Text(mapping.phase)),
                    ("condition", Fact::Text(mapping.condition)),
                    (
                        "rule",
                        Fact::Text(mapping.rule.expect("registered normative check")),
                    ),
                    (
                        "spec_paths",
                        Fact::Sequence(vec![Fact::Text(record.path.as_str())]),
                    ),
                    ("context", Fact::Object(context)),
                    ("severity", Fact::Text(severity.name())),
                    ("offending_keys", Fact::Sequence(keys)),
                    ("log_context", Fact::Object(log_context)),
                ]
                .into(),
            );
        }
        if phase == Phase::Verification {
            let check = if code == ConditionCode::VerificationMaxLengthFailed {
                "max_length"
            } else if matches!(
                code,
                ConditionCode::ValueLevelMultipleMatches | ConditionCode::ValueLevelUnmatched
            ) {
                "value_level"
            } else {
                mapping
                    .condition
                    .strip_suffix("_failed")
                    .ok_or(Error::InvalidObservation)?
            };
            observations.push(Fact::Object(
                [
                    ("specification", Fact::Text(specification)),
                    ("spec_path", Fact::Text(record.path.as_str())),
                    ("check", Fact::Text(check)),
                    (
                        "target",
                        compiled
                            .verification_target(record.path.as_str())
                            .map(Fact::Text)
                            .unwrap_or(Fact::Null),
                    ),
                    (
                        "rule",
                        Fact::Text(mapping.rule.expect("registered normative check")),
                    ),
                    (
                        "verification_id",
                        compiled
                            .verification_identity(record.path.as_str())
                            .map(Fact::Text)
                            .unwrap_or(Fact::Null),
                    ),
                    ("severity", Fact::Text(severity.name())),
                    ("evaluated_count", Fact::Unsigned(record.evaluated_count)),
                    ("failure", failure),
                ]
                .into(),
            ));
        }
    }
    Ok(Projection {
        observations,
        diagnostics,
    })
}
