//! Retained typed execution results and bounded report observations.
pub(crate) use crate::dataset_snapshot::Snapshot;
use crate::{
    callback::{BoundaryError, CallbackError},
    execution_diagnostics::{self as diagnostics, arithmetic, conversion, Diagnostic},
    ipc_table::{bounded_json, encode_dataset, TableError},
    report_scalar::ScalarValue,
};
use serde::Serialize;
use std::fmt;
use yamaa_core::reduction::ReductionError;
use yamaa_engine::{
    dataset::{self, CheckRecord, ExecutionError, Resource, RowIdentity},
    table_grouping::GroupingError,
    table_reduction::TableReductionError,
};
pub(crate) const MAX_SOURCE_CELLS: usize = yamaa_engine::capacity::SOURCE_CELLS;
pub(crate) const LIMITS: dataset::Limits = dataset::Limits {
    source_rows: yamaa_engine::capacity::SOURCE_ROWS,
    output_rows: yamaa_engine::capacity::OUTPUT_ROWS,
    output_cells: yamaa_engine::capacity::OUTPUT_CELLS,
    key_cells: yamaa_engine::capacity::KEY_CELLS,
    work_cells: yamaa_engine::capacity::EXECUTION_WORK,
    scalar_text_bytes: 16 * 1024 * 1024,
    output_text_bytes: yamaa_engine::capacity::OUTPUT_TEXT_BYTES,
    identity_cells: yamaa_engine::capacity::IDENTITY_CELLS,
    identity_text_bytes: yamaa_engine::capacity::IDENTITY_TEXT_BYTES,
};
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Typed result and boundary failures are not language diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatasetError {
    InvalidObservation,
    UnsupportedOutcome,
    OutputLimit,
    Function(BoundaryError),
    FunctionBinding,
    Table(TableError),
    Internal,
}
impl DatasetError {
    /// Host adapters distinguish unexpected internal failures from rejected requests.
    pub fn is_internal(self) -> bool {
        matches!(
            self,
            Self::Internal
                | Self::Table(TableError::Internal)
                | Self::Function(BoundaryError::Internal | BoundaryError::Interrupted)
        )
    }
}
impl fmt::Display for DatasetError {
    /// Stable messages never echo supplied request text or panic payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidObservation => "invalid typed execution observation",
            Self::UnsupportedOutcome => "unsupported typed execution outcome",

            Self::OutputLimit => "dataset response exceeds byte limit",
            Self::Function(error) => return error.fmt(f),
            Self::FunctionBinding => {
                "dataset callback bindings do not match the admitted signatures"
            }
            Self::Table(error) => return error.fmt(f),
            Self::Internal => "internal execution result failure",
        })
    }
}
impl std::error::Error for DatasetError {}
type Error = DatasetError;

/// A checked table is present only for success. JSON carries exact typed observations.
pub struct DatasetResponse {
    pub table: Option<Vec<u8>>,
    pub outcome: String,
    // The application publishes from its accepted typed result, never by
    // reconstructing execution success from JSON or decoding its own IPC output.
    pub(crate) execution: Option<dataset::Execution>,
    pub(crate) retained_verifications: Vec<dataset::CheckRecord>,
    pub(crate) intermediate_diagnostics: Vec<yamaa_core::diagnostic::Diagnostic>,
    // Preserve the typed completed failure ledger for core report projection;
    // all observations remain typed until report projection.
    pub(crate) failed_verifications: Option<(
        yamaa_core::verification_diagnostics::Phase,
        Vec<CheckRecord>,
    )>,
}

#[derive(Serialize)]
struct Envelope {
    protocol: &'static str,
    outcome: Outcome,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    handler_counts: Vec<HandlerCount>,
}
#[derive(Serialize)]
struct HandlerCount {
    spec_path: String,
    handler: &'static str,
    count: String,
}
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Success {
        verifications: Vec<Record>,
    },
    Failure {
        phase: &'static str,
        verifications: Vec<Record>,
    },
    Condition {
        diagnostic: Box<Diagnostic>,
        identity: Option<Identity>,
        #[serde(skip_serializing_if = "Option::is_none")]
        partition: Option<Vec<PartitionValue>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        matched_key: Option<Vec<PartitionValue>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        verifications: Option<Vec<Record>>,
    },
    Limit {
        resource: &'static str,
        limit: Option<String>,
        required: Option<String>,
    },
}
#[derive(Serialize)]
struct PartitionValue {
    name: String,
    value: ScalarValue,
}
#[derive(Serialize)]
struct Identity {
    position: String,
    keys: Vec<ScalarValue>,
}
#[derive(Serialize)]
struct CodelistRecord {
    id: String,
    values: Vec<ScalarValue>,
}
#[derive(Serialize)]
struct Record {
    #[serde(skip_serializing_if = "Option::is_none")]
    codelist: Option<CodelistRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inventory: Option<Vec<ScalarValue>>,
    spec_path: String,
    condition: &'static str,
    requirement: &'static str,
    evaluated_count: String,
    failed_count: String,
    output_rows: String,
    offending_rows: Vec<Identity>,
}
/// Move owned identities into the scalar codec without another copy of key strings.
fn identity(row: RowIdentity) -> Identity {
    Identity {
        position: row.position.to_string(),
        keys: row.values.into_iter().map(ScalarValue::from_core).collect(),
    }
}
fn native_records(records: Vec<CheckRecord>) -> Result<Vec<Record>, Error> {
    records
        .into_iter()
        .map(|record| {
            let definition = record.definition();
            Ok(Record {
                codelist: record.codelist.map(|observation| CodelistRecord {
                    id: observation.id,
                    values: observation
                        .values
                        .into_iter()
                        .map(ScalarValue::from_core)
                        .collect(),
                }),
                inventory: record
                    .inventory
                    .map(|values| values.into_iter().map(ScalarValue::from_core).collect()),
                spec_path: record.path,
                condition: definition.condition,
                requirement: definition.requirement.ok_or(Error::Internal)?,
                evaluated_count: record.evaluated_count.to_string(),
                failed_count: record.failed_count.to_string(),
                output_rows: record.output_rows.to_string(),
                offending_rows: record.offending_rows.into_iter().map(identity).collect(),
            })
        })
        .collect()
}
/// Expose resource policy separately from semantic conversion/reduction/check failures.
type FailedChecks = (
    yamaa_core::verification_diagnostics::Phase,
    Vec<CheckRecord>,
);
fn failure(error: ExecutionError<CallbackError>) -> Result<(Outcome, Option<FailedChecks>), Error> {
    use yamaa_core::verification_diagnostics::Phase;
    match error {
        ExecutionError::Function { .. } => Err(Error::FunctionBinding),
        other => {
            // Encode before moving the ledger; no host error is cloned or inspected.
            let outcome = semantic_failure(&other, true)?;
            let records = match other {
                ExecutionError::KeyFailures(records) => Some((Phase::Output, records)),
                ExecutionError::VerificationFailures(records)
                | ExecutionError::VerificationDiagnostic { records, .. }
                | ExecutionError::VerificationDeclaration { records, .. }
                | ExecutionError::VerificationPredicate { records, .. } => {
                    Some((Phase::Verification, records))
                }
                _ => None,
            };
            Ok((outcome, records))
        }
    }
}

/// Read core-owned semantic facts without cloning or classifying any host payload.
/// Project calls and codelist records retain their separate typed formatters.
fn semantic_failure<E>(error: &ExecutionError<E>, retain_records: bool) -> Result<Outcome, Error> {
    let encode_records = native_records;
    Ok(match error {
        ExecutionError::Function { .. } => return Err(Error::UnsupportedOutcome),
        ExecutionError::FunctionBinding { .. } => return Err(Error::FunctionBinding),
        // Project failures use the owned application report with original host payloads.
        ExecutionError::ProjectFunction { .. } => return Err(Error::UnsupportedOutcome),
        ExecutionError::SemanticDiagnostic {
            diagnostic,
            identity: keys,
        } => Outcome::Condition {
            diagnostic: diagnostics::project(diagnostic.clone())?,
            identity: keys.clone().map(identity),
            matched_key: None,
            partition: None,
            verifications: None,
        },
        ExecutionError::IntermediateVerifications(diagnostics) => Outcome::Condition {
            diagnostic: diagnostics::project(diagnostics.first().ok_or(Error::Internal)?.clone())?,
            identity: None,
            matched_key: None,
            partition: None,
            verifications: None,
        },
        ExecutionError::ScalarDiagnostic(diagnostic) => Outcome::Condition {
            diagnostic: diagnostics::project(diagnostic.clone())?,
            identity: None,
            matched_key: None,
            partition: None,
            verifications: None,
        },
        ExecutionError::Numeric {
            error,
            identity: keys,
        } => Outcome::Condition {
            diagnostic: crate::execution_diagnostics::numeric(error.clone())
                .map_err(|_| Error::Internal)?,
            identity: keys.clone().map(identity),
            matched_key: None,
            partition: None,
            verifications: None,
        },
        ExecutionError::BaselineAmbiguity { partition, .. } => Outcome::Condition {
            diagnostic: diagnostics::failure(error, MAX_OUTPUT_BYTES)?,
            identity: None,
            matched_key: None,
            partition: Some(
                partition
                    .iter()
                    .map(|(name, value)| PartitionValue {
                        name: name.clone(),
                        value: ScalarValue::from_core(value.clone()),
                    })
                    .collect(),
            ),
            verifications: None,
        },

        ExecutionError::MultipleMatches {
            matched_key,
            identity: keys,
            ..
        } => Outcome::Condition {
            diagnostic: diagnostics::failure(error, MAX_OUTPUT_BYTES)?,
            identity: keys.clone().map(identity),
            partition: None,
            verifications: None,
            matched_key: Some(
                matched_key
                    .iter()
                    .map(|(name, value)| PartitionValue {
                        name: name.clone(),
                        value: ScalarValue::from_core(value.clone()),
                    })
                    .collect(),
            ),
        },
        ExecutionError::UnmatchedKey {
            matched_key,
            identity: keys,
            ..
        } => Outcome::Condition {
            diagnostic: diagnostics::failure(error, MAX_OUTPUT_BYTES)?,
            identity: keys.clone().map(identity),
            partition: None,
            verifications: None,
            matched_key: Some(
                matched_key
                    .iter()
                    .map(|(name, value)| PartitionValue {
                        name: name.clone(),
                        value: ScalarValue::from_core(value.clone()),
                    })
                    .collect(),
            ),
        },
        ExecutionError::MultipleValues { identity: keys, .. } => Outcome::Condition {
            matched_key: None,
            partition: None,
            diagnostic: diagnostics::failure(error, MAX_OUTPUT_BYTES)?,
            identity: keys.clone().map(identity),
            verifications: None,
        },
        ExecutionError::Limit {
            resource,
            limit,
            required,
        } => Outcome::Limit {
            resource: match resource {
                Resource::WorkCells => "work_cells",
                Resource::PredicateWork => "predicate_work",
                Resource::PredicateResolutions => "predicate_resolutions",
                Resource::PredicateTextBytes => "predicate_text_bytes",
                Resource::PredicateLikeWork => "predicate_like_work",
                Resource::PredicateRegexSubjectBytes => "predicate_regex_subject_bytes",
                Resource::PredicateRegexWork => "predicate_regex_work",
                Resource::PredicateRegexStateCells => "predicate_regex_state_cells",
                Resource::ScalarTextBytes => "scalar_text_bytes",
                Resource::OutputTextBytes => "output_text_bytes",
                Resource::IdentityCells => "identity_cells",
                Resource::IdentityTextBytes => "identity_text_bytes",
            },
            limit: Some(limit.to_string()),
            required: required.map(|value| value.to_string()),
        },
        ExecutionError::Capacity => Outcome::Limit {
            resource: "shape",
            limit: None,
            required: None,
        },
        ExecutionError::Grouping(GroupingError::RowLimit { limit, required }) => Outcome::Limit {
            resource: "source_rows",
            limit: Some(limit.to_string()),
            required: Some(required.to_string()),
        },
        ExecutionError::Grouping(GroupingError::KeyCellLimit { limit })
        | ExecutionError::OutputGrouping(GroupingError::KeyCellLimit { limit }) => Outcome::Limit {
            resource: "key_cells",
            limit: Some(limit.to_string()),
            required: None,
        },
        ExecutionError::KeyFailures(found) => Outcome::Failure {
            phase: "output",
            verifications: encode_records(found.clone())?,
        },
        ExecutionError::VerificationFailures(found) => Outcome::Failure {
            phase: "verification",
            verifications: encode_records(found.clone())?,
        },
        ExecutionError::Conversion {
            path,
            error,
            identity: keys,
            ..
        } => Outcome::Condition {
            matched_key: None,
            partition: None,
            verifications: None,
            diagnostic: conversion(error.clone(), path.clone())?,
            identity: keys.clone().map(identity),
        },
        ExecutionError::Predicate { error, .. } => Outcome::Condition {
            matched_key: None,
            partition: None,
            verifications: None,
            diagnostic: crate::execution_diagnostics::predicate(error)?,
            identity: None,
        },
        ExecutionError::ReductionType { .. } => Outcome::Condition {
            matched_key: None,
            partition: None,
            verifications: None,
            identity: None,
            diagnostic: diagnostics::failure(error, MAX_OUTPUT_BYTES)?,
        },
        ExecutionError::Reduction {
            path,
            error: TableReductionError::Reduction(ReductionError::Arithmetic { error, .. }),
            identity: keys,
        } => Outcome::Condition {
            matched_key: None,
            partition: None,
            verifications: None,
            diagnostic: arithmetic(error.clone(), path.clone())?,
            identity: keys.clone().map(identity),
        },
        ExecutionError::VerificationDiagnostic {
            diagnostic,
            records: completed,
        } => Outcome::Condition {
            matched_key: None,
            partition: None,
            identity: None,
            diagnostic: diagnostics::project(diagnostic.clone())?,
            verifications: if retain_records {
                Some(encode_records(completed.clone())?)
            } else {
                None
            },
        },
        ExecutionError::VerificationDeclaration {
            path,
            code,
            reason,
            records: completed,
        } => Outcome::Condition {
            matched_key: None,
            partition: None,
            identity: None,
            diagnostic: crate::execution_diagnostics::declaration(
                path.clone(),
                *code,
                reason.clone(),
            )?,
            verifications: if retain_records {
                Some(encode_records(completed.clone())?)
            } else {
                None
            },
        },
        ExecutionError::VerificationPredicate {
            error,
            records: completed,
        } => Outcome::Condition {
            matched_key: None,
            partition: None,
            diagnostic: crate::execution_diagnostics::predicate(error)?,
            identity: None,
            verifications: if retain_records {
                Some(encode_records(completed.clone())?)
            } else {
                None
            },
        },
        ExecutionError::SchemaMismatch => return Err(Error::InvalidObservation),
        // Admitted references and normalized owned Arrow values make other semantic
        // and coordinate failures impossible. Never invent missing or acceptance.
        _ => return Err(Error::Internal),
    })
}

/// Borrow a retained non-host failure for the complete project report formatter.
pub(crate) fn semantic_condition<E>(error: &ExecutionError<E>) -> Result<serde_json::Value, Error> {
    let outcome = semantic_failure(error, false)?;
    serde_json::to_value(outcome).map_err(|_| Error::Internal)
}

/// Encode the admitted execution attempt, retaining typed results for publication.
pub(crate) fn specification_response(
    attempt: dataset::ExecutionAttempt<CallbackError>,
) -> Result<DatasetResponse, Error> {
    let intermediate_diagnostics = match &attempt.result {
        Err(error) => match error.as_ref() {
            ExecutionError::IntermediateVerifications(diagnostics) => diagnostics.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let (table, outcome, execution, failed_verifications) = match attempt.result {
        Ok(result) => {
            let table = Some(encode_dataset(&result.dataset).map_err(Error::Table)?);
            let outcome = Outcome::Success {
                verifications: native_records(result.verifications.clone())?,
            };
            (table, outcome, Some(result), None)
        }
        Err(error) => {
            let (outcome, records) = failure(*error)?;
            (None, outcome, None, records)
        }
    };
    let outcome = bounded_json(
        &Envelope {
            protocol: "execution/1",
            outcome,
            handler_counts: attempt
                .handler_counts
                .into_iter()
                .map(|entry| HandlerCount {
                    spec_path: entry.spec_path,
                    handler: entry.handler.name(),
                    count: entry.count.to_string(),
                })
                .collect(),
        },
        MAX_OUTPUT_BYTES,
    )
    .map_err(|error| {
        if error == TableError::OutputLimit {
            Error::OutputLimit
        } else {
            Error::Internal
        }
    })?;
    Ok(DatasetResponse {
        table,
        outcome,
        execution,
        retained_verifications: attempt.retained_verifications,
        intermediate_diagnostics,
        failed_verifications,
    })
}
