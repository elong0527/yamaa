//! Bounded dataset/1 typed-plan bridge over copied, verified Arrow snapshots.
//! This protocol is not a specification compiler, file runner or default backend.
use crate::{
    dataset_profile::{DatasetProfile, ProfilePhase},
    function_transport::{CallbackError, FunctionTransportError},
    numeric_transport::{arithmetic, conversion, Diagnostic},
    predicate_transport::Predicate,
    scalar_transport::{ScalarValue, MAX_REQUEST_BYTES},
    table_transport::{bounded_json, decode_snapshot, encode_dataset, TableTransportError},
};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_core::{
    reduction::{NumericReducer, ReductionError},
    table::{Column, TableAccess, TableSchema},
    value::ColumnType,
};
use yamaa_engine::{
    dataset::{self, CheckRecord, DatasetPlan, ExecutionError, Limits, Resource, RowIdentity},
    table_grouping::GroupingError,
    table_reduction::TableReductionError,
};

#[path = "dataset_functions.rs"]
mod functions;

const PROTOCOL: &str = "dataset/1";
/// Discover additive typed-plan features before callers acquire source data.
pub fn capabilities() -> &'static str {
    r#"{"protocol":"dataset/1","features":["row_filter","predicate_checks","key_grain","window_numbering","window_filter","window_values","window_baseline","root_filter","source_filter","source_selection","multi_source","named_intermediate","numeric_compute","unconvertible","row_source_lookup","host_functions","function_source_collection","grouped_count","predicate_regex"]}"#
}

/// Bound host argument collections before copying any source buffers.
pub const MAX_SOURCES: usize = 8;
const MAX_SOURCE_CELLS: usize = 262_144;
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
    Function(FunctionTransportError),
    FunctionBinding,
    Table(TableTransportError),
    Internal,
}
impl DatasetTransportError {
    /// Host adapters distinguish unexpected internal failures from rejected requests.
    pub fn is_internal(self) -> bool {
        matches!(
            self,
            Self::Internal
                | Self::Table(TableTransportError::Internal)
                | Self::Function(
                    FunctionTransportError::Internal | FunctionTransportError::Interrupted
                )
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
            Self::Function(error) => return error.fmt(f),
            Self::FunctionBinding => {
                "dataset callback bindings do not match the admitted signatures"
            }
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
    Compute(Compute),
    Function(functions::Call),
    Number(Window),
    Window(Window),
    Source(usize),
    Collect(CollectedSource),
    Lookup(Lookup),
    RowLookup(RowLookup),
    Intermediate(IntermediateRead),
    Column(usize),
    Reduce(Reduction),
    Count(Count),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Compute {
    text: String,
    bindings: Vec<crate::predicate_transport::Binding>,
}

impl Compute {
    /// Compile the supported numerical policy and bind every name before snapshot decoding.
    fn prepare(self, path: &str) -> Result<dataset::BoundNumeric, Error> {
        use yamaa_core::{
            numeric_compiler::{compile_numeric, CompileError, CompileLimits},
            numeric_parser::ParseError,
        };
        if self.bindings.len() > 4096 {
            return Err(Error::RequestLimit);
        }
        let expression = compile_numeric(&self.text, path, CompileLimits::default()).map_err(
            |error| match error {
                CompileError::ResolutionLimit { .. }
                | CompileError::Parse(ParseError::Limit { .. }) => Error::RequestLimit,
                _ => Error::InvalidPlan,
            },
        )?;
        let bindings = self
            .bindings
            .into_iter()
            .map(crate::predicate_transport::Binding::prepare)
            .collect::<Result<Vec<_>, _>>()?;
        dataset::BoundNumeric::new(expression, bindings).map_err(|_| Error::InvalidPlan)
    }
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
struct SourceSelection {
    order_by: Vec<OrderTerm>,
    keep: Keep,
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Keep {
    First,
    Last,
}
impl SourceSelection {
    /// Bound source order terms before the engine validates their coordinates.
    fn prepare(self) -> Result<dataset::SourceSelection, Error> {
        if self.order_by.len() > MAX_COLUMNS {
            return Err(Error::RequestLimit);
        }
        Ok(dataset::SourceSelection {
            order_by: self
                .order_by
                .into_iter()
                .map(|term| dataset::OrderTerm {
                    column: term.column,
                    descending: term.descending,
                    nulls_first: term.nulls_first,
                })
                .collect(),
            keep: match self.keep {
                Keep::First => dataset::Keep::First,
                Keep::Last => dataset::Keep::Last,
            },
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectedSource {
    column: usize,
    identifier: String,
    #[serde(default)]
    filter: Option<Predicate>,
    #[serde(default)]
    selection: Option<SourceSelection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Lookup {
    source: usize,
    column: usize,
    keys: Vec<MatchKey>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RowMatchKey {
    source_column: usize,
    driver_column: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RowLookup {
    source: usize,
    column: usize,
    keys: Vec<RowMatchKey>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntermediateRead {
    index: usize,
    column: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Intermediate {
    identifier: String,
    path: String,
    source: usize,
    keys: Vec<MatchKey>,
    #[serde(default)]
    filter: Option<Predicate>,
    #[serde(default)]
    selection: Option<SourceSelection>,
    #[serde(default)]
    no_match: Option<ScalarValue>,
}

impl Intermediate {
    /// Admit bounded declaration payloads before accepting source IPC.
    fn prepare(self) -> Result<dataset::Intermediate, Error> {
        path(&self.path)?;
        path(&self.identifier)?;
        if self.keys.len() > MAX_COLUMNS {
            return Err(Error::RequestLimit);
        }
        Ok(dataset::Intermediate {
            identifier: self.identifier,
            path: self.path,
            source: self.source,
            keys: self
                .keys
                .into_iter()
                .map(|key| dataset::MatchKey {
                    source_column: key.source_column,
                    output_column: key.output_column,
                })
                .collect(),
            filter: self.filter.map(Predicate::prepare).transpose()?,
            selection: self.selection.map(SourceSelection::prepare).transpose()?,
            no_match: self
                .no_match
                .map(|value| value.into_core().map_err(|_| Error::InvalidScalar))
                .transpose()?,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchKey {
    source_column: usize,
    output_column: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SecondarySource {
    name: String,
    schema: Vec<Field>,
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
    identifier: Option<String>,
    column: usize,
    reducer: Reducer,
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Count {
    column: Option<usize>,
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
#[serde(deny_unknown_fields)]
struct ConversionHandler {
    assignment_path: String,
    path: String,
    value: ScalarValue,
}
impl ConversionHandler {
    /// Decode literal replacement and both authored paths before source access.
    fn prepare(self) -> Result<dataset::ConversionHandler, Error> {
        path(&self.assignment_path)?;
        path(&self.path)?;
        Ok(dataset::ConversionHandler {
            assignment_path: self.assignment_path,
            handler: yamaa_engine::numeric_lifecycle::LiteralHandler {
                spec_path: self.path,
                value: self.value.into_core().map_err(|_| Error::InvalidScalar)?,
            },
        })
    }
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
    #[serde(default)]
    functions: Vec<crate::function_signature::Signature>,
    #[serde(default)]
    unconvertible: Vec<ConversionHandler>,
    source: Vec<Field>,
    #[serde(default)]
    secondary: Vec<SecondarySource>,
    #[serde(default)]
    intermediates: Vec<Intermediate>,
    output: Vec<Field>,
    templates: Vec<Template>,
    columns: Vec<Assignment>,
    keys: Vec<usize>,
    verifications: Vec<Verification>,
}

/// The prepared bridge owns its admitted plan; no host objects or source buffers survive.
pub struct PreparedDataset {
    plan: DatasetPlan,
    secondary_count: usize,
    functions: Vec<yamaa_engine::function_invocation::InvocationPlan>,
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

/// Admit a complete multi-source request before decoding any source snapshot.
pub fn execute_dataset_sources(
    request: &str,
    source: &[u8],
    secondary: &[&[u8]],
) -> Result<DatasetResponse, Error> {
    PreparedDataset::parse(request)?.execute_sources(source, secondary)
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
            if request.unconvertible.len() > MAX_COLUMNS * (MAX_TEMPLATES + 1)
                || request.templates.len() > MAX_TEMPLATES
                || request.verifications.len() > MAX_CHECKS
                || request.keys.len() > MAX_COLUMNS
                || request.secondary.len() >= MAX_SOURCES
                || request.intermediates.len() > MAX_COLUMNS
            {
                return Err(Error::RequestLimit);
            }
            let mut functions = functions::Admission::new(request.functions)?;
            let secondary_count = request.secondary.len();
            let secondary = request
                .secondary
                .into_iter()
                .map(|relation| {
                    if relation.name.is_empty() {
                        return Err(Error::InvalidRequest);
                    }
                    if relation.name.len() > MAX_NAME {
                        return Err(Error::RequestLimit);
                    }
                    Ok(dataset::SecondarySource {
                        name: relation.name,
                        schema: schema(relation.schema)?,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
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
                        assignments: assignments(template.assignments, &mut functions)?,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let columns = assignments(request.columns, &mut functions)?;
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
            let handlers = request
                .unconvertible
                .into_iter()
                .map(ConversionHandler::prepare)
                .collect::<Result<Vec<_>, Error>>()?;
            let plan = DatasetPlan::new_with_intermediates(
                dataset::SourceSchemas {
                    primary: source,
                    secondary,
                },
                request
                    .intermediates
                    .into_iter()
                    .map(Intermediate::prepare)
                    .collect::<Result<Vec<_>, Error>>()?,
                output,
                templates,
                columns,
                request.keys,
                verifications,
            )
            .and_then(|plan| plan.with_conversion_handlers(handlers))
            .map_err(|_| Error::InvalidPlan)?;
            Ok(Self {
                plan,
                secondary_count,
                functions: functions.signatures,
            })
        })
        .map_err(|_| Error::Internal)?
    }

    /// Copy and validate a complete snapshot; every run gets fresh resource counters.
    pub fn execute(&self, source: &[u8]) -> Result<DatasetResponse, Error> {
        self.execute_sources(source, &[])
    }

    /// Decode independently owned snapshots under one aggregate input policy.
    pub fn execute_sources(
        &self,
        source: &[u8],
        secondary: &[&[u8]],
    ) -> Result<DatasetResponse, Error> {
        self.execute_sources_functions(source, secondary, &mut functions::Unavailable)
    }

    /// Borrow admitted metadata for a host-owned, stable registry; labels do not prove activation.
    pub fn function_signatures(&self) -> &[yamaa_engine::function_invocation::InvocationPlan] {
        &self.functions
    }

    /// Admit every binding before decoding IPC, then invoke only on the caller's thread.
    /// Call effects are neither retried nor rolled back after any failure or unwind.
    pub fn execute_sources_functions(
        &self,
        source: &[u8],
        secondary: &[&[u8]],
        functions: &mut dyn dataset::FunctionBindings<Error = CallbackError>,
    ) -> Result<DatasetResponse, Error> {
        catch_unwind(AssertUnwindSafe(|| {
            self.execute_bound(source, secondary, functions, None)
        }))
        .map_err(|_| Error::Internal)?
    }

    /// Measure one actual execution with unchanged result bytes and callback authority.
    /// The profile is caller-owned; resource/host failures never become successful samples.
    pub fn execute_sources_functions_profiled(
        &self,
        source: &[u8],
        secondary: &[&[u8]],
        functions: &mut dyn dataset::FunctionBindings<Error = CallbackError>,
        profile: &mut DatasetProfile,
    ) -> Result<DatasetResponse, Error> {
        catch_unwind(AssertUnwindSafe(|| {
            self.execute_bound(source, secondary, functions, Some(profile))
        }))
        .map_err(|_| Error::Internal)?
    }

    /// The containment boundary includes binding metadata inspection and all callback effects.
    fn execute_bound(
        &self,
        source: &[u8],
        secondary: &[&[u8]],
        functions: &mut dyn dataset::FunctionBindings<Error = CallbackError>,
        mut profile: Option<&mut DatasetProfile>,
    ) -> Result<DatasetResponse, Error> {
        for (slot, signature) in self.functions.iter().enumerate() {
            if functions.signature(slot) != Some(signature) {
                return Err(Error::FunctionBinding);
            }
        }
        if secondary.len() != self.secondary_count {
            return Err(Error::InvalidRequest);
        }
        let bytes = secondary
            .iter()
            .try_fold(source.len(), |sum, bytes| sum.checked_add(bytes.len()));
        if bytes.is_none_or(|size| size > crate::table_transport::MAX_INPUT_BYTES) {
            return Err(Error::Table(TableTransportError::InputLimit));
        }
        if let Some(profile) = profile.as_deref_mut() {
            profile.enter(ProfilePhase::SnapshotDecode);
        }
        {
            let source = functions::Snapshot(decode_snapshot(source).map_err(Error::Table)?);
            let secondary = secondary
                .iter()
                .map(|bytes| {
                    decode_snapshot(bytes)
                        .map(functions::Snapshot)
                        .map_err(Error::Table)
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let total_cells = secondary.iter().chain(core::iter::once(&source)).try_fold(
                0_usize,
                |sum, table| {
                    table
                        .row_count()
                        .checked_mul(table.schema().columns().len())
                        .and_then(|cells| sum.checked_add(cells))
                },
            );
            if total_cells.is_none_or(|cells| cells > MAX_SOURCE_CELLS) {
                return Err(Error::Table(TableTransportError::ShapeLimit));
            }
            let references: Vec<&dyn TableAccess<Error = CallbackError>> = secondary
                .iter()
                .map(|table| table as &dyn TableAccess<Error = CallbackError>)
                .collect();
            let attempt = if let Some(profile) = profile {
                self.plan.execute_with_phase_observer(
                    &source,
                    &references,
                    functions,
                    LIMITS,
                    &mut |phase| profile.engine_phase(phase),
                )
            } else {
                self.plan
                    .execute_observed_functions(&source, &references, functions, LIMITS)
            };
            response(attempt)
        }
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
fn assignments(
    values: Vec<Assignment>,
    functions: &mut functions::Admission,
) -> Result<Vec<dataset::Assignment>, Error> {
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
                Expression::Function(call) => dataset::Expression::Function(functions.bind(call)?),
                Expression::Compute(expression) => {
                    dataset::Expression::Compute(expression.prepare(&assignment.path)?)
                }
                Expression::Source(column) => dataset::Expression::Source(column),
                Expression::Intermediate(read) => dataset::Expression::Intermediate {
                    index: read.index,
                    column: read.column,
                },
                Expression::Collect(source) => {
                    path(&source.identifier)?;
                    dataset::Expression::Collect {
                        column: source.column,
                        identifier: source.identifier,
                        filter: source.filter.map(Predicate::prepare).transpose()?,
                        selection: source.selection.map(SourceSelection::prepare).transpose()?,
                    }
                }
                Expression::RowLookup(lookup) => {
                    if lookup.keys.len() > MAX_COLUMNS {
                        return Err(Error::RequestLimit);
                    }
                    dataset::Expression::RowLookup(dataset::RowLookup {
                        source: lookup.source,
                        column: lookup.column,
                        keys: lookup
                            .keys
                            .into_iter()
                            .map(|key| dataset::RowMatchKey {
                                source_column: key.source_column,
                                driver_column: key.driver_column,
                            })
                            .collect(),
                    })
                }
                Expression::Lookup(lookup) => {
                    if lookup.keys.len() > MAX_COLUMNS {
                        return Err(Error::RequestLimit);
                    }
                    dataset::Expression::Lookup(dataset::Lookup {
                        source: lookup.source,
                        column: lookup.column,
                        keys: lookup
                            .keys
                            .into_iter()
                            .map(|key| dataset::MatchKey {
                                source_column: key.source_column,
                                output_column: key.output_column,
                            })
                            .collect(),
                    })
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
                Expression::Count(count) => {
                    path(&count.text)?;
                    dataset::Expression::Count {
                        column: count.column,
                        text: count.text,
                    }
                }
                Expression::Reduce(reduction) => {
                    path(&reduction.text)?;
                    if let Some(identifier) = &reduction.identifier {
                        path(identifier)?;
                    }
                    dataset::Expression::Reduce {
                        identifier: reduction.identifier,
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
    #[serde(rename = "condition")]
    FunctionCondition {
        diagnostic: functions::Diagnostic,
        identity: Option<Identity>,
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
fn failure(error: ExecutionError<CallbackError>) -> Result<Outcome, Error> {
    Ok(match error {
        ExecutionError::Function {
            path,
            error,
            identity: keys,
        } => Outcome::FunctionCondition {
            diagnostic: functions::Diagnostic::new(path, error)?,
            identity: keys.map(identity),
        },
        ExecutionError::FunctionBinding { .. } => return Err(Error::FunctionBinding),
        ExecutionError::Numeric {
            error,
            identity: keys,
        } => Outcome::Condition {
            diagnostic: crate::numeric_transport::numeric(error).map_err(|_| Error::Internal)?,
            identity: keys.map(identity),
            matched_key: None,
            partition: None,
            verifications: None,
        },
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
            matched_key: None,
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

        ExecutionError::MultipleMatches {
            path,
            dataset,
            intermediate,
            match_count,
            matched_key,
            identity: keys,
        } => Outcome::Condition {
            diagnostic: crate::numeric_transport::join_condition(
                path,
                dataset,
                intermediate,
                Some(match_count),
            )?,
            identity: keys.map(identity),
            partition: None,
            verifications: None,
            matched_key: Some(
                matched_key
                    .into_iter()
                    .map(|(name, value)| PartitionValue {
                        name,
                        value: ScalarValue::from_core(value),
                    })
                    .collect(),
            ),
        },
        ExecutionError::UnmatchedKey {
            path,
            dataset,
            intermediate,
            matched_key,
            identity: keys,
        } => Outcome::Condition {
            diagnostic: crate::numeric_transport::join_condition(
                path,
                dataset,
                intermediate,
                None,
            )?,
            identity: keys.map(identity),
            partition: None,
            verifications: None,
            matched_key: Some(
                matched_key
                    .into_iter()
                    .map(|(name, value)| PartitionValue {
                        name,
                        value: ScalarValue::from_core(value),
                    })
                    .collect(),
            ),
        },
        ExecutionError::MultipleValues {
            path,
            identifier,
            value_count,
            identity: keys,
        } => Outcome::Condition {
            matched_key: None,
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
            matched_key: None,
            partition: None,
            verifications: None,
            diagnostic: conversion(error, path),
            identity: keys.map(identity),
        },
        ExecutionError::Predicate { error, .. } => Outcome::Condition {
            matched_key: None,
            partition: None,
            verifications: None,
            diagnostic: crate::numeric_transport::predicate(error)?,
            identity: None,
        },
        ExecutionError::ReductionType {
            path,
            expression,
            reducer,
            source,
            actual,
        } => Outcome::Condition {
            matched_key: None,
            partition: None,
            verifications: None,
            identity: None,
            diagnostic: crate::numeric_transport::reduction_type(
                path,
                expression,
                reducer.name(),
                source,
                actual,
            ),
        },
        ExecutionError::Reduction {
            path,
            error: TableReductionError::Reduction(ReductionError::Arithmetic { error, .. }),
            identity: keys,
        } => Outcome::Condition {
            matched_key: None,
            partition: None,
            verifications: None,
            diagnostic: arithmetic(error, path),
            identity: keys.map(identity),
        },
        ExecutionError::VerificationDeclaration {
            path,
            condition,
            requirement,
            reason,
            records: completed,
        } => Outcome::Condition {
            matched_key: None,
            partition: None,
            identity: None,
            diagnostic: crate::numeric_transport::declaration(path, condition, requirement, reason),
            verifications: Some(records(completed)),
        },
        ExecutionError::VerificationPredicate {
            error,
            records: completed,
        } => Outcome::Condition {
            matched_key: None,
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

/// Encode engine observations through one existing diagnostic vocabulary.
fn response(attempt: dataset::ExecutionAttempt<CallbackError>) -> Result<DatasetResponse, Error> {
    let (table, outcome) = match attempt.result {
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
        if error == TableTransportError::OutputLimit {
            Error::OutputLimit
        } else {
            Error::Internal
        }
    })?;
    Ok(DatasetResponse { table, outcome })
}

/// Execute an already bound shared compiler plan over an owned source snapshot.
/// Only the closed compiler calls this entry point; callbacks/secondary sources
/// are not implicit, and the same response limits apply as the typed-plan bridge.
pub(crate) fn execute_specification_plan(
    plan: &DatasetPlan,
    source: &crate::arrow_table::ArrowTable,
) -> Result<DatasetResponse, Error> {
    catch_unwind(AssertUnwindSafe(|| {
        let cells = source
            .row_count()
            .checked_mul(source.schema().columns().len());
        if cells.is_none_or(|cells| cells > MAX_SOURCE_CELLS) {
            return Err(Error::Table(TableTransportError::ShapeLimit));
        }
        let source = functions::Snapshot(source);
        response(plan.execute_observed(&source, LIMITS))
    }))
    .map_err(|_| Error::Internal)?
}
