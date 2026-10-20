//! The one result builder: format one observed attempt, with or without a
//! project, and publish only through explicit host authority. Classified
//! resource findings retain observations; opaque host failures stay opaque.
use crate::json;
use crate::{
    dataset_result::{Condition, Outcome},
    project_report::{Classified, FactMap},
    report_storage::{StoredReport, StoredTable},
    specification_diagnostics,
    specification_run::{Attempt, BoundaryFailure, CapturedAttempt, PortError, PreparedRun},
};
use yamaa_core::application_issue::{Fault, Stage};
use yamaa_core::dataset::ExecutionError;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::json::{Map, Value};
use yamaa_core::numeric_lifecycle::HandlerCount;
use yamaa_core::specification_run::{RunError, SourceIssue, SourceOrigin};
use yamaa_core::{table::TableAccess, value::Value as Scalar};

pub struct Identity<'a> {
    pub runtime: &'a str,
    pub runtime_version: &'a str,
    pub engine_version: &'a str,
    pub example: &'a str,
    pub specification: &'a str,
    pub base_directory: &'a str,
}
impl<'a> Identity<'a> {
    /// Every host reports the same engine version and base directory; only
    /// the runtime name, its version, the example and the specification vary.
    pub fn new(
        runtime: &'a str,
        runtime_version: &'a str,
        example: &'a str,
        specification: &'a str,
    ) -> Self {
        Self {
            runtime,
            runtime_version,
            engine_version: yamaa_core::VERSION,
            example,
            specification,
            base_directory: ".",
        }
    }
    /// Map the five positional report fields both hosts capture, in order:
    /// runtime version, engine version, example, specification, base directory.
    pub fn from_fields(runtime: &'a str, fields: &'a [String]) -> Self {
        Self {
            runtime,
            runtime_version: &fields[0],
            engine_version: &fields[1],
            example: &fields[2],
            specification: &fields[3],
            base_directory: &fields[4],
        }
    }
}
#[derive(Debug)]
pub enum Error {
    UnsupportedOutcome,
    CapabilityUnavailable { feature: &'static str },
    InvalidObservation,
    ResourceLimit(crate::resource_diagnostics::Refusal),
}
impl From<crate::source_diagnostics::SourceProjectionError> for Error {
    fn from(error: crate::source_diagnostics::SourceProjectionError) -> Self {
        match error {
            crate::source_diagnostics::SourceProjectionError::InvalidContext => {
                Self::InvalidObservation
            }
        }
    }
}
/// One authored scalar as a plain context value: text, number, boolean or null.
fn plain(value: &Scalar) -> Value {
    match value {
        Scalar::Missing => Value::Null,
        Scalar::Str(v) => json!(v),
        Scalar::Int(v) => json!(v),
        Scalar::Float(v) => json!(v.get()),
        Scalar::Bool(v) => json!(v),
        Scalar::Date(v) => json!(v.to_string()),
        Scalar::DateTime(v) => json!(v.to_string()),
    }
}
fn fields(values: &[(String, Scalar)]) -> Map<String, Value> {
    values
        .iter()
        .map(|(name, value)| (name.clone(), plain(value)))
        .collect()
}
/// An execution finding with the row keys, matched intermediate key or
/// baseline partition it names added to its context.
pub(crate) fn condition(run: &PreparedRun, condition: &Condition) -> Result<Diagnostic, Error> {
    let mut diagnostic = condition.diagnostic.clone();
    if let Some(identity) = &condition.identity {
        let names = run.compiled().key_names().collect::<Vec<_>>();
        if names.len() != identity.values.len() {
            return Err(Error::InvalidObservation);
        }
        let keys = names
            .into_iter()
            .zip(&identity.values)
            .map(|(name, value)| (name.to_owned(), plain(value)))
            .collect::<Map<_, _>>();
        diagnostic
            .context
            .insert("keys".into(), json!([Value::Object(keys)]));
    }
    if let Some(matched) = &condition.matched_key {
        let names = matched
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        diagnostic.context.insert("key".into(), json!(names));
        diagnostic
            .context
            .insert("intermediate_key".into(), Value::Object(fields(matched)));
    }
    if let Some(partition) = &condition.partition {
        diagnostic
            .context
            .insert("keys".into(), json!([Value::Object(fields(partition))]));
    }
    Ok(diagnostic)
}
/// A classified capture failure terminates the request sequence. Reject malformed
/// observations rather than attaching a resource finding to an unrelated failure.
fn captured_failure<C, E>(
    attempt: &CapturedAttempt<C, E>,
) -> Result<&crate::specification_run::CapturedSource<E>, Error> {
    if !matches!(attempt.result, Err(PortError::Capture(_))) {
        return Err(Error::UnsupportedOutcome);
    }
    let (last, prefix) = attempt
        .sources
        .split_last()
        .ok_or(Error::InvalidObservation)?;
    if last.read.captured
        || last.read.failure.is_none()
        || last.snapshot.is_some()
        || last.owned_snapshot.is_some()
        || last.table.is_some()
        || prefix
            .iter()
            .any(|s| !s.read.captured || s.read.failure.is_some())
    {
        return Err(Error::UnsupportedOutcome);
    }
    Ok(last)
}

fn inspected_failures<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a CapturedAttempt<C, E>,
) -> Result<&'a [crate::specification_run::InspectionFailure<C>], Error> {
    let Err(PortError::Inspect(failures)) = &attempt.result else {
        return Err(Error::UnsupportedOutcome);
    };
    if !attempt.sources.is_empty() || failures.is_empty() {
        return Err(Error::InvalidObservation);
    }
    let mut declarations = run.compiled().sources().iter();
    for failure in failures {
        if failure.failure.is_none() {
            return Err(Error::UnsupportedOutcome);
        }
        if !declarations.any(|source| source == &failure.source) {
            return Err(Error::InvalidObservation);
        }
    }
    Ok(failures)
}

/// The findings of a run that stopped while reading its sources, each
/// naming its own declaration.
fn run_findings(
    run: &PreparedRun,
    error: &RunError<crate::specification_run::Error>,
) -> Result<Option<Vec<Diagnostic>>, Error> {
    use crate::resource_diagnostics as resources;
    use crate::specification_run::Error as E;
    let sources = run.compiled().sources();
    Ok(match error {
        RunError::Decode(error) => resources::source_findings(error, run.source())?,
        RunError::Sources(errors) if sources.len() == 1 => {
            let (_, error) = errors.last().ok_or(Error::InvalidObservation)?;
            resources::source_findings(error, run.source())?
        }
        RunError::Sources(errors) => {
            let errors = errors
                .iter()
                .map(|(index, error)| {
                    Ok((sources.get(*index).ok_or(Error::InvalidObservation)?, error))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            resources::sources_findings(&errors)?
        }
        RunError::AdmittedSources(errors) => {
            let values = errors
                .iter()
                .map(|(_, issue)| match issue {
                    SourceIssue::OdmValues(values) => Some(E::OdmValues(values.clone())),
                    SourceIssue::Decode(_) => None,
                })
                .collect::<Vec<_>>();
            let errors = errors
                .iter()
                .zip(&values)
                .map(|((index, issue), value)| {
                    let error = match (issue, value) {
                        (SourceIssue::Decode(error), _) => error,
                        (_, Some(value)) => value,
                        (SourceIssue::OdmValues(_), None) => unreachable!("ODM values are held"),
                    };
                    Ok((sources.get(*index).ok_or(Error::InvalidObservation)?, error))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            resources::sources_findings(&errors)?
        }
        RunError::Bind(error) => specification_diagnostics::binding(error, run.source()),
        error @ RunError::Boundary { .. } => resources::run_limit(error, sources)
            .map(|refusal| vec![refusal.diagnostic(Stage::Ingest, Fault::SourceLimit)]),
    })
}

/// The observations of a failed attempt that called no project function,
/// as its report holds them before output preparation. Outcomes outside this
/// formatter are refused instead of inventing observations.
pub fn failure<C, E>(
    run: &PreparedRun,
    attempt: &Attempt<C, E>,
    id: Identity<'_>,
) -> Result<StoredReport, Error> {
    dataset_failure(run, &attempt.dataset, &FactMap::new(), &id).map_err(|refusal| match refusal {
        Refusal::Report(error) => error,
        Refusal::Opaque(_) | Refusal::Boundary(_) => Error::UnsupportedOutcome,
    })
}

/// Refuse outcomes outside this formatter instead of inventing observations.
fn dataset_failure<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a CapturedAttempt<C, E>,
    supplied: &FactMap<'a, '_, E>,
    id: &Identity<'_>,
) -> Result<StoredReport, Refusal<'a, E>> {
    if attempt.sources.is_empty() && !matches!(attempt.result, Err(PortError::Inspect(_))) {
        return Err(Error::UnsupportedOutcome.into());
    }
    let mut verifications = Vec::new();
    let diagnostics = match &attempt.result {
        Ok(execution) => {
            let Err(error) = &execution.result else {
                return Err(Error::UnsupportedOutcome.into());
            };
            if let ExecutionError::ProjectFunction {
                path,
                error,
                identity,
            } = error.as_ref()
            {
                // A project function's failure keeps its host payload: the
                // completed checks, then the invocation finding.
                let (checks, mut findings) = crate::project_report::checked_records(
                    run,
                    &execution.retained_verifications,
                    "verification",
                    id,
                )?;
                verifications.extend(checks);
                findings.push(
                    crate::project_function_diagnostics::invocation(
                        path,
                        error,
                        identity.as_ref(),
                        &run.compiled().key_names().collect::<Vec<_>>(),
                        crate::project_report::invocation_host(supplied, &error.kind),
                    )
                    .map_err(crate::project_report::project_error)?,
                );
                findings
            } else {
                let response = crate::dataset_result::response(execution)
                    .map_err(|_| Error::UnsupportedOutcome)?;
                if !response.retained_verifications.is_empty() {
                    verifications.extend(
                        check_observations(
                            run,
                            response.retained_verifications,
                            yamaa_core::verification_diagnostics::Phase::Verification,
                            id,
                        )?
                        .0,
                    );
                }
                match &response.outcome {
                    Outcome::Condition(found) => {
                        if let Some((phase, completed)) = response.failed_verifications {
                            verifications.extend(check_observations(run, completed, phase, id)?.0);
                        }
                        if response.intermediate_diagnostics.is_empty() {
                            vec![condition(run, found)?]
                        } else {
                            response.intermediate_diagnostics.to_vec()
                        }
                    }
                    Outcome::Failure => {
                        let (phase, completed) = response
                            .failed_verifications
                            .ok_or(Error::InvalidObservation)?;
                        let (records, diagnostics) = check_observations(run, completed, phase, id)?;
                        verifications.extend(records);
                        diagnostics
                    }
                    Outcome::Limit(bound) => vec![crate::resource_diagnostics::outcome(
                        bound,
                        run.compiled().sources(),
                        run.compiled().driver_index(),
                    )
                    .map(|refusal| refusal.diagnostic(Stage::Execute, Fault::ExecutionLimit))
                    .ok_or(Error::UnsupportedOutcome)?],
                    Outcome::Success => return Err(Error::UnsupportedOutcome.into()),
                }
            }
        }
        Err(PortError::Run(error)) => run_findings(run, error)?.ok_or(Error::UnsupportedOutcome)?,
        Err(PortError::Capture(_)) => {
            let source = captured_failure(attempt)?;
            let written = run
                .document()
                .written_source_path(&source.read.source.name)
                .ok_or(Error::InvalidObservation)?;
            vec![source
                .read
                .failure
                .ok_or(Error::InvalidObservation)?
                .diagnostic(&source.read.source.name, written)]
        }
        Err(PortError::Inspect(_)) => {
            let failures = inspected_failures(run, attempt)?;
            let known_filter_types =
                yamaa_core::specification_run::inspection_declaration_diagnostics(
                    run.compiled(),
                    failures,
                )
                .cloned()
                .collect::<Vec<_>>();
            if !known_filter_types.is_empty() {
                known_filter_types
            } else {
                failures
                    .iter()
                    .map(|failure| {
                        let written = run
                            .document()
                            .written_source_path(&failure.source.name)
                            .ok_or(Error::InvalidObservation)?;
                        Ok(failure
                            .failure
                            .ok_or(Error::InvalidObservation)?
                            .diagnostic(&failure.source.name, written))
                    })
                    .collect::<Result<Vec<_>, Error>>()?
            }
        }
        _ => return Err(Error::UnsupportedOutcome.into()),
    };
    let mut report = envelope(run, attempt, id)?;
    if let Ok(execution) = &attempt.result {
        set_handler_counts(&mut report, &execution.handler_counts);
    }
    report.set_diagnostics(diagnostics);
    report["verifications"] = json!(verifications);
    Ok(report)
}
pub(crate) fn table_observation_projected<T: TableAccess>(
    table: &T,
    id: &Identity<'_>,
    stage: &str,
    name: &str,
) -> Result<StoredTable, Error> {
    let columns = table.schema().columns();

    let rows = crate::report_storage::encode_rows(table)?;
    Ok(StoredTable {
        metadata: json!({"specification":id.specification,"stage":stage,"name":name,"columns":columns.iter().map(|c|&c.name).collect::<Vec<_>>(),"types":columns.iter().map(|c|specification_diagnostics::type_name(c.kind)).collect::<Vec<_>>(),"rows":null}),
        rows,
    })
}

fn envelope<C, E>(
    run: &PreparedRun,
    attempt: &CapturedAttempt<C, E>,
    id: &Identity<'_>,
) -> Result<StoredReport, Error> {
    if attempt.sources.is_empty() {
        inspected_failures(run, attempt)?;
    }
    if attempt.sources.iter().any(|source| !source.read.captured) {
        captured_failure(attempt)?;
    } else if attempt
        .sources
        .iter()
        .any(|source| source.read.failure.is_some())
    {
        return Err(Error::InvalidObservation);
    }
    envelope_sources(&attempt.sources, id, false)
}

pub(crate) fn envelope_sources<T: TableAccess>(
    sources: &[yamaa_core::specification_run::CapturedSource<T>],
    id: &Identity<'_>,
    partial: bool,
) -> Result<StoredReport, Error> {
    let mut tables = Vec::new();
    if partial || sources.iter().all(|source| source.table.is_some()) {
        for source in sources {
            let Some(table) = source.table.as_ref() else {
                continue;
            };
            tables.push(table_observation_projected(
                table,
                id,
                "source",
                &source.read.source.name,
            )?);
        }
    }
    let mut reads = Vec::new();
    for source in sources
        .iter()
        .filter(|source| source.read.origin == Some(SourceOrigin::Resource))
    {
        if !partial && source.read.snapshots_created.is_none() {
            return Err(Error::InvalidObservation);
        }
        reads.push(json!({"base_directory":id.base_directory,"path":source.read.source.path,"outcome":if source.read.captured {"captured"} else if partial && source.read.snapshots_created.is_none() {"attempted"} else {"failure"},"condition":source.read.failure.map(|failure| failure.code().definition().condition),"snapshots_created":source.read.snapshots_created}));
    }
    Ok(StoredReport::with_tables(
        json!({"report_version":"0.3.0-draft","runtime":id.runtime,"backend":"rust","runtime_version":id.runtime_version,"engine_version":id.engine_version,"example":id.example,
        "outcome":"failure","artifacts":[],"diagnostics":[],"unsupported":[],"handler_counts":[],
        "nodes":[{"specification":id.specification,"outcome":"failure","diagnostics":[],"unsupported":[],"handler_counts":[]}],
        "tables":[],"verifications":[],"callbacks":[],
        "source_reads":reads,"error":null}),
        tables,
    ))
}
/// Record every registered handler path and its count, in registration order.
pub(crate) fn set_handler_counts(report: &mut Value, counts: &[HandlerCount]) {
    report["handler_counts"] = Value::Array(
        counts
            .iter()
            .map(|entry| {
                json!({"spec_path":entry.spec_path,"handler":entry.handler.name(),"count":entry.count})
            })
            .collect(),
    );
    report["nodes"][0]["handler_counts"] = report["handler_counts"].clone();
}

pub use yamaa_core::specification_output::ArtifactPort;
#[derive(Debug)]
pub enum CompleteError<E> {
    Report(Error),
    Publish(E),
}

/// An owned build result; obtaining it never calls a publisher. Table bytes are
/// the admitted output projection, while private report artifacts are prospective.
/// Every result holds its verified run report.
#[derive(Debug)]
pub struct BuildResult {
    prepared: yamaa_core::specification_output::PreparedOutput<StoredReport>,
    table: Option<(crate::public_table::PublicTable, Vec<u8>)>,
    issues: Vec<Diagnostic>,
    report: crate::run_report::HeldReport,
}
impl BuildResult {
    pub fn output(&self) -> Option<&[u8]> {
        self.table.as_ref().map(|(_, bytes)| bytes.as_slice())
    }
    pub(crate) fn output_table(&self) -> Option<&crate::public_table::PublicTable> {
        self.table.as_ref().map(|(table, _)| table)
    }
    pub fn issues(&self) -> &[Diagnostic] {
        &self.issues
    }
    /// The run report's owned Arrow IPC view.
    pub fn run_report(&self) -> &[u8] {
        self.report.ipc()
    }
    pub(crate) fn report_table(&self) -> &crate::public_table::PublicTable {
        self.report.table()
    }
    /// Whether the specification declares a report file for publication.
    pub(crate) fn report_file(&self) -> bool {
        self.report.file().is_some()
    }
    /// The accepted primary artifact's serialized bytes, shared, not copied.
    pub(crate) fn artifact(&self) -> Option<std::sync::Arc<[u8]>> {
        self.output()?;
        self.prepared
            .artifact()
            .map(yamaa_core::specification_output::Artifact::retained_bytes)
    }
    /// Failed results have only an explicit diagnostic publication capability:
    /// the declared report file. The primary artifact requires the accepted
    /// primary owner.
    pub(crate) fn publication_items(&self) -> Vec<(&str, &[u8], &Value)> {
        let mut items = Vec::new();
        if let Some(file) = self.report.file() {
            items.push(file);
        }
        if self.output().is_some() {
            if let Some(artifact) = self.prepared.artifact() {
                items.push((
                    artifact.path(),
                    artifact.bytes(),
                    self.prepared.prepared_report()["artifacts"]
                        .as_array()
                        .expect("prepared artifacts")
                        .last()
                        .expect("primary artifact"),
                ));
            }
        }
        items
    }
    pub(crate) fn publication_observations(&self, completed: usize) -> Result<StoredReport, Error> {
        let items = self.publication_items();
        if completed > items.len() || items.len() > 2 {
            return Err(Error::InvalidObservation);
        }
        let original = self.prepared.prepared_report();

        // Do not clone prospective artifacts merely to discard them. The
        // complete actual prefix is admitted before any report ownership grows.
        let mut report = original.without_artifacts_cloned()?;
        report["artifacts"] = Value::Array(
            items
                .into_iter()
                .take(completed)
                .map(|(_, _, value)| value.clone())
                .collect(),
        );
        Ok(report)
    }
    /// Build-phase observations exclude later save requests and their artifacts.
    pub fn observations(&self) -> Value {
        self.prepared.prepared_report().observations_value(true)
    }
    pub fn observations_json(&self) -> String {
        self.prepared.prepared_report().observations_json(true)
    }
    /// Return complete publication observations only after the publisher succeeds.
    pub fn save<P: ArtifactPort>(
        &self,
        publisher: &mut P,
    ) -> Result<&StoredReport, yamaa_core::specification_output::SaveError<P::Error>> {
        for (path, bytes, _) in self.publication_items() {
            publisher
                .publish(path, bytes)
                .map_err(yamaa_core::specification_output::SaveError::Publish)?;
        }
        if self.output().is_none() {
            return Err(yamaa_core::specification_output::SaveError::FailedBuild);
        }
        Ok(self.prepared.prepared_report())
    }
}

/// Why one attempt has no public result: its report could not be built, a
/// host failure carries no classification facts, or the activation boundary
/// ended without a classified failure. Each refuses the build with one closed
/// code; the original payload stays with the attempt.
#[derive(Debug)]
pub enum Refusal<'a, E> {
    Report(Error),
    Opaque(&'a E),
    Boundary(&'a BoundaryFailure<E>),
}
impl<E> From<Error> for Refusal<'_, E> {
    fn from(error: Error) -> Self {
        Self::Report(error)
    }
}
impl<E> From<crate::source_diagnostics::SourceProjectionError> for Refusal<'_, E> {
    fn from(error: crate::source_diagnostics::SourceProjectionError) -> Self {
        Self::Report(error.into())
    }
}
impl<E> Refusal<'_, E> {
    /// The issues that refuse a build with no public result.
    pub fn issues(&self) -> crate::file_application::Failure {
        let code = match self {
            Self::Opaque(_) => Fault::OpaqueHost,
            Self::Boundary(_) => Fault::ActivationBoundary,
            Self::Report(Error::ResourceLimit(limit)) => {
                return limit.issues(Stage::Build, Fault::ResultProjection)
            }
            Self::Report(_) => Fault::ResultProjection,
        };
        crate::file_application::rejected(Stage::Build, code)
    }
}

/// Prepare the public result of one attempt without recapturing data or
/// publishing bytes. Every build, with or without a project, reports here;
/// `facts` classify the attempt's retained host failures, and a build with a
/// project also records its activation evidence.
pub fn build_result<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a Attempt<C, E>,
    id: Identity<'_>,
    facts: &[Classified<'a, E>],
) -> Result<BuildResult, Refusal<'a, E>> {
    let supplied = crate::project_report::supplied(attempt, facts)?;
    let activation = crate::project_report::activation_evidence(run, attempt)?;
    let mut report = match &attempt.boundary {
        Err(boundary) => {
            crate::project_report::activation_failure(run, attempt, boundary, &supplied, &id)?
        }
        Ok(()) => observations(run, &attempt.dataset, &supplied, &id)?,
    };
    if run.has_project() {
        report["activation"] = activation;
    }
    let execution = crate::project_attempt::execution(attempt);
    let facts = crate::run_report::RunFacts::captured(
        &attempt.dataset.sources,
        attempt
            .dataset
            .result
            .as_ref()
            .map_or(&[], |dataset| dataset.template_rows.as_slice()),
    );
    Ok(prepare_result(run, execution, id, report, &facts)?)
}

pub(crate) fn prepare_result(
    run: &PreparedRun,
    execution: Option<&yamaa_core::dataset::Execution>,
    id: Identity<'_>,
    observations: StoredReport,
    facts: &crate::run_report::RunFacts,
) -> Result<BuildResult, Error> {
    use yamaa_core::specification_output::{self as output, CompleteError as E};
    let fallback = observations.clone();
    // Every run reports, so a failed output preparation becomes a failed run
    // whose report records the failure rather than a refused result.
    let mut prepared = output::prepare(
        run.compiled(),
        execution,
        &mut Report {
            run,
            id,
            value: Some(observations),
        },
        &mut Encoder(
            run.compiled().output_profile(),
            run.compiled().output_decimals(),
        ),
    )
    .map_err(|error| match error {
        E::Report(e) | E::Encode(e) => e,
        E::Projection => Error::InvalidObservation,
        E::Limit(error) => Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
            error.resource,
            error.limit,
            error.actual,
            vec!["output.path".into()],
        )),
        E::Publish(never) => match never {},
    })
    .or_else(|error| rejected_output(fallback, &error))?;
    let table = prepared
        .artifact()
        .map(|artifact| {
            crate::public_table::PublicTable::from_table(
                &execution.ok_or(Error::InvalidObservation)?.dataset,
                artifact.projection(),
            )
            .and_then(|table| table.ipc().map(|bytes| (table, bytes)))
            .map_err(|e| match e {
                crate::ipc_table::TableError::Limit {
                    resource,
                    limit,
                    required,
                } => Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
                    resource,
                    limit,
                    required,
                    vec!["output.path".into()],
                )),
                _ => Error::InvalidObservation,
            })
        })
        .transpose();
    let table = match table {
        Ok(table) => table,
        Err(error) => {
            // Host-view preparation grants no output authority. The run fails
            // and its report records why; the primary artifact is revoked.
            prepared = rejected_output(prepared.prepared_report().clone(), &error)?;
            None
        }
    };
    let issues = prepared.prepared_report().diagnostics().to_vec();
    let condition = if table.is_some() && prepared.artifact().is_some() {
        None
    } else {
        Some(
            issues
                .first()
                .map(Diagnostic::condition)
                .ok_or(Error::InvalidObservation)?,
        )
    };
    let report =
        crate::run_report::prepare(run, prepared.prepared_report(), facts, condition, execution)?;
    // Retain complete prospective artifact observations in publication order.
    // Ordinary build observations strip them until an actual Save enters them.
    if let Some((_, _, observation)) = report.file() {
        let mut stored = prepared.prepared_report().clone();
        let Value::Array(primary) = std::mem::take(&mut stored["artifacts"]) else {
            return Err(Error::InvalidObservation);
        };
        let mut artifacts = vec![observation.clone()];
        artifacts.extend(primary);
        stored["artifacts"] = Value::Array(artifacts);
        prepared.replace_report(stored);
    }
    Ok(BuildResult {
        prepared,
        table,
        issues: crate::file_application::collect(issues),
        report,
    })
}

fn rejected_output(
    mut report: StoredReport,
    error: &Error,
) -> Result<yamaa_core::specification_output::PreparedOutput<StoredReport>, Error> {
    let mut diagnostics = report.diagnostics().to_vec();
    diagnostics.push(match error {
        Error::ResourceLimit(refusal) => refusal.diagnostic(Stage::Output, Fault::OutputLimit),
        _ => yamaa_core::application_issue::rejected(Stage::Output, Fault::OutputProjection),
    });
    report.set_diagnostics(diagnostics);
    report["outcome"] = json!("failure");
    report["nodes"][0]["outcome"] = json!("failure");
    report["artifacts"] = json!([]);
    Ok(yamaa_core::specification_output::PreparedOutput::failed(
        report,
    ))
}
impl<E> From<Error> for CompleteError<E> {
    fn from(error: Error) -> Self {
        Self::Report(error)
    }
}
pub(crate) fn check_observations(
    run: &PreparedRun,
    records: &[yamaa_core::dataset::CheckRecord],
    phase: yamaa_core::verification_diagnostics::Phase,
    id: &Identity<'_>,
) -> Result<(Vec<Value>, Vec<Diagnostic>), Error> {
    use yamaa_core::verification_diagnostics::Error as ProjectionError;
    let projection = yamaa_core::verification_diagnostics::project(
        run.compiled(),
        records,
        phase,
        id.specification,
    )
    .map_err(|error| match error {
        ProjectionError::InvalidObservation => Error::InvalidObservation,
        ProjectionError::UnsupportedOutcome => Error::UnsupportedOutcome,
    })?;
    Ok((
        projection
            .observations
            .into_iter()
            .map(crate::project_diagnostic_projection::fact)
            .collect(),
        projection
            .diagnostics
            .into_iter()
            .map(crate::project_diagnostic_projection::owned)
            .collect(),
    ))
}
pub(crate) fn output_diagnostics(
    findings: &[yamaa_core::specification::OutputFinding],
) -> Vec<Diagnostic> {
    findings
        .iter()
        .map(|finding| finding.diagnostic())
        .collect()
}
/// Render all retained observations and validate output boundaries before
/// publication. A host publication failure is retained, never a success report.
pub fn complete<C, E, P: ArtifactPort>(
    run: &PreparedRun,
    attempt: &Attempt<C, E>,
    id: Identity<'_>,
    publisher: &mut P,
) -> Result<Value, CompleteError<P::Error>> {
    let result = build_result(run, attempt, id, &[]).map_err(|refusal| match refusal {
        Refusal::Report(error) => CompleteError::Report(error),
        Refusal::Opaque(_) | Refusal::Boundary(_) => {
            CompleteError::Report(Error::UnsupportedOutcome)
        }
    })?;
    let items = result.publication_items();

    let report = result.publication_observations(items.len())?;
    for (path, bytes, _) in &items {
        publisher
            .publish(path, bytes)
            .map_err(CompleteError::Publish)?;
    }
    Ok(report.observations_value(false))
}

struct Encoder<'a>(Option<&'static str>, Option<&'a str>);
impl yamaa_core::specification_output::ArtifactEncoder for Encoder<'_> {
    type Error = Error;
    fn encode(
        &mut self,
        dataset: &yamaa_core::dataset::Dataset,
        projection: &[usize],
    ) -> Result<Vec<u8>, Error> {
        match self.0 {
            Some("csv") => crate::csv_artifact::render(dataset, projection, self.1).map_err(
                |error| match error {
                    crate::csv_artifact::Error::Limit {
                        resource,
                        limit,
                        actual,
                        decimal_formatting,
                    } => Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
                        resource,
                        limit,
                        actual,
                        vec![if decimal_formatting {
                            "output.decimals"
                        } else {
                            "output.path"
                        }
                        .into()],
                    )),
                    _ => Error::InvalidObservation,
                },
            ),
            Some("parquet") => {
                crate::parquet_artifact::render(dataset, projection).map_err(|error| match error {
                    crate::parquet_artifact::Error::Unavailable => {
                        Error::CapabilityUnavailable { feature: "parquet" }
                    }
                    crate::parquet_artifact::Error::Limit {
                        resource,
                        limit,
                        actual,
                    } => Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
                        resource,
                        limit,
                        actual,
                        vec!["output.path".into()],
                    )),
                    _ => Error::InvalidObservation,
                })
            }
            _ => Err(Error::InvalidObservation),
        }
    }
}

fn observations<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a CapturedAttempt<C, E>,
    supplied: &FactMap<'a, '_, E>,
    id: &Identity<'_>,
) -> Result<StoredReport, Refusal<'a, E>> {
    let Some((execution, success)) = attempt
        .result
        .as_ref()
        .ok()
        .and_then(|execution| Some((execution, execution.result.as_ref().ok()?)))
    else {
        return dataset_failure(run, attempt, supplied, id);
    };
    let mut report = envelope(run, attempt, id)?;
    let (checks, unexpected) = check_observations(
        run,
        &success.verifications,
        yamaa_core::verification_diagnostics::Phase::Verification,
        id,
    )?;
    if !unexpected.is_empty() {
        return Err(Error::InvalidObservation.into());
    }
    set_handler_counts(&mut report, &execution.handler_counts);
    report["verifications"] = json!(checks);
    Ok(report)
}

struct Report<'a, 'i> {
    run: &'a PreparedRun,
    id: Identity<'i>,
    value: Option<StoredReport>,
}
impl yamaa_core::specification_output::OutputReport for Report<'_, '_> {
    type Error = Error;
    type Report = StoredReport;
    fn failure(&mut self) -> Result<StoredReport, Error> {
        self.value.take().ok_or(Error::InvalidObservation)
    }
    fn begin(&mut self, _execution: &yamaa_core::dataset::Execution) -> Result<(), Error> {
        self.value.as_ref().ok_or(Error::InvalidObservation)?;
        Ok(())
    }
    fn rejected(
        &mut self,
        findings: &[yamaa_core::specification::OutputFinding],
    ) -> Result<StoredReport, Error> {
        let mut report = self.value.take().ok_or(Error::InvalidObservation)?;
        report.set_diagnostics(output_diagnostics(findings));
        Ok(report)
    }
    fn success(
        &mut self,
        execution: &yamaa_core::dataset::Execution,
        projection: &[usize],
        bytes: &[u8],
    ) -> Result<StoredReport, Error> {
        artifact_report(
            self.run,
            &self.id,
            self.value.take().ok_or(Error::InvalidObservation)?,
            execution,
            projection,
            bytes,
        )
    }
}

pub(crate) fn artifact_report(
    run: &PreparedRun,
    id: &Identity<'_>,
    mut report: StoredReport,
    execution: &yamaa_core::dataset::Execution,
    projection: &[usize],
    bytes: &[u8],
) -> Result<StoredReport, Error> {
    let table = &execution.dataset;
    let profile = run
        .compiled()
        .output_profile()
        .ok_or(Error::InvalidObservation)?;

    let (content, records) = if profile == "csv" {
        let content = std::str::from_utf8(bytes).map_err(|_| Error::InvalidObservation)?;

        let records = content
            .strip_suffix('\n')
            .ok_or(Error::InvalidObservation)?
            .split('\n')
            .map(String::from)
            .collect::<Vec<_>>();
        (content, records)
    } else {
        let records =
            crate::parquet_artifact::records(table, projection).map_err(|error| match error {
                crate::parquet_artifact::Error::Limit {
                    resource,
                    limit,
                    actual,
                } => Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
                    resource,
                    limit,
                    actual,
                    vec!["output.path".into()],
                )),
                _ => Error::InvalidObservation,
            })?;
        ("", records)
    };
    report["artifacts"] = json!([{"name":run.compiled().output_name(),"profile":profile,"columns":run.compiled().projection(),"types":projection.iter().map(|&c|specification_diagnostics::type_name(table.schema().columns()[c].kind)).collect::<Vec<_>>(),"row_count":table.row_count(),"records":records,"byte_length":bytes.len(),"content":content}]);
    report.push_table(table_observation_projected(table, id, "derived", "output")?)?;
    report["outcome"] = json!("success");
    report["nodes"][0]["outcome"] = json!("success");
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constructors_share_engine_facts() {
        let identity = Identity::new("python", "3.14", "domain", "spec.yaml");
        assert_eq!(identity.runtime, "python");
        assert_eq!(identity.runtime_version, "3.14");
        assert_eq!(identity.engine_version, yamaa_core::VERSION);
        assert_eq!(identity.example, "domain");
        assert_eq!(identity.specification, "spec.yaml");
        assert_eq!(identity.base_directory, ".");
        let fields: Vec<String> = ["3.14", "0.4.0", "check", "spec.yaml", "."]
            .iter()
            .map(|field| field.to_string())
            .collect();
        let identity = Identity::from_fields("r", &fields);
        assert_eq!(identity.runtime, "r");
        assert_eq!(identity.runtime_version, "3.14");
        assert_eq!(identity.engine_version, "0.4.0");
        assert_eq!(identity.example, "check");
        assert_eq!(identity.specification, "spec.yaml");
        assert_eq!(identity.base_directory, ".");
    }
}
