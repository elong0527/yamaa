//! Format observed bounded standalone runs and publish through explicit host authority.
//! Classified resource findings retain observations; opaque host failures stay opaque.
use crate::{
    report_storage::{StoredReport, StoredTable},
    scalar_transport::ScalarValue,
    specification_diagnostics,
    specification_run::{CapturedAttempt, PortError},
    specification_run_view::RunView,
};
use serde_json::{json, Map, Value};
use yamaa_core::{table::TableAccess, value::Value as Scalar};
use yamaa_engine::specification_run::SourceOrigin;

pub struct Identity<'a> {
    pub runtime: &'a str,
    pub runtime_version: &'a str,
    pub engine_version: &'a str,
    pub example: &'a str,
    pub specification: &'a str,
    pub base_directory: &'a str,
}
#[derive(Debug)]
pub enum Error {
    UnsupportedOutcome,
    InvalidObservation,
    OutputLimit,
}
fn plain(value: &Value) -> Result<Value, Error> {
    if let Ok(scalar) = serde_json::from_value::<ScalarValue>(value.clone()) {
        return Ok(
            match scalar.into_core().map_err(|_| Error::InvalidObservation)? {
                Scalar::Missing => Value::Null,
                Scalar::Str(v) => json!(v),
                Scalar::Int(v) => json!(v),
                Scalar::Float(v) => json!(v.get()),
                Scalar::Bool(v) => json!(v),
                Scalar::Date(v) => json!(v.to_string()),
                Scalar::DateTime(v) => json!(v.to_string()),
            },
        );
    }
    if let Value::Array(values) = value {
        return values
            .iter()
            .map(plain)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    if let Value::Object(values) = value {
        if values.len() == 1 {
            if let Some(integer) = values.get("integer").and_then(Value::as_str) {
                return integer
                    .parse()
                    .map(Value::Number)
                    .map_err(|_| Error::InvalidObservation);
            }
        }
        return values
            .iter()
            .map(|(key, value)| Ok((key.clone(), plain(value)?)))
            .collect::<Result<Map<_, _>, Error>>()
            .map(Value::Object);
    }
    Err(Error::InvalidObservation)
}
pub(crate) fn condition(run: &dyn RunView, value: &Value) -> Result<Value, Error> {
    let diagnostic = value.get("diagnostic").ok_or(Error::InvalidObservation)?;
    let mut context = diagnostic
        .get("context")
        .and_then(Value::as_object)
        .ok_or(Error::InvalidObservation)?
        .iter()
        .map(|(name, v)| Ok((name.clone(), plain(v)?)))
        .collect::<Result<Map<_, _>, Error>>()?;
    if let Some(identity) = value.get("identity").filter(|v| !v.is_null()) {
        let keys = identity
            .get("keys")
            .and_then(Value::as_array)
            .ok_or(Error::InvalidObservation)?;
        let names = run.compiled().key_names().collect::<Vec<_>>();
        if names.len() != keys.len() {
            return Err(Error::InvalidObservation);
        }
        let keys = names
            .into_iter()
            .zip(keys)
            .map(|(name, v)| Ok((name.to_owned(), plain(v)?)))
            .collect::<Result<Map<_, _>, Error>>()?;
        context.insert("keys".into(), json!([keys]));
    }
    if let Some(matched) = value.get("matched_key").filter(|v| !v.is_null()) {
        let matched = matched.as_array().ok_or(Error::InvalidObservation)?;
        let mut names = Vec::new();
        let mut key = Map::new();
        for field in matched {
            let name = field["name"].as_str().ok_or(Error::InvalidObservation)?;
            names.push(name);
            key.insert(name.into(), plain(&field["value"])?);
        }
        context.insert("key".into(), json!(names));
        context.insert("intermediate_key".into(), key.into());
    }
    if let Some(partition) = value.get("partition").filter(|v| !v.is_null()) {
        let partition = partition.as_array().ok_or(Error::InvalidObservation)?;
        let mut keys = Map::new();
        for field in partition {
            let name = field["name"].as_str().ok_or(Error::InvalidObservation)?;
            keys.insert(name.into(), plain(&field["value"])?);
        }
        context.insert("keys".into(), json!([keys]));
    }
    Ok(
        json!({"phase":diagnostic["phase"],"condition":diagnostic["condition"],"requirement":diagnostic["requirement"],"spec_paths":diagnostic["spec_paths"],"context":context}),
    )
}
/// A classified capture failure terminates the request sequence. Reject malformed
/// observations rather than attaching a resource finding to an unrelated failure.
fn captured_failure<E>(
    attempt: &CapturedAttempt<E>,
) -> Result<&crate::specification_run::CapturedSource, Error> {
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
        || last.table.is_some()
        || prefix
            .iter()
            .any(|s| !s.read.captured || s.read.failure.is_some())
    {
        return Err(Error::UnsupportedOutcome);
    }
    Ok(last)
}

fn inspected_failures<'a, E>(
    run: &dyn RunView,
    attempt: &'a CapturedAttempt<E>,
) -> Result<&'a [crate::specification_run::InspectionFailure<E>], Error> {
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

/// Refuse outcomes outside this formatter instead of inventing observations.
pub fn failure<E>(
    run: &dyn RunView,
    attempt: &CapturedAttempt<E>,
    id: Identity<'_>,
) -> Result<StoredReport, Error> {
    if attempt.sources.is_empty() && !matches!(attempt.result, Err(PortError::Inspect(_))) {
        return Err(Error::UnsupportedOutcome);
    }
    let mut verifications = Vec::new();
    let diagnostics = match &attempt.result {
        Ok(response) => {
            if response.table.is_some() {
                return Err(Error::UnsupportedOutcome);
            }
            let value: Value =
                serde_json::from_str(&response.outcome).map_err(|_| Error::InvalidObservation)?;
            if !response.retained_verifications.is_empty() {
                verifications.extend(
                    check_observations(
                        run,
                        &response.retained_verifications,
                        yamaa_core::verification_diagnostics::Phase::Verification,
                        &id,
                    )?
                    .0,
                );
            }
            match value["outcome"]["status"].as_str() {
                Some("condition") => {
                    if let Some((phase, completed)) = &response.failed_verifications {
                        verifications.extend(check_observations(run, completed, *phase, &id)?.0);
                    }
                    if response.intermediate_diagnostics.is_empty() {
                        vec![condition(run, &value["outcome"])?]
                    } else {
                        response
                            .intermediate_diagnostics
                            .iter()
                            .map(|diagnostic| {
                                specification_diagnostics::portable_diagnostic(diagnostic.clone())
                                    .ok_or(Error::InvalidObservation)
                            })
                            .collect::<Result<Vec<_>, _>>()?
                    }
                }
                Some("failure") => {
                    let (phase, completed) = response
                        .failed_verifications
                        .as_ref()
                        .ok_or(Error::InvalidObservation)?;
                    let (records, diagnostics) = check_observations(run, completed, *phase, &id)?;
                    verifications.extend(records);
                    diagnostics
                }
                _ => return Err(Error::UnsupportedOutcome),
            }
        }
        Err(PortError::Run(error)) => {
            specification_diagnostics::findings(error, Some(run.source()))
                .ok_or(Error::UnsupportedOutcome)?
        }
        Err(PortError::Capture(_)) => {
            let source = captured_failure(attempt)?;
            let written = run
                .document()
                .written_source_path(&source.read.source.name)
                .ok_or(Error::InvalidObservation)?;
            let diagnostic = source
                .read
                .failure
                .ok_or(Error::InvalidObservation)?
                .diagnostic(&source.read.source.name, written);
            vec![specification_diagnostics::portable_diagnostic(diagnostic)
                .ok_or(Error::InvalidObservation)?]
        }
        Err(PortError::Inspect(_)) => inspected_failures(run, attempt)?
            .iter()
            .map(|failure| {
                let written = run
                    .document()
                    .written_source_path(&failure.source.name)
                    .ok_or(Error::InvalidObservation)?;
                specification_diagnostics::portable_diagnostic(
                    failure
                        .failure
                        .ok_or(Error::InvalidObservation)?
                        .diagnostic(&failure.source.name, written),
                )
                .ok_or(Error::InvalidObservation)
            })
            .collect::<Result<Vec<_>, Error>>()?,
        _ => return Err(Error::UnsupportedOutcome),
    };
    let mut report = envelope(run, attempt, &id)?;
    if let Ok(response) = &attempt.result {
        let value: Value =
            serde_json::from_str(&response.outcome).map_err(|_| Error::InvalidObservation)?;
        set_handler_counts(&mut report, &value)?;
    }
    set_diagnostics(&mut report, diagnostics);
    report["verifications"] = json!(verifications);
    bounded(report)
}
pub(crate) fn table_observation_projected<T: TableAccess>(
    table: &T,
    id: &Identity<'_>,
    stage: &str,
    name: &str,
    budget: Option<&mut crate::report_projection::Budget>,
) -> Result<StoredTable, Error> {
    let mut local_budget = budget.is_none().then(|| {
        crate::report_projection::Budget::new(yamaa_engine::capacity::REPORT_ENCODED_BYTES)
    });
    let mut budget = budget.or(local_budget.as_mut());
    let columns = table.schema().columns();
    if let Some(budget) = budget.as_deref_mut() {
        budget.row_shape(table.row_count(), columns.len())?;
        for text in [id.specification, stage, name] {
            budget.text(text)?;
        }
        for column in columns {
            budget.text(&column.name)?;
        }
    }
    let rows = crate::report_storage::encode_rows(table, budget)?;
    Ok(StoredTable {
        metadata: json!({"specification":id.specification,"stage":stage,"name":name,"columns":columns.iter().map(|c|&c.name).collect::<Vec<_>>(),"types":columns.iter().map(|c|specification_diagnostics::type_name(c.kind)).collect::<Vec<_>>(),"rows":null}),
        rows,
    })
}

fn envelope<E>(
    run: &dyn RunView,
    attempt: &CapturedAttempt<E>,
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
    envelope_sources(run, &attempt.sources, id)
}

pub(crate) fn envelope_sources<T: TableAccess>(
    run: &dyn RunView,
    sources: &[yamaa_engine::specification_run::CapturedSource<T>],
    id: &Identity<'_>,
) -> Result<StoredReport, Error> {
    envelope_sources_projected(run, sources, id, None)
}
pub(crate) fn envelope_sources_projected<T: TableAccess>(
    _run: &dyn RunView,
    sources: &[yamaa_engine::specification_run::CapturedSource<T>],
    id: &Identity<'_>,
    budget: Option<&mut crate::report_projection::Budget>,
) -> Result<StoredReport, Error> {
    let partial = budget.is_some();
    let mut local_budget = budget.is_none().then(|| {
        crate::report_projection::Budget::new(yamaa_engine::capacity::REPORT_ENCODED_BYTES)
    });
    let mut budget = budget.or(local_budget.as_mut());
    if let Some(budget) = budget.as_deref_mut() {
        budget.entries(32)?;
        budget.entries(sources.len())?;
        for text in [
            id.runtime,
            id.runtime_version,
            id.engine_version,
            id.example,
            id.specification,
            id.base_directory,
        ] {
            budget.text(text)?;
        }
        for source in sources {
            budget.text(&source.read.source.name)?;
            budget.text(&source.read.source.path)?;
            budget.text(id.base_directory)?;
        }
    }
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
                budget.as_deref_mut(),
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
pub(crate) fn bounded(report: StoredReport) -> Result<StoredReport, Error> {
    crate::report_projection::encoded_within_limit(
        &report,
        yamaa_engine::capacity::REPORT_ENCODED_BYTES,
    )?;
    Ok(report)
}
pub(crate) fn set_handler_counts(report: &mut Value, response: &Value) -> Result<(), Error> {
    let Some(raw) = response.get("handler_counts") else {
        return Ok(());
    };
    let counts = raw
        .as_array()
        .ok_or(Error::InvalidObservation)?
        .iter()
        .map(|entry| {
            let path = entry["spec_path"]
                .as_str()
                .ok_or(Error::InvalidObservation)?;
            let handler = entry["handler"].as_str().ok_or(Error::InvalidObservation)?;
            Ok(json!({"spec_path":path,"handler":handler,"count":count(entry,"count")?}))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    report["handler_counts"] = json!(counts);
    report["nodes"][0]["handler_counts"] = report["handler_counts"].clone();
    Ok(())
}
pub(crate) fn set_diagnostics(report: &mut Value, diagnostics: Vec<Value>) {
    report["diagnostics"] = json!(diagnostics);
    report["nodes"][0]["diagnostics"] = report["diagnostics"].clone();
}

pub use yamaa_engine::specification_output::ArtifactPort;
#[derive(Debug)]
pub enum CompleteError<E> {
    Report(Error),
    Publish(E),
}

/// An owned build result; obtaining it never calls a publisher. Table bytes are
/// the admitted output projection, while private report artifacts are prospective.
#[derive(Debug)]
pub struct BuildResult {
    prepared: yamaa_engine::specification_output::PreparedOutput<StoredReport>,
    table: Option<Vec<u8>>,
    issues: Vec<crate::issue_rows::Issue>,
    verification: Option<crate::verification_logs::HeldLog>,
    warning: Option<crate::verification_logs::HeldLog>,
}
impl BuildResult {
    pub(crate) fn reject_host_output(&mut self) -> Result<(), Error> {
        if self.verification.is_none() {
            return Err(Error::InvalidObservation);
        }
        let mut budget = crate::report_projection::Budget::new(16_777_216);
        budget.shared_report(self.prepared.prepared_report())?;
        self.prepared = rejected_output(
            self.prepared.prepared_report().clone(),
            &Error::InvalidObservation,
            "output",
        )?;
        self.table = None;
        self.warning = None;
        self.issues = self.prepared.prepared_report()["diagnostics"]
            .as_array()
            .ok_or(Error::InvalidObservation)?
            .iter()
            .cloned()
            .map(crate::issue_rows::Issue::from_diagnostic)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::InvalidObservation)?;
        if !crate::issue_rows::within_limit(
            &self.issues,
            crate::specification_check::MAX_ISSUE_BYTES,
        ) {
            return Err(Error::OutputLimit);
        }
        Ok(())
    }
    pub fn output(&self) -> Option<&[u8]> {
        self.table.as_deref()
    }
    pub fn issues(&self) -> &[crate::issue_rows::Issue] {
        &self.issues
    }
    pub fn verification_log(&self) -> Option<&[u8]> {
        self.verification.as_ref().map(|log| log.ipc())
    }
    pub fn warning_log(&self) -> Option<&[u8]> {
        self.warning.as_ref().map(|log| log.ipc())
    }
    pub(crate) fn log_table(&self, warning: bool) -> Option<&crate::public_table::PublicTable> {
        (if warning {
            &self.warning
        } else {
            &self.verification
        })
        .as_ref()
        .map(|log| log.table())
    }
    /// Failed results have only an explicit diagnostic publication capability.
    /// Warning and primary artifacts require the accepted primary owner.
    pub(crate) fn publication_items(&self) -> Vec<(&str, &[u8], &Value)> {
        let mut items = Vec::new();
        if let Some(log) = &self.verification {
            items.push((log.path(), log.bytes(), log.observation()));
        }
        if self.output().is_some() {
            if let Some(log) = &self.warning {
                items.push((log.path(), log.bytes(), log.observation()));
            }
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
    pub(crate) fn publication_observations(
        &self,
        completed: usize,
        budget: &mut crate::report_projection::Budget,
    ) -> Result<StoredReport, Error> {
        let items = self.publication_items();
        if completed > items.len() || items.len() > 3 {
            return Err(Error::InvalidObservation);
        }
        let original = self.prepared.prepared_report();
        budget.json_object_without(original, "artifacts")?;
        budget.entries(original.row_blocks())?;
        for (_, _, value) in items.iter().take(completed) {
            budget.json(value)?;
        }
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
        bounded(report)
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
    ) -> Result<&StoredReport, yamaa_engine::specification_output::SaveError<P::Error>> {
        for (path, bytes, _) in self.publication_items() {
            publisher
                .publish(path, bytes)
                .map_err(yamaa_engine::specification_output::SaveError::Publish)?;
        }
        if self.output().is_none() {
            return Err(yamaa_engine::specification_output::SaveError::FailedBuild);
        }
        Ok(self.prepared.prepared_report())
    }
}

/// Prepare a public-result candidate without recapturing data or publishing bytes.
pub fn build_result<E>(
    run: &dyn RunView,
    attempt: &CapturedAttempt<E>,
    id: Identity<'_>,
) -> Result<BuildResult, Error> {
    let execution = attempt
        .result
        .as_ref()
        .ok()
        .and_then(|r| r.execution.as_ref());
    let observations = observations(run, attempt, &id)?;
    prepare_result(run, execution, id, observations)
}

pub(crate) fn prepare_result(
    run: &dyn RunView,
    execution: Option<&yamaa_engine::dataset::Execution>,
    id: Identity<'_>,
    observations: StoredReport,
) -> Result<BuildResult, Error> {
    use yamaa_engine::specification_output::{self as output, CompleteError as E};
    let success = execution.is_some() && run.compiled().output_findings().is_empty();
    let logs = crate::verification_logs::prepare(run, &observations, success)?;
    let verification = logs.verification;
    let mut warning = logs.warning;
    let mut copy_budget = crate::report_projection::Budget::new(16_777_216);
    copy_budget.shared_report(&observations)?;
    let prepared = if let Some(error) = logs.failure {
        rejected_output(observations, &error, "log")
    } else {
        let fallback = observations.clone();
        output::prepare(
            run.compiled(),
            execution,
            8 * 1024 * 1024,
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
            E::OutputLimit => Error::OutputLimit,
            E::Publish(never) => match never {},
        })
        .or_else(|error| {
            if verification.is_some() {
                rejected_output(fallback, &error, "output")
            } else {
                Err(error)
            }
        })
    };
    let mut prepared = prepared?;
    if prepared.artifact().is_none() {
        warning = None;
    }
    let table = prepared
        .artifact()
        .map(|artifact| {
            crate::table_transport::encode_projected_dataset(
                &execution.ok_or(Error::InvalidObservation)?.dataset,
                artifact.projection(),
            )
            .map_err(|e| match e {
                crate::table_transport::TableTransportError::OutputLimit => Error::OutputLimit,
                _ => Error::InvalidObservation,
            })
        })
        .transpose();
    let table = match table {
        Ok(table) => table,
        Err(error) => {
            if verification.is_none() {
                return Err(error);
            }
            // Host-view preparation grants no output authority. Preserve the
            // completed verification capability and revoke warning/primary.
            copy_budget.shared_report(prepared.prepared_report())?;
            prepared = rejected_output(prepared.prepared_report().clone(), &error, "output")?;
            warning = None;
            None
        }
    };
    // Retain complete prospective artifact observations in publication order.
    // Ordinary build observations strip them until an actual Save enters them.
    if verification.is_some() || warning.is_some() {
        copy_budget.shared_report(prepared.prepared_report())?;
        for log in verification.iter().chain(warning.iter()) {
            copy_budget.json(log.observation())?;
        }
        let mut report = prepared.prepared_report().clone();
        let Value::Array(primary) = std::mem::take(&mut report["artifacts"]) else {
            return Err(Error::InvalidObservation);
        };
        let mut artifacts = Vec::new();
        if let Some(log) = &verification {
            artifacts.push(log.observation().clone());
        }
        if let Some(log) = &warning {
            artifacts.push(log.observation().clone());
        }
        artifacts.extend(primary);
        report["artifacts"] = Value::Array(artifacts);
        let report = bounded(report)?;
        prepared.replace_report(report);
    }
    let issues = prepared.prepared_report()["diagnostics"]
        .as_array()
        .ok_or(Error::InvalidObservation)?
        .iter()
        .cloned()
        .map(crate::issue_rows::Issue::from_diagnostic)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::InvalidObservation)?;
    if !crate::issue_rows::within_limit(&issues, crate::specification_check::MAX_ISSUE_BYTES) {
        return Err(Error::OutputLimit);
    }
    Ok(BuildResult {
        prepared,
        table,
        issues,
        verification,
        warning,
    })
}

fn rejected_output(
    mut report: StoredReport,
    error: &Error,
    stage: &str,
) -> Result<yamaa_engine::specification_output::PreparedOutput<StoredReport>, Error> {
    let reason = match (stage, error) {
        ("log", Error::OutputLimit) => "log_output_limit",
        ("log", _) => "log_output_projection",
        (_, Error::OutputLimit) => "output_limit",
        _ => "output_projection",
    };
    let diagnostic = specification_diagnostics::portable_diagnostic(
        yamaa_core::application_issue::rejected("output", reason),
    )
    .ok_or(Error::InvalidObservation)?;
    let Value::Array(mut diagnostics) = std::mem::take(&mut report["diagnostics"]) else {
        return Err(Error::InvalidObservation);
    };
    diagnostics.push(diagnostic);
    set_diagnostics(&mut report, diagnostics);
    report["outcome"] = json!("failure");
    report["nodes"][0]["outcome"] = json!("failure");
    report["artifacts"] = json!([]);
    Ok(yamaa_engine::specification_output::PreparedOutput::failed(
        bounded(report)?,
    ))
}
impl<E> From<Error> for CompleteError<E> {
    fn from(error: Error) -> Self {
        Self::Report(error)
    }
}
fn count(record: &Value, name: &str) -> Result<usize, Error> {
    record[name]
        .as_str()
        .and_then(|s| s.parse::<usize>().ok())
        .ok_or(Error::InvalidObservation)
}
pub(crate) fn check_observations(
    run: &dyn RunView,
    records: &[yamaa_engine::dataset::CheckRecord],
    phase: yamaa_core::verification_diagnostics::Phase,
    id: &Identity<'_>,
) -> Result<(Vec<Value>, Vec<Value>), Error> {
    use yamaa_core::verification_diagnostics::Error as ProjectionError;
    let projection = yamaa_engine::verification_diagnostics::project(
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
            .map(crate::project_diagnostic_projection::value)
            .collect(),
    ))
}
pub(crate) fn output_diagnostics(
    findings: &[yamaa_core::specification::OutputFinding],
) -> Result<Vec<Value>, Error> {
    findings
        .iter()
        .map(|finding| {
            crate::specification_diagnostics::portable_diagnostic(finding.diagnostic())
                .ok_or(Error::InvalidObservation)
        })
        .collect()
}
/// Render all portable observations and enforce report/output budgets before
/// publication. A host publication failure is retained, never a success report.
pub fn complete<E, P: ArtifactPort>(
    run: &dyn RunView,
    attempt: &CapturedAttempt<E>,
    id: Identity<'_>,
    publisher: &mut P,
) -> Result<Value, CompleteError<P::Error>> {
    let result = build_result(run, attempt, id)?;
    let items = result.publication_items();
    let mut budget = crate::report_projection::Budget::new(16_777_216);
    let report = result.publication_observations(items.len(), &mut budget)?;
    for (path, bytes, _) in &items {
        publisher
            .publish(path, bytes)
            .map_err(CompleteError::Publish)?;
    }
    Ok(report.observations_value(false))
}

struct Encoder<'a>(Option<&'static str>, Option<&'a str>);
impl yamaa_engine::specification_output::ArtifactEncoder for Encoder<'_> {
    type Error = Error;
    fn encode(
        &mut self,
        dataset: &yamaa_engine::dataset::Dataset,
        projection: &[usize],
        byte_limit: usize,
    ) -> Result<Vec<u8>, Error> {
        match self.0 {
            Some("csv") => {
                crate::csv_artifact::render_with_decimals(dataset, projection, self.1, byte_limit)
                    .map_err(|error| match error {
                        crate::csv_artifact::Error::Limit => Error::OutputLimit,
                        _ => Error::InvalidObservation,
                    })
            }
            Some("parquet") => crate::parquet_artifact::render(
                dataset,
                projection,
                crate::parquet_artifact::Limits {
                    output_bytes: byte_limit,
                    staged_bytes: 8 * 1024 * 1024,
                    cells: 1_048_576,
                    columns: 4096,
                },
            )
            .map_err(|error| match error {
                crate::parquet_artifact::Error::Limit => Error::OutputLimit,
                _ => Error::InvalidObservation,
            }),
            _ => Err(Error::InvalidObservation),
        }
    }
}

fn observations<E>(
    run: &dyn RunView,
    attempt: &CapturedAttempt<E>,
    id: &Identity<'_>,
) -> Result<StoredReport, Error> {
    let Some(response) = attempt
        .result
        .as_ref()
        .ok()
        .filter(|r| r.execution.is_some())
    else {
        return failure(run, attempt, id.borrow());
    };
    let value: Value =
        serde_json::from_str(&response.outcome).map_err(|_| Error::InvalidObservation)?;
    let mut report = envelope(run, attempt, id)?;
    let completed = &response
        .execution
        .as_ref()
        .ok_or(Error::InvalidObservation)?
        .verifications;
    let (checks, unexpected) = check_observations(
        run,
        completed,
        yamaa_core::verification_diagnostics::Phase::Verification,
        id,
    )?;
    if !unexpected.is_empty() {
        return Err(Error::InvalidObservation);
    }
    set_handler_counts(&mut report, &value)?;
    report["verifications"] = json!(checks);
    bounded(report)
}

struct Report<'a, 'i> {
    run: &'a dyn RunView,
    id: Identity<'i>,
    value: Option<StoredReport>,
}
impl yamaa_engine::specification_output::OutputReport for Report<'_, '_> {
    type Error = Error;
    type Report = StoredReport;
    fn failure(&mut self) -> Result<StoredReport, Error> {
        bounded(self.value.take().ok_or(Error::InvalidObservation)?)
    }
    fn begin(&mut self, _execution: &yamaa_engine::dataset::Execution) -> Result<(), Error> {
        self.value.as_ref().ok_or(Error::InvalidObservation)?;
        Ok(())
    }
    fn rejected(
        &mut self,
        findings: &[yamaa_core::specification::OutputFinding],
    ) -> Result<StoredReport, Error> {
        let mut report = self.value.take().ok_or(Error::InvalidObservation)?;
        set_diagnostics(&mut report, output_diagnostics(findings)?);
        bounded(report)
    }
    fn success(
        &mut self,
        execution: &yamaa_engine::dataset::Execution,
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
            None,
        )
    }
}

pub(crate) fn artifact_report(
    run: &dyn RunView,
    id: &Identity<'_>,
    mut report: StoredReport,
    execution: &yamaa_engine::dataset::Execution,
    projection: &[usize],
    bytes: &[u8],
    mut budget: Option<&mut crate::report_projection::Budget>,
) -> Result<StoredReport, Error> {
    let table = &execution.dataset;
    let profile = run
        .compiled()
        .output_profile()
        .ok_or(Error::InvalidObservation)?;
    if let Some(budget) = budget.as_deref_mut() {
        budget.entries(16)?;
        budget.entries(projection.len())?;
        budget.text(run.compiled().output_name())?;
        budget.text(profile)?;
        for name in run.compiled().projection() {
            budget.text(name)?;
        }
    }
    let (content, records) = if profile == "csv" {
        let content = std::str::from_utf8(bytes).map_err(|_| Error::InvalidObservation)?;
        if let Some(budget) = budget.as_deref_mut() {
            budget.text(content)?;
            budget.text(content)?; // content and record strings are separately retained.
            budget.entries(content.bytes().filter(|&b| b == b'\n').count())?;
        }
        let records = content
            .strip_suffix('\n')
            .ok_or(Error::InvalidObservation)?
            .split('\n')
            .map(String::from)
            .collect::<Vec<_>>();
        (content, records)
    } else {
        if let Some(budget) = budget.as_deref_mut() {
            budget.shape(table.row_count(), projection.len())?;
            for row in 0..table.row_count() {
                for &column in projection {
                    let value = table
                        .cell(row, column)
                        .map_err(|_| Error::InvalidObservation)?;
                    budget.cell(&value)?;
                }
            }
        }
        let records = crate::parquet_artifact::records(table, projection, 8 * 1024 * 1024)
            .map_err(|error| match error {
                crate::parquet_artifact::Error::Limit => Error::OutputLimit,
                _ => Error::InvalidObservation,
            })?;
        ("", records)
    };
    report["artifacts"] = json!([{"name":run.compiled().output_name(),"profile":profile,"columns":run.compiled().projection(),"types":projection.iter().map(|&c|specification_diagnostics::type_name(table.schema().columns()[c].kind)).collect::<Vec<_>>(),"row_count":table.row_count(),"records":records,"byte_length":bytes.len(),"content":content}]);
    report.push_table(table_observation_projected(
        table, id, "derived", "output", budget,
    )?)?;
    report["outcome"] = json!("success");
    report["nodes"][0]["outcome"] = json!("success");
    bounded(report)
}

impl Identity<'_> {
    pub(crate) fn borrow(&self) -> Identity<'_> {
        Identity {
            runtime: self.runtime,
            runtime_version: self.runtime_version,
            engine_version: self.engine_version,
            example: self.example,
            specification: self.specification,
            base_directory: self.base_directory,
        }
    }
}
