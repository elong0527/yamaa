//! One native lifecycle behind both hosts' file entry points: a specification
//! alone and a member of a project run prepare, run and report on one route.
use crate::report_storage::StoredReport;
use crate::{
    file_preparation::{self, FileSpecification},
    specification_report::{self, BuildResult, Identity},
    specification_run::Attempt,
};
use std::any::Any;
use yamaa_core::application_issue::{Fault, Stage};
use yamaa_core::application_observer::Scope;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::domain_entry::{self, Request};
use yamaa_core::json::Value;
use yamaa_core::project_activation::ActivationPort;
use yamaa_core::project_function::Language;

/// The diagnostics of a refused operation, in authored order.
pub type Failure = Vec<Diagnostic>;

pub fn unsupported(operation: &str, path: Option<&str>) -> Failure {
    vec![yamaa_core::application_issue::unsupported(operation, path)]
}
pub fn rejected(stage: Stage, fault: Fault) -> Failure {
    vec![yamaa_core::application_issue::rejected(stage, fault)]
}
/// The public issues of one result: every finding up to the findings limit.
/// Raw facts and retained reports remain complete; taking the prefix here
/// preserves authored stage order across one result.
pub fn collect(issues: impl IntoIterator<Item = Diagnostic>) -> Failure {
    issues
        .into_iter()
        .take(yamaa_core::limits::Limits::FINDINGS)
        .collect()
}
pub(crate) fn classified_resource(diagnostic: &Diagnostic) -> Failure {
    vec![diagnostic.clone()]
}
pub(crate) fn resource_failure(stage: Stage, error: &crate::file_resources::Error) -> Failure {
    with_source(resource_diagnostic(stage, error), None)
}
/// Verify a compiled value's held metadata at the start of a run, before
/// activation or any study read. Every snapshot is captured again through the
/// compile authority and compared with its held reference and bytes. A file
/// that changed or disappeared refuses the run with its own condition,
/// `resource_path_content_changed` or `resource_path_missing`, naming the field
/// that declared it and the path as written (FILE-10); `declared` finds that
/// declaration from the held identity. A resolver failure that is neither
/// stays the opaque resource refusal. Nothing is recompiled.
pub(crate) fn verify_held(
    resources: &crate::file_resources::Resources,
    declared: impl FnOnce(
        &str,
        yamaa_core::resource::ResourceFailure,
    ) -> Option<yamaa_core::diagnostic::Diagnostic>,
) -> Result<(), Failure> {
    use yamaa_core::resource::ResourceFailure as R;
    resources.verify_held().map_err(|(identity, error)| {
        crate::file_resources::resource_failure(&error)
            .filter(|failure| matches!(failure, R::ContentChanged | R::Missing))
            .and_then(|failure| declared(&identity, failure))
            .map_or_else(
                || resource_failure_from(Stage::Prepare, &error, &identity),
                |diagnostic| classified_resource(&diagnostic),
            )
    })
}
/// A failure observed on an already captured file also names that file.
pub(crate) fn resource_failure_from(
    stage: Stage,
    error: &crate::file_resources::Error,
    source: &str,
) -> Failure {
    with_source(resource_diagnostic(stage, error), Some(source))
}
fn with_source(mut diagnostic: Diagnostic, source: Option<&str>) -> Failure {
    if let Some(source) = source {
        diagnostic
            .context
            .insert("source".into(), Value::String(source.into()));
    }
    vec![diagnostic]
}
fn resource_diagnostic(stage: Stage, error: &crate::file_resources::Error) -> Diagnostic {
    if let crate::file_resources::Error::Limit(limit) = error {
        return yamaa_core::application_issue::resource_rejected(
            stage,
            Fault::ResourceLimit,
            limit.resource,
            limit.limit,
            limit.actual,
            Vec::new(),
        );
    }
    yamaa_core::application_issue::rejected(stage, resource_fault(error))
}
/// The closed code of the opaque refusal a resource failure reports. A bound
/// reports its facts beside the code (`resource_limit`).
pub(crate) fn resource_fault(error: &crate::file_resources::Error) -> Fault {
    use crate::file_resources::Error as E;
    match error {
        E::Host(_) => Fault::HostResourceFailure,
        E::Missing => Fault::ResourceMissing,
        E::Unreadable => Fault::ResourceUnreadable,
        E::NotRegularFile => Fault::ResourceNotRegular,
        E::InvalidRoot => Fault::ResourceRoot,
        E::InvalidBase => Fault::ResourceBase,
        E::InvalidPath => Fault::ResourcePath,
        E::WrittenForm(_) => Fault::ResourcePath,
        E::OutsideRoots => Fault::ResourceOutsideRoots,
        E::Symlink => Fault::ResourceLink,
        E::Changed => Fault::ResourceChanged,
        E::InvalidLock => Fault::InvalidPackagingLock,
        E::Limit(_) => Fault::ResourceLimit,
    }
}
/// The closed code a refused publication reports, selected from its typed cause.
pub(crate) fn publication_fault(error: &crate::file_publication::Error) -> Fault {
    use crate::file_publication::Error as E;
    match error {
        E::Host(crate::host_resources::Failure::Cleanup) => Fault::CleanupIncomplete,
        E::Host(crate::host_resources::Failure::Cancelled) => Fault::PublicationCancelled,
        E::Host(_) => Fault::HostPublicationFailure,
        E::Cleanup { .. } => Fault::CleanupIncomplete,
        E::Changed => Fault::TargetChanged,
        E::Limit(_) => Fault::PublicationLimit,
        E::StagingCollision => Fault::StagingCollision,
        E::InvalidTarget => Fault::InvalidTarget,
        E::PathMismatch => Fault::PathMismatch,
        E::Io(_) => Fault::PublicationFailed,
    }
}
pub(crate) fn publication_failure(error: &crate::file_publication::Error) -> Failure {
    use crate::file_publication::Error as E;
    use yamaa_core::json::Value as JsonValue;

    if let E::Limit(error) = error {
        return vec![yamaa_core::application_issue::resource_rejected(
            Stage::Output,
            Fault::PublicationLimit,
            error.resource,
            error.limit,
            error.actual,
            Vec::new(),
        )];
    }
    let mut finding =
        yamaa_core::application_issue::rejected(Stage::Output, publication_fault(error));
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
    vec![finding]
}

pub struct Domain {
    result: BuildResult,
    targets: Option<Vec<crate::publication::AnchoredTarget>>,
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
/// What a save admitted for one result before it staged anything: the
/// complete publication observations, with an empty slot for each file it
/// writes, and each file's output field.
pub struct Admitted {
    report: StoredReport,
    artifacts: std::vec::IntoIter<Value>,
    fields: Vec<&'static str>,
    written: Vec<String>,
}
fn publication_at(
    mut issues: Failure,
    field: &str,
    path: &str,
    attempt: &PublicationAttempt,
) -> Failure {
    for issue in &mut issues {
        issue.spec_paths = vec![format!("output.{field}")];
        for (key, value) in [
            ("path", Value::String(path.into())),
            ("entered", Value::from(attempt.entered)),
            ("completed", Value::from(attempt.completed)),
        ] {
            issue.context.insert(key.into(), value);
        }
    }
    issues
}
impl Domain {
    /// Hold one built result with the anchored save gate of its declared
    /// files. Save targets resolve from the run's scope.
    pub fn new(
        result: BuildResult,
        prepared: &FileSpecification,
        run: &crate::file_resources::RunScope,
    ) -> Self {
        Self::from_build(result, |field| {
            prepared.publication_field_target(run, field)
        })
    }
    fn from_build(
        result: BuildResult,
        mut target: impl FnMut(
            &str,
        ) -> Result<
            crate::publication::AnchoredTarget,
            crate::file_resources::Error,
        >,
    ) -> Self {
        let fields = [
            ("report", result.report_file()),
            ("path", result.output().is_some()),
        ];
        let targets = fields
            .into_iter()
            .filter(|(_, present)| *present)
            .map(|(field, _)| target(field))
            .collect::<Result<Vec<_>, _>>();
        let (targets, publication_issues) = match targets {
            Ok(targets) => (Some(targets), Vec::new()),
            Err(error) => (None, resource_failure(Stage::Output, &error)),
        };
        Self {
            result,
            targets,
            publication_issues,
            published: None,
            publication_attempt: Default::default(),
        }
    }
    pub fn output(&self) -> Option<&[u8]> {
        self.result.output()
    }
    pub fn output_table(&self) -> Option<&crate::public_table::PublicTable> {
        self.result.output_table()
    }
    /// The run report's owned Arrow IPC view; every build holds one.
    pub fn run_report(&self) -> &[u8] {
        self.result.run_report()
    }
    pub fn report_table(&self) -> &crate::public_table::PublicTable {
        self.result.report_table()
    }
    pub fn issues(&self) -> Failure {
        collect(
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
    /// The accepted primary artifact's serialized bytes; a project run hands
    /// them to the consumers it builds later. Absent when the build failed.
    pub fn artifact(&self) -> Option<std::sync::Arc<[u8]>> {
        self.result.artifact()
    }
    /// The files a save of this result writes, in publication order: its
    /// declared report file, then its primary artifact unless `report_only`
    /// or the build failed (OUTPUT-10). A failed build that declares no report
    /// file writes nothing. The complete publication observations are
    /// admitted first, so a report too large to record refuses the save before
    /// any file is staged; every prefix of them then fits.
    pub fn plan(
        &self,
        report_only: bool,
    ) -> Result<Option<(Vec<crate::publication::File<'_>>, Admitted)>, Failure> {
        let diagnostic = report_only || self.result.output().is_none();
        if diagnostic && !self.result.report_file() {
            return Ok(None);
        }
        let Some(targets) = self.targets.as_ref() else {
            return Err(self.publication_issues.clone());
        };
        let mut items = self.result.publication_items();
        if diagnostic {
            // The report file comes first; a diagnostic save writes it alone.
            items.truncate(1);
        }
        // Own one complete report and every artifact entry before staging.
        // Later success only moves an already-owned entry into its slot.
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
        let (report, artifacts) = report.map_err(|error| match &error {
            specification_report::Error::ResourceLimit(refusal) => {
                refusal.issues(Stage::Output, Fault::PublicationReportLimit)
            }
            _ => rejected(Stage::Output, Fault::PublicationReportLimit),
        })?;
        let fields = [("report", self.result.report_file()), ("path", !diagnostic)]
            .into_iter()
            .filter_map(|(field, present)| present.then_some(field))
            .collect();
        let files = targets
            .iter()
            .zip(&items)
            .map(|(target, (written, bytes, _))| crate::publication::File {
                target,
                written,
                bytes,
            })
            .collect();
        let written = items
            .iter()
            .map(|(written, _, _)| (*written).to_owned())
            .collect();
        Ok(Some((
            files,
            Admitted {
                report,
                artifacts,
                fields,
                written,
            },
        )))
    }
    /// A save of this result refused before it staged anything, for `issues`.
    pub fn refuse(&mut self, issues: Failure) {
        self.published = None;
        self.publication_attempt = Default::default();
        self.publication_issues = issues;
    }
    /// What the latest save did with this result's files: `committed` of
    /// them replaced their targets, and, when the save stopped at one of
    /// them, which one and why. The observations then list the replaced
    /// files, and a stop joins the issues as the save's own finding.
    pub fn record(
        &mut self,
        admitted: Admitted,
        committed: usize,
        stopped: Option<(usize, crate::publication::Cause)>,
    ) {
        use crate::publication::Cause;
        let Admitted {
            mut report,
            mut artifacts,
            fields,
            written,
        } = admitted;
        let slots = report
            .get_mut("artifacts")
            .and_then(Value::as_array_mut)
            .expect("admitted artifact slots");
        slots.extend(artifacts.by_ref().take(committed));
        self.published = Some(report);
        self.publication_attempt = PublicationAttempt {
            entered: committed,
            completed: committed,
            ..Default::default()
        };
        self.publication_issues = Vec::new();
        let Some((file, cause)) = stopped else {
            return;
        };
        let attempt = &mut self.publication_attempt;
        let at = |issues, attempt: &PublicationAttempt| {
            publication_at(issues, fields[file], &written[file], attempt)
        };
        self.publication_issues = match cause {
            // The save never entered this file: its target is refused.
            Cause::Target(error) => {
                attempt.entered = 0;
                let issues = publication_failure(&error);
                attempt.failure = Some(error);
                issues
            }
            Cause::Collision { .. } => {
                attempt.entered = 0;
                let error = crate::file_publication::Error::InvalidTarget;
                let issues = publication_failure(&error);
                attempt.failure = Some(error);
                issues
            }
            Cause::Stage(error) | Cause::Commit(error) => {
                attempt.entered = file + 1;
                let issues = at(publication_failure(&error), attempt);
                attempt.failure = Some(error);
                issues
            }
            Cause::Unwind(payload) => {
                attempt.entered = file + 1;
                let issues = at(rejected(Stage::Output, Fault::PublicationUnwind), attempt);
                attempt.unwind = Some(payload);
                issues
            }
        };
    }
    /// Save this result alone, as an outcome of one member saves it: the
    /// declared report file, then the primary artifact. A failed build writes
    /// only its report file and still reports a failed save. Build itself
    /// never writes.
    pub fn save(&mut self) -> Result<bool, FailedBuild> {
        self.save_with(false, |_| Ok(()))
    }
    /// Publish the declared report file alone, as a failed build saves, even
    /// when the build succeeded. `Err` means there is no report file to write.
    pub fn save_report(&mut self) -> Result<bool, FailedBuild> {
        if !self.result.report_file() {
            return Err(FailedBuild);
        }
        let failed = self.result.output().is_none();
        match self.save_with(true, |_| Ok(())) {
            // A failed build reports its failure after the report was written.
            Err(FailedBuild) if failed => Ok(self.publication_issues.is_empty()),
            outcome => outcome,
        }
    }
    /// [`Self::save`], or [`Self::save_report`] with `report_only`, calling
    /// `before` with each file's position before it is committed.
    pub(crate) fn save_with(
        &mut self,
        report_only: bool,
        before: impl FnMut(usize) -> Result<(), crate::file_publication::Error>,
    ) -> Result<bool, FailedBuild> {
        let failed = self.result.output().is_none();
        let fail = || if failed { Err(FailedBuild) } else { Ok(false) };
        let (staged, admitted) = match self.plan(report_only) {
            Ok(None) => return fail(),
            Ok(Some((files, admitted))) => (crate::publication::stage(&files), admitted),
            Err(issues) => {
                self.refuse(issues);
                return fail();
            }
        };
        let (committed, stopped) = match staged.and_then(|staged| staged.commit_with(before)) {
            Ok(paths) => (paths.len(), None),
            Err(stop) => (stop.committed, Some((stop.file, stop.cause))),
        };
        let saved = stopped.is_none();
        self.record(admitted, committed, stopped);
        if failed {
            Err(FailedBuild)
        } else {
            Ok(saved)
        }
    }
}
#[derive(Debug)]
pub struct FailedBuild;

/// How a host classifies one retained failure of its functions port: an
/// ordinary host failure with its details, lock findings, or an opaque failure
/// reported without classification facts.
pub enum Classification {
    Host(crate::callback::CallbackError),
    Lock(Vec<crate::project_lock::Finding>),
    Opaque,
}

/// Run one prepared specification in `run` and build its result: activate the
/// project functions it calls through `activation`, read its inputs from the
/// scope, execute, and report. A specification alone and a member of a project
/// run take this one route; Save targets resolve in the scope. `classify`
/// reads the retained failures of `activation`. The attempt is returned beside
/// the result for the host failures it holds.
pub fn run<A: ActivationPort>(
    prepared: &FileSpecification,
    run: &mut crate::file_resources::RunScope,
    activation: &mut A,
    classify: impl Fn(&A::Error) -> Classification,
    identity: Identity<'_>,
    scope: Scope<'_>,
) -> (
    Attempt<crate::file_resources::Error, A::Error>,
    Result<Domain, Failure>,
) {
    let attempt = prepared.build(run, activation, scope);
    if let Some(error) = file_preparation::opaque_resource_failure(&attempt.dataset) {
        let failure = resource_failure(Stage::Capture, error);
        return (attempt, Err(failure));
    }
    scope.phase(yamaa_core::application_observer::Phase::Output);
    let converted = result(prepared, run, &attempt, classify, identity);
    scope.exit(
        yamaa_core::application_observer::Phase::Output,
        if converted.is_ok() {
            yamaa_core::application_observer::Outcome::Succeeded
        } else {
            yamaa_core::application_observer::Outcome::Failed
        },
    );
    (attempt, converted)
}

/// Report one attempt, classifying its retained host failures first.
fn result<E>(
    prepared: &FileSpecification,
    run: &crate::file_resources::RunScope,
    attempt: &Attempt<crate::file_resources::Error, E>,
    classify: impl Fn(&E) -> Classification,
    identity: Identity<'_>,
) -> Result<Domain, Failure> {
    use crate::project_report::Classified;
    let mut held = Vec::new();
    crate::project_attempt::visit_host_failures(attempt, |_, error| held.push(error));
    let classified = held.iter().map(|error| classify(error)).collect::<Vec<_>>();
    let facts = held
        .iter()
        .zip(&classified)
        .filter_map(|(failure, classified)| match classified {
            Classification::Host(error) => {
                crate::callback::details(error).map(|details| Classified::Host {
                    failure: *failure,
                    details,
                })
            }
            Classification::Lock(findings) => Some(Classified::Lock {
                failure: *failure,
                findings,
            }),
            Classification::Opaque => None,
        })
        .collect::<Vec<_>>();
    let result = specification_report::build_result(prepared.run(), attempt, identity, &facts)
        .map_err(|refusal| refusal.issues())?;
    Ok(Domain::new(result, prepared, run))
}

/// Prepare one request through `resolver`, or the local file resolver, for
/// the host language `host`, or for an engine that claims none, which reads
/// no project's `environments` entry (#3030 section 2.10).
pub fn select(
    host: Option<Language>,
    request: Request<'_>,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
) -> Result<FileSpecification, crate::file_workflow::SelectionError> {
    match resolver {
        Some(resolver) => crate::file_workflow::select_with_resolver(request, host, resolver),
        None => crate::file_workflow::select_request(request, host),
    }
}

pub fn check(host: Language, request: Request<'_>, identity: Identity<'_>) -> Failure {
    check_with_resolver(Some(host), request, identity, None)
}
/// The static check of one request: preparation findings, or the compiled
/// specification's own findings. No study input is read.
pub fn check_with_resolver(
    host: Option<Language>,
    request: Request<'_>,
    _identity: Identity<'_>,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
) -> Failure {
    collect(match select(host, request, resolver) {
        Ok(prepared) => crate::specification_check::issues(prepared.run()),
        Err(error) => crate::project_application::selection_failure(&error),
    })
}
/// Prepare and run one request with no project functions: a request with a
/// project, whose functions need a host, is unsupported here.
pub fn domain(
    host: Language,
    request: Request<'_>,
    identity: Identity<'_>,
) -> Result<Domain, Failure> {
    domain_observed(host, request, identity, Scope::default())
}
fn domain_observed(
    host: Language,
    request: Request<'_>,
    identity: Identity<'_>,
    scope: Scope<'_>,
) -> Result<Domain, Failure> {
    domain_entry::domain(
        scope,
        || {
            select(Some(host), request, None)
                .map_err(|error| crate::project_application::selection_failure(&error))
        },
        |prepared| {
            if prepared.run().has_project() {
                return Err(unsupported("environment", Some("environment")));
            }
            let mut run = prepared.run_scope(None);
            let (_, result) = self::run(
                prepared,
                &mut run,
                &mut crate::specification_run::NoActivation,
                |_| Classification::Opaque,
                identity,
                scope,
            );
            result
        },
    )
    .map_err(collect)
}

#[cfg(test)]
mod collect_tests {
    use super::*;

    #[test]
    fn the_findings_limit_keeps_the_first_ten_thousand_across_stages() {
        use yamaa_core::application_issue::{rejected, Fault, Stage};
        fn row(index: usize) -> Diagnostic {
            let stage = if index < 6_000 {
                Stage::Ingest
            } else {
                Stage::Output
            };
            let mut finding = rejected(stage, Fault::Internal);
            finding.spec_paths = vec![format!("columns.C{index}")];
            finding
        }
        for length in [0, 9_999, 10_000, 10_001] {
            let actual = collect((0..length).map(row));
            assert_eq!(actual.len(), length.min(10_000));
            for (index, actual) in actual.iter().enumerate() {
                assert_eq!(actual, &row(index));
            }
        }
        let mut complete = row(0);
        complete
            .context
            .insert("detail".into(), Value::String("é🙂".repeat(50_000)));
        assert_eq!(collect([complete.clone()]), [complete]);
    }
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
    fn primary_above_the_former_eight_megabyte_quota_retains_the_report_and_saves_held_bytes() {
        let root = std::env::temp_dir().join(format!(
            "yamaa-output-report-cap-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("input")).unwrap();
        let study = Study(crate::canonical_path::canonicalize(root).unwrap());
        const REPORT_HEADER: &str =
            "ARTIFACT,KIND,SPEC_PATH,NAME,ID,TARGET,RULE,SEVERITY,OUTCOME,CONDITION,COUNT,TOTAL,KEYS,DETAILS\n";
        // The output preserves declared precision: each large
        // finite float needs 309 whole digits plus the declared 18 fractional digits.
        let mut input = String::from("ID,VALUE\n");
        for id in 1..=26_000 {
            input.push_str(&format!("{id},1e308\n"));
        }
        fs::write(study.0.join("input/input.csv"), input).unwrap();
        let entry = study.0.join("spec.yaml");
        fs::write(&entry, "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input/input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE], decimals: 18, report: checks.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: float\n    derivation: SRC.VALUE\n    verifications:\n      - not_missing: {}\n      - range: {max: 0, severity: warning}\n").unwrap();
        let written = entry.to_str().unwrap();
        let (mut resources, entry) = file_configuration::resources(written).unwrap();
        let document =
            crate::file_preparation::prepare_document(&mut resources, &entry, None).unwrap();
        let prepared = crate::specification_run::PreparedRun::prepare(document).unwrap();
        let attempt = prepared.execute(&mut resources, Scope::default());
        let result = specification_report::build_result(
            &prepared,
            &attempt,
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "report",
                specification: written,
                base_directory: ".",
            },
            &[],
        )
        .unwrap();
        let mut result = Domain::from_build(result, |field| {
            let origin = prepared
                .document()
                .written_output_field_origin(field)
                .ok_or(crate::file_resources::Error::InvalidPath)?;
            resources.publication_target_from(origin.base_source, origin.written)
        });
        assert!(result.output().is_some());
        assert!(result.result.publication_items().last().unwrap().1.len() > 8 * 1024 * 1024);
        // One run, input and base row, two column rows and two check rows.
        assert_eq!(result.report_table().rows(), 7);
        assert!(result.issues().is_empty());
        fs::remove_file(study.0.join("input/input.csv")).unwrap();
        fs::remove_file(study.0.join("spec.yaml")).unwrap();
        for _ in 0..2 {
            assert!(result.save().unwrap());
            let bytes = fs::read(study.0.join("out.csv")).unwrap();
            assert!(bytes.len() > 8 * 1024 * 1024);
            assert!(bytes.starts_with(b"ID,VALUE\n1,"));
            let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
            assert!(checks.starts_with(REPORT_HEADER));
            assert!(checks.contains("\nout.csv,run,output,,,,,,succeeded,,26000,,,{}\n"));
            assert!(checks.contains(
                "check,columns.VALUE.verifications[0].not_missing,not_missing,,VALUE,CHECK-5,error,held,,0,26000,,{}\n"
            ));
            assert!(checks.contains(
                "check,columns.VALUE.verifications[1].range,range,,VALUE,CHECK-7,warning,violated,range_failed,26000,26000,\"[{\"\"ID\"\":1},"
            ));
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                2
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
            "yamaa-save-report-attempt-{}-{}",
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
            for name in ["adsl-checks.csv", "adsl.csv"] {
                specification = specification.replace(name, &format!("nested/{name}"));
            }
        }
        fs::write(root.join("spec.yaml"), specification).unwrap();
        let entry = root.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "report",
            specification: &entry,
            base_directory: ".",
        };
        let request = || Request {
            specification: &entry,
            environment: None,
        };
        let ordinary = domain(Language::Python, request(), identity()).unwrap();
        let result = if let Some(measurements) = measurements {
            let observed = domain_observed(
                Language::Python,
                request(),
                identity(),
                Scope::new(measurements),
            )
            .unwrap_or_else(|_| panic!("observed fixture failed"));
            measurements.complete(observed.output().is_some());
            assert_eq!(observed.observations_json(), ordinary.observations_json());
            assert_eq!(observed.output(), ordinary.output());
            assert_eq!(observed.run_report(), ordinary.run_report());
            assert_eq!(
                observed
                    .issues()
                    .iter()
                    .map(|issue| (issue.condition(), &issue.context))
                    .collect::<Vec<_>>(),
                ordinary
                    .issues()
                    .iter()
                    .map(|issue| (issue.condition(), &issue.context))
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
    fn actual_observed_owner_preserves_run_report_bytes_original_save_failure_and_retry() {
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
            .save_with(false, |_| Err(crate::file_publication::Error::Io(
                std::io::Error::other(Marker(Arc::clone(&marker)))
            )))
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
            2
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
        for fail_at in 0..2 {
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
                .save_with(false, |index| {
                    entered += 1;
                    if index == fail_at {
                        return Err(crate::file_publication::Error::Io(std::io::Error::other(
                            Marker(Arc::clone(&marker)),
                        )));
                    }
                    Ok(())
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
            for (index, name) in ["adsl-checks.csv", "adsl.csv"].iter().enumerate() {
                assert_eq!(study.0.join(name).exists(), index < fail_at);
            }
            let issue = result.issues().pop().unwrap();
            assert_eq!(
                issue.spec_paths,
                [format!("output.{}", ["report", "path"][fail_at])]
            );
            let context = Value::Object(issue.context.clone());
            assert_eq!(context["entered"], fail_at + 1);
            assert_eq!(context["completed"], fail_at);
            assert!(result.save().unwrap());
            assert!(result.publication_attempt.failure.is_none());
            assert!(result.issues().is_empty());
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                2
            );
            assert_eq!(
                result.observations_json(),
                crate::json_wire::encode(&result.observations())
            );
        }
    }
    #[test]
    fn failed_build_report_keeps_original_failure_and_fresh_retry_prefix() {
        let (study, mut result) = built_with(true, false);
        assert!(result.output().is_none());
        assert!(result.result.report_file());
        assert!(!result.run_report().is_empty());
        let original = result.observations();
        let marker = Arc::new(());
        assert!(result
            .save_with(false, |_| {
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
            .save_with(false, |_| panic!("linked parent entered publisher"))
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
        assert_eq!(result.publication_attempt.completed, 2);
        assert_eq!(
            result.observations()["artifacts"].as_array().unwrap().len(),
            2
        );
    }
    #[test]
    fn native_unwind_keeps_the_failing_boundary_and_original_payload() {
        let (study, mut result) = built();
        let marker = Arc::new(());
        let mut entered = 0;
        assert!(!result
            .save_with(false, |_| {
                entered += 1;
                if entered == 2 {
                    std::panic::panic_any(Arc::clone(&marker));
                }
                Ok(())
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
                if verification { ", report: checks.csv" } else { "" }
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
            let attempt = prepared.execute(&mut resources, Scope::default());
            assert!(attempt.dataset.result.as_ref().unwrap().result.is_ok());
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
                &[],
            )
            .unwrap();
            assert!(result.issues().is_empty());
            let public = result.output_table().unwrap();
            assert_eq!(public.columns().len(), 40);
            assert_eq!(public.rows(), 1);
            let domain = Domain::from_build(result, |field| {
                let origin = prepared
                    .document()
                    .written_output_field_origin(field)
                    .ok_or(crate::file_resources::Error::InvalidPath)?;
                resources.publication_target_from(origin.base_source, origin.written)
            });
            let mut domain = domain;
            assert!(domain.output().is_some());
            assert!(domain.issues().is_empty());
            assert_eq!(domain.result.report_file(), verification);
            // One run, input and base row and one row for each of the 40 columns.
            assert_eq!(domain.report_table().rows(), 43);
            fs::remove_file(study.0.join("input.csv")).unwrap();
            fs::remove_file(study.0.join("spec.yaml")).unwrap();
            assert!(domain.save().unwrap());
            let held = fs::read(study.0.join("out.csv")).unwrap();
            assert!(held.starts_with(b"ID,V0_"));
            assert!(domain.save().unwrap());
            assert_eq!(fs::read(study.0.join("out.csv")).unwrap(), held);
            if verification {
                let report = fs::read_to_string(study.0.join("checks.csv")).unwrap();
                assert!(report.starts_with(
                    "ARTIFACT,KIND,SPEC_PATH,NAME,ID,TARGET,RULE,SEVERITY,OUTCOME,CONDITION,COUNT,TOTAL,KEYS,DETAILS\nout.csv,run,output,,,,,,succeeded,,1,,,{}\n"
                ));
                assert_eq!(report.lines().count(), 44);
            } else {
                assert!(!study.0.join("checks.csv").exists());
            }
        }
    }
}
