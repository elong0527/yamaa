//! Bounded dataset/1 typed-plan bridge over copied, verified Arrow snapshots.
//! This protocol is not a specification compiler, file runner or default backend.
use crate::{
    numeric_transport::{arithmetic, conversion, Diagnostic},
    predicate_transport::Predicate,
    scalar_transport::{ScalarValue, MAX_REQUEST_BYTES},
    table_transport::{bounded_json, decode_snapshot, encode_dataset, TableTransportError},
};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, fmt, panic::catch_unwind};
use yamaa_core::{
    reduction::{NumericReducer, ReductionError},
    table::{Column, TableSchema},
    value::ColumnType,
};
use yamaa_engine::{
    dataset::{self, CheckRecord, DatasetPlan, ExecutionError, Limits, Resource, RowIdentity},
    table_grouping::GroupingError,
    table_reduction::TableReductionError,
};

const PROTOCOL: &str = "dataset/1";
/// Discover additive typed-plan features before callers acquire source data.
pub fn capabilities() -> &'static str {
    r#"{"protocol":"dataset/1","features":["row_filter","predicate_checks","key_grain","window_numbering","window_filter","window_values","window_baseline"]}"#
}

const MAX_COLUMNS: usize = 64;
const MAX_TEMPLATES: usize = 16;
const MAX_CHECKS: usize = 16;
const MAX_NAME: usize = 256;
const MAX_PATH: usize = 1024;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const LIMITS: Limits = Limits {
    source_rows: 65_536,
    output_rows: 65_536,
    output_cells: 262_144,
    key_cells: 262_144,
    work_cells: 4_194_304,
    scalar_text_bytes: 16 * 1024 * 1024,
    output_text_bytes: 1_048_576,
    identity_cells: 65_536,
    identity_text_bytes: 1_048_576,
};

/// Malformed typed requests and boundary failures are not language diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatasetTransportError {
    RequestLimit,
    InvalidRequest,
    UnsupportedProtocol,
    InvalidScalar,
    InvalidPlan,
    OutputLimit,
    Table(TableTransportError),
    Internal,
}
impl DatasetTransportError {
    /// Host adapters distinguish unexpected internal failures from rejected requests.
    pub fn is_internal(self) -> bool {
        matches!(
            self,
            Self::Internal | Self::Table(TableTransportError::Internal)
        )
    }
}
impl fmt::Display for DatasetTransportError {
    /// Stable messages never echo supplied request text or panic payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RequestLimit => "dataset plan exceeds resource limit",
            Self::InvalidRequest => "invalid dataset plan request",
            Self::UnsupportedProtocol => "unsupported dataset protocol",
            Self::InvalidScalar => "invalid dataset literal representation",
            Self::InvalidPlan => "invalid bound dataset plan",
            Self::OutputLimit => "dataset response exceeds byte limit",
            Self::Table(error) => return error.fmt(f),
            Self::Internal => "internal dataset transport failure",
        })
    }
}
impl std::error::Error for DatasetTransportError {}
type Error = DatasetTransportError;

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Str,
    Int,
    Float,
    Date,
    Datetime,
}
impl Kind {
    /// Map only existing column types; bool is deliberately not a destination.
    fn core(self) -> ColumnType {
        match self {
            Self::Str => ColumnType::Str,
            Self::Int => ColumnType::Int,
            Self::Float => ColumnType::Float,
            Self::Date => ColumnType::Date,
            Self::Datetime => ColumnType::DateTime,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    name: String,
    kind: Kind,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Expression {
    Literal(ScalarValue),
    Number(Window),
    Window(Window),
    Source(usize),
    Collect(CollectedSource),
    Column(usize),
    Reduce(Reduction),
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum WindowKind {
    RowNumber,
    Competition,
    Dense,
    RowValue { column: usize, offset: String },
    PreviousNonMissing { column: usize },
    Locf { column: usize },
    BaselineFlag { date: usize, reference_date: usize },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderTerm {
    column: usize,
    descending: bool,
    nulls_first: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Window {
    kind: WindowKind,
    group_by: Vec<usize>,
    order_by: Vec<OrderTerm>,
    #[serde(default)]
    filter: Option<Predicate>,
}
impl Window {
    /// Lower one bounded window operation while retaining the legacy number request form.
    fn prepare(self) -> Result<dataset::Window, Error> {
        if self.group_by.len() > MAX_COLUMNS || self.order_by.len() > MAX_COLUMNS {
            return Err(Error::RequestLimit);
        }
        let kind = match self.kind {
            WindowKind::RowNumber => dataset::WindowKind::RowNumber,
            WindowKind::Competition => dataset::WindowKind::Competition,
            WindowKind::Dense => dataset::WindowKind::Dense,
            WindowKind::RowValue { column, offset } => dataset::WindowKind::RowValue {
                column,
                offset: bound(Some(offset))?.ok_or(Error::InvalidRequest)?,
            },
            WindowKind::PreviousNonMissing { column } => {
                dataset::WindowKind::PreviousNonMissing { column }
            }
            WindowKind::Locf { column } => dataset::WindowKind::Locf { column },
            WindowKind::BaselineFlag {
                date,
                reference_date,
            } => dataset::WindowKind::BaselineFlag {
                date,
                reference_date,
            },
        };
        Ok(dataset::Window {
            kind,
            group_by: self.group_by,
            order_by: self
                .order_by
                .into_iter()
                .map(|term| dataset::OrderTerm {
                    column: term.column,
                    descending: term.descending,
                    nulls_first: term.nulls_first,
                })
                .collect(),
            filter: self.filter.map(Predicate::prepare).transpose()?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectedSource {
    column: usize,
    identifier: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum Reducer {
    Sum,
    Mean,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reduction {
    column: usize,
    reducer: Reducer,
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Assignment {
    column: usize,
    path: String,
    expression: Expression,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Mode {
    Records(()),
    Groups(Vec<usize>),
    Keys(()),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Template {
    mode: Mode,
    assignments: Vec<Assignment>,
    #[serde(default)]
    filter: Option<Predicate>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Check {
    Assert(Predicate),
    PredicateDeclaration(Predicate),
    Implies { when: Predicate, then: Predicate },
    Unique(Vec<usize>),
    RowCount(Bounds),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bounds {
    min: Option<String>,
    max: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Verification {
    path: String,
    check: Check,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    source: Vec<Field>,
    output: Vec<Field>,
    templates: Vec<Template>,
    columns: Vec<Assignment>,
    keys: Vec<usize>,
    verifications: Vec<Verification>,
}

/// The prepared bridge owns its admitted plan; no host objects or source buffers survive.
pub struct PreparedDataset {
    plan: DatasetPlan,
}

/// A checked table is present only for success. JSON carries exact typed observations.
pub struct DatasetResponse {
    pub table: Option<Vec<u8>>,
    pub outcome: String,
}

/// Validate plan bytes before any IPC decoding, then execute once with no fallback.
pub fn execute_dataset(request: &str, source: &[u8]) -> Result<DatasetResponse, Error> {
    PreparedDataset::parse(request)?.execute(source)
}

impl PreparedDataset {
    /// Bound request bytes and plan complexity, then admit all references without data.
    pub fn parse(request: &str) -> Result<Self, Error> {
        if request.len() > MAX_REQUEST_BYTES {
            return Err(Error::RequestLimit);
        }
        catch_unwind(|| {
            let request: Request =
                serde_json::from_str(request).map_err(|_| Error::InvalidRequest)?;
            if request.protocol != PROTOCOL {
                return Err(Error::UnsupportedProtocol);
            }
            if request.templates.len() > MAX_TEMPLATES
                || request.verifications.len() > MAX_CHECKS
                || request.keys.len() > MAX_COLUMNS
            {
                return Err(Error::RequestLimit);
            }
            let source = schema(request.source)?;
            let output = schema(request.output)?;
            let templates = request
                .templates
                .into_iter()
                .map(|template| {
                    let mode = match template.mode {
                        Mode::Records(()) => dataset::RowMode::Records,
                        Mode::Keys(()) => dataset::RowMode::Keys,
                        Mode::Groups(keys) => {
                            if keys.len() > MAX_COLUMNS {
                                return Err(Error::RequestLimit);
                            }
                            dataset::RowMode::Groups(keys)
                        }
                    };
                    Ok(dataset::RowTemplate {
                        filter: template.filter.map(Predicate::prepare).transpose()?,
                        mode,
                        assignments: assignments(template.assignments)?,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let columns = assignments(request.columns)?;
            let verifications = request
                .verifications
                .into_iter()
                .map(|verification| {
                    path(&verification.path)?;
                    let check = match verification.check {
                        Check::Assert(predicate) => dataset::Check::Assert(predicate.prepare()?),
                        Check::PredicateDeclaration(predicate) => {
                            dataset::Check::PredicateDeclaration(predicate.prepare()?)
                        }
                        Check::Implies { when, then } => dataset::Check::Implies {
                            when: when.prepare()?,
                            then: then.prepare()?,
                        },
                        Check::Unique(columns) => {
                            if columns.len() > MAX_COLUMNS {
                                return Err(Error::RequestLimit);
                            }
                            dataset::Check::Unique(columns)
                        }
                        Check::RowCount(bounds) => dataset::Check::RowCount {
                            min: bound(bounds.min)?,
                            max: bound(bounds.max)?,
                        },
                    };
                    Ok(dataset::Verification {
                        path: verification.path,
                        check,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let plan = DatasetPlan::new(
                source,
                output,
                templates,
                columns,
                request.keys,
                verifications,
            )
            .map_err(|_| Error::InvalidPlan)?;
            Ok(Self { plan })
        })
        .map_err(|_| Error::Internal)?
    }

    /// Copy and validate a complete snapshot; every run gets fresh resource counters.
    pub fn execute(&self, source: &[u8]) -> Result<DatasetResponse, Error> {
        catch_unwind(|| {
            let source = decode_snapshot(source).map_err(Error::Table)?;
            let (table, outcome) = match self.plan.execute(&source, LIMITS) {
                Ok(result) => (
                    Some(encode_dataset(&result.dataset).map_err(Error::Table)?),
                    Outcome::Success {
                        verifications: records(result.verifications),
                    },
                ),
                Err(error) => (None, failure(*error)?),
            };
            let outcome = bounded_json(
                &Envelope {
                    protocol: PROTOCOL,
                    outcome,
                },
                MAX_OUTPUT_BYTES,
            )
            .map_err(|error| {
                if error == TableTransportError::OutputLimit {
                    Error::OutputLimit
                } else {
                    Error::Internal
                }
            })?;
            Ok(DatasetResponse { table, outcome })
        })
        .map_err(|_| Error::Internal)?
    }
}

/// Bound schema names and columns before constructing the ordered core schema.
fn schema(fields: Vec<Field>) -> Result<TableSchema, Error> {
    if fields.len() > MAX_COLUMNS || fields.iter().any(|field| field.name.len() > MAX_NAME) {
        return Err(Error::RequestLimit);
    }
    TableSchema::new(
        fields
            .into_iter()
            .map(|field| Column {
                name: field.name,
                kind: field.kind.core(),
            })
            .collect(),
    )
    .map_err(|_| Error::InvalidPlan)
}
/// Keep diagnostic provenance bounded and nonempty without treating it as file authority.
fn path(path: &str) -> Result<(), Error> {
    if path.is_empty() {
        return Err(Error::InvalidRequest);
    }
    if path.len() > MAX_PATH {
        return Err(Error::RequestLimit);
    }
    Ok(())
}
/// Canonical decimal text retains full signed bounds through hosts with binary64 JSON.
fn bound(value: Option<String>) -> Result<Option<i64>, Error> {
    value
        .map(|text| {
            let value = text.parse::<i64>().map_err(|_| Error::InvalidRequest)?;
            if value.to_string() != text {
                return Err(Error::InvalidRequest);
            }
            Ok(value)
        })
        .transpose()
}
/// Lower the closed expression vocabulary without performing any evaluation.
fn assignments(values: Vec<Assignment>) -> Result<Vec<dataset::Assignment>, Error> {
    if values.len() > MAX_COLUMNS {
        return Err(Error::RequestLimit);
    }
    values
        .into_iter()
        .map(|assignment| {
            path(&assignment.path)?;
            let expression = match assignment.expression {
                Expression::Literal(value) => dataset::Expression::Literal(
                    value.into_core().map_err(|_| Error::InvalidScalar)?,
                ),
                Expression::Source(column) => dataset::Expression::Source(column),
                Expression::Collect(source) => {
                    path(&source.identifier)?;
                    dataset::Expression::Collect {
                        column: source.column,
                        identifier: source.identifier,
                    }
                }
                Expression::Column(column) => dataset::Expression::Column(column),
                Expression::Number(window) => {
                    if !matches!(
                        window.kind,
                        WindowKind::RowNumber | WindowKind::Competition | WindowKind::Dense
                    ) {
                        return Err(Error::InvalidPlan);
                    }
                    dataset::Expression::Window(window.prepare()?)
                }
                Expression::Window(window) => {
                    if matches!(
                        window.kind,
                        WindowKind::RowNumber | WindowKind::Competition | WindowKind::Dense
                    ) {
                        return Err(Error::InvalidPlan);
                    }
                    dataset::Expression::Window(window.prepare()?)
                }
                Expression::Reduce(reduction) => {
                    path(&reduction.text)?;
                    dataset::Expression::Reduce {
                        column: reduction.column,
                        reducer: match reduction.reducer {
                            Reducer::Sum => NumericReducer::Sum,
                            Reducer::Mean => NumericReducer::Mean,
                        },
                        text: reduction.text,
                    }
                }
            };
            Ok(dataset::Assignment {
                column: assignment.column,
                path: assignment.path,
                expression,
            })
        })
        .collect()
}

#[derive(Serialize)]
struct Envelope {
    protocol: &'static str,
    outcome: Outcome,
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
struct Record {
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
/// Preserve every completed check in declaration order, including successful checks.
fn records(records: Vec<CheckRecord>) -> Vec<Record> {
    records
        .into_iter()
        .map(|record| Record {
            spec_path: record.path,
            condition: record.condition,
            requirement: record.requirement,
            evaluated_count: record.evaluated_count.to_string(),
            failed_count: record.failed_count.to_string(),
            output_rows: record.output_rows.to_string(),
            offending_rows: record.offending_rows.into_iter().map(identity).collect(),
        })
        .collect()
}
/// Expose resource policy separately from semantic conversion/reduction/check failures.
fn failure(error: ExecutionError<Infallible>) -> Result<Outcome, Error> {
    Ok(match error {
        ExecutionError::BaselineAmbiguity {
            path,
            column,
            date,
            match_count,
            partition,
        } => Outcome::Condition {
            diagnostic: crate::numeric_transport::baseline_ambiguity(
                path,
                column,
                date,
                match_count,
            )?,
            identity: None,
            partition: Some(
                partition
                    .into_iter()
                    .map(|(name, value)| PartitionValue {
                        name,
                        value: ScalarValue::from_core(value),
                    })
                    .collect(),
            ),
            verifications: None,
        },

        ExecutionError::MultipleValues {
            path,
            identifier,
            value_count,
            identity: keys,
        } => Outcome::Condition {
            partition: None,
            diagnostic: crate::numeric_transport::multiple_values(path, identifier, value_count)?,
            identity: keys.map(identity),
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
            verifications: records(found),
        },
        ExecutionError::VerificationFailures(found) => Outcome::Failure {
            phase: "verification",
            verifications: records(found),
        },
        ExecutionError::Conversion {
            path,
            error,
            identity: keys,
            ..
        } => Outcome::Condition {
            partition: None,
            verifications: None,
            diagnostic: conversion(error, path),
            identity: keys.map(identity),
        },
        ExecutionError::Predicate { error, .. } => Outcome::Condition {
            partition: None,
            verifications: None,
            diagnostic: crate::numeric_transport::predicate(error)?,
            identity: None,
        },
        ExecutionError::Reduction {
            path,
            error: TableReductionError::Reduction(ReductionError::Arithmetic { error, .. }),
            identity: keys,
        } => Outcome::Condition {
            partition: None,
            verifications: None,
            diagnostic: arithmetic(error, path),
            identity: keys.map(identity),
        },
        ExecutionError::VerificationPredicate {
            error,
            records: completed,
        } => Outcome::Condition {
            partition: None,
            diagnostic: crate::numeric_transport::predicate(error)?,
            identity: None,
            verifications: Some(records(completed)),
        },
        ExecutionError::SchemaMismatch => return Err(Error::InvalidRequest),
        // Admitted references and normalized owned Arrow values make other semantic
        // and coordinate failures impossible. Never invent missing or acceptance.
        _ => return Err(Error::Internal),
    })
}
