//! Complete borrowed verification reports from retained engine observations.
//! Formatting never reads a table, rechecks a row, or narrows authored bounds.
use crate::{
    dataset::{Check, VerificationSeverity},
    diagnostic::ConditionCode,
    project_codelist_diagnostics::{CheckObservation, RowObservation},
    project_diagnostics::{Context, Diagnostic, Fact},
    specification::PreparedSpecification,
};
use alloc::{vec, vec::Vec};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidObservation,
    UnsupportedOutcome,
}
pub struct Projection<'a> {
    pub observations: Vec<Fact<'a>>,
    pub diagnostics: Vec<Diagnostic<'a>>,
}

fn code<R>(record: &CheckObservation<'_, R>, phase: Phase) -> Result<ConditionCode, Error> {
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
            | VerificationAllowedValuesFailed
            | VerificationCodelistFailed
            | VerificationRangeFailed
            | VerificationMaxLengthFailed
            | VerificationMatchesFailed
    ) && record.code.definition().phase == phase.name()
    {
        Ok(record.code)
    } else {
        Err(Error::UnsupportedOutcome)
    }
}

fn keys<'a, R: RowObservation>(names: &[&'a str], rows: &'a [R]) -> Result<Vec<Fact<'a>>, Error> {
    rows.iter()
        .map(|row| {
            if names.len() != row.values().len() {
                return Err(Error::InvalidObservation);
            }
            Ok(Fact::Object(
                names
                    .iter()
                    .copied()
                    .zip(row.values().iter().map(Fact::Scalar))
                    .collect(),
            ))
        })
        .collect()
}

fn base_context<'a, R>(
    compiled: &'a PreparedSpecification,
    record: &CheckObservation<'a, R>,
    code: ConditionCode,
) -> Result<Context<'a>, Error> {
    use ConditionCode::*;
    let failed = Fact::Unsigned(record.failed_count);
    Ok(match code {
        VerificationRowCountFailed => [
            ("count", Fact::Unsigned(record.output_rows)),
            ("failure_count", failed),
        ]
        .into(),
        VerificationUniqueFailed => {
            let Some(Check::Unique(columns)) = compiled
                .verifications()
                .iter()
                .find(|check| check.path == record.path)
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
        | VerificationAllowedValuesFailed
        | VerificationCodelistFailed
        | VerificationRangeFailed
        | VerificationMaxLengthFailed
        | VerificationMatchesFailed => {
            let column = compiled
                .verification_target(record.path)
                .ok_or(Error::InvalidObservation)?;
            let check = compiled
                .column_check(record.path)
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
            if let Check::Codelist { id, .. } = check {
                context.insert("codelist", Fact::Text(id));
            }
            context
        }
        _ => return Err(Error::UnsupportedOutcome),
    })
}

/// Preserve every check, its declaration identity, complete failure keys and log
/// context. Ordinary issue display uses the existing first-five-key policy;
/// codelist values retain every key. Existing report admission/byte limits remain
/// at the caller, and no new approximation or bound carrier is introduced here.
pub fn project<'a, R: RowObservation>(
    compiled: &'a PreparedSpecification,
    records: &[CheckObservation<'a, R>],
    phase: Phase,
    specification: &'a str,
) -> Result<Projection<'a>, Error> {
    let names: Vec<_> = compiled.key_names().collect();
    let mut observations = Vec::new();
    let mut diagnostics = Vec::new();
    for record in records {
        let code = code(record, phase)?;
        let mapping = code.definition();
        let severity = compiled.verification_severity(record.path);
        let mut failure = Fact::Null;
        if record.failed_count > 0 {
            let mut keys = keys(&names, record.offending_rows)?;
            if code == ConditionCode::VerificationRowCountFailed {
                keys = vec![Fact::Object(Context::new())];
            }
            let mut context = base_context(compiled, record, code)?;
            if phase == Phase::Verification {
                if let Some(identity) = compiled.verification_identity(record.path) {
                    context.insert("verification_id", Fact::Text(identity));
                }
            }
            if let Some(codelist) = record.codelist {
                let Some(Check::Codelist { id, .. }) = compiled.column_check(record.path) else {
                    return Err(Error::InvalidObservation);
                };
                if id != codelist.id {
                    return Err(Error::InvalidObservation);
                }
                context.insert(
                    "values",
                    Fact::Sequence(codelist.values.iter().map(Fact::Scalar).collect()),
                );
            }
            let mut log_context = context.clone();
            log_context.insert("keys", Fact::Sequence(keys.clone()));
            let display = if record.codelist.is_some() {
                keys.clone()
            } else {
                keys[..keys.len().min(5)].to_vec()
            };
            context.insert("keys", Fact::Sequence(display));
            if severity == VerificationSeverity::Error {
                diagnostics.push(Diagnostic {
                    code,
                    spec_path: record.path.into(),
                    context: context.clone(),
                });
            }
            failure = Fact::Object(
                [
                    ("phase", Fact::Text(mapping.phase)),
                    ("condition", Fact::Text(mapping.condition)),
                    (
                        "requirement",
                        Fact::Text(mapping.requirement.expect("registered normative check")),
                    ),
                    ("spec_paths", Fact::Sequence(vec![Fact::Text(record.path)])),
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
            } else {
                mapping
                    .condition
                    .strip_suffix("_failed")
                    .ok_or(Error::InvalidObservation)?
            };
            observations.push(Fact::Object(
                [
                    ("specification", Fact::Text(specification)),
                    ("spec_path", Fact::Text(record.path)),
                    ("check", Fact::Text(check)),
                    (
                        "target",
                        compiled
                            .verification_target(record.path)
                            .map(Fact::Text)
                            .unwrap_or(Fact::Null),
                    ),
                    (
                        "requirement",
                        Fact::Text(mapping.requirement.expect("registered normative check")),
                    ),
                    (
                        "verification_id",
                        compiled
                            .verification_identity(record.path)
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
