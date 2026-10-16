//! Public root results and explicit ordered publication from a sealed native owner.
//! Successful node reports still gate artifact consumption inside the build.
use crate::{
    ipc_table::{self, TableError},
    issue_rows::Issue,
    producer_report::{self, NativeAttempt, ProjectionError},
    project_report::Classified,
    public_table::PublicTable,
    report_storage::StoredReport,
    specification_report::{Error as ReportError, Identity},
};
use std::sync::Arc;
use yamaa_core::json::Value;
use yamaa_core::table::TableAccess;

pub struct RootResult {
    observations: StoredReport,
    issues: Vec<Issue>,
    output: Option<(PublicTable, Vec<u8>)>,
    root: usize,
    logs: Arc<[crate::producer_report::NodeLogs]>,
    log_publication: Option<Arc<crate::producer_log_publication::Publication>>,
    log_save_attempt: Option<crate::producer_log_publication::Attempt>,
    _output_failure: Option<TableError>,
    publication: Option<Arc<crate::producer_publication::Publication>>,
    publication_refusal: Option<&'static str>,
    base_issues: usize,
    latest: Option<StoredReport>,
    save_attempt: Option<crate::producer_publication::Attempt>,
}
impl RootResult {
    pub fn observations(&self) -> Value {
        self.latest
            .as_ref()
            .unwrap_or(&self.observations)
            .observations_value(false)
    }
    pub fn observations_json(&self) -> String {
        self.latest
            .as_ref()
            .unwrap_or(&self.observations)
            .observations_json(false)
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
        let outcome = self.save_actual();
        self.issues = crate::issue_rows::collect(std::mem::take(&mut self.issues));
        outcome
    }
    fn save_actual(&mut self) -> Result<bool, crate::file_application::FailedBuild> {
        if self.log_publication.is_some() {
            return self.save_logs_with(|publisher, path, bytes| publisher.publish(path, bytes));
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

        let admitted = (|| {
            let mut value = self.observations.clone();
            value["publication"] = yamaa_core::json::json!({"status":"failure",
                "entered":publication.artifacts().iter().map(|item|yamaa_core::json::json!({"node":item.node(),"published":false})).collect::<Vec<_>>(),
                "error":null});
            let locations = publication
                .artifacts()
                .iter()
                .map(|item| crate::report_publication::Location {
                    key: item.node(),
                    node: item.node(),
                    field: None,
                    path: None,
                })
                .collect::<Vec<_>>();
            let fields = crate::report_publication::Prepared::new(&mut value, &locations, false)?;
            Ok::<_, ReportError>((value, fields))
        })();
        let (mut value, mut fields) = match admitted {
            Ok(report) => report,
            Err(error) => {
                self.issues.extend(match error {
                    ReportError::ResourceLimit(refusal) => {
                        refusal.issues("output", "publication_report_limit")
                    }
                    _ => crate::file_application::rejected("output", "publication_report_limit"),
                });
                return Ok(false);
            }
        };
        let mut attempt = crate::producer_publication::Attempt::default();
        publication.save_into(&mut attempt);
        use yamaa_core::producer_publication::Failure;
        let (saved, code, node) = if let Some(failure) = publication.admission_failure() {
            use crate::producer_publication::AdmissionFailure;
            match failure {
                AdmissionFailure::Incomplete => (false, "publication_incomplete", None),
                AdmissionFailure::Rejected(code) => (false, *code, None),
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
        let entries = value["publication"]["entered"]
            .as_array_mut()
            .expect("admitted publication entries");
        entries.truncate(attempt.entered.entered.len());
        for (entry, actual) in entries.iter_mut().zip(&attempt.entered.entered) {
            entry["published"] = Value::Bool(actual.published);
        }
        fields.finish(&mut value, saved, code, node);
        self.latest = Some(value);
        self.save_attempt = Some(attempt);
        Ok(saved)
    }
    fn save_logs_with(
        &mut self,
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

        let report = (|| {
            // Own the complete report before any write. Every actual prefix is
            // formed by dropping already-owned artifact/entry suffixes; it never
            // reprojects observations or clones their tables and CSV content.
            let mut value = publication.prefix_report(&self.observations, failed, count, count)?;
            let locations = publication
                .requests(failed)
                .map(|(key, request)| crate::report_publication::Location {
                    key,
                    node: request.node,
                    field: Some(request.field),
                    path: Some(&request.path),
                })
                .collect::<Vec<_>>();
            let fields = crate::report_publication::Prepared::new(&mut value, &locations, true)?;
            Ok::<_, ReportError>((value, fields))
        })();
        let (mut value, mut fields) = match report {
            Ok(report) => report,
            Err(error) => {
                self.issues.extend(match error {
                    ReportError::ResourceLimit(refusal) => {
                        refusal.issues("output", "publication_report_limit")
                    }
                    _ => crate::file_application::rejected("output", "publication_report_limit"),
                });
                return if failed {
                    Err(crate::file_application::FailedBuild)
                } else {
                    Ok(false)
                };
            }
        };
        let mut attempt = crate::producer_log_publication::Attempt::default();
        publication.save_into(failed, &mut attempt, publish);
        let (saved, code, request) = if let Some(failure) = publication.admission_failure(failed) {
            use crate::producer_publication::AdmissionFailure;
            match &failure.failure {
                AdmissionFailure::Incomplete => (false, "publication_incomplete", failure.request),
                AdmissionFailure::Rejected(code) => (false, *code, failure.request),
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
        fields.finish(&mut value, saved, code, request);
        if !saved {
            let actual = attempt
                .entered
                .last()
                .filter(|entry| Some(entry.request) == request);
            let request = request.map(|index| publication.request(index));
            if let Some(request) = request {
                for issue in &mut self.issues[self.base_issues..] {
                    issue.spec_paths = vec![format!(
                        "output.{}",
                        actual.map_or(request.field, |entry| entry.field)
                    )];
                    let mut context: Value =
                        serde_json::from_str(&issue.context).expect("native issue context");
                    context["node"] =
                        yamaa_core::json::json!(actual.map_or(request.node, |entry| entry.node));
                    context["path"] = yamaa_core::json::json!(
                        actual.map_or(request.path.as_str(), |entry| entry.path.as_str())
                    );
                    context["entered"] = yamaa_core::json::json!(entered);
                    context["completed"] = yamaa_core::json::json!(completed);
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
        observations: StoredReport,
        error: TableError,
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
    facts: &[Classified<'a, E>],
) -> Result<RootResult, Error<'a, E>> {
    let observations =
        producer_report::attempt_report_classified(attempt, id, facts).map_err(Error::Report)?;
    prepare_observations(attempt, observations, true)
}
fn prepare_observations<'a, E>(
    attempt: &'a NativeAttempt<E>,
    observations: StoredReport,
    output_allowed: bool,
) -> Result<RootResult, Error<'a, E>> {
    let diagnostics =
        observations["diagnostics"]
            .as_array()
            .ok_or(Error::Report(ProjectionError::Report(
                ReportError::InvalidObservation,
            )))?;
    let mut issues = diagnostics
        .iter()
        .cloned()
        .map(Issue::from_diagnostic)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Report(ProjectionError::Report(ReportError::InvalidObservation)))?;
    if (!attempt.accepted() || !output_allowed) && issues.is_empty() {
        issues = crate::file_application::rejected("build", "producer_build");
    }
    let output = if attempt.accepted() && output_allowed {
        match root_output(attempt) {
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
    let issues = crate::issue_rows::collect(issues);
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
    facts: &[Classified<'a, E>],
) -> Result<RootResult, Error<'a, E>> {
    let has_verification = attempt
        .logs()
        .iter()
        .any(|logs| logs.logs.verification.is_some());
    let mut result = match prepare_classified(attempt, id, facts) {
        Ok(result) => result,
        Err(Error::Output {
            mut observations,
            error,
            ..
        }) if has_verification => {
            observations["outcome"] = Value::String("failure".into());
            observations["error"] =
                yamaa_core::json::json!({"stage":"output","code":"output_projection"});
            let finding = crate::specification_diagnostics::portable_diagnostic(
                yamaa_core::application_issue::rejected("output", "output_projection"),
            );
            observations["diagnostics"]
                .as_array_mut()
                .ok_or(Error::Report(ProjectionError::Report(
                    ReportError::InvalidObservation,
                )))?
                .push(finding);
            let mut result = prepare_observations(attempt, observations, false)?;
            result._output_failure = Some(error);
            result
        }
        Err(Error::Report(ProjectionError::Original { observations, .. })) if has_verification => {
            prepare_observations(attempt, observations, false)?
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

fn root_output<E>(attempt: &NativeAttempt<E>) -> Result<(PublicTable, Vec<u8>), TableError> {
    let invalid = TableError::InvalidTable;
    let metadata = attempt.provenance.metadata();
    // Root identity and compiler are borrowed from the sealed complete owner.

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

    if projection.len() != view.compiled().projection().len()
        || projection
            .iter()
            .zip(view.compiled().projection())
            .any(|(&index, name)| columns.get(index).is_none_or(|column| &column.name != name))
    {
        return Err(invalid);
    }
    let bytes = ipc_table::encode_projected_dataset(table, projection)?;
    let table = PublicTable::decode(&bytes)?;
    let bytes = table.ipc()?;
    Ok((table, bytes))
}

#[cfg(all(test, feature = "yaml"))]
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
        function_invocation::{Argument, HostError},
        project_activation::ActivationPort,
    };
    use yamaa_core::{
        function_signature::{LogicalSignature, ProjectFunctionIdentity},
        project_environment::LockReference,
        project_function::Language,
        value::Value as Scalar,
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
            )
        }
        fn owned(&self, language: Language) -> OwnedAttempt<Infallible> {
            self.graph(language)
                .unwrap_or_else(|error| panic!("graph fixture: {:?}", error.error()))
                .into_build()
                .build_owned_reported(&mut NoActivation, id(), scope())
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
    fn scope() -> yamaa_core::application_observer::Scope<'static> {
        yamaa_core::application_observer::Scope::new(&yamaa_core::application_observer::Noop)
    }
    fn root(owned: &OwnedAttempt<Infallible>) -> RootResult {
        prepare_owned_classified(owned, id(), &[]).unwrap()
    }
    fn execution(mut report: Value) -> Value {
        report.as_object_mut().unwrap().remove("publication");
        report.as_object_mut().unwrap().remove("artifacts");
        report
    }
    #[test]
    #[cfg(unix)]
    fn actual_sparse_source_boundary_keeps_facts_across_plain_project_and_graph_results() {
        for route in ["plain", "project", "producer"] {
            let study = Study::new();
            study.fixture(false, false, false);
            let source = fs::OpenOptions::new()
                .write(true)
                .open(study.0.join("input.csv"))
                .unwrap();
            source.set_len(10 * 1024 * 1024 * 1024 + 1).unwrap();
            drop(source);
            let resources = || Resources::new(&study.path(), &study.path(), &[]).unwrap();
            let issues = match route {
                "plain" => {
                    let prepared = crate::file_preparation::FileSpecification::prepare(
                        resources(),
                        "producer.yaml",
                    )
                    .unwrap();
                    let result = crate::file_application::plain(prepared, id()).unwrap();
                    assert!(result.output().is_none());
                    result.issues()
                }
                "project" => {
                    study.write("environment.yaml", "schema_version: '1.0'\n");
                    let mut prepared = crate::file_project::FileProject::prepare(
                        resources(),
                        "producer.yaml",
                        "environment.yaml",
                        Language::Python,
                        shipped_schema::capture().unwrap(),
                        shipped_schema::capture_environment().unwrap(),
                    )
                    .unwrap();
                    let attempt = prepared.build(&mut NoActivation);
                    let result =
                        crate::project_report::build_result(prepared.run(), &attempt, id(), &[])
                            .unwrap();
                    assert!(result.output().is_none());
                    result.issues().to_vec()
                }
                _ => {
                    let owned = study.owned(Language::Python);
                    assert!(!owned.accepted());
                    let result = root(&owned);
                    assert!(result.output().is_none());
                    result.issues().to_vec()
                }
            };
            assert_eq!(issues.len(), 1, "{route}");
            assert_eq!(
                issues[0].condition,
                yamaa_core::diagnostic::ConditionCode::ApplicationRejected
                    .definition()
                    .condition
            );
            assert_eq!(issues[0].requirement, None);
            assert_eq!(issues[0].spec_paths, ["input.SRC.path"]);
            let context: Value = serde_json::from_str(&issues[0].context).unwrap();
            assert_eq!(
                context,
                yamaa_core::json::json!({"stage":"capture", "code":"resource_limit", "resource":"source_bytes",
                "limit":10_737_418_240_u64,"actual":10_737_418_241_u64,"dataset":"SRC","path":"input.csv"}),
                "{route}"
            );
            assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
        }
    }
    #[test]
    fn actual_alias_logs_publish_in_node_order_and_repeat_only_held_bytes() {
        for language in [Language::Python, Language::R] {
            let study = Study::new();
            study.fixture(false, false, true);
            let owned = study.owned(language);
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
        let owned = study.owned(Language::Python);
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
            let owned = study.owned(Language::Python);
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
            let owned = study.owned(Language::Python);
            let mut result = root(&owned);
            let before = execution(result.observations().clone());
            study.write("generated/root.csv", b"earlier root");
            study.remove_inputs();
            let payload = Arc::new(());
            let mut calls = 0;
            assert!(!result
                .save_logs_with(|publisher, path, bytes| {
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
                .filter(|issue| {
                    issue.requirement.as_deref()
                        == Some(
                            crate::test_diagnostic_truth::text("publication", "requirement")
                                .as_str(),
                        )
                })
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
                let owned = study.owned(Language::Python);
                let mut result = root(&owned);
                let before = execution(result.observations().clone());
                study.remove_inputs();
                let mut calls = 0;
                assert!(!result
                    .save_logs_with(|publisher, path, bytes| {
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
                        yamaa_core::json::json!({
                            "node":node, "field":field, "path":format!("generated/{name}"),
                            "published":position + 1 < boundary
                        })
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    report["publication"]["entered"],
                    yamaa_core::json::json!(expected)
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
    fn whole_set_parent_preflight_enters_no_publishers() {
        let study = Study::new();
        study.fixture(false, false, true);
        let owned = study.owned(Language::Python);
        let mut result = root(&owned);
        fs::remove_dir(study.0.join("generated")).unwrap();
        assert!(!result
            .save_logs_with(|_, _, _| panic!("all parents first"))
            .unwrap());
        assert!(result.log_save_attempt.as_ref().unwrap().entered.is_empty());
        assert!(!result
            .issues()
            .iter()
            .any(|issue| issue.requirement.as_deref()
                == Some(
                    crate::test_diagnostic_truth::text("publication", "requirement").as_str()
                )));
        fs::create_dir(study.0.join("generated")).unwrap();
        assert!(result.save().unwrap());
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
        let owned = study.owned(Language::Python);
        assert!(owned.accepted());
        let mut result = root(&owned);
        let before = execution(result.observations().clone());
        study.remove_inputs();
        for _ in 0..2 {
            assert!(!result
                .save_logs_with(|_, _, _| panic!("all target admission precedes publication"))
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
                .any(|issue| issue.requirement.as_deref()
                    == Some(
                        crate::test_diagnostic_truth::text("publication", "requirement").as_str()
                    )));
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
            let owned = study.owned(Language::Python);
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
                    .save_logs_with(|_, _, _| panic!("whole physical target set first"))
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
            .build_reported(&mut NoActivation, id(), scope());
        assert!(actual.accepted());
        let authored = NativeAttempt {
            provenance: Arc::clone(&actual.provenance),
            boundary: Ok(()),
            graph: actual.graph,
            resources: actual.resources,
        };
        let mut result = prepare_classified(&authored, id(), &[]).unwrap();
        assert!(result.output().is_some());
        assert!(result.verification_log().is_none());
        assert!(result.save().is_err());
        assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
    }
    #[test]
    fn actual_root_at_the_fixed_column_boundary_retains_logs_and_held_output() {
        for logs in [false, true] {
            let study = Study::new();
            study.fixture(false, false, logs);
            let columns = (0..1_024)
                .map(|index| {
                    format!("  - {{name: C{index}, type: int, label: Column, derivation: A.ID}}\n")
                })
                .collect::<String>();
            let names = (0..1_024)
                .map(|index| format!("C{index}"))
                .collect::<Vec<_>>();
            let sidecars = if logs {
                ", verification_log: generated/root-checks.csv, warning_log: generated/root-warnings.csv"
            } else {
                ""
            };
            study.write("root.yaml", format!("schema_version: '1.0'\ndomain: TEST\nkeys: [C0]\nbase: A\ninput: {{A: {{path: generated/producer.csv, schema: producer.yaml}}}}\ncolumns:\n{columns}output: {{path: generated/root.csv, columns: [{}]{sidecars}}}\n", names.join(", ")));
            let owned = study.owned(Language::Python);
            assert!(owned.accepted());
            let artifact = owned.graph.nodes[1]
                .output
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .artifact()
                .unwrap();
            let expected = format!(
                "{}\n{}\n{}\n",
                names.join(","),
                vec!["1"; 1_024].join(","),
                vec!["2"; 1_024].join(",")
            );
            assert_eq!(artifact.bytes(), expected.as_bytes());
            let mut result = root(&owned);
            assert!(result.output().is_some());
            assert_eq!(result.output_table().unwrap().columns().len(), 1_024);
            assert_eq!(result.verification_log().is_some(), logs);
            assert_eq!(result.warning_log().is_some(), logs);
            study.remove_inputs();
            for _ in 0..2 {
                assert!(result.save().unwrap());
                assert_eq!(study.generated("root.csv"), expected.as_bytes());
            }
        }
    }

    fn projection_fixture(study: &Study, rows: usize, extra_columns: usize, payload: &str) {
        let id_type = if extra_columns == 0 { "int" } else { "str" };
        let csv = format!(
            "ID,VALUE\n{}",
            (1..=rows)
                .map(|id| format!("{id},1.234\n"))
                .collect::<String>()
        );
        study.write("input.csv", csv);
        study.write("producer.yaml", format!(
            "schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\ninput: {{SRC: {{path: input.csv, types: {{ID: {id_type}, VALUE: float}}}}}}\ncolumns:\n  - {{name: ID, type: {id_type}, label: Identifier, derivation: SRC.ID}}\n  - {{name: VALUE, type: float, label: Value, derivation: SRC.VALUE}}\noutput: {{path: generated/producer.csv, columns: [ID, VALUE], decimals: 2}}\n"
        ));
        let (extra, projection) = if extra_columns == 0 {
            (
                format!("  - {{name: PAYLOAD, type: str, label: Payload, derivation: {{literal: '{payload}'}}}}\n"),
                "ID, VALUE, PAYLOAD".to_owned(),
            )
        } else {
            let names = (0..extra_columns)
                .map(|index| format!("EXTRA{index}"))
                .collect::<Vec<_>>();
            let columns = names.iter().map(|name| format!(
                "  - {{name: {name}, type: str, label: Extra value, derivation: {{literal: null}}}}\n"
            )).collect::<String>();
            (columns, format!("ID, VALUE, {}", names.join(", ")))
        };
        study.write("root.yaml", format!(
            "schema_version: '1.0'\ndomain: TEST\nbase: FIRST\nkeys: [ID]\ninput:\n  FIRST: {{path: generated/producer.csv, schema: producer.yaml}}\n  SECOND: {{path: generated/producer.csv, schema: ./producer.yaml}}\nintermediates: [{{id: SECOND_REFERENCE, dataset: SECOND, key: [ID], no_match: null}}]\ncolumns:\n  - {{name: ID, type: {id_type}, label: Identifier, derivation: FIRST.ID}}\n  - {{name: FIRST_VALUE, type: float, label: First value, derivation: FIRST.VALUE}}\n  - {{name: SECOND_VALUE, type: float, label: Second value, derivation: SECOND_REFERENCE.VALUE}}\n  - {{name: VALUE, type: float, label: Value, derivation: {{compute: {{expr: 'FIRST_VALUE + SECOND_VALUE'}}}}}}\n{extra}output: {{path: generated/root.csv, columns: [{projection}], decimals: 2}}\n"
        ));
    }

    fn selected_projection(rows: usize, extra_columns: usize, payload: &str) {
        let study = Study::new();
        projection_fixture(&study, rows, extra_columns, payload);
        let owned = study.owned(Language::Python);
        assert!(owned.accepted());
        assert_eq!(
            owned
                .graph
                .nodes
                .iter()
                .map(|node| node.node)
                .collect::<Vec<_>>(),
            [1, 0]
        );
        let output = owned.graph.nodes[1]
            .output
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap();
        let artifact = output.artifact().unwrap();
        let expected = if extra_columns == 0 {
            format!(
                "ID,VALUE,PAYLOAD\n{}",
                (1..=rows)
                    .map(|id| { format!("{id},2.46,{payload}\n") })
                    .collect::<String>()
            )
        } else {
            let names = (0..extra_columns)
                .map(|index| format!("EXTRA{index}"))
                .collect::<Vec<_>>();
            format!(
                "ID,VALUE,{}\n{}",
                names.join(","),
                (1..=rows)
                    .map(|id| { format!("{id},2.46{}\n", ",".repeat(extra_columns)) })
                    .collect::<String>()
            )
        };
        assert_eq!(artifact.bytes(), expected.as_bytes());
        assert_eq!(
            owned.graph.nodes[1]
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap()
                .dataset
                .schema()
                .columns()
                .len(),
            4 + if extra_columns == 0 { 1 } else { extra_columns }
        );
        study.remove_inputs();
        for _ in 0..2 {
            let observations = producer_report::attempt_report(&owned, id()).unwrap();
            assert_eq!(observations["outcome"], "success");
            assert_eq!(observations["artifacts"].as_array().unwrap().len(), 2);
            assert!(observations["error"].is_null());
            let result = prepare_observations(&owned, observations, true).unwrap();
            let table = result.output_table().unwrap();
            assert_eq!(table.rows(), rows);
            assert_eq!(
                table.columns().len(),
                2 + if extra_columns == 0 { 1 } else { extra_columns }
            );
            assert_eq!(result.observations()["outcome"], "success");
            assert_eq!(artifact.bytes(), expected.as_bytes());
            assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
        }
    }

    #[test]
    fn large_selected_projection_keeps_original_cells_and_artifacts() {
        selected_projection(23_000, 59, "");
    }
    #[test]
    fn long_selected_projection_keeps_complete_text_and_artifacts() {
        selected_projection(4_095, 0, &"x".repeat(2_048));
    }
}
