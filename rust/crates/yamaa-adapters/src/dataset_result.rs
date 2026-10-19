//! Retained typed execution results and bounded report observations.
pub(crate) use crate::dataset_snapshot::Snapshot;
use crate::{
    callback::{BoundaryError, CallbackError},
    execution_diagnostics::{self as diagnostics, arithmetic, conversion, Diagnostic},
    ipc_table::{encode_dataset, serialize_json, TableError},
    report_scalar::ScalarValue,
};
use serde::Serialize;
use std::fmt;
use yamaa_core::reduction::ReductionError;
use yamaa_core::{
    dataset::{self, CheckRecord, ExecutionError, RowIdentity},
    table_grouping::GroupingError,
    table_reduction::TableReductionError,
};
/// Typed result and boundary failures are not language diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatasetError {
    InvalidObservation,
    UnsupportedOutcome,
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
    // Row templates the attempt completed, for the run report.
    pub(crate) template_rows: Vec<dataset::TemplateRows>,
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
        #[serde(serialize_with = "serialize_diagnostic")]
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
        #[serde(skip_serializing_if = "Vec::is_empty")]
        spec_paths: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_index: Option<usize>,
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
    // Encode before moving the ledger; no host error is cloned or inspected.
    let outcome = semantic_failure(&error, true)?;
    let records = match error {
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

/// Read core-owned semantic facts without cloning or classifying any host payload.
/// Project calls and codelist records retain their separate typed formatters.
fn semantic_failure<E>(error: &ExecutionError<E>, retain_records: bool) -> Result<Outcome, Error> {
    let encode_records = native_records;
    Ok(match error {
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
        ExecutionError::IntermediateVerifications(diagnostics)
        | ExecutionError::IntermediateFilterTypes(diagnostics) => Outcome::Condition {
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
            diagnostic: diagnostics::failure(error)?,
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
            diagnostic: diagnostics::failure(error)?,
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
            diagnostic: diagnostics::failure(error)?,
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
            diagnostic: diagnostics::failure(error)?,
            identity: keys.clone().map(identity),
            verifications: None,
        },
        ExecutionError::Limit {
            resource,
            limit,
            required,
        } => Outcome::Limit {
            resource: crate::resource_diagnostics::resource_name(*resource),
            limit: Some(limit.to_string()),
            required: required.map(|value| value.to_string()),
            spec_paths: Vec::new(),
            source_index: None,
        },
        ExecutionError::SourceLimit {
            resource,
            limit,
            required,
            source_index,
        } => Outcome::Limit {
            resource: crate::resource_diagnostics::resource_name(*resource),
            limit: Some(limit.to_string()),
            required: required.map(|value| value.to_string()),
            spec_paths: Vec::new(),
            source_index: Some(*source_index),
        },
        ExecutionError::ScopedLimit {
            resource,
            limit,
            required,
            spec_paths,
        } => Outcome::Limit {
            resource: crate::resource_diagnostics::resource_name(*resource),
            limit: Some(limit.to_string()),
            required: required.map(|value| value.to_string()),
            spec_paths: spec_paths.clone(),
            source_index: None,
        },
        ExecutionError::Capacity => Outcome::Limit {
            resource: "shape",
            limit: None,
            required: None,
            spec_paths: Vec::new(),
            source_index: None,
        },
        ExecutionError::Grouping(GroupingError::Limit(error))
        | ExecutionError::OutputGrouping(GroupingError::Limit(error))
        | ExecutionError::Reduction {
            error: TableReductionError::Limit(error),
            ..
        } => Outcome::Limit {
            resource: error.resource,
            limit: Some(error.limit.to_string()),
            required: error.actual.map(|value| value.to_string()),
            spec_paths: Vec::new(),
            source_index: None,
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
            diagnostic: diagnostics::failure(error)?,
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

fn serialize_diagnostic<S: serde::Serializer>(
    value: &Diagnostic,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    crate::json_wire::WireValue(value).serialize(serializer)
}

/// Project one scalar record fact into the owned tree without text reparse.
fn scalar_json(value: &ScalarValue) -> yamaa_core::json::Value {
    value.to_json()
}

fn identity_json(identity: &Identity) -> yamaa_core::json::Value {
    use yamaa_core::json::{Map, Value as Json};
    Json::Object(
        [
            ("position".into(), Json::String(identity.position.clone())),
            (
                "keys".into(),
                Json::Array(identity.keys.iter().map(scalar_json).collect()),
            ),
        ]
        .into_iter()
        .collect::<Map<_, _>>(),
    )
}

fn record_json(record: &Record) -> yamaa_core::json::Value {
    use yamaa_core::json::{Map, Value as Json};
    let mut object = Map::new();
    if let Some(codelist) = &record.codelist {
        object.insert(
            "codelist".into(),
            Json::Object(
                [
                    ("id".into(), Json::String(codelist.id.clone())),
                    (
                        "values".into(),
                        Json::Array(codelist.values.iter().map(scalar_json).collect()),
                    ),
                ]
                .into_iter()
                .collect::<Map<_, _>>(),
            ),
        );
    }
    if let Some(inventory) = &record.inventory {
        object.insert(
            "inventory".into(),
            Json::Array(inventory.iter().map(scalar_json).collect()),
        );
    }
    object.insert("spec_path".into(), Json::String(record.spec_path.clone()));
    object.insert("condition".into(), Json::String(record.condition.into()));
    object.insert(
        "requirement".into(),
        Json::String(record.requirement.into()),
    );
    object.insert(
        "evaluated_count".into(),
        Json::String(record.evaluated_count.clone()),
    );
    object.insert(
        "failed_count".into(),
        Json::String(record.failed_count.clone()),
    );
    object.insert(
        "output_rows".into(),
        Json::String(record.output_rows.clone()),
    );
    object.insert(
        "offending_rows".into(),
        Json::Array(record.offending_rows.iter().map(identity_json).collect()),
    );
    Json::Object(object)
}

fn partition_json(value: &PartitionValue) -> yamaa_core::json::Value {
    use yamaa_core::json::{Map, Value as Json};
    Json::Object(
        [
            ("name".into(), Json::String(value.name.clone())),
            ("value".into(), scalar_json(&value.value)),
        ]
        .into_iter()
        .collect::<Map<_, _>>(),
    )
}

/// Direct typed projection of the serde outcome shape into the owned tree.
/// This replaces the removed stringify-and-reparse helper: the diagnostic
/// subtree is already an owned tree and scalar facts project through
/// `ScalarValue::to_json`, so exact number tokens never narrow.
fn outcome_json(outcome: &Outcome) -> yamaa_core::json::Value {
    use yamaa_core::json::{Map, Value as Json};
    let mut object = Map::new();
    match outcome {
        Outcome::Success { verifications } => {
            object.insert("status".into(), Json::String("success".into()));
            object.insert(
                "verifications".into(),
                Json::Array(verifications.iter().map(record_json).collect()),
            );
        }
        Outcome::Failure {
            phase,
            verifications,
        } => {
            object.insert("status".into(), Json::String("failure".into()));
            object.insert("phase".into(), Json::String((*phase).into()));
            object.insert(
                "verifications".into(),
                Json::Array(verifications.iter().map(record_json).collect()),
            );
        }
        Outcome::Condition {
            diagnostic,
            identity,
            partition,
            matched_key,
            verifications,
        } => {
            object.insert("status".into(), Json::String("condition".into()));
            object.insert("diagnostic".into(), (**diagnostic).clone());
            object.insert(
                "identity".into(),
                identity.as_ref().map_or(Json::Null, identity_json),
            );
            if let Some(partition) = partition {
                object.insert(
                    "partition".into(),
                    Json::Array(partition.iter().map(partition_json).collect()),
                );
            }
            if let Some(matched_key) = matched_key {
                object.insert(
                    "matched_key".into(),
                    Json::Array(matched_key.iter().map(partition_json).collect()),
                );
            }
            if let Some(verifications) = verifications {
                object.insert(
                    "verifications".into(),
                    Json::Array(verifications.iter().map(record_json).collect()),
                );
            }
        }
        Outcome::Limit {
            resource,
            limit,
            required,
            spec_paths,
            source_index,
        } => {
            object.insert("status".into(), Json::String("limit".into()));
            object.insert("resource".into(), Json::String((*resource).into()));
            object.insert(
                "limit".into(),
                limit.clone().map_or(Json::Null, Json::String),
            );
            object.insert(
                "required".into(),
                required.clone().map_or(Json::Null, Json::String),
            );
            if !spec_paths.is_empty() {
                object.insert(
                    "spec_paths".into(),
                    Json::Array(
                        spec_paths
                            .iter()
                            .map(|path| Json::String(path.clone()))
                            .collect(),
                    ),
                );
            }
            if let Some(source_index) = source_index {
                object.insert("source_index".into(), Json::from(*source_index));
            }
        }
    }
    Json::Object(object)
}

/// Borrow a retained non-host failure for the complete project report formatter.
pub(crate) fn semantic_condition<E>(
    error: &ExecutionError<E>,
) -> Result<yamaa_core::json::Value, Error> {
    let outcome = semantic_failure(error, false)?;
    Ok(outcome_json(&outcome))
}

/// Encode the admitted execution attempt, retaining typed results for publication.
pub(crate) fn specification_response(
    attempt: dataset::ExecutionAttempt<CallbackError>,
) -> Result<DatasetResponse, Error> {
    let intermediate_diagnostics = match &attempt.result {
        Err(error) => match error.as_ref() {
            ExecutionError::IntermediateVerifications(diagnostics)
            | ExecutionError::IntermediateFilterTypes(diagnostics) => diagnostics.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let template_rows = attempt.template_rows;
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
    let outcome = serialize_json(&Envelope {
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
    })
    .map_err(|_| Error::Internal)?;
    Ok(DatasetResponse {
        table,
        outcome,
        execution,
        retained_verifications: attempt.retained_verifications,
        intermediate_diagnostics,
        failed_verifications,
        template_rows,
    })
}
