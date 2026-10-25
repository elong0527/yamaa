//! Retained typed execution results and bounded report observations.
use crate::{
    callback::{BoundaryError, CallbackError},
    execution_diagnostics as diagnostics,
};
use std::fmt;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::numeric_lifecycle::HandlerCount;
use yamaa_core::reduction::ReductionError;
use yamaa_core::value::Value;
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
    Internal,
}
impl DatasetError {
    /// Host adapters distinguish unexpected internal failures from rejected requests.
    pub fn is_internal(self) -> bool {
        matches!(
            self,
            Self::Internal | Self::Function(BoundaryError::Internal | BoundaryError::Interrupted)
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
            Self::Internal => "internal execution result failure",
        })
    }
}
impl std::error::Error for DatasetError {}
type Error = DatasetError;

/// The checked dataset of one execution from held bytes, present only for
/// success, with its typed outcome and handler evidence.
pub struct DatasetResponse {
    pub table: Option<dataset::Dataset>,
    pub outcome: Outcome,
    /// Every handler path the attempt registered, in registration order.
    pub handler_counts: Vec<HandlerCount>,
}

/// The typed outcome of one execution attempt.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Success,
    /// Key or verification failures; the response keeps their records.
    Failure,
    /// One language finding and the row and keys it names.
    Condition(Box<Condition>),
    /// A resource bound.
    Limit(Limit),
}

/// One language finding of an execution, with the facts that locate it.
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub diagnostic: Diagnostic,
    /// The output row the finding names.
    pub identity: Option<RowIdentity>,
    /// The partition an ambiguous baseline names, field by field.
    pub partition: Option<Vec<(String, Value)>>,
    /// The intermediate key a lookup matched, field by field.
    pub matched_key: Option<Vec<(String, Value)>>,
}

/// An execution bound: its resource, the limit and the rule when
/// known, and the paths or the source coordinate that own it.
#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    pub resource: &'static str,
    pub limit: Option<usize>,
    pub required: Option<usize>,
    pub spec_paths: Vec<String>,
    pub source_index: Option<usize>,
}

fn condition(diagnostic: Diagnostic, identity: Option<&RowIdentity>) -> Outcome {
    Outcome::Condition(Box::new(Condition {
        diagnostic,
        identity: identity.cloned(),
        partition: None,
        matched_key: None,
    }))
}

fn limit(
    resource: &'static str,
    limit: usize,
    required: Option<usize>,
    spec_paths: Vec<String>,
    source_index: Option<usize>,
) -> Outcome {
    Outcome::Limit(Limit {
        resource,
        limit: Some(limit),
        required,
        spec_paths,
        source_index,
    })
}

/// Read core-owned semantic facts without cloning or classifying any host payload.
/// Project calls and codelist records retain their separate typed formatters.
fn semantic_failure<E>(error: &ExecutionError<E>) -> Result<Outcome, Error> {
    Ok(match error {
        ExecutionError::FunctionBinding { .. } => return Err(Error::FunctionBinding),
        // Project failures use the owned application report with original host payloads.
        ExecutionError::ProjectFunction { .. } => return Err(Error::UnsupportedOutcome),
        ExecutionError::SemanticDiagnostic {
            diagnostic,
            identity,
        } => condition(diagnostic.clone(), identity.as_ref()),
        ExecutionError::IntermediateVerifications(diagnostics)
        | ExecutionError::IntermediateFilterTypes(diagnostics) => {
            condition(diagnostics.first().ok_or(Error::Internal)?.clone(), None)
        }
        ExecutionError::ScalarDiagnostic(diagnostic) => condition(diagnostic.clone(), None),
        ExecutionError::Numeric { error, identity } => {
            condition(diagnostics::numeric(error.clone())?, identity.as_ref())
        }
        ExecutionError::BaselineAmbiguity { partition, .. } => {
            Outcome::Condition(Box::new(Condition {
                diagnostic: diagnostics::failure(error)?,
                identity: None,
                partition: Some(partition.clone()),
                matched_key: None,
            }))
        }
        ExecutionError::MultipleMatches {
            matched_key,
            identity,
            ..
        }
        | ExecutionError::UnmatchedKey {
            matched_key,
            identity,
            ..
        } => Outcome::Condition(Box::new(Condition {
            diagnostic: diagnostics::failure(error)?,
            identity: identity.clone(),
            partition: None,
            matched_key: Some(matched_key.clone()),
        })),
        ExecutionError::MultipleValues { identity, .. } => {
            condition(diagnostics::failure(error)?, identity.as_ref())
        }
        ExecutionError::Limit {
            resource,
            limit: bound,
            required,
        } => limit(
            crate::resource_diagnostics::resource_name(*resource),
            *bound,
            *required,
            Vec::new(),
            None,
        ),
        ExecutionError::SourceLimit {
            resource,
            limit: bound,
            required,
            source_index,
        } => limit(
            crate::resource_diagnostics::resource_name(*resource),
            *bound,
            *required,
            Vec::new(),
            Some(*source_index),
        ),
        ExecutionError::ScopedLimit {
            resource,
            limit: bound,
            required,
            spec_paths,
        } => limit(
            crate::resource_diagnostics::resource_name(*resource),
            *bound,
            *required,
            spec_paths.clone(),
            None,
        ),
        ExecutionError::Capacity => Outcome::Limit(Limit {
            resource: "shape",
            limit: None,
            required: None,
            spec_paths: Vec::new(),
            source_index: None,
        }),
        ExecutionError::Grouping(GroupingError::Limit(error))
        | ExecutionError::OutputGrouping(GroupingError::Limit(error))
        | ExecutionError::Reduction {
            error: TableReductionError::Limit(error),
            ..
        } => limit(error.resource, error.limit, error.actual, Vec::new(), None),
        ExecutionError::KeyFailures(_) | ExecutionError::VerificationFailures(_) => {
            Outcome::Failure
        }
        ExecutionError::Conversion {
            path,
            error,
            identity,
            ..
        } => condition(
            error.clone().into_diagnostic(path.clone()),
            identity.as_ref(),
        ),
        ExecutionError::Predicate { error, .. } => condition(diagnostics::predicate(error)?, None),
        ExecutionError::ReductionType { .. } => condition(diagnostics::failure(error)?, None),
        ExecutionError::Reduction {
            path,
            error: TableReductionError::Reduction(ReductionError::Arithmetic { error, .. }),
            identity,
        } => condition(
            error.clone().into_diagnostic(path.clone()),
            identity.as_ref(),
        ),
        ExecutionError::VerificationDiagnostic { diagnostic, .. } => {
            condition(diagnostic.clone(), None)
        }
        ExecutionError::VerificationDeclaration {
            path, code, reason, ..
        } => condition(
            diagnostics::declaration(path.clone(), *code, reason.clone())?,
            None,
        ),
        ExecutionError::VerificationPredicate { error, .. } => {
            condition(diagnostics::predicate(error)?, None)
        }
        ExecutionError::PartitionGlobal { path } => {
            condition(yamaa_core::application_issue::partition_global(path), None)
        }
        ExecutionError::SchemaMismatch => return Err(Error::InvalidObservation),
        // Admitted references and normalized owned Arrow values make other semantic
        // and coordinate failures impossible. Never invent missing or acceptance.
        _ => return Err(Error::Internal),
    })
}

/// One execution attempt as its report reads it: the typed outcome and the
/// evidence the attempt retained, borrowed from it.
pub struct Response<'a> {
    pub outcome: Outcome,
    pub retained_verifications: &'a [CheckRecord],
    pub intermediate_diagnostics: &'a [Diagnostic],
    pub failed_verifications: Option<(
        yamaa_core::verification_diagnostics::Phase,
        &'a [CheckRecord],
    )>,
}

/// Classify one execution attempt without moving or encoding anything. A
/// project-function failure keeps its own formatter with the original host
/// payload; any other failure the language does not name is refused.
pub fn response<E>(attempt: &dataset::ExecutionAttempt<E>) -> Result<Response<'_>, Error> {
    use yamaa_core::verification_diagnostics::Phase;
    let (outcome, intermediate_diagnostics, failed_verifications) = match &attempt.result {
        Ok(_) => (Outcome::Success, &[][..], None),
        Err(error) => {
            let outcome = semantic_failure(error)?;
            let intermediate: &[Diagnostic] = match error.as_ref() {
                ExecutionError::IntermediateVerifications(diagnostics)
                | ExecutionError::IntermediateFilterTypes(diagnostics) => diagnostics,
                _ => &[],
            };
            let records = match error.as_ref() {
                ExecutionError::KeyFailures(records) => Some((Phase::Output, records.as_slice())),
                ExecutionError::VerificationFailures(records)
                | ExecutionError::VerificationDiagnostic { records, .. }
                | ExecutionError::VerificationDeclaration { records, .. }
                | ExecutionError::VerificationPredicate { records, .. } => {
                    Some((Phase::Verification, records.as_slice()))
                }
                _ => None,
            };
            (outcome, intermediate, records)
        }
    };
    Ok(Response {
        outcome,
        retained_verifications: &attempt.retained_verifications,
        intermediate_diagnostics,
        failed_verifications,
    })
}

/// The typed outcome of one execution attempt.
pub fn outcome<E>(attempt: &dataset::ExecutionAttempt<E>) -> Result<Outcome, Error> {
    response(attempt).map(|response| response.outcome)
}

/// Classify an execution from held bytes and keep its checked dataset.
pub fn specification_response(
    attempt: dataset::ExecutionAttempt<CallbackError>,
) -> Result<DatasetResponse, Error> {
    let outcome = self::outcome(&attempt)?;
    let table = attempt.result.ok().map(|result| result.dataset);
    Ok(DatasetResponse {
        table,
        outcome,
        handler_counts: attempt.handler_counts,
    })
}
