//! Public root results and explicit ordered publication from a sealed native owner.
//! Successful node reports still gate artifact consumption inside the build.
use crate::{
    issue_rows::Issue,
    producer_report::{self, NativeAttempt, ProjectionError},
    project_report::Classified,
    public_table::PublicTable,
    report_projection::Budget,
    specification_report::{Error as ReportError, Identity},
    table_transport::{self, TableTransportError},
};
use serde_json::Value;
use std::sync::Arc;
use yamaa_core::table::TableAccess;

pub struct RootResult {
    observations: Value,
    issues: Vec<Issue>,
    output: Option<(PublicTable, Vec<u8>)>,
    root: usize,
    logs: Arc<[crate::producer_report::NodeLogs]>,
    log_publication: Option<Arc<crate::producer_log_publication::Publication>>,
    log_save_attempt: Option<crate::producer_log_publication::Attempt>,
    _output_failure: Option<TableTransportError>,
    publication: Option<Arc<crate::producer_publication::Publication>>,
    publication_refusal: Option<&'static str>,
    base_issues: usize,
    latest: Option<Value>,
    save_attempt: Option<crate::producer_publication::Attempt>,
}
impl RootResult {
    pub fn observations(&self) -> &Value {
        self.latest.as_ref().unwrap_or(&self.observations)
    }
    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }
    pub fn output(&self) -> Option<&[u8]> {
        self.output.as_ref().map(|(_, bytes)| bytes.as_slice())
    }
    pub fn output_table(&self) -> Option<&PublicTable> {
        self.output.as_ref().map(|(table, _)| table)
    }
    fn held_log(&self, warning: bool) -> Option<&crate::verification_logs::HeldLog> {
        if warning && self.output.is_none() {
            return None;
        }
        let logs = &self.logs.iter().find(|logs| logs.node == self.root)?.logs;
        (if warning {
            &logs.warning
        } else {
            &logs.verification
        })
        .as_ref()
    }
    pub fn verification_log(&self) -> Option<&[u8]> {
        self.held_log(false).map(|log| log.ipc())
    }
    pub fn warning_log(&self) -> Option<&[u8]> {
        self.held_log(true).map(|log| log.ipc())
    }
    pub fn log_table(&self, warning: bool) -> Option<&PublicTable> {
        self.held_log(warning).map(|log| log.table())
    }
    /// Retain the original operational failure and actual entered prefix beside
    /// the unchanged build owner. Reading evidence grants no publication port.
    pub fn publication_attempt(&self) -> Option<&crate::producer_publication::Attempt> {
        self.save_attempt.as_ref()
    }
    pub fn save(&mut self) -> Result<bool, crate::file_application::FailedBuild> {
        self.save_with_limits(Default::default(), 16_777_216)
    }
    pub fn save_with_limits(
        &mut self,
        limits: yamaa_engine::producer_publication::Limits,
        maximum: usize,
    ) -> Result<bool, crate::file_application::FailedBuild> {
        if self.log_publication.is_some() {
            return self.save_logs_with(limits, maximum, |publisher, path, bytes| {
                publisher.publish(path, bytes)
            });
        }
        if self.output.is_none()
            || (self.publication.is_none() && self.publication_refusal.is_none())
        {
            return Err(crate::file_application::FailedBuild);
        }
        self.issues.truncate(self.base_issues);
        self.latest = None;
        self.save_attempt = None;
        let Some(publication) = self.publication.as_ref() else {
            self.issues.extend(crate::file_application::rejected(
                "output",
                self.publication_refusal.unwrap(),
            ));
            return Ok(false);
        };
        // Complete report ownership and prefix projection admission precedes
        // every publisher. Save never classifies a host error or rerenders Build.
        let mut budget = Budget::new(maximum);
        let admitted = (|| {
            budget.json(&self.observations)?;
            budget.entries(publication.artifacts().len().saturating_mul(64))?;
            for artifact in publication.artifacts() {
                for _ in 0..4 {
                    budget.text(artifact.path())?;
                }
            }
            Ok::<_, ReportError>(())
        })();
        if admitted.is_err() {
            self.issues.extend(crate::file_application::rejected(
                "output",
                "publication_report_limit",
            ));
            return Ok(false);
        }
        let mut value = self.observations.clone();
        let mut attempt = crate::producer_publication::Attempt::default();
        publication.save_into(limits, &mut attempt);
        use yamaa_engine::producer_publication::Failure;
        let (saved, code, node) = if let Some(failure) = publication.admission_failure() {
            use crate::producer_publication::AdmissionFailure;
            match failure {
                AdmissionFailure::Incomplete => (false, "publication_incomplete", None),
                AdmissionFailure::Limit(code) => (false, *code, None),
                AdmissionFailure::Resource { node, error } => {
                    self.issues
                        .extend(crate::file_application::resource_failure("output", error));
                    (false, "target_admission", Some(*node))
                }
            }
        } else if attempt.unwind.is_some() {
            (false, "publication_unwind", None)
        } else {
            match &attempt.entered.outcome {
                Ok(()) => (true, "", None),
                Err(Failure::Incomplete) => (false, "publication_incomplete", None),
                Err(Failure::Limit(code)) => (false, *code, None),
                Err(Failure::Preflight(error)) => {
                    self.issues
                        .extend(crate::file_application::publication_failure(&error.error));
                    (false, "target_preflight", Some(error.node))
                }
                Err(Failure::Publish { node, error }) => {
                    self.issues
                        .extend(crate::file_application::publication_failure(&error.error));
                    if let Some(artifact) =
                        publication.artifacts().iter().find(|a| a.node() == *node)
                    {
                        self.issues.push(
                            Issue::from_core(yamaa_core::application_issue::publication_failed(
                                artifact.path(),
                            ))
                            .expect("publication issue"),
                        );
                    }
                    (
                        false,
                        yamaa_core::diagnostic::ConditionCode::OutputPublicationFailed
                            .definition()
                            .condition,
                        Some(*node),
                    )
                }
            }
        };
        if !saved && self.issues.len() == self.base_issues {
            self.issues
                .extend(crate::file_application::rejected("output", code));
        }
        value["publication"] = serde_json::json!({"status":if saved {"success"} else {"failure"},
            "entered":attempt.entered.entered.iter().map(|item|serde_json::json!({"node":item.node,"published":item.published})).collect::<Vec<_>>(),
            "error":if saved {Value::Null} else {serde_json::json!({"stage":"publication","code":code,"node":node})}});
        self.latest = Some(value);
        self.save_attempt = Some(attempt);
        Ok(saved)
    }
    fn save_logs_with(
        &mut self,
        limits: yamaa_engine::producer_publication::Limits,
        maximum: usize,
        publish: impl FnMut(
            &mut crate::file_publication::Publisher,
            &str,
            &[u8],
        ) -> Result<(), crate::file_publication::Error>,
    ) -> Result<bool, crate::file_application::FailedBuild> {
        use crate::producer_log_publication::Failure;
        let failed = self.output.is_none();
        if failed
            && !self
                .logs
                .iter()
                .any(|logs| logs.logs.verification.is_some())
        {
            return Err(crate::file_application::FailedBuild);
        }
        self.issues.truncate(self.base_issues);
        self.latest = None;
        self.save_attempt = None;
        self.log_save_attempt = None;
        let publication = self.log_publication.as_ref().expect("logged owner");
        let count = publication.requests(failed).count();
        let mut budget = Budget::new(maximum);
        let report = (|| {
            budget.entries(count.saturating_mul(64))?;
            for (_, request) in publication.requests(failed) {
                for _ in 0..8 {
                    budget.text(&request.path)?;
                }
                budget.text(request.field)?;
            }
            // Own the complete report before any write. Every actual prefix is
            // formed by dropping already-owned artifact/entry suffixes; it never
            // reprojects observations or clones their tables and CSV content.
            publication.prefix_report(&self.observations, failed, count, count, &mut budget)
        })();
        let mut value = match report {
            Ok(report) => report,
            Err(_) => {
                self.issues.extend(crate::file_application::rejected(
                    "output",
                    "publication_report_limit",
                ));
                return if failed {
                    Err(crate::file_application::FailedBuild)
                } else {
                    Ok(false)
                };
            }
        };
        let mut attempt = crate::producer_log_publication::Attempt::default();
        publication.save_into(failed, limits, &mut attempt, publish);
        let (saved, code, request) = if let Some(failure) = publication.admission_failure(failed) {
            use crate::producer_publication::AdmissionFailure;
            match &failure.failure {
                AdmissionFailure::Incomplete => (false, "publication_incomplete", failure.request),
                AdmissionFailure::Limit(code) => (false, *code, failure.request),
                AdmissionFailure::Resource { error, .. } => {
                    self.issues
                        .extend(crate::file_application::resource_failure("output", error));
                    (false, "target_admission", failure.request)
                }
            }
        } else if attempt.unwind.is_some() {
            (
                false,
                "publication_unwind",
                attempt.entered.last().map(|entry| entry.request),
            )
        } else {
            match &attempt.outcome {
                Ok(()) => (true, "", None),
                Err(Failure::Incomplete | Failure::Admission) => {
                    (false, "publication_incomplete", None)
                }
                Err(Failure::Limit(code)) => (false, *code, None),
                Err(Failure::Preflight(failure)) => {
                    self.issues
                        .extend(crate::file_application::publication_failure(&failure.error));
                    (false, "target_preflight", Some(failure.request))
                }
                Err(Failure::Publish(failure)) => {
                    self.issues
                        .extend(crate::file_application::publication_failure(&failure.error));
                    self.issues.push(
                        Issue::from_core(yamaa_core::application_issue::publication_failed(
                            &publication.request(failure.request).path,
                        ))
                        .expect("publication issue"),
                    );
                    (
                        false,
                        yamaa_core::diagnostic::ConditionCode::OutputPublicationFailed
                            .definition()
                            .condition,
                        Some(failure.request),
                    )
                }
            }
        };
        if !saved && self.issues.len() == self.base_issues {
            self.issues
                .extend(crate::file_application::rejected("output", code));
        }
        let entered = attempt.entered.len();
        let completed = attempt
            .entered
            .iter()
            .filter(|entry| entry.published)
            .count();
        value["artifacts"]
            .as_array_mut()
            .expect("admitted publication artifacts")
            .truncate(completed);
        let entries = value["publication"]["entered"]
            .as_array_mut()
            .expect("admitted publication entries");
        entries.truncate(entered);
        for (entry, actual) in entries.iter_mut().zip(&attempt.entered) {
            entry["published"] = Value::Bool(actual.published);
        }
        value["publication"]["status"] =
            Value::String(if saved { "success" } else { "failure" }.into());
        if saved {
            value["publication"]["error"] = Value::Null;
        } else {
            let actual = attempt
                .entered
                .last()
                .filter(|entry| Some(entry.request) == request);
            let request = request.map(|index| publication.request(index));
            value["publication"]["error"] = serde_json::json!({"stage":"publication","code":code,
                "node":actual.map(|entry|entry.node).or_else(||request.map(|request|request.node)),
                "field":actual.map(|entry|entry.field).or_else(||request.map(|request|request.field)),
                "path":actual.map(|entry|entry.path.as_str()).or_else(||request.map(|request|request.path.as_str()))});
            if let Some(request) = request {
                for issue in &mut self.issues[self.base_issues..] {
                    issue.spec_paths = vec![format!("output.{}", request.field)];
                    let mut context: Value =
                        serde_json::from_str(&issue.context).expect("native issue context");
                    context["node"] = serde_json::json!(request.node);
                    context["path"] = serde_json::json!(request.path);
                    context["entered"] = serde_json::json!(entered);
                    context["completed"] = serde_json::json!(completed);
                    issue.context = context.to_string();
                }
            }
        }
        self.latest = Some(value);
        self.log_save_attempt = Some(attempt);
        if failed {
            Err(crate::file_application::FailedBuild)
        } else {
            Ok(saved)
        }
    }
}

/// Refusal preserves the original whole attempt and, when already completed,
/// its bounded report. There is no partial successful public root result.
pub enum Error<'a, E> {
    Report(ProjectionError<'a, E>),
    Output {
        attempt: &'a NativeAttempt<E>,
        observations: Value,
        error: TableTransportError,
    },
}
impl<E> std::fmt::Debug for Error<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Report(error) => f.debug_tuple("Report").field(error).finish(),
            Self::Output { error, .. } => f.debug_tuple("Output").field(error).finish(),
        }
    }
}

/// Both host facts and root projection originate in this exact whole owner.
/// A failed graph may retain earlier producer artifacts as evidence, but has
/// no accepted public output. No metadata, study or host port is entered here.
pub fn prepare_classified<'a, E>(
    attempt: &'a NativeAttempt<E>,
    id: Identity<'_>,
    maximum: usize,
    facts: &[Classified<'a, E>],
) -> Result<RootResult, Error<'a, E>> {
    let observations = producer_report::attempt_report_classified(attempt, id, maximum, facts)
        .map_err(Error::Report)?;
    prepare_observations(attempt, observations, maximum, true)
}
fn prepare_observations<'a, E>(
    attempt: &'a NativeAttempt<E>,
    observations: Value,
    maximum: usize,
    output_allowed: bool,
) -> Result<RootResult, Error<'a, E>> {
    let mut budget = Budget::new(maximum);
    let diagnostics =
        observations["diagnostics"]
            .as_array()
            .ok_or(Error::Report(ProjectionError::Report(
                ReportError::InvalidObservation,
            )))?;
    let admit = (|| {
        budget.entries(diagnostics.len())?;
        for diagnostic in diagnostics {
            // JSON clone, five-field owned rows and escaped context text.
            for _ in 0..3 {
                budget.json(diagnostic)?;
            }
        }
        Ok::<_, ReportError>(())
    })();
    admit.map_err(|error| Error::Report(ProjectionError::Report(error)))?;
    let mut issues = diagnostics
        .iter()
        .cloned()
        .map(Issue::from_diagnostic)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Report(ProjectionError::Report(ReportError::InvalidObservation)))?;
    if (!attempt.accepted() || !output_allowed) && issues.is_empty() {
        issues = crate::file_application::rejected("build", "producer_build");
    }
    if !crate::issue_rows::within_limit(
        &issues,
        maximum.min(crate::specification_check::MAX_ISSUE_BYTES),
    ) {
        return Err(Error::Report(ProjectionError::Report(
            ReportError::OutputLimit,
        )));
    }
    let output = if attempt.accepted() && output_allowed {
        match root_output(attempt, &mut budget) {
            Ok(output) => Some(output),
            Err(error) => {
                return Err(Error::Output {
                    attempt,
                    observations,
                    error,
                });
            }
        }
    } else {
        None
    };
    Ok(RootResult {
        observations,
        base_issues: issues.len(),
        issues,
        output,
        root: attempt.provenance.metadata().root(),
        logs: Arc::from([]),
        log_publication: None,
        log_save_attempt: None,
        _output_failure: None,
        publication: None,
        publication_refusal: None,
        latest: None,
        save_attempt: None,
    })
}

/// Only a privately constructed actual build owner can grant Save. Report and
/// public root projection must both finish before that authority is attached.
pub fn prepare_owned_classified<'a, E>(
    attempt: &'a crate::file_producer_build::OwnedAttempt<E>,
    id: Identity<'_>,
    maximum: usize,
    facts: &[Classified<'a, E>],
) -> Result<RootResult, Error<'a, E>> {
    let has_verification = attempt
        .logs()
        .iter()
        .any(|logs| logs.logs.verification.is_some());
    let mut result = match prepare_classified(attempt, id, maximum, facts) {
        Ok(result) => result,
        Err(Error::Output {
            mut observations,
            error,
            ..
        }) if has_verification => {
            let mut budget = Budget::new(maximum);
            budget
                .json(&observations)
                .and_then(|()| budget.entries(32))
                .map_err(|error| Error::Report(ProjectionError::Report(error)))?;
            observations["outcome"] = Value::String("failure".into());
            observations["error"] =
                serde_json::json!({"stage":"output","code":"output_projection"});
            let finding = crate::specification_diagnostics::portable_diagnostic(
                yamaa_core::application_issue::rejected("output", "output_projection"),
            )
            .expect("native rejection");
            observations["diagnostics"]
                .as_array_mut()
                .ok_or(Error::Report(ProjectionError::Report(
                    ReportError::InvalidObservation,
                )))?
                .push(finding);
            let mut result = prepare_observations(attempt, observations, maximum, false)?;
            result._output_failure = Some(error);
            result
        }
        Err(Error::Report(ProjectionError::Original { observations, .. })) if has_verification => {
            prepare_observations(attempt, observations, maximum, false)?
        }
        Err(error) => return Err(error),
    };
    result.logs = Arc::clone(attempt.logs());
    result.log_publication = attempt.log_publication().cloned();
    if result.output.is_some() {
        result.publication = attempt.publication().cloned();
        result.publication_refusal = attempt.publication_refusal();
    }
    Ok(result)
}

fn root_output<E>(
    attempt: &NativeAttempt<E>,
    budget: &mut Budget,
) -> Result<(PublicTable, Vec<u8>), TableTransportError> {
    let invalid = TableTransportError::InvalidTable;
    let metadata = attempt.provenance.metadata();
    // Root identity and compiler are borrowed from the sealed complete owner.
    budget
        .entries(metadata.nodes().len())
        .map_err(|_| TableTransportError::OutputLimit)?;
    budget
        .work(
            metadata
                .nodes()
                .len()
                .checked_mul(3)
                .ok_or(TableTransportError::OutputLimit)?,
        )
        .map_err(|_| TableTransportError::OutputLimit)?;
    let root = metadata.root();
    let view = attempt.provenance.report_views().nth(root).ok_or(invalid)?;
    let mut matches = attempt.graph.nodes.iter().filter(|node| node.node == root);
    let entered = matches.next().ok_or(invalid)?;
    if matches.next().is_some() {
        return Err(invalid);
    }
    let execution = entered
        .dataset
        .result
        .as_ref()
        .map_err(|_| invalid)?
        .result
        .as_ref()
        .map_err(|_| invalid)?;
    let artifact = entered
        .output
        .as_ref()
        .map_err(|_| invalid)?
        .as_ref()
        .and_then(|output| output.artifact())
        .ok_or(invalid)?;
    let table = &execution.dataset;
    let columns = table.schema().columns();
    let projection = artifact.projection();
    budget
        .entries(projection.len())
        .map_err(|_| TableTransportError::OutputLimit)?;
    budget
        .work(
            projection
                .len()
                .checked_mul(columns.len())
                .ok_or(TableTransportError::OutputLimit)?,
        )
        .map_err(|_| TableTransportError::OutputLimit)?;
    if projection.len() != view.compiled().projection().len()
        || projection
            .iter()
            .zip(view.compiled().projection())
            .any(|(&index, name)| columns.get(index).is_none_or(|column| &column.name != name))
    {
        return Err(invalid);
    }
    // The qualified encoder sanitizes the complete table before projection.
    // Admit full shape, names, cells and work for Arrow, public columns and IPC
    // before any of those owners allocate. Serialized CSV precision is irrelevant.
    for _ in 0..3 {
        budget
            .shape(table.row_count(), columns.len())
            .map_err(|_| TableTransportError::OutputLimit)?;
        for (index, column) in columns.iter().enumerate() {
            budget
                .text(&column.name)
                .map_err(|_| TableTransportError::OutputLimit)?;
            for row in 0..table.row_count() {
                let cell = table.cell(row, index).map_err(|_| invalid)?;
                budget
                    .cell(&cell)
                    .map_err(|_| TableTransportError::OutputLimit)?;
            }
        }
    }
    let bytes = table_transport::encode_projected_dataset(table, projection)?;
    let table = PublicTable::decode(&bytes)?;
    let bytes = table.ipc()?;
    Ok((table, bytes))
}

#[cfg(test)]
mod log_tests {
    use super::*;
    use crate::{
        file_graph::FileGraph,
        file_producer_build::OwnedAttempt,
        file_resources::Resources,
        project_source::{self, CapturePort, OwnedEnvironment, Reply, Request},
        shipped_schema,
        specification_source::Source,
    };
    use std::{
        convert::Infallible,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use yamaa_core::{
        function_signature::{LogicalSignature, ProjectFunctionIdentity},
        project_environment::LockReference,
        project_function::Language,
        value::Value as Scalar,
    };
    use yamaa_engine::{
        function_invocation::{Argument, HostError},
        project_activation::ActivationPort,
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Study(PathBuf);
    impl Study {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "yamaa-producer-logs-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("generated")).unwrap();
            Self(fs::canonicalize(root).unwrap())
        }
        fn path(&self) -> String {
            let path = self.0.to_str().unwrap().to_owned();
            #[cfg(windows)]
            let path = path
                .strip_prefix(r"\\?\")
                .unwrap_or(&path)
                .replace('\\', "/");
            path
        }
        fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
            fs::write(self.0.join(name), bytes).unwrap();
        }
        fn fixture(&self, fatal_leaf: bool, fatal_root: bool, logs: bool) {
            self.write("input.csv", "ID\n1\n2\n");
            let yaml = |source: &str, base: &str, name: &str, fatal: bool| {
                let logs = if logs {
                    format!(
                        ", verification_log: generated/{name}-checks.csv, warning_log: generated/{name}-warnings.csv"
                    )
                } else {
                    String::new()
                };
                let severity = if fatal { "error" } else { "warning" };
                let checks = if logs.is_empty() {
                    String::new()
                } else {
                    format!(
                        "    verifications: [{{range: {{max: 0, severity: {severity}, id: {name}-range}}}}]\n"
                    )
                };
                format!(
                    "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {source}\ncolumns:\n  - name: ID\n    type: int\n    label: Identifier\n    derivation: {base}.ID\n{checks}output: {{path: generated/{name}.csv, columns: [ID]{logs}}}\n"
                )
            };
            self.write(
                "producer.yaml",
                yaml(
                    "{SRC: {path: input.csv, types: {ID: int}}}",
                    "SRC",
                    "producer",
                    fatal_leaf,
                ),
            );
            let root = yaml("{A: {path: generated/producer.csv, schema: producer.yaml}, B: {path: generated/producer.csv, schema: ./producer.yaml}}", "A", "root", fatal_root)
                .replace("columns:\n", "intermediates: [{id: SHARED, dataset: B, key: [ID], no_match: null}]\ncolumns:\n");
            self.write("root.yaml", root);
        }
        fn graph(
            &self,
            language: Language,
        ) -> Result<FileGraph, Box<crate::file_graph::RejectedGraph>> {
            FileGraph::prepare(
                Resources::new(&self.path(), &self.path(), &[]).unwrap(),
                "root.yaml",
                environment(language),
                shipped_schema::capture().unwrap(),
                Default::default(),
            )
        }
        fn owned(
            &self,
            language: Language,
            limits: crate::file_producer_build::Limits,
        ) -> OwnedAttempt<Infallible> {
            self.graph(language)
                .unwrap_or_else(|error| panic!("graph fixture: {:?}", error.error()))
                .into_build()
                .build_owned_reported(&mut NoActivation, id(), limits)
        }
        fn remove_inputs(&self) {
            for name in ["root.yaml", "producer.yaml", "input.csv"] {
                fs::remove_file(self.0.join(name)).unwrap();
                assert!(!self.0.join(name).exists());
            }
        }
        fn generated(&self, name: &str) -> Vec<u8> {
            fs::read(self.0.join("generated").join(name)).unwrap()
        }
    }
    impl Drop for Study {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    struct NoMetadata;
    impl CapturePort for NoMetadata {
        type Error = Infallible;
        fn capture(&mut self, _: Request<'_>) -> Result<Reply, Infallible> {
            panic!("no metadata dependency")
        }
    }
    fn environment(language: Language) -> OwnedEnvironment {
        project_source::prepare(
            shipped_schema::capture_environment().unwrap(),
            Source {
                identity: "held/environment.yaml".into(),
                bytes: b"schema_version: '1.0'\n".to_vec(),
            },
            language,
            &mut NoMetadata,
            Default::default(),
        )
        .unwrap()
        .into_owned()
    }
    struct NoActivation;
    impl ActivationPort for NoActivation {
        type Handle = ();
        type Error = Infallible;
        fn verify_lock(
            &mut self,
            _: Language,
            _: &LockReference,
            _: &[ProjectFunctionIdentity],
        ) -> Result<(), Infallible> {
            panic!("no lock")
        }
        fn bind(
            &mut self,
            _: &ProjectFunctionIdentity,
            _: &LogicalSignature,
        ) -> Result<(), Infallible> {
            panic!("no binding")
        }
        fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Scalar, HostError<Infallible>> {
            panic!("no function")
        }
    }
    fn id() -> Identity<'static> {
        Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "producer-logs",
            specification: "root.yaml",
            base_directory: ".",
        }
    }
    fn root(owned: &OwnedAttempt<Infallible>) -> RootResult {
        prepare_owned_classified(owned, id(), 16_777_216, &[]).unwrap()
    }
    fn execution(mut report: Value) -> Value {
        report.as_object_mut().unwrap().remove("publication");
        report.as_object_mut().unwrap().remove("artifacts");
        report
    }
    #[test]
    fn actual_alias_logs_publish_in_node_order_and_repeat_only_held_bytes() {
        for language in [Language::Python, Language::R] {
            let study = Study::new();
            study.fixture(false, false, true);
            let owned = study.owned(language, Default::default());
            assert!(owned.accepted());
            let source = &owned.graph.nodes[1].dataset.sources;
            assert!(Arc::ptr_eq(
                source[0].snapshot.as_ref().unwrap(),
                source[1].snapshot.as_ref().unwrap()
            ));
            let mut result = root(&owned);
            let before = execution(result.observations().clone());
            let output = result.output().unwrap().to_vec();
            let warning = result.warning_log().unwrap().to_vec();
            assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
            study.remove_inputs();
            let mut snapshots = None;
            for _ in 0..2 {
                assert!(result.save().unwrap());
                let report = result.observations();
                assert_eq!(
                    report["publication"]["entered"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|item| (
                            item["node"].as_u64().unwrap(),
                            item["field"].as_str().unwrap()
                        ))
                        .collect::<Vec<_>>(),
                    [
                        (1, "verification_log"),
                        (1, "warning_log"),
                        (1, "path"),
                        (0, "verification_log"),
                        (0, "warning_log"),
                        (0, "path")
                    ]
                );
                assert_eq!(report["artifacts"].as_array().unwrap().len(), 6);
                assert_eq!(execution(report.clone()), before);
                assert_eq!(result.output().unwrap(), output);
                assert_eq!(result.warning_log().unwrap(), warning);
                assert!(result.issues().is_empty());
                let bytes = [
                    "producer-checks.csv",
                    "producer-warnings.csv",
                    "producer.csv",
                    "root-checks.csv",
                    "root-warnings.csv",
                    "root.csv",
                ]
                .map(|name| study.generated(name));
                assert_eq!(bytes[2], b"ID\n1\n2\n");
                assert_eq!(bytes[5], b"ID\n1\n2\n");
                if let Some(first) = &snapshots {
                    assert_eq!(&bytes, first);
                } else {
                    snapshots = Some(bytes);
                }
            }
        }
    }
    #[test]
    fn retained_two_node_report_saves_without_copying_every_possible_prefix() {
        let study = Study::new();
        let text = "x".repeat(32_768);
        let csv = format!("ID,VALUE\n1,{text}\n");
        study.write("input.csv", &csv);
        let yaml = |source: &str, base: &str, name: &str| {
            format!(
                "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {source}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - {{name: VALUE, type: str, label: Value, derivation: {base}.VALUE}}\noutput: {{path: generated/{name}.csv, columns: [ID, VALUE], verification_log: generated/{name}-checks.csv, warning_log: generated/{name}-warnings.csv}}\n"
            )
        };
        study.write("producer.yaml", yaml("{SRC: input.csv}", "SRC", "producer"));
        study.write(
            "root.yaml",
            yaml(
                "{A: {path: generated/producer.csv, schema: producer.yaml}}",
                "A",
                "root",
            ),
        );
        let owned = study.owned(Language::Python, Default::default());
        assert!(owned.accepted());
        let mut result = root(&owned);
        let before = execution(result.observations().clone());
        study.remove_inputs();
        assert!(result.save().unwrap(), "{:?}", result.issues());
        assert_eq!(study.generated("producer.csv"), csv.as_bytes());
        assert_eq!(study.generated("root.csv"), csv.as_bytes());
        let report = result.observations();
        assert_eq!(execution(report.clone()), before);
        assert_eq!(report["artifacts"].as_array().unwrap().len(), 6);
        assert_eq!(
            report["publication"]["entered"].as_array().unwrap().len(),
            6
        );
        assert!(report["publication"]["entered"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["published"] == true));
        assert!(result.issues().is_empty());
    }
    #[test]
    fn fatal_leaf_or_dependent_keeps_actual_v_prefix_and_revokes_every_primary_and_warning() {
        for (leaf, expected_nodes, expected_v) in [(true, 1, 1), (false, 2, 2)] {
            let study = Study::new();
            study.fixture(leaf, !leaf, true);
            let owned = study.owned(Language::Python, Default::default());
            assert!(!owned.accepted());
            assert_eq!(owned.graph.nodes.len(), expected_nodes);
            let mut result = root(&owned);
            assert!(result.output().is_none());
            assert!(result.warning_log().is_none());
            let before = execution(result.observations().clone());
            let issues = result.issues().to_vec();
            for name in [
                "producer.csv",
                "root.csv",
                "producer-warnings.csv",
                "root-warnings.csv",
            ] {
                study.write(&format!("generated/{name}"), b"held sentinel");
            }
            study.remove_inputs();
            for _ in 0..2 {
                assert!(result.save().is_err());
                assert_eq!(result.observations()["publication"]["status"], "success");
                assert_eq!(result.observations()["publication"]["mode"], "diagnostic");
                assert_eq!(
                    result.observations()["artifacts"].as_array().unwrap().len(),
                    expected_v
                );
                assert_eq!(execution(result.observations().clone()), before);
                assert_eq!(result.issues(), issues);
                for name in [
                    "producer.csv",
                    "root.csv",
                    "producer-warnings.csv",
                    "root-warnings.csv",
                ] {
                    assert_eq!(study.generated(name), b"held sentinel");
                }
            }
        }
    }
    #[derive(Debug)]
    struct Payload(Arc<()>);
    impl std::fmt::Display for Payload {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("original publication failure")
        }
    }
    impl std::error::Error for Payload {}
    #[test]
    fn original_error_or_unwind_retains_entered_prefix_and_retry_replaces_only_save_state() {
        for unwind in [false, true] {
            let study = Study::new();
            study.fixture(false, false, true);
            let owned = study.owned(Language::Python, Default::default());
            let mut result = root(&owned);
            let before = execution(result.observations().clone());
            study.write("generated/root.csv", b"earlier root");
            study.remove_inputs();
            let payload = Arc::new(());
            let mut calls = 0;
            assert!(!result
                .save_logs_with(Default::default(), 16_777_216, |publisher, path, bytes| {
                    calls += 1;
                    if calls == 5 {
                        if unwind {
                            std::panic::panic_any(Payload(Arc::clone(&payload)));
                        }
                        return Err(crate::file_publication::Error::Io(std::io::Error::other(
                            Payload(Arc::clone(&payload)),
                        )));
                    }
                    publisher.publish(path, bytes)
                })
                .unwrap());
            let saved = result.log_save_attempt.as_ref().unwrap();
            assert_eq!(saved.entered.len(), 5);
            assert_eq!(
                saved.entered.iter().filter(|entry| entry.published).count(),
                4
            );
            assert_eq!(saved.entered[4].field, "warning_log");
            assert_eq!(saved.entered[4].node, 0);
            if unwind {
                assert!(Arc::ptr_eq(
                    &saved
                        .unwind
                        .as_ref()
                        .unwrap()
                        .downcast_ref::<Payload>()
                        .unwrap()
                        .0,
                    &payload
                ));
            } else {
                let Err(crate::producer_log_publication::Failure::Publish(failure)) =
                    &saved.outcome
                else {
                    panic!("original publish failure")
                };
                let crate::file_publication::Error::Io(error) = &failure.error else {
                    panic!("original IO")
                };
                assert!(Arc::ptr_eq(
                    &error
                        .get_ref()
                        .unwrap()
                        .downcast_ref::<Payload>()
                        .unwrap()
                        .0,
                    &payload
                ));
            }
            assert_eq!(study.generated("producer.csv"), b"ID\n1\n2\n");
            assert_eq!(study.generated("root.csv"), b"earlier root");
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                4
            );
            assert_eq!(execution(result.observations().clone()), before);
            assert!(result
                .issues()
                .iter()
                .any(|issue| issue.spec_paths == ["output.warning_log"]));
            let normative = result
                .issues()
                .iter()
                .filter(|issue| issue.requirement.as_deref() == Some("REQ-0764"))
                .collect::<Vec<_>>();
            if unwind {
                assert!(
                    normative.is_empty(),
                    "an opaque unwind does not establish atomic replacement failure"
                );
            } else {
                assert_eq!(
                    normative.len(),
                    1,
                    "actual publication failure must report REQ-0764"
                );
                assert_eq!(normative[0].spec_paths, ["output.warning_log"]);
                let context: Value = serde_json::from_str(&normative[0].context).unwrap();
                assert_eq!(context["node"], 0);
                assert_eq!(context["path"], "generated/root-warnings.csv");
                assert_eq!(context["entered"], 5);
                assert_eq!(context["completed"], 4);
            }
            assert!(result.save().unwrap());
            assert!(result.issues().is_empty());
            assert_eq!(result.log_save_attempt.as_ref().unwrap().entered.len(), 6);
            assert!(result.log_save_attempt.as_ref().unwrap().unwind.is_none());
            assert_eq!(execution(result.observations().clone()), before);
        }
    }
    #[test]
    fn every_error_and_unwind_boundary_keeps_the_actual_owned_prefix() {
        let requests = [
            (1, "verification_log", "producer-checks.csv"),
            (1, "warning_log", "producer-warnings.csv"),
            (1, "path", "producer.csv"),
            (0, "verification_log", "root-checks.csv"),
            (0, "warning_log", "root-warnings.csv"),
            (0, "path", "root.csv"),
        ];
        for unwind in [false, true] {
            for boundary in 1..=requests.len() {
                let study = Study::new();
                study.fixture(false, false, true);
                let owned = study.owned(Language::Python, Default::default());
                let mut result = root(&owned);
                let before = execution(result.observations().clone());
                study.remove_inputs();
                let mut calls = 0;
                assert!(!result
                    .save_logs_with(Default::default(), 16_777_216, |publisher, path, bytes| {
                        calls += 1;
                        if calls == boundary {
                            if unwind {
                                std::panic::panic_any(Payload(Arc::new(())));
                            }
                            return Err(crate::file_publication::Error::Io(std::io::Error::other(
                                Payload(Arc::new(())),
                            )));
                        }
                        publisher.publish(path, bytes)
                    })
                    .unwrap());
                let report = result.observations();
                let expected = requests[..boundary]
                    .iter()
                    .enumerate()
                    .map(|(position, (node, field, name))| {
                        serde_json::json!({
                            "node":node, "field":field, "path":format!("generated/{name}"),
                            "published":position + 1 < boundary
                        })
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    report["publication"]["entered"],
                    serde_json::json!(expected)
                );
                assert_eq!(report["artifacts"].as_array().unwrap().len(), boundary - 1);
                assert_eq!(execution(report.clone()), before);
                for (position, (_, _, name)) in requests.iter().enumerate() {
                    assert_eq!(
                        study.0.join("generated").join(name).exists(),
                        position + 1 < boundary
                    );
                }
                assert!(result.save().unwrap());
                assert_eq!(
                    result.observations()["artifacts"].as_array().unwrap().len(),
                    requests.len()
                );
                assert_eq!(execution(result.observations().clone()), before);
                assert!(result.issues().is_empty());
            }
        }
    }
    #[test]
    fn cumulative_admission_and_whole_set_parent_preflight_enter_no_publishers() {
        let study = Study::new();
        study.fixture(false, false, true);
        let owned = study.owned(Language::Python, Default::default());
        let mut result = root(&owned);
        let bytes = owned
            .logs()
            .iter()
            .flat_map(|held| held.logs.verification.iter().chain(&held.logs.warning))
            .map(|log| log.bytes().len())
            .sum::<usize>()
            + owned
                .graph
                .nodes
                .iter()
                .map(|entered| {
                    entered
                        .output
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .artifact()
                        .unwrap()
                        .bytes()
                        .len()
                })
                .sum::<usize>();
        for limits in [
            yamaa_engine::producer_publication::Limits {
                nodes: 1,
                ..Default::default()
            },
            yamaa_engine::producer_publication::Limits {
                path_bytes: 1,
                ..Default::default()
            },
            yamaa_engine::producer_publication::Limits {
                artifact_bytes: bytes - 1,
                ..Default::default()
            },
            yamaa_engine::producer_publication::Limits {
                work: 1,
                ..Default::default()
            },
        ] {
            assert!(!result
                .save_logs_with(limits, 16_777_216, |_, _, _| panic!(
                    "unadmitted publication"
                ))
                .unwrap());
            assert!(result.log_save_attempt.as_ref().unwrap().entered.is_empty());
        }
        assert!(!result
            .save_logs_with(Default::default(), 1, |_, _, _| panic!(
                "report quota precedes ports"
            ))
            .unwrap());
        assert!(result.log_save_attempt.is_none());
        fs::remove_dir(study.0.join("generated")).unwrap();
        assert!(!result
            .save_logs_with(Default::default(), 16_777_216, |_, _, _| panic!(
                "all parents first"
            ))
            .unwrap());
        assert!(result.log_save_attempt.as_ref().unwrap().entered.is_empty());
        assert!(!result
            .issues()
            .iter()
            .any(|issue| issue.requirement.as_deref() == Some("REQ-0764")));
        fs::create_dir(study.0.join("generated")).unwrap();
        assert!(result.save().unwrap());
    }
    #[test]
    fn warning_byte_refusal_retains_verification_and_stops_dependent_before_entry() {
        let study = Study::new();
        study.fixture(false, false, true);
        let baseline = study.owned(Language::Python, Default::default());
        let leaf = &baseline
            .logs()
            .iter()
            .find(|held| held.node == 1)
            .unwrap()
            .logs;
        let bytes = leaf.verification.as_ref().unwrap().bytes().len()
            + leaf.warning.as_ref().unwrap().bytes().len()
            - 1;
        let mut limits = crate::file_producer_build::Limits::default();
        limits.engine.output_bytes = bytes;
        let owned = study.owned(Language::Python, limits);
        assert!(!owned.accepted());
        assert_eq!(owned.graph.nodes.len(), 1);
        assert!(owned.logs()[0].logs.verification.is_some());
        assert!(owned.logs()[0].logs.warning.is_none());
        let mut result = root(&owned);
        study.remove_inputs();
        assert!(result.save().is_err());
        assert_eq!(
            result.observations()["artifacts"].as_array().unwrap().len(),
            1
        );
        assert!(!study.0.join("generated/producer.csv").exists());
    }
    #[test]
    fn logged_graph_metadata_refuses_cross_node_aliases_before_study_and_missing_parents_stay_allowed(
    ) {
        let study = Study::new();
        study.fixture(false, false, true);
        let raw = fs::read_to_string(study.0.join("root.yaml")).unwrap();
        study.write(
            "root.yaml",
            raw.replace(
                "generated/root-checks.csv",
                "generated/../generated/producer.csv",
            ),
        );
        let rejected = match study.graph(Language::Python) {
            Err(error) => error,
            Ok(_) => panic!("cross-node collision"),
        };
        assert_eq!(rejected.capture_reads(), 2);
        assert!(format!("{:?}", rejected.error()).contains("Collision"));
        study.write("root.yaml", raw);
        fs::remove_dir(study.0.join("generated")).unwrap();
        assert!(study.graph(Language::Python).is_ok());
    }
    #[test]
    fn target_admission_retains_the_actual_warning_field_and_its_original_path() {
        let study = Study::new();
        study.fixture(false, false, true);
        let raw = fs::read_to_string(study.0.join("root.yaml")).unwrap();
        study.write(
            "root.yaml",
            raw.replace(
                "generated/root-warnings.csv",
                "https://example.invalid/root-warnings.csv",
            ),
        );
        let owned = study.owned(Language::Python, Default::default());
        assert!(owned.accepted());
        let mut result = root(&owned);
        let before = execution(result.observations().clone());
        study.remove_inputs();
        for _ in 0..2 {
            assert!(!result
                .save_logs_with(Default::default(), 16_777_216, |_, _, _| panic!(
                    "all target admission precedes publication"
                ))
                .unwrap());
            let error = &result.observations()["publication"]["error"];
            assert_eq!(error["code"], "target_admission");
            assert_eq!(error["node"], 0);
            assert_eq!(error["field"], "warning_log");
            assert_eq!(error["path"], "https://example.invalid/root-warnings.csv");
            assert!(result.issues()[result.base_issues..]
                .iter()
                .all(|issue| issue.spec_paths == ["output.warning_log"]));
            assert!(result.log_save_attempt.as_ref().unwrap().entered.is_empty());
            assert!(!result
                .issues()
                .iter()
                .any(|issue| issue.requirement.as_deref() == Some("REQ-0764")));
            assert_eq!(execution(result.observations().clone()), before);
        }
        assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
    }
    #[cfg(windows)]
    #[test]
    fn absent_windows_graph_sidecar_case_alias_is_metadata_refused_before_study() {
        let study = Study::new();
        study.fixture(false, false, true);
        let raw = fs::read_to_string(study.0.join("root.yaml")).unwrap();
        study.write(
            "root.yaml",
            raw.replace("generated/root-checks.csv", "generated/PRODUCER-checks.csv"),
        );
        let rejected = study
            .graph(Language::Python)
            .err()
            .expect("ordinal target alias");
        assert_eq!(rejected.capture_reads(), 2);
        assert!(format!("{:?}", rejected.error()).contains("Collision"));
        assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_graph_sidecar_absent_entry_and_physical_parent_aliases_preflight_as_one_set() {
        let study = Study::new();
        study.write("generated/case-probe", b"native case witness");
        let sensitive = !study.0.join("generated/CASE-PROBE").exists();
        fs::remove_file(study.0.join("generated/case-probe")).unwrap();
        for (name, physical_parent) in [
            ("generated/PRODUCER-checks.csv", false),
            ("GENERATED/producer-checks.csv", true),
            (r#""generated/\u00e9.csv""#, false),
        ] {
            if physical_parent && sensitive {
                continue;
            }
            study.fixture(false, false, true);
            let raw = fs::read_to_string(study.0.join("root.yaml")).unwrap();
            study.write("root.yaml", raw.replace("generated/root-checks.csv", name));
            if physical_parent {
                assert_eq!(
                    fs::canonicalize(study.0.join("GENERATED")).unwrap(),
                    fs::canonicalize(study.0.join("generated")).unwrap()
                );
            }
            assert!(!study.0.join("generated/producer-checks.csv").exists());
            let owned = study.owned(Language::Python, Default::default());
            assert!(owned.accepted());
            let mut result = root(&owned);
            let before = execution(result.observations().clone());
            if name == "generated/PRODUCER-checks.csv" && sensitive {
                assert!(result.save().unwrap());
                for entry in fs::read_dir(study.0.join("generated")).unwrap() {
                    fs::remove_file(entry.unwrap().path()).unwrap();
                }
            } else {
                assert!(!result
                    .save_logs_with(Default::default(), 16_777_216, |_, _, _| panic!(
                        "whole physical target set first"
                    ))
                    .unwrap());
                assert_eq!(
                    result.observations()["publication"]["error"]["code"],
                    "target_preflight"
                );
                assert_eq!(
                    result.observations()["publication"]["error"]["field"],
                    "verification_log"
                );
                assert!(result.log_save_attempt.as_ref().unwrap().entered.is_empty());
            }
            assert!(result.output().is_some());
            assert_eq!(execution(result.observations().clone()), before);
            assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
        }
    }
    #[test]
    fn caller_authored_logged_success_cannot_gain_any_log_save_authority() {
        let study = Study::new();
        study.fixture(false, false, true);
        let actual = study
            .graph(Language::Python)
            .unwrap()
            .into_build()
            .build_reported(&mut NoActivation, id(), Default::default());
        assert!(actual.accepted());
        let authored = NativeAttempt {
            provenance: Arc::clone(&actual.provenance),
            boundary: Ok(()),
            graph: actual.graph,
            resources: actual.resources,
        };
        let mut result = prepare_classified(&authored, id(), 16_777_216, &[]).unwrap();
        assert!(result.output().is_some());
        assert!(result.verification_log().is_none());
        assert!(result.save().is_err());
        assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
    }
    #[test]
    fn actual_root_ipc_shape_refusal_preserves_v_only_and_original_no_v_error() {
        for logs in [false, true] {
            let study = Study::new();
            study.fixture(false, false, logs);
            let columns = (0..65)
                .map(|index| {
                    format!("  - {{name: C{index}, type: int, label: Column, derivation: A.ID}}\n")
                })
                .collect::<String>();
            let names = (0..65)
                .map(|index| format!("C{index}"))
                .collect::<Vec<_>>()
                .join(", ");
            let sidecars = if logs {
                ", verification_log: generated/root-checks.csv, warning_log: generated/root-warnings.csv"
            } else {
                ""
            };
            study.write("root.yaml",format!("schema_version: '1.0'\ndomain: TEST\nkeys: [C0]\nbase: A\ninput: {{A: {{path: generated/producer.csv, schema: producer.yaml}}}}\ncolumns:\n{columns}output: {{path: generated/root.csv, columns: [{names}]{sidecars}}}\n"));
            // Production compilation caps columns at 64. Enlarge only this
            // fixture's compiler admission to reach the real unchanged IPC
            // 64-column boundary without fabricating a transport error.
            let mut metadata = crate::producer_graph::Limits::default();
            metadata.graph.metadata.compilation.columns = 128;
            metadata.graph.metadata.compilation.projected_columns = 128;
            let owned = FileGraph::prepare(
                Resources::new(&study.path(), &study.path(), &[]).unwrap(),
                "root.yaml",
                environment(Language::Python),
                shipped_schema::capture().unwrap(),
                metadata,
            )
            .unwrap_or_else(|error| panic!("shape fixture: {:?}", error.error()))
            .into_build()
            .build_owned_reported(&mut NoActivation, id(), Default::default());
            assert!(owned.accepted());
            let prepared = prepare_owned_classified(&owned, id(), 16_777_216, &[]);
            if !logs {
                assert!(matches!(
                    prepared,
                    Err(Error::Output {
                        error: TableTransportError::ShapeLimit,
                        ..
                    })
                ));
                continue;
            }
            let mut result = prepared.unwrap();
            assert_eq!(
                result._output_failure,
                Some(TableTransportError::ShapeLimit)
            );
            assert!(result.output().is_none());
            assert!(result.warning_log().is_none());
            assert!(result.verification_log().is_some());
            assert_eq!(result.observations()["error"]["code"], "output_projection");
            study.write("generated/root.csv", b"held sentinel");
            study.remove_inputs();
            for _ in 0..2 {
                assert!(result.save().is_err());
                assert_eq!(
                    result.observations()["artifacts"].as_array().unwrap().len(),
                    2
                );
                assert_eq!(study.generated("root.csv"), b"held sentinel");
                assert!(!study.0.join("generated/producer.csv").exists());
            }
        }
    }
}
