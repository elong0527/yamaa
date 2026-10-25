//! The one result builder: format one observed attempt, with or without a
//! project, and prepare the files its save publishes; building never
//! publishes. Classified resource findings retain observations; opaque host
//! failures stay opaque.
use crate::json;
use crate::{
    dataset_result::{Condition, Outcome},
    project_report::{Classified, FactMap},
    report_storage::{Rows, StoredReport, StoredTable},
    specification_diagnostics,
    specification_run::{Attempt, BoundaryFailure, CapturedAttempt, PortError, PreparedRun},
};
use yamaa_core::application_issue::{Fault, Step};
use yamaa_core::dataset::ExecutionError;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::json::{Map, Value};
use yamaa_core::numeric_lifecycle::HandlerCount;
use yamaa_core::resolver::Origin;
use yamaa_core::specification_run::{RunError, SourceIssue};
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
pub use crate::report_storage::Error;
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
            .map(|refusal| vec![refusal.diagnostic(Step::Ingest, Fault::SourceLimit)]),
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
                    .map(|refusal| refusal.diagnostic(Step::Execute, Fault::ExecutionLimit))
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
/// The observation of a table whose records divide into `partitions` of
/// these sizes, each admitted on its own: one partition for a table held
/// whole, and those of an output held in partitions (D264).
fn table_observation<T: TableAccess>(
    table: &T,
    id: &Identity<'_>,
    stage: &str,
    name: &str,
    partitions: &[usize],
) -> Result<StoredTable, Error> {
    let rows = crate::report_storage::encode_partitions(table, partitions)?;
    Ok(StoredTable {
        metadata: table_metadata(table.schema(), id, stage, name),
        rows: Rows::Encoded(rows),
    })
}

/// The observation of the run's output, whose records divide into
/// `partitions` of these sizes (#3870): each admitted now, on its own, and
/// the rows encoded only when the observations are serialized, so the run
/// holds no copy of them.
fn deferred_observation(
    table: &yamaa_core::dataset::Dataset,
    id: &Identity<'_>,
    stage: &str,
    name: &str,
    partitions: Vec<usize>,
) -> Result<StoredTable, Error> {
    crate::report_storage::admit_partitions(table.schema().columns().len(), &partitions)?;
    let metadata = table_metadata(table.schema(), id, stage, name);
    let table = table.clone();
    Ok(StoredTable {
        metadata,
        rows: Rows::Deferred(std::sync::Arc::new(move || {
            crate::report_storage::encode_partitions(&table, &partitions)
        })),
    })
}

/// The report metadata of a table of `schema`.
fn table_metadata(
    schema: &yamaa_core::table::TableSchema,
    id: &Identity<'_>,
    stage: &str,
    name: &str,
) -> Value {
    let columns = schema.columns();
    json!({"specification":id.specification,"stage":stage,"name":name,"columns":columns.iter().map(|c|&c.name).collect::<Vec<_>>(),"types":columns.iter().map(|c|specification_diagnostics::type_name(c.kind)).collect::<Vec<_>>(),"rows":null})
}

/// The records a source read by partition encodes at a time when its
/// observation is serialized: a streamed batch's.
const OBSERVED_RECORDS: usize = 65_536;

/// Bound each observation decode by both its batch size and an input
/// partition the scan already admitted. Joining admitted text columns can
/// exceed Arrow Utf8's offsets even when each partition fits on its own.
fn observation_ranges(
    partitions: &[std::ops::Range<usize>],
) -> impl Iterator<Item = std::ops::Range<usize>> + '_ {
    partitions.iter().flat_map(|partition| {
        std::iter::successors(Some(partition.start), move |start| {
            let end = partition.end.min(start.saturating_add(OBSERVED_RECORDS));
            (end < partition.end).then_some(end)
        })
        .map(move |start| start..partition.end.min(start.saturating_add(OBSERVED_RECORDS)))
    })
}

/// The observation of a source read by partition (#3792), whose records the
/// run holds no copy of (#3830): its rows are decoded again from `bytes`,
/// the held bytes its scan read, a partition at a time, and encoded only
/// when the observations are serialized, so they are the rows a whole read
/// would give.
fn scanned_observation(
    scanned: &std::sync::Arc<crate::source_decoding::Scanned>,
    bytes: std::sync::Arc<[u8]>,
    id: &Identity<'_>,
    name: &str,
) -> StoredTable {
    let metadata = table_metadata(&scanned.schema, id, "source", name);
    let scanned = std::sync::Arc::clone(scanned);
    StoredTable {
        metadata,
        rows: Rows::Deferred(std::sync::Arc::new(move || {
            let mut writer = crate::report_storage::RowWriter::default();
            for range in observation_ranges(&scanned.partitions) {
                let part = crate::source_decoding::decode_range(&scanned, &bytes, range)
                    .map_err(|_| Error::InvalidObservation)?;
                writer.table(&part)?;
            }
            writer.finish()
        })),
    }
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

pub(crate) fn envelope_sources<E>(
    sources: &[crate::specification_run::CapturedSource<E>],
    id: &Identity<'_>,
    partial: bool,
) -> Result<StoredReport, Error> {
    let mut tables = Vec::new();
    if partial || sources.iter().all(|source| source.table.is_some()) {
        for source in sources {
            let Some(table) = source.table.as_ref() else {
                continue;
            };
            let name = &source.read.source.name;
            tables.push(match (table.scan(), &source.snapshot) {
                (Some(scanned), Some(bytes)) => {
                    scanned_observation(scanned, std::sync::Arc::clone(bytes), id, name)
                }
                _ => table_observation(table, id, "source", name, &[table.row_count()])?,
            });
        }
    }
    let mut reads = Vec::new();
    for source in sources
        .iter()
        .filter(|source| source.read.origin == Some(Origin::Resource))
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

/// The primary artifact of an accepted build: the path its specification
/// declares, its encoded bytes, where each partition of them ends, and the
/// projection they hold. They belong to the build; a save publishes them a
/// partition at a time and never encodes again.
#[derive(Debug)]
struct Artifact {
    path: String,
    bytes: std::sync::Arc<[u8]>,
    /// The end of each partition's bytes, the last at their length, for an
    /// output encoded a partition at a time (D264); empty for one encoded
    /// whole.
    partitions: Vec<usize>,
    projection: Vec<usize>,
}

/// A completed build's prepared observations, with its primary artifact only
/// when the build was accepted. The observations may describe a prospective
/// publication; only a save performs one.
#[derive(Debug)]
struct PreparedOutput {
    report: StoredReport,
    artifact: Option<Artifact>,
}
impl PreparedOutput {
    /// A diagnostic-only result: it has no primary artifact, including after
    /// an encoding refusal.
    fn failed(report: StoredReport) -> Self {
        Self {
            report,
            artifact: None,
        }
    }
    fn prepared_report(&self) -> &StoredReport {
        &self.report
    }
    /// Formatting may enrich the prospective observations without replacing
    /// the artifact.
    fn replace_report(&mut self, report: StoredReport) {
        self.report = report;
    }
    fn artifact(&self) -> Option<&Artifact> {
        self.artifact.as_ref()
    }
}

/// How a build hands its accepted output to its host (#3697).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostView {
    /// The whole table, built with the result.
    Table,
    /// No table: a run whose output is read as a stream builds each batch
    /// as it is read, from the accepted rows (`PublicBatches`). The build
    /// checks that the whole table could be built, so it fails exactly when
    /// a [`HostView::Table`] build fails.
    Deferred,
}

/// The host view an accepted result holds.
#[derive(Debug)]
enum View {
    /// The output table.
    Table(crate::public_table::PublicTable),
    /// Nothing built: the batches are built as they are read.
    Deferred,
}

/// An owned build result; obtaining it never calls a publisher. Its table is
/// the admitted output projection, while private report artifacts are prospective.
/// Every result holds its verified run report.
#[derive(Debug)]
pub struct BuildResult {
    prepared: PreparedOutput,
    view: Option<View>,
    issues: Vec<Diagnostic>,
    report: crate::run_report::HeldReport,
}
impl BuildResult {
    /// The accepted output table, held only by a whole-table build.
    pub fn output(&self) -> Option<&crate::public_table::PublicTable> {
        match &self.view {
            Some(View::Table(table)) => Some(table),
            _ => None,
        }
    }
    /// Whether the build accepted its primary output, whatever its view.
    pub fn accepted(&self) -> bool {
        self.view.is_some()
    }
    /// The positions of the output's columns among the accepted dataset's,
    /// for a build that accepted its output.
    pub(crate) fn projection(&self) -> Option<&[usize]> {
        self.view.as_ref()?;
        self.prepared
            .artifact()
            .map(|artifact| artifact.projection.as_slice())
    }
    pub fn issues(&self) -> &[Diagnostic] {
        &self.issues
    }
    /// The run report table, which every result holds.
    pub fn run_report(&self) -> &crate::public_table::PublicTable {
        self.report.table()
    }
    /// Whether the specification declares a report file for publication.
    pub(crate) fn report_file(&self) -> bool {
        self.report.file().is_some()
    }
    /// The accepted primary artifact's serialized bytes, shared, not copied.
    pub(crate) fn artifact(&self) -> Option<std::sync::Arc<[u8]>> {
        self.view.as_ref()?;
        self.prepared
            .artifact()
            .map(|artifact| std::sync::Arc::clone(&artifact.bytes))
    }
    /// Failed results have only an explicit diagnostic publication capability:
    /// the declared report file. The primary artifact requires the accepted
    /// primary owner. Each item is the path its specification wrote, its
    /// complete bytes, where each of its partitions ends (none for a file
    /// written whole) and its artifact observation.
    pub(crate) fn publication_items(&self) -> Vec<(&str, &[u8], &[usize], &Value)> {
        let mut items = Vec::new();
        if let Some((path, bytes, observation)) = self.report.file() {
            items.push((path, bytes, &[][..], observation));
        }
        if self.accepted() {
            if let Some(artifact) = self.prepared.artifact() {
                items.push((
                    artifact.path.as_str(),
                    &artifact.bytes[..],
                    &artifact.partitions[..],
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
    /// The files a save of this result writes, each with the path its
    /// specification wrote and its complete bytes, in publication order: the
    /// declared report file, then the accepted primary artifact (OUTPUT-10).
    /// A failed build writes only its report file.
    pub fn files(&self) -> Vec<(&str, &[u8])> {
        self.publication_items()
            .into_iter()
            .map(|(path, bytes, _, _)| (path, bytes))
            .collect()
    }
    /// The observations a save that writes every one of [`Self::files`]
    /// records: the build's observations with each file's artifact entry.
    pub fn saved_observations(&self) -> Result<Value, Error> {
        let files = self.publication_items().len();
        Ok(self
            .publication_observations(files)?
            .observations_value(false))
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
                .map(|(_, _, _, value)| value.clone())
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
                return limit.issues(Step::Build, Fault::ResultProjection)
            }
            Self::Report(_) => Fault::ResultProjection,
        };
        crate::file_application::rejected(Step::Build, code)
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
    build_result_with(run, attempt, id, facts, HostView::Table)
}

/// [`build_result`] with the host view `view`: a [`HostView::Deferred`]
/// result builds no table, and its report, issues and status are the
/// [`HostView::Table`] result's.
pub fn build_result_with<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a Attempt<C, E>,
    id: Identity<'_>,
    facts: &[Classified<'a, E>],
    view: HostView,
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
    Ok(prepare_result(run, execution, id, report, &facts, view)?)
}

pub(crate) fn prepare_result(
    run: &PreparedRun,
    execution: Option<&yamaa_core::dataset::Execution>,
    id: Identity<'_>,
    observations: StoredReport,
    facts: &crate::run_report::RunFacts,
    view: HostView,
) -> Result<BuildResult, Error> {
    let fallback = observations.clone();
    // Every run reports, so a failed output preparation becomes a failed run
    // whose report records the failure rather than a refused result.
    let mut prepared = prepare_output(run, execution, &id, observations)
        .or_else(|error| rejected_output(fallback, &error))?;
    let table = prepared
        .artifact()
        .map(|artifact| {
            let execution = execution.ok_or(Error::InvalidObservation)?;
            let (dataset, partition) = (&execution.dataset, execution.partition_rows);
            match view {
                HostView::Table => crate::public_table::PublicTable::from_partitions(
                    dataset,
                    &artifact.projection,
                    partition,
                )
                .map(View::Table),
                // Built batch by batch as it is read, after the same checks.
                HostView::Deferred => {
                    crate::public_table::check_table(dataset, &artifact.projection, partition)
                        .map(|_| View::Deferred)
                }
            }
            .map_err(|e| match e {
                crate::public_table::TableError::Limit {
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
        view: table,
        issues: crate::file_application::collect(issues),
        report,
    })
}

/// Prepare one build's output without publishing it. Only a successful
/// execution reaches the encoder: a failed capture, decoding, binding or
/// execution supplies none, and its observations are the failed report.
/// Output declaration findings precede encoding, and the encoded bytes are
/// bounded before the report that describes them is completed.
fn prepare_output(
    run: &PreparedRun,
    execution: Option<&yamaa_core::dataset::Execution>,
    id: &Identity<'_>,
    observations: StoredReport,
) -> Result<PreparedOutput, Error> {
    let Some(execution) = execution else {
        return Ok(PreparedOutput::failed(observations));
    };
    let compiled = run.compiled();
    let findings = compiled.output_findings();
    if !findings.is_empty() {
        let mut report = observations;
        report.set_diagnostics(output_diagnostics(&findings));
        return Ok(PreparedOutput::failed(report));
    }
    let projection = compiled
        .projection()
        .iter()
        .map(|name| {
            execution
                .dataset
                .schema()
                .columns()
                .iter()
                .position(|column| &column.name == name)
                .ok_or(Error::InvalidObservation)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (bytes, partitions) = encode(
        compiled.output_profile(),
        compiled.output_decimals(),
        execution,
        &projection,
    )?;
    yamaa_core::limits::Limits::output_bytes(bytes.len()).map_err(|error| {
        Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
            error.resource,
            error.limit,
            error.actual,
            vec!["output.path".into()],
        ))
    })?;
    let bytes: std::sync::Arc<[u8]> = bytes.into();
    let report = artifact_report(run, id, observations, execution, &projection, &bytes)?;
    Ok(PreparedOutput {
        report,
        artifact: Some(Artifact {
            path: compiled.output_path().into(),
            bytes,
            partitions,
            projection,
        }),
    })
}

fn rejected_output(mut report: StoredReport, error: &Error) -> Result<PreparedOutput, Error> {
    let mut diagnostics = report.diagnostics().to_vec();
    diagnostics.push(match error {
        Error::ResourceLimit(refusal) => refusal.diagnostic(Step::Output, Fault::OutputLimit),
        _ => yamaa_core::application_issue::rejected(Step::Output, Fault::OutputProjection),
    });
    report.set_diagnostics(diagnostics);
    report["outcome"] = json!("failure");
    report["nodes"][0]["outcome"] = json!("failure");
    report["artifacts"] = json!([]);
    Ok(PreparedOutput::failed(report))
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
/// Encode the projected output under the specification's output `profile`,
/// with its CSV display `decimals`, one partition of the output at a time
/// (D264): its bytes, and where each partition's bytes end, the last at
/// their length. An output admitted whole is one partition. Each codec
/// bounds its allocation as it encodes; the caller bounds the bytes it
/// returns.
fn encode(
    profile: Option<&'static str>,
    decimals: Option<&str>,
    execution: &yamaa_core::dataset::Execution,
    projection: &[usize],
) -> Result<(Vec<u8>, Vec<usize>), Error> {
    let dataset = &execution.dataset;
    let partitions = execution.output_partitions();
    match profile {
        Some("csv") => {
            crate::csv_artifact::render_partitions(dataset, projection, decimals, &partitions)
                .map_err(|error| match error {
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
                })
        }
        Some("parquet") => {
            crate::parquet_artifact::render_partitions(dataset, projection, &partitions).map_err(
                |error| match error {
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
                },
            )
        }
        _ => Err(Error::InvalidObservation),
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

pub(crate) fn artifact_report(
    run: &PreparedRun,
    id: &Identity<'_>,
    mut report: StoredReport,
    execution: &yamaa_core::dataset::Execution,
    projection: &[usize],
    bytes: &std::sync::Arc<[u8]>,
) -> Result<StoredReport, Error> {
    let table = &execution.dataset;
    let profile = run
        .compiled()
        .output_profile()
        .ok_or(Error::InvalidObservation)?;
    // The artifact's records and content are the output's, encoded only when
    // the observations are serialized, so the run holds no copy of them
    // (#3870); what could refuse them is checked now.
    let encode: crate::report_storage::ArtifactContent = if profile == "csv" {
        std::str::from_utf8(bytes)
            .ok()
            .and_then(|content| content.strip_suffix('\n'))
            .ok_or(Error::InvalidObservation)?;
        let bytes = std::sync::Arc::clone(bytes);
        crate::report_storage::ArtifactContent {
            name: run.compiled().output_name().into(),
            encode: std::sync::Arc::new(move || {
                let content = std::str::from_utf8(&bytes).map_err(|_| Error::InvalidObservation)?;
                let records = content
                    .strip_suffix('\n')
                    .ok_or(Error::InvalidObservation)?
                    .split('\n')
                    .collect::<Vec<_>>();
                Ok((raw(&records)?, raw(content)?))
            }),
        }
    } else {
        yamaa_core::limits::Limits::partitioned(
            table.row_count(),
            execution.partition_rows,
            projection.len(),
        )
        .map_err(|error| {
            Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
                error.resource,
                error.limit,
                error.actual,
                vec!["output.path".into()],
            ))
        })?;
        let (table, projection, partition) =
            (table.clone(), projection.to_vec(), execution.partition_rows);
        crate::report_storage::ArtifactContent {
            name: run.compiled().output_name().into(),
            encode: std::sync::Arc::new(move || {
                let records = crate::parquet_artifact::records(&table, &projection, partition)
                    .map_err(|_| Error::InvalidObservation)?;
                Ok((raw(&records)?, raw("")?))
            }),
        }
    };
    report["artifacts"] = json!([{"name":run.compiled().output_name(),"profile":profile,"columns":run.compiled().projection(),"types":projection.iter().map(|&c|specification_diagnostics::type_name(table.schema().columns()[c].kind)).collect::<Vec<_>>(),"row_count":table.row_count(),"records":null,"byte_length":bytes.len(),"content":null}]);
    report.defer_content(encode);
    // The output observed as it is held: whole, or a partition at a time,
    // its rows encoded only when the observations are serialized (#3870).
    let partitions = execution
        .output_partitions()
        .iter()
        .map(|rows| rows.len())
        .collect::<Vec<_>>();
    report.push_table(deferred_observation(
        table, id, "derived", "output", partitions,
    )?)?;
    report["outcome"] = json!("success");
    report["nodes"][0]["outcome"] = json!("success");
    Ok(report)
}

/// `value` as JSON text.
fn raw<T: serde::Serialize + ?Sized>(
    value: &T,
) -> Result<std::sync::Arc<serde_json::value::RawValue>, Error> {
    serde_json::value::to_raw_value(value)
        .map(std::sync::Arc::from)
        .map_err(|_| Error::InvalidObservation)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Seam: the decode requests of a deferred report must retain the scan's
    // representation admission, without allocating multi-gigabyte strings.
    #[test]
    fn observation_batches_preserve_each_admitted_partitions_text_offset_bound() {
        let text_bytes = i32::MAX as usize / 2 + 1;
        assert_eq!(
            (
                crate::column_table::fits_utf8_offsets([text_bytes]),
                crate::column_table::fits_utf8_offsets([text_bytes, text_bytes]),
            ),
            (true, false),
            "one record fits; combining two admitted records would not"
        );
        let batch = OBSERVED_RECORDS;
        for (label, partitions, expected) in [
            (
                "empty",
                std::iter::once(0..0).collect(),
                std::iter::once(0..0).collect(),
            ),
            (
                "two separately admitted text columns",
                vec![0..1, 1..2],
                vec![0..1, 1..2],
            ),
            (
                "a batch cap inside an admitted partition",
                vec![0..batch + 1, batch + 1..batch + 3],
                vec![0..batch, batch..batch + 1, batch + 1..batch + 3],
            ),
        ] {
            assert_eq!(
                observation_ranges(&partitions).collect::<Vec<_>>(),
                expected,
                "{label}"
            );
        }
    }

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
