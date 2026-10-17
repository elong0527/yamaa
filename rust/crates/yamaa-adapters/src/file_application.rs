//! One native lifecycle behind both hosts' file entry points.
use crate::report_storage::StoredReport;
use crate::{
    file_preparation::{self, FileSpecification},
    issue_rows::Issue,
    specification_report::{self, BuildResult, Identity},
};
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_core::application_observer::Scope;
use yamaa_core::domain_entry::{self, Request};
use yamaa_core::json::Value;
use yamaa_core::project_function::Language;

pub type Failure = Vec<Issue>;

pub fn unsupported(operation: &str, path: Option<&str>) -> Failure {
    vec![
        Issue::from_core(yamaa_core::application_issue::unsupported(operation, path))
            .expect("application issue"),
    ]
}
pub fn rejected(stage: &str, code: &str) -> Failure {
    vec![
        Issue::from_core(yamaa_core::application_issue::rejected(stage, code))
            .expect("application issue"),
    ]
}
pub(crate) fn resource_rejected(
    stage: &str,
    code: &str,
    resource: &str,
    limit: usize,
    actual: Option<usize>,
    paths: Vec<String>,
) -> Failure {
    vec![
        Issue::from_core(yamaa_core::application_issue::resource_rejected(
            stage, code, resource, limit, actual, paths,
        ))
        .expect("application resource issue"),
    ]
}
/// A build that produced no public result: its closed refusal code and, for a
/// classified resource limit, the limit facts. Both hosts convert this value;
/// neither chooses the code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildRefusal {
    pub code: &'static str,
    pub limit: Option<crate::resource_diagnostics::Refusal>,
}
impl BuildRefusal {
    pub fn issues(&self) -> Failure {
        self.limit.as_ref().map_or_else(
            || rejected("build", self.code),
            |limit| limit.issues("build", self.code),
        )
    }
}
fn report_failure(stage: &str, code: &str, error: &specification_report::Error) -> Failure {
    match error {
        specification_report::Error::ResourceLimit(refusal) => refusal.issues(stage, code),
        _ => rejected(stage, code),
    }
}
pub(crate) fn classified_resource(diagnostic: &yamaa_core::diagnostic::Diagnostic) -> Failure {
    vec![Issue::from_core(diagnostic.clone()).expect("classified resource finding")]
}
pub(crate) fn resource_failure(stage: &str, error: &crate::file_resources::Error) -> Failure {
    with_source(resource_diagnostic(stage, error), None)
}
/// A failure observed on an already captured file also names that file.
pub(crate) fn resource_failure_from(
    stage: &str,
    error: &crate::file_resources::Error,
    source: &str,
) -> Failure {
    with_source(resource_diagnostic(stage, error), Some(source))
}
/// A typed limit keeps its resource facts and, when known, its owning source.
pub(crate) fn resource_limit_from(
    stage: &str,
    limit: &yamaa_core::limits::LimitExceeded,
    source: Option<&str>,
) -> Failure {
    with_source(limit_diagnostic(stage, limit), source)
}
/// An internal graph invariant names its fixed reason and, when known, the
/// source document; it never claims a language or resource cause.
pub(crate) fn graph_boundary(stage: &str, reason: &str, source: Option<&str>) -> Failure {
    let mut diagnostic = yamaa_core::application_issue::rejected(stage, "graph_boundary");
    diagnostic
        .context
        .insert("reason".into(), Value::String(reason.into()));
    with_source(diagnostic, source)
}
fn with_source(
    mut diagnostic: yamaa_core::diagnostic::Diagnostic,
    source: Option<&str>,
) -> Failure {
    if let Some(source) = source {
        diagnostic
            .context
            .insert("source".into(), Value::String(source.into()));
    }
    vec![Issue::from_core(diagnostic).expect("application issue")]
}
fn limit_diagnostic(
    stage: &str,
    limit: &yamaa_core::limits::LimitExceeded,
) -> yamaa_core::diagnostic::Diagnostic {
    yamaa_core::application_issue::resource_rejected(
        stage,
        "resource_limit",
        limit.resource,
        limit.limit,
        limit.actual,
        Vec::new(),
    )
}
fn resource_diagnostic(
    stage: &str,
    error: &crate::file_resources::Error,
) -> yamaa_core::diagnostic::Diagnostic {
    use crate::file_resources::Error as E;
    let code = match error {
        E::Host(_) => "host_resource_failure",
        E::Missing => "resource_missing",
        E::Unreadable => "resource_unreadable",
        E::NotRegularFile => "resource_not_regular",
        E::InvalidRoot => "resource_root",
        E::InvalidBase => "resource_base",
        E::InvalidPath => "resource_path",
        E::WrittenForm(_) => "resource_path",
        E::OutsideRoots => "resource_outside_roots",
        E::Symlink => "resource_link",
        E::Changed => "resource_changed",
        E::InvalidLock => "invalid_packaging_lock",
        E::Limit(limit) => return limit_diagnostic(stage, limit),
    };
    yamaa_core::application_issue::rejected(stage, code)
}
pub(crate) fn publication_failure(error: &crate::file_publication::Error) -> Failure {
    use crate::file_publication::Error as E;
    use yamaa_core::json::Value as JsonValue;

    let code = match error {
        E::Host(crate::host_resources::Failure::Cleanup) => "cleanup_incomplete",
        E::Host(crate::host_resources::Failure::Cancelled) => "publication_cancelled",
        E::Host(_) => "host_publication_failure",
        E::Cleanup { .. } => "cleanup_incomplete",
        E::Changed => "target_changed",
        E::Limit(error) => {
            return resource_rejected(
                "output",
                "publication_limit",
                error.resource,
                error.limit,
                error.actual,
                Vec::new(),
            )
        }
        E::StagingCollision => "staging_collision",
        E::InvalidTarget => "invalid_target",
        E::PathMismatch => "path_mismatch",
        E::Io(_) => {
            yamaa_core::diagnostic::ConditionCode::OutputPublicationFailed
                .definition()
                .condition
        }
    };
    let mut finding = yamaa_core::application_issue::rejected("output", code);
    if let E::Cleanup {
        operation,
        staging,
        cleanup,
    } = error
    {
        for (name, value) in [
            ("operation", operation.message()),
            ("staging", staging.to_string_lossy().into_owned()),
            ("cleanup", cleanup.to_string()),
        ] {
            finding
                .context
                .insert(name.into(), JsonValue::String(value));
        }
    }
    vec![Issue::from_core(finding).expect("application issue")]
}
pub(crate) fn preparation_failure(message: String) -> Failure {
    let Ok(value) = crate::json_wire::decode(&message) else {
        return rejected("prepare", "resource_boundary");
    };
    let outcome = &value["outcome"];
    match outcome["status"].as_str() {
        Some("invalid") => {
            let Some(rows) = outcome["diagnostics"].as_array() else {
                return rejected("prepare", "issue_projection");
            };
            let Ok(issues) = rows
                .iter()
                .cloned()
                .map(Issue::from_diagnostic)
                .collect::<Result<Vec<_>, _>>()
            else {
                return rejected("prepare", "issue_projection");
            };

            issues
        }
        Some("unsupported") => {
            let Some(features) = outcome["features"].as_array() else {
                return rejected("prepare", "issue_projection");
            };
            let mut issues = Vec::new();
            for feature in features {
                let (Some(operation), Some(path)) =
                    (feature["operation"].as_str(), feature["spec_path"].as_str())
                else {
                    return rejected("prepare", "issue_projection");
                };
                issues.extend(unsupported(operation, Some(path)));
            }
            issues
        }
        Some("rejected" | "resource_limit") => {
            let stage = outcome["stage"].as_str().unwrap_or("prepare");
            let code = if outcome["status"] == "resource_limit" {
                "resource_limit"
            } else {
                outcome["code"].as_str().unwrap_or("preparation_boundary")
            };
            let (Some(resource), Some(limit)) =
                (outcome["resource"].as_str(), outcome["limit"].as_u64())
            else {
                return rejected(stage, code);
            };
            let Ok(limit) = usize::try_from(limit) else {
                return rejected("prepare", "issue_projection");
            };
            let actual = outcome["actual"]
                .as_u64()
                .and_then(|v| usize::try_from(v).ok());
            let paths = outcome["spec_paths"]
                .as_array()
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| path.as_str().map(String::from))
                        .collect::<Option<Vec<_>>>()
                })
                .unwrap_or_else(|| Some(Vec::new()));
            let Some(paths) = paths else {
                return rejected("prepare", "issue_projection");
            };
            let mut issues = resource_rejected(stage, code, resource, limit, actual, paths);
            if let Some(phase) = outcome["phase"].as_str() {
                for issue in &mut issues {
                    issue.context["phase"] = crate::json!(phase);
                }
            }

            issues
        }
        _ => rejected("prepare", "preparation_boundary"),
    }
}

pub struct Domain {
    result: BuildResult,
    targets: Option<Vec<crate::resource_publication::AnchoredTarget>>,
    publication_issues: Failure,
    published: Option<StoredReport>,
    publication_attempt: PublicationAttempt,
}
#[derive(Default)]
struct PublicationAttempt {
    entered: usize,
    completed: usize,
    failure: Option<crate::file_publication::Error>,
    unwind: Option<Box<dyn Any + Send>>,
}
fn publication_at(
    mut issues: Failure,
    field: &str,
    path: &str,
    attempt: &PublicationAttempt,
) -> Failure {
    for issue in &mut issues {
        issue.spec_paths = vec![format!("output.{field}")];
        issue.context["path"] = crate::json!(path);
        issue.context["entered"] = crate::json!(attempt.entered);
        issue.context["completed"] = crate::json!(attempt.completed);
    }
    issues
}
impl Domain {
    /// Use the same owned output and anchored save gate for project reports.
    pub fn from_project(
        result: BuildResult,
        project: &crate::file_project::FileProject,
    ) -> Result<Self, Failure> {
        Self::from_build(result, |field| {
            if field == "path" {
                project.publication_target()
            } else {
                project.publication_field_target(field)
            }
        })
    }
    fn from_build(
        result: BuildResult,
        mut target: impl FnMut(
            &str,
        ) -> Result<
            crate::resource_publication::AnchoredTarget,
            crate::file_resources::Error,
        >,
    ) -> Result<Self, Failure> {
        let fields = [
            ("verification_log", result.verification_log().is_some()),
            (
                "warning_log",
                result.warning_log().is_some() && result.output().is_some(),
            ),
            ("path", result.output().is_some()),
        ];
        let targets = fields
            .into_iter()
            .filter(|(_, present)| *present)
            .map(|(field, _)| target(field))
            .collect::<Result<Vec<_>, _>>();
        let (targets, publication_issues) = match targets {
            Ok(targets) => (Some(targets), Vec::new()),
            Err(error) => (None, resource_failure("output", &error)),
        };
        Ok(Self {
            result,
            targets,
            publication_issues,
            published: None,
            publication_attempt: Default::default(),
        })
    }
    pub fn output(&self) -> Option<&[u8]> {
        self.result.output()
    }
    pub fn output_table(&self) -> Option<&crate::public_table::PublicTable> {
        self.result.output_table()
    }
    pub fn verification_log(&self) -> Option<&[u8]> {
        self.result.verification_log()
    }
    pub fn warning_log(&self) -> Option<&[u8]> {
        self.result.warning_log()
    }
    pub fn log_table(&self, warning: bool) -> Option<&crate::public_table::PublicTable> {
        self.result.log_table(warning)
    }
    pub fn issues(&self) -> Failure {
        crate::issue_rows::collect(
            self.result
                .issues()
                .iter()
                .cloned()
                .chain(self.publication_issues.iter().cloned()),
        )
    }
    /// Retain complete run observations without recapture. Artifacts describe
    /// only a successful latest save; a refused save returns build observations.
    pub fn observations(&self) -> Value {
        self.published.as_ref().map_or_else(
            || self.result.observations(),
            |report| report.observations_value(false),
        )
    }
    pub fn observations_json(&self) -> String {
        self.published.as_ref().map_or_else(
            || self.result.observations_json(),
            |report| report.observations_json(false),
        )
    }
    /// Preflight the complete target set before writing held log/primary bytes.
    /// Failed builds may publish their diagnostic log on explicit Save and still
    /// raise the original failed-build result. Build itself never writes.
    pub fn save(&mut self) -> Result<bool, FailedBuild> {
        self.save_with(|publisher, path, bytes| publisher.publish(path, bytes))
    }
    fn save_with(
        &mut self,
        mut publish: impl FnMut(
            &mut crate::resource_publication::Publisher,
            &str,
            &[u8],
        ) -> Result<(), crate::file_publication::Error>,
    ) -> Result<bool, FailedBuild> {
        let failed = self.result.output().is_none();
        if failed && self.result.verification_log().is_none() {
            return Err(FailedBuild);
        }
        self.published = None;
        self.publication_attempt = Default::default();
        let Some(targets) = self.targets.as_ref() else {
            return if failed { Err(FailedBuild) } else { Ok(false) };
        };
        let items = self.result.publication_items();
        // Own one complete report and all at-most-three artifact entries before
        // publication. Every prefix fits the admitted complete encoded report;
        // its artifact slots are reserved before the first publisher. Later
        // success only moves an already-owned entry into an admitted slot.
        let report = (|| {
            let mut report = self.result.publication_observations(items.len())?;
            let Value::Array(artifacts) = report["artifacts"].take() else {
                return Err(specification_report::Error::InvalidObservation);
            };
            if artifacts.len() != items.len() {
                return Err(specification_report::Error::InvalidObservation);
            }
            report["artifacts"] = Value::Array(Vec::with_capacity(artifacts.len()));
            Ok((report, artifacts.into_iter()))
        })();
        let (report, mut artifacts) = match report {
            Ok(report) => report,
            Err(error) => {
                self.publication_issues =
                    report_failure("output", "publication_report_limit", &error);
                return if failed { Err(FailedBuild) } else { Ok(false) };
            }
        };
        let fields = [
            ("verification_log", self.result.verification_log().is_some()),
            (
                "warning_log",
                self.result.warning_log().is_some() && !failed,
            ),
            ("path", !failed),
        ]
        .into_iter()
        .filter_map(|(field, present)| present.then_some(field))
        .collect::<Vec<_>>();
        let publishers = targets
            .iter()
            .zip(&items)
            .map(|(target, (path, _, _))| target.publisher(path))
            .collect::<Result<Vec<_>, _>>();
        self.publication_issues.clear();
        let mut publishers = match publishers {
            Ok(publishers) => publishers,
            Err(error) => {
                self.publication_issues = publication_failure(&error);
                self.publication_attempt.failure = Some(error);
                return if failed { Err(FailedBuild) } else { Ok(false) };
            }
        };
        // Physical alias checks are performed on these retained parent handles
        // before the first publication call (see Publisher::same_target).
        for index in 0..publishers.len() {
            for prior in &publishers[..index] {
                match publishers[index].same_target(prior) {
                    Ok(false) => (),
                    result => {
                        let error = result
                            .err()
                            .unwrap_or(crate::file_publication::Error::InvalidTarget);
                        self.publication_issues = publication_failure(&error);
                        self.publication_attempt.failure = Some(error);
                        return if failed { Err(FailedBuild) } else { Ok(false) };
                    }
                }
            }
        }
        self.published = Some(report);
        for (index, (publisher, (path, bytes, _))) in publishers.iter_mut().zip(items).enumerate() {
            self.publication_attempt.entered = index + 1;
            match catch_unwind(AssertUnwindSafe(|| publish(publisher, path, bytes))) {
                Ok(Ok(())) => {
                    self.publication_attempt.completed = index + 1;
                    self.published
                        .as_mut()
                        .expect("admitted report")
                        .get_mut("artifacts")
                        .and_then(Value::as_array_mut)
                        .expect("admitted artifact slots")
                        .push(artifacts.next().expect("admitted artifact entry"));
                }
                Ok(Err(error)) => {
                    self.publication_issues = publication_at(
                        publication_failure(&error),
                        fields[index],
                        path,
                        &self.publication_attempt,
                    );
                    self.publication_attempt.failure = Some(error);
                    return if failed { Err(FailedBuild) } else { Ok(false) };
                }
                Err(payload) => {
                    self.publication_attempt.unwind = Some(payload);
                    self.publication_issues = publication_at(
                        rejected("output", "publication_unwind"),
                        fields[index],
                        path,
                        &self.publication_attempt,
                    );
                    return if failed { Err(FailedBuild) } else { Ok(false) };
                }
            }
        }
        if failed {
            Err(FailedBuild)
        } else {
            Ok(true)
        }
    }
}
#[derive(Debug)]
pub struct FailedBuild;

struct Native<'a> {
    identity: Identity<'a>,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
}
/// The host provides activation and original-error conversion. All route
/// preparation, static checks and dispatch stay behind the shared application.
pub trait RuntimePort {
    type Result;
    type Error;
    fn resolver(&self) -> Option<std::sync::Arc<dyn crate::host_resources::Resolver>> {
        None
    }
    fn plain(
        &mut self,
        prepared: FileSpecification,
        scope: Scope<'_>,
    ) -> Result<Self::Result, Self::Error>;
    fn project(
        &mut self,
        prepared: Box<crate::file_project::FileProject>,
        scope: Scope<'_>,
    ) -> Result<Self::Result, Self::Error>;
    fn producer(
        &mut self,
        prepared: Box<crate::file_graph::FileGraph>,
        scope: Scope<'_>,
    ) -> Result<Self::Result, Self::Error>;
}
pub enum ApplicationError<E> {
    Preparation(crate::file_workflow::SelectionError),
    Runtime(E),
}
pub struct Application<R> {
    pub host: Language,
    pub runtime: R,
}
impl<R: RuntimePort> yamaa_core::build::Ports for Application<R> {
    type Plain = FileSpecification;
    type Project = Box<crate::file_project::FileProject>;
    type Producer = Box<crate::file_graph::FileGraph>;
    type CheckResult = Failure;
    type BuildResult = R::Result;
    type Failure = ApplicationError<R::Error>;
    fn prepare(
        &mut self,
        request: Request<'_>,
    ) -> Result<crate::file_workflow::Selected, Self::Failure> {
        match self.runtime.resolver() {
            Some(resolver) => {
                crate::file_workflow::select_with_resolver(request, self.host, resolver)
            }
            None => crate::file_workflow::select_request(request, self.host),
        }
        .map_err(ApplicationError::Preparation)
    }
    fn check_plain(&mut self, prepared: FileSpecification) -> Result<Failure, Self::Failure> {
        Ok(crate::specification_check::issue_rows(prepared.run())
            .unwrap_or_else(|_| rejected("check", "issue_projection")))
    }
    fn check_project(&mut self, prepared: Self::Project) -> Result<Failure, Self::Failure> {
        Ok(crate::project_check::prepared(prepared.run())
            .unwrap_or_else(|_| rejected("check", "issue_projection")))
    }
    fn check_producer(&mut self, prepared: Self::Producer) -> Result<Failure, Self::Failure> {
        Ok(crate::producer_check::prepared(&prepared)
            .unwrap_or_else(|_| rejected("check", "issue_projection")))
    }
    fn build_plain(
        &mut self,
        prepared: FileSpecification,
        scope: Scope<'_>,
    ) -> Result<R::Result, Self::Failure> {
        self.runtime
            .plain(prepared, scope)
            .map_err(ApplicationError::Runtime)
    }
    fn build_project(
        &mut self,
        prepared: Self::Project,
        scope: Scope<'_>,
    ) -> Result<R::Result, Self::Failure> {
        self.runtime
            .project(prepared, scope)
            .map_err(ApplicationError::Runtime)
    }
    fn build_producer(
        &mut self,
        prepared: Self::Producer,
        scope: Scope<'_>,
    ) -> Result<R::Result, Self::Failure> {
        self.runtime
            .producer(prepared, scope)
            .map_err(ApplicationError::Runtime)
    }
}
impl RuntimePort for Native<'_> {
    type Result = Domain;
    type Error = Failure;
    fn resolver(&self) -> Option<std::sync::Arc<dyn crate::host_resources::Resolver>> {
        self.resolver.clone()
    }
    fn plain(
        &mut self,
        mut prepared: FileSpecification,
        scope: Scope<'_>,
    ) -> Result<Domain, Failure> {
        let attempt = prepared.build(scope);
        if let Some(error) = file_preparation::opaque_resource_failure(&attempt) {
            return Err(resource_failure("capture", error));
        }
        scope.phase(yamaa_core::application_observer::Phase::Output);
        let converted = (|| {
            let result = specification_report::build_result(
                prepared.run(),
                &attempt,
                Identity {
                    runtime: self.identity.runtime,
                    runtime_version: self.identity.runtime_version,
                    engine_version: self.identity.engine_version,
                    example: self.identity.example,
                    specification: self.identity.specification,
                    base_directory: self.identity.base_directory,
                },
            )
            .map_err(|error| report_failure("build", "result_projection", &error))?;
            Domain::from_build(result, |field| {
                if field == "path" {
                    prepared.publication_target()
                } else {
                    prepared.publication_field_target(field)
                }
            })
        })();
        scope.exit(
            yamaa_core::application_observer::Phase::Output,
            if converted.is_ok() {
                yamaa_core::application_observer::Outcome::Succeeded
            } else {
                yamaa_core::application_observer::Outcome::Failed
            },
        );
        converted
    }
    fn project(
        &mut self,
        _: Box<crate::file_project::FileProject>,
        _: Scope<'_>,
    ) -> Result<Domain, Failure> {
        Err(unsupported("environment", Some("environment")))
    }
    fn producer(
        &mut self,
        _: Box<crate::file_graph::FileGraph>,
        _: Scope<'_>,
    ) -> Result<Domain, Failure> {
        Err(unsupported("producer_host", None))
    }
}

/// Convert an already checked plain capability without recapturing metadata.
pub fn plain(
    prepared: FileSpecification,
    identity: Identity<'_>,
    scope: Scope<'_>,
) -> Result<Domain, Failure> {
    Native {
        identity,
        resolver: None,
    }
    .plain(prepared, scope)
}
/// The host language is configured by the caller, never derived from the
/// runtime name: an embedder may name its runtime anything.
fn native(host: Language, identity: Identity<'_>) -> Application<Native<'_>> {
    Application {
        host,
        runtime: Native {
            identity,
            resolver: None,
        },
    }
}
pub fn check(host: Language, request: Request<'_>, identity: Identity<'_>) -> Failure {
    check_with_resolver(host, request, identity, None)
}
pub fn check_with_resolver(
    host: Language,
    request: Request<'_>,
    identity: Identity<'_>,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
) -> Failure {
    let mut application = native(host, identity);
    application.runtime.resolver = resolver;
    crate::issue_rows::collect(
        domain_entry::check(request, &mut application).unwrap_or_else(|error| match error {
            ApplicationError::Preparation(error) => {
                crate::project_application::selection_failure(&error, application.host)
            }
            ApplicationError::Runtime(issues) => issues,
        }),
    )
}
pub fn domain(
    host: Language,
    request: Request<'_>,
    identity: Identity<'_>,
) -> Result<Domain, Failure> {
    let mut application = native(host, identity);
    domain_entry::domain(request, &mut application, Scope::default())
        .map_err(|error| match error {
            ApplicationError::Preparation(error) => {
                crate::project_application::selection_failure(&error, application.host)
            }
            ApplicationError::Runtime(issues) => issues,
        })
        .map_err(crate::issue_rows::collect)
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;
    use crate::file_configuration;
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    #[test]
    fn primary_above_the_former_eight_megabyte_quota_retains_logs_and_saves_held_bytes() {
        let root = std::env::temp_dir().join(format!(
            "yamaa-output-log-cap-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("input")).unwrap();
        let study = Study(crate::canonical_path::canonicalize(root).unwrap());
        const VERIFICATION_HEADER: &str = "REPORT_VERSION,ARTIFACT,SPEC_PATH,VERIFICATION_ID,CHECK,TARGET,REQUIREMENT,SEVERITY,OUTCOME,CONDITION,EVALUATED_COUNT,FAILURE_COUNT,DETAILS\n";
        // The output preserves declared precision: each large
        // finite float needs 309 whole digits plus the declared 18 fractional digits.
        let mut input = String::from("ID,VALUE\n");
        for id in 1..=26_000 {
            input.push_str(&format!("{id},1e308\n"));
        }
        fs::write(study.0.join("input/input.csv"), input).unwrap();
        let entry = study.0.join("spec.yaml");
        fs::write(&entry, "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input/input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE], decimals: 18, warning_log: warnings.csv, verification_log: checks.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: float\n    derivation: SRC.VALUE\n    verifications:\n      - not_missing: {}\n      - range: {max: 0, severity: warning}\n").unwrap();
        let written = entry.to_str().unwrap();
        let (mut resources, entry) = file_configuration::resources(written).unwrap();
        let document =
            crate::file_preparation::prepare_document(&mut resources, &entry, None).unwrap();
        let prepared = crate::specification_run::PreparedRun::prepare(document).unwrap();
        let attempt = prepared.execute_with_port(&mut resources, Scope::default());
        let result = specification_report::build_result(
            &prepared,
            &attempt,
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "logs",
                specification: written,
                base_directory: ".",
            },
        )
        .unwrap();
        let mut result = Domain::from_build(result, |field| {
            let origin = prepared
                .document()
                .written_output_field_origin(field)
                .ok_or(crate::file_resources::Error::InvalidPath)?;
            resources.publication_target_from(origin.base_source, origin.written)
        })
        .unwrap();
        assert!(result.output().is_some());
        assert!(result.result.publication_items().last().unwrap().1.len() > 8 * 1024 * 1024);
        assert!(result.warning_log().is_some());
        assert_eq!(result.log_table(false).unwrap().rows(), 2);
        assert!(result.issues().is_empty());
        fs::remove_file(study.0.join("input/input.csv")).unwrap();
        fs::remove_file(study.0.join("spec.yaml")).unwrap();
        for _ in 0..2 {
            assert!(result.save().unwrap());
            let bytes = fs::read(study.0.join("out.csv")).unwrap();
            assert!(bytes.len() > 8 * 1024 * 1024);
            assert!(bytes.starts_with(b"ID,VALUE\n1,"));
            assert!(study.0.join("warnings.csv").exists());
            let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
            assert!(checks.starts_with(VERIFICATION_HEADER));
            assert!(checks.contains("not_missing,VALUE,REQ-0375,error,held"));
            assert!(checks.contains("range,VALUE,REQ-0377,warning,violated,range_failed"));
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                3
            );
            assert!(result.issues().is_empty());
        }
    }

    struct Study(PathBuf);
    impl Drop for Study {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn built() -> (Study, Domain) {
        built_with(false, false)
    }
    fn built_with(fatal: bool, nested: bool) -> (Study, Domain) {
        built_observed(fatal, nested, None)
    }
    fn built_observed(
        fatal: bool,
        nested: bool,
        measurements: Option<&crate::application_measurements::Measurements>,
    ) -> (Study, Domain) {
        let root = std::env::temp_dir().join(format!(
            "yamaa-save-log-attempt-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("input")).unwrap();
        let root = crate::canonical_path::canonicalize(root).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks/schema-verification-log");
        fs::copy(fixture.join("input/dm.csv"), root.join("input/dm.csv")).unwrap();
        let mut specification = fs::read_to_string(fixture.join("spec.yaml")).unwrap();
        if fatal {
            specification = specification.replace("severity: warning", "severity: error");
        }
        if nested {
            fs::create_dir(root.join("nested")).unwrap();
            for name in ["adsl-checks.csv", "adsl-warnings.csv", "adsl.csv"] {
                specification = specification.replace(name, &format!("nested/{name}"));
            }
        }
        fs::write(root.join("spec.yaml"), specification).unwrap();
        let entry = root.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "logs",
            specification: &entry,
            base_directory: ".",
        };
        let request = || Request {
            specification: &entry,
            environment: None,
        };
        let ordinary = domain(Language::Python, request(), identity()).unwrap();
        let result = if let Some(measurements) = measurements {
            let mut application = native(Language::Python, identity());
            let observed =
                domain_entry::domain(request(), &mut application, Scope::new(measurements))
                    .unwrap_or_else(|_| panic!("observed fixture failed"));
            measurements.complete(observed.output().is_some());
            assert_eq!(observed.observations_json(), ordinary.observations_json());
            assert_eq!(observed.output(), ordinary.output());
            assert_eq!(observed.verification_log(), ordinary.verification_log());
            assert_eq!(observed.warning_log(), ordinary.warning_log());
            assert_eq!(
                observed
                    .issues()
                    .iter()
                    .map(|issue| (&issue.condition, &issue.context))
                    .collect::<Vec<_>>(),
                ordinary
                    .issues()
                    .iter()
                    .map(|issue| (&issue.condition, &issue.context))
                    .collect::<Vec<_>>()
            );
            observed
        } else {
            ordinary
        };
        fs::remove_file(root.join("input/dm.csv")).unwrap();
        fs::remove_file(root.join("spec.yaml")).unwrap();
        (Study(root), result)
    }
    #[derive(Debug)]
    struct Marker(Arc<()>);
    impl std::fmt::Display for Marker {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("original publication marker")
        }
    }
    impl std::error::Error for Marker {}
    #[test]
    fn actual_observed_owner_preserves_report_log_bytes_original_save_failure_and_retry() {
        use crate::application_measurements::Measurements;
        let measurements = Measurements::new();
        let (study, mut result) = built_observed(false, false, Some(&measurements));
        let original = result.observations_json();
        let evidence = measurements.evidence();
        assert_eq!(evidence["admitted"], true);
        assert_eq!(evidence["build_accepted"], true);
        for phase in [
            "preparation",
            "build_service",
            "input",
            "execution_admission",
            "execution_derivation",
            "output_keys",
            "verification",
            "output_preparation",
        ] {
            assert!(
                evidence["stages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|stage| stage["phase"] == phase && stage["outcome"] == "succeeded"),
                "missing actual stage {phase}: {}",
                crate::json_wire::encode(&evidence)
            );
        }
        assert_eq!(
            crate::json_wire::decode(&measurements.envelope(Some(original.clone()))).unwrap()
                ["report"],
            result.observations()
        );
        let marker = Arc::new(());
        measurements.save_entered();
        assert!(!result
            .save_with(
                |_, _, _| Err(crate::file_publication::Error::Io(std::io::Error::other(
                    Marker(Arc::clone(&marker))
                )))
            )
            .unwrap());
        measurements.save_completed("refused");
        let Some(crate::file_publication::Error::Io(error)) = &result.publication_attempt.failure
        else {
            panic!("original error was lost")
        };
        assert!(Arc::ptr_eq(
            &error.get_ref().unwrap().downcast_ref::<Marker>().unwrap().0,
            &marker
        ));
        assert_eq!(result.observations_json(), original);
        for _ in 0..2 {
            measurements.save_entered();
            assert!(result.save().unwrap());
            measurements.save_completed("succeeded");
        }
        let after = measurements.evidence();
        assert_eq!(after["stages"], evidence["stages"]);
        assert_eq!(after["function_calls"], evidence["function_calls"]);
        assert_eq!(after["admitted"], true);
        assert_eq!(after["save_attempts"], 3);
        assert_eq!(after["saves"][0]["outcome"], "refused");
        assert_eq!(after["saves"][2]["outcome"], "succeeded");
        assert_eq!(
            result.observations()["artifacts"].as_array().unwrap().len(),
            3
        );
        assert!(study.0.join("adsl.csv").exists());
        let failed = Measurements::new();
        let (_, mut result) = built_observed(true, false, Some(&failed));
        assert_eq!(failed.evidence()["admitted"], true);
        assert_eq!(failed.evidence()["build_accepted"], false);
        failed.save_entered();
        assert!(result.save().is_err());
        failed.save_completed("failed");
        assert_eq!(failed.evidence()["saves"][0]["outcome"], "failed");
    }
    #[test]
    fn actual_late_failures_retain_original_error_entered_prefix_and_retry() {
        for fail_at in 0..3 {
            let (study, mut result) = built();
            let mut expected = result.observations();
            assert_eq!(
                result.observations_json(),
                crate::json_wire::encode(&expected)
            );
            expected["artifacts"] = Value::Array(
                result
                    .result
                    .publication_items()
                    .into_iter()
                    .take(fail_at)
                    .map(|(_, _, value)| value.clone())
                    .collect(),
            );
            let marker = Arc::new(());
            let mut entered = 0;
            assert!(!result
                .save_with(|publisher, path, bytes| {
                    let index = entered;
                    entered += 1;
                    if index == fail_at {
                        return Err(crate::file_publication::Error::Io(std::io::Error::other(
                            Marker(Arc::clone(&marker)),
                        )));
                    }
                    publisher.publish(path, bytes)
                })
                .unwrap());
            assert_eq!(entered, fail_at + 1);
            assert_eq!(result.publication_attempt.entered, fail_at + 1);
            assert_eq!(result.publication_attempt.completed, fail_at);
            let Some(crate::file_publication::Error::Io(error)) =
                &result.publication_attempt.failure
            else {
                panic!("original IO error was lost")
            };
            assert!(Arc::ptr_eq(
                &error.get_ref().unwrap().downcast_ref::<Marker>().unwrap().0,
                &marker
            ));
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                fail_at
            );
            assert_eq!(result.observations(), expected);
            assert_eq!(
                result.observations_json(),
                crate::json_wire::encode(&expected)
            );
            for (index, name) in ["adsl-checks.csv", "adsl-warnings.csv", "adsl.csv"]
                .iter()
                .enumerate()
            {
                assert_eq!(study.0.join(name).exists(), index < fail_at);
            }
            let issue = result.issues().pop().unwrap();
            assert_eq!(
                issue.spec_paths,
                [format!(
                    "output.{}",
                    ["verification_log", "warning_log", "path"][fail_at]
                )]
            );
            let context: Value = issue.context.clone();
            assert_eq!(context["entered"], fail_at + 1);
            assert_eq!(context["completed"], fail_at);
            assert!(result.save().unwrap());
            assert!(result.publication_attempt.failure.is_none());
            assert!(result.issues().is_empty());
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                3
            );
            assert_eq!(
                result.observations_json(),
                crate::json_wire::encode(&result.observations())
            );
        }
    }
    #[test]
    fn failed_build_ancillary_log_keeps_original_failure_and_fresh_retry_prefix() {
        let (study, mut result) = built_with(true, false);
        assert!(result.output().is_none());
        assert!(result.verification_log().is_some());
        let original = result.observations();
        let marker = Arc::new(());
        assert!(result
            .save_with(|_, _, _| {
                Err(crate::file_publication::Error::Io(std::io::Error::other(
                    Marker(Arc::clone(&marker)),
                )))
            })
            .is_err());
        assert_eq!(result.publication_attempt.entered, 1);
        assert_eq!(result.publication_attempt.completed, 0);
        let Some(crate::file_publication::Error::Io(error)) = &result.publication_attempt.failure
        else {
            panic!("original ancillary error lost")
        };
        assert!(Arc::ptr_eq(
            &error.get_ref().unwrap().downcast_ref::<Marker>().unwrap().0,
            &marker
        ));
        assert_eq!(result.observations(), original);
        assert!(result.save().is_err());
        assert_eq!(result.publication_attempt.entered, 1);
        assert_eq!(result.publication_attempt.completed, 1);
        assert!(result.publication_attempt.failure.is_none());
        assert!(study.0.join("adsl-checks.csv").exists());
        assert!(!study.0.join("adsl-warnings.csv").exists());
        assert!(!study.0.join("adsl.csv").exists());
        let mut expected = original;
        expected["artifacts"] = Value::Array(
            result
                .result
                .publication_items()
                .into_iter()
                .map(|(_, _, value)| value.clone())
                .collect(),
        );
        assert_eq!(result.observations(), expected);
        assert_eq!(
            result.observations_json(),
            crate::json_wire::encode(&expected)
        );
    }
    #[cfg(unix)]
    #[test]
    fn linked_parent_refuses_before_publish_and_restored_parent_retries_from_empty_prefix() {
        let (study, mut result) = built_with(false, true);
        let original = result.observations();
        fs::remove_dir(study.0.join("nested")).unwrap();
        fs::create_dir(study.0.join("foreign")).unwrap();
        std::os::unix::fs::symlink("foreign", study.0.join("nested")).unwrap();
        assert!(!result
            .save_with(|_, _, _| panic!("linked parent entered publisher"))
            .unwrap());
        assert_eq!(result.publication_attempt.entered, 0);
        assert_eq!(result.publication_attempt.completed, 0);
        assert!(matches!(
            result.publication_attempt.failure,
            Some(crate::file_publication::Error::InvalidTarget)
        ));
        assert_eq!(result.observations(), original);
        assert_eq!(fs::read_dir(study.0.join("foreign")).unwrap().count(), 0);
        fs::remove_file(study.0.join("nested")).unwrap();
        fs::create_dir(study.0.join("nested")).unwrap();
        assert!(result.save().unwrap());
        assert_eq!(result.publication_attempt.completed, 3);
        assert_eq!(
            result.observations()["artifacts"].as_array().unwrap().len(),
            3
        );
    }
    #[test]
    fn native_unwind_keeps_the_failing_boundary_and_original_payload() {
        let (study, mut result) = built();
        let marker = Arc::new(());
        let mut entered = 0;
        assert!(!result
            .save_with(|publisher, path, bytes| {
                entered += 1;
                if entered == 2 {
                    std::panic::panic_any(Arc::clone(&marker));
                }
                publisher.publish(path, bytes)
            })
            .unwrap());
        assert!(Arc::ptr_eq(
            result
                .publication_attempt
                .unwind
                .as_ref()
                .unwrap()
                .downcast_ref::<Arc<()>>()
                .unwrap(),
            &marker
        ));
        assert_eq!(result.publication_attempt.entered, 2);
        assert_eq!(result.publication_attempt.completed, 1);
        assert_eq!(
            result.observations()["artifacts"].as_array().unwrap().len(),
            1
        );
        assert!(study.0.join("adsl-checks.csv").exists());
        assert!(!study.0.join("adsl-warnings.csv").exists());
        assert!(!study.0.join("adsl.csv").exists());
        assert!(result.save().unwrap());
        assert!(result.publication_attempt.unwind.is_none());
    }
    #[test]
    fn actual_host_view_admits_metadata_beyond_the_old_64k_limit_and_saves_held_bytes() {
        for verification in [false, true] {
            let root = std::env::temp_dir().join(format!(
                "yamaa-host-view-refusal-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            let study = Study(crate::canonical_path::canonicalize(root).unwrap());
            fs::write(study.0.join("input.csv"), "ID\n1\n").unwrap();
            // This real native output exceeded the former 64 KiB IPC metadata
            // allowance. The wider allowance admits it without losing public
            // output or verification ownership. Long names need a fixture-only
            // regex allowance; the production width and compiler policy apply.
            let names = (0..39)
                .map(|index| format!("V{index}_{}", "a".repeat(2048)))
                .collect::<Vec<_>>();
            let columns = std::iter::once("ID")
                .chain(names.iter().map(String::as_str))
                .collect::<Vec<_>>();
            let mut raw = format!(
                "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: out.csv, columns: [{}]{} }}\ncolumns:\n",
                columns.join(", "),
                if verification { ", verification_log: checks.csv" } else { "" }
            );
            for name in columns {
                raw.push_str(&format!(
                    "  - {{name: {name}, type: int, derivation: SRC.ID}}\n"
                ));
            }
            let entry = study.0.join("spec.yaml");
            fs::write(&entry, raw).unwrap();
            let written = entry.to_str().unwrap();
            let (mut resources, entry) = file_configuration::resources(written).unwrap();
            let schema = crate::specification_source::CapturedSchema::admit(
                crate::shipped_schema::capture().unwrap().sources().to_vec(),
                0,
            )
            .unwrap();
            let document =
                crate::file_preparation::prepare_document(&mut resources, &entry, Some(schema))
                    .unwrap();
            let prepared = crate::specification_run::PreparedRun::prepare(document).unwrap();
            let attempt = prepared.execute_with_port(&mut resources, Scope::default());
            assert!(attempt.result.as_ref().unwrap().execution.is_some());
            let result = specification_report::build_result(
                &prepared,
                &attempt,
                Identity {
                    runtime: "test",
                    runtime_version: "test",
                    engine_version: yamaa_core::VERSION,
                    example: "host-view-refusal",
                    specification: written,
                    base_directory: ".",
                },
            )
            .unwrap();
            assert!(result.issues().is_empty());
            let public =
                crate::public_table::PublicTable::decode(result.output().unwrap()).unwrap();
            assert_eq!(public.columns().len(), 40);
            assert_eq!(public.rows(), 1);
            let domain = Domain::from_build(result, |field| {
                let origin = prepared
                    .document()
                    .written_output_field_origin(field)
                    .ok_or(crate::file_resources::Error::InvalidPath)?;
                resources.publication_target_from(origin.base_source, origin.written)
            });
            let mut domain = domain.unwrap();
            assert!(domain.output().is_some());
            assert!(domain.issues().is_empty());
            assert_eq!(domain.log_table(false).is_some(), verification);
            fs::remove_file(study.0.join("input.csv")).unwrap();
            fs::remove_file(study.0.join("spec.yaml")).unwrap();
            assert!(domain.save().unwrap());
            let held = fs::read(study.0.join("out.csv")).unwrap();
            assert!(held.starts_with(b"ID,V0_"));
            assert!(domain.save().unwrap());
            assert_eq!(fs::read(study.0.join("out.csv")).unwrap(), held);
            if verification {
                assert_eq!(fs::read_to_string(study.0.join("checks.csv")).unwrap(),
                        "REPORT_VERSION,ARTIFACT,SPEC_PATH,VERIFICATION_ID,CHECK,TARGET,REQUIREMENT,SEVERITY,OUTCOME,CONDITION,EVALUATED_COUNT,FAILURE_COUNT,DETAILS\n");
            }
        }
    }
}
