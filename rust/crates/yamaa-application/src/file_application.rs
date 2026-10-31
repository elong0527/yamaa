//! One native lifecycle behind both hosts' file entry points: a specification
//! alone and a member of a project run prepare, run and report on one route.
use crate::report_storage::StoredReport;
use crate::{
    file_preparation::{self, FileSpecification},
    specification_report::{self, BuildResult, HostView, Identity},
    specification_run::{Attempt, BoundaryFailure},
};
use std::any::Any;
use yamaa_core::application_issue::{Fault, Step};
use yamaa_core::application_observer::Scope;
use yamaa_core::diagnostic::{ConditionCode, Diagnostic};
use yamaa_core::domain_entry::Request;
use yamaa_core::host_functions::HostFunctions;
use yamaa_core::json::Value;
use yamaa_core::project_function::Language;

/// The diagnostics of a refused operation, in authored order.
pub type Failure = Vec<Diagnostic>;

pub fn unsupported(operation: &str, path: Option<&str>) -> Failure {
    vec![yamaa_core::application_issue::unsupported(operation, path)]
}
pub fn rejected(step: Step, fault: Fault) -> Failure {
    vec![yamaa_core::application_issue::rejected(step, fault)]
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
pub(crate) fn resource_failure(step: Step, error: &crate::file_resources::Error) -> Failure {
    with_source(resource_diagnostic(step, error), None)
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
                || resource_failure_from(Step::Prepare, &error, &identity),
                |diagnostic| classified_resource(&diagnostic),
            )
    })
}
/// A failure observed on an already captured file also names that file.
pub(crate) fn resource_failure_from(
    step: Step,
    error: &crate::file_resources::Error,
    source: &str,
) -> Failure {
    with_source(resource_diagnostic(step, error), Some(source))
}
fn with_source(mut diagnostic: Diagnostic, source: Option<&str>) -> Failure {
    if let Some(source) = source {
        diagnostic
            .context
            .insert("source".into(), Value::String(source.into()));
    }
    vec![diagnostic]
}
fn resource_diagnostic(step: Step, error: &crate::file_resources::Error) -> Diagnostic {
    if let crate::file_resources::Error::Limit(limit) = error {
        return yamaa_core::application_issue::resource_rejected(
            step,
            Fault::ResourceLimit,
            limit.resource,
            limit.limit,
            limit.actual,
            Vec::new(),
        );
    }
    yamaa_core::application_issue::rejected(step, crate::file_resources::resource_fault(error))
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
        // A walk refusal reports its file rule's condition where the save
        // knows the file (see `target_failure`).
        E::InvalidTarget | E::Unreachable(_) => Fault::InvalidTarget,
        E::PathMismatch => Fault::PathMismatch,
        E::Io(_) => Fault::PublicationFailed,
    }
}
/// The issues of a save refused at the file `written`, which the field
/// `field` declares, or no field for a file the caller names: a target the
/// walk cannot reach fails with its file rule's condition under `output`,
/// naming the file as written (FILE-7, FILE-8, FILE-11; #3030 D189), and any
/// other refusal is the engine's.
pub(crate) fn target_failure(
    error: &crate::file_publication::Error,
    field: Option<&str>,
    written: &str,
) -> Failure {
    match error {
        crate::file_publication::Error::Unreachable(failure) => failure
            .target_diagnostic(field, written)
            .map_or_else(|| publication_failure(error), |issue| vec![issue]),
        error => publication_failure(error),
    }
}
/// The issues of a target that could not be resolved when a run was built:
/// a kind its resolver reports refuses it as the walk does
/// ([`target_failure`]), and any other failure stays the opaque refusal.
pub(crate) fn unresolved_target(
    error: &crate::file_resources::Error,
    field: Option<&str>,
    written: &str,
) -> Failure {
    crate::file_resources::resource_failure(error)
        .and_then(|failure| failure.target_diagnostic(field, written))
        .map_or_else(
            || resource_failure(Step::Output, error),
            |issue| vec![issue],
        )
}
pub(crate) fn publication_failure(error: &crate::file_publication::Error) -> Failure {
    use crate::file_publication::Error as E;
    use yamaa_core::json::Value as JsonValue;

    if let E::Limit(error) = error {
        return vec![yamaa_core::application_issue::resource_rejected(
            Step::Output,
            Fault::PublicationLimit,
            error.resource,
            error.limit,
            error.actual,
            Vec::new(),
        )];
    }
    let mut finding =
        yamaa_core::application_issue::rejected(Step::Output, publication_fault(error));
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
    targets: Option<Vec<crate::publisher::AnchoredTarget>>,
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
    issues: Failure,
    field: &str,
    path: &str,
    attempt: &PublicationAttempt,
) -> Failure {
    publication_stop(
        issues,
        Some(format!("output.{field}")),
        path,
        attempt.entered,
        attempt.completed,
    )
}
/// `issues` of a save that stopped at the file `path` names, after it
/// entered `entered` files and completed `completed`: each names the file as
/// written and the attempt, at `spec_path` when a specification declares the
/// file.
pub(crate) fn publication_stop(
    mut issues: Failure,
    spec_path: Option<String>,
    path: &str,
    entered: usize,
    completed: usize,
) -> Failure {
    for issue in &mut issues {
        issue.spec_paths = spec_path.iter().cloned().collect();
        for (key, value) in [
            ("path", Value::String(path.into())),
            ("entered", Value::from(entered)),
            ("completed", Value::from(completed)),
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
        mut target: impl FnMut(&str) -> Result<crate::publisher::AnchoredTarget, Failure>,
    ) -> Self {
        let fields = [
            ("report", result.report_file()),
            ("path", result.accepted()),
        ];
        let targets = fields
            .into_iter()
            .filter(|(_, present)| *present)
            .map(|(field, _)| target(field))
            .collect::<Result<Vec<_>, _>>();
        let (targets, publication_issues) = match targets {
            Ok(targets) => (Some(targets), Vec::new()),
            Err(issues) => (None, issues),
        };
        Self {
            result,
            targets,
            publication_issues,
            published: None,
            publication_attempt: Default::default(),
        }
    }
    /// The output table, which only an accepted output has.
    pub fn output(&self) -> Option<&crate::public_table::PublicTable> {
        self.result.output()
    }
    /// The output table's rows and columns, without building the table,
    /// which is built when first read (#4201).
    pub fn output_dimensions(&self) -> Option<(usize, usize)> {
        self.result.output_dimensions()
    }
    /// Whether the build accepted its primary output: built as a table, or
    /// deferred to be read as a stream (#3697).
    pub fn accepted(&self) -> bool {
        self.result.accepted()
    }
    /// The accepted output's columns, as positions among the accepted
    /// dataset's, which a deferred build's batches project.
    pub fn projection(&self) -> Option<&[usize]> {
        self.result.projection()
    }
    /// The run report table; every build holds one.
    pub fn run_report(&self) -> &crate::public_table::PublicTable {
        self.result.run_report()
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
        let diagnostic = report_only || !self.result.accepted();
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
                refusal.issues(Step::Output, Fault::PublicationReportLimit)
            }
            _ => rejected(Step::Output, Fault::PublicationReportLimit),
        })?;
        let fields = [("report", self.result.report_file()), ("path", !diagnostic)]
            .into_iter()
            .filter_map(|(field, present)| present.then_some(field))
            .collect();
        let files = targets
            .iter()
            .zip(&items)
            .map(
                |(target, (written, bytes, partitions, _))| crate::publication::File {
                    target,
                    written,
                    bytes,
                    partitions,
                },
            )
            .collect();
        let written = items
            .iter()
            .map(|(written, _, _, _)| (*written).to_owned())
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
        let field = format!("output.{}", fields[file]);
        self.publication_issues = match cause {
            // The save never entered this file: its target is refused.
            Cause::Target(error) => {
                attempt.entered = 0;
                let issues = target_failure(&error, Some(&field), &written[file]);
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
                let issues = at(
                    target_failure(&error, Some(&field), &written[file]),
                    attempt,
                );
                attempt.failure = Some(error);
                issues
            }
            // A save cancelled between two partitions of the file (D264); a
            // facade save fails as cancelled instead and records nothing.
            Cause::Cancelled => {
                attempt.entered = file + 1;
                let error =
                    crate::file_publication::Error::Host(crate::host_resources::Failure::Cancelled);
                let issues = at(publication_failure(&error), attempt);
                attempt.failure = Some(error);
                issues
            }
            Cause::Unwind(payload) => {
                attempt.entered = file + 1;
                let issues = at(rejected(Step::Output, Fault::PublicationUnwind), attempt);
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
        self.save_with(false, |_, _| Ok(()), |_| Ok(()))
    }
    /// Publish the declared report file alone, as a failed build saves, even
    /// when the build succeeded. `Err` means there is no report file to write.
    pub fn save_report(&mut self) -> Result<bool, FailedBuild> {
        if !self.result.report_file() {
            return Err(FailedBuild);
        }
        let failed = !self.result.accepted();
        match self.save_with(true, |_, _| Ok(()), |_| Ok(())) {
            // A failed build reports its failure after the report was written.
            Err(FailedBuild) if failed => Ok(self.publication_issues.is_empty()),
            outcome => outcome,
        }
    }
    /// [`Self::save`], or [`Self::save_report`] with `report_only`, calling
    /// `between` before each partition of a file after its first, with the
    /// file's and the partition's positions ([`crate::publication::stage_with`]),
    /// and `before` with each file's position before it is committed.
    pub(crate) fn save_with(
        &mut self,
        report_only: bool,
        between: impl FnMut(usize, usize) -> Result<(), crate::publisher::Interruption>,
        before: impl FnMut(usize) -> Result<(), crate::file_publication::Error>,
    ) -> Result<bool, FailedBuild> {
        let failed = !self.result.accepted();
        let fail = || if failed { Err(FailedBuild) } else { Ok(false) };
        let (staged, admitted) = match self.plan(report_only) {
            Ok(None) => return fail(),
            Ok(Some((files, admitted))) => {
                (crate::publication::stage_with(&files, between), admitted)
            }
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
/// project functions it calls through `functions`, the build's host-functions
/// port, read its inputs from the scope, execute, and report. A specification
/// alone and a member of a project run take this one route; Save targets
/// resolve in the scope. `classify` reads the retained failures of
/// `functions`. The attempt is returned beside the result for the host
/// failures it holds.
///
/// Through the empty port, which keeps no project functions, a build that
/// calls one is refused before it reads any input (#3030 D110, D123), as one
/// `unsupported_operation` issue with operation `environment` at the first
/// derivation that calls one (D221).
pub fn run<A: HostFunctions>(
    prepared: &FileSpecification,
    run: &mut crate::file_resources::RunScope,
    functions: &mut A,
    classify: impl Fn(&A::Error) -> Classification,
    identity: Identity<'_>,
    scope: Scope<'_>,
) -> (
    Attempt<crate::file_resources::Error, A::Error>,
    Result<Domain, Failure>,
) {
    run_with_view(
        prepared,
        run,
        functions,
        classify,
        identity,
        scope,
        HostView::Table,
    )
}

/// [`run`] with the host view `view` (#3697). A [`HostView::Deferred`]
/// result builds no output table; its accepted rows stay in the attempt, from
/// which [`take_dataset`] moves them to be read as a stream.
pub fn run_with_view<A: HostFunctions>(
    prepared: &FileSpecification,
    run: &mut crate::file_resources::RunScope,
    functions: &mut A,
    classify: impl Fn(&A::Error) -> Classification,
    identity: Identity<'_>,
    scope: Scope<'_>,
    view: HostView,
) -> (
    Attempt<crate::file_resources::Error, A::Error>,
    Result<Domain, Failure>,
) {
    let attempt = prepared.build(run, functions, scope);
    if let Err(BoundaryFailure::Activation(
        yamaa_core::project_activation::Failure::FunctionLanguageMismatch {
            selected: None, ..
        },
    )) = &attempt.boundary
    {
        let derivation = prepared.run().compiled().first_calling_derivation();
        return (attempt, Err(unsupported("environment", derivation)));
    }
    if let Some(error) = file_preparation::opaque_resource_failure(&attempt.dataset) {
        let failure = resource_failure(Step::Capture, error);
        return (attempt, Err(failure));
    }
    scope.phase(yamaa_core::application_observer::Phase::Output);
    let converted = result(prepared, run, &attempt, classify, identity, view);
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
    view: HostView,
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
    let result = specification_report::build_result_with(
        prepared.run(),
        attempt,
        identity,
        &facts,
        view,
        &run.input_aliases().collect::<Vec<_>>(),
    )
    .map_err(|refusal| refusal.issues())?;
    Ok(Domain::new(result, prepared, run))
}

/// Move the accepted dataset out of `attempt`, for a deferred build whose
/// output is read as a stream (#3697), with the most rows one partition of
/// it holds when the run admitted it a partition at a time (D264); none when
/// the attempt accepted no dataset. The attempt then holds no execution
/// result, so read whatever else it holds, such as its host failures, first.
pub fn take_dataset<C, E>(
    attempt: &mut Attempt<C, E>,
) -> Option<(yamaa_core::dataset::Dataset, Option<std::num::NonZeroUsize>)> {
    let result = std::mem::replace(
        &mut attempt.dataset.result,
        Err(yamaa_core::specification_run::PortError::Incomplete),
    );
    match result {
        Ok(yamaa_core::dataset::ExecutionAttempt {
            result: Ok(execution),
            ..
        }) => Some((execution.dataset, execution.partition_rows)),
        other => {
            attempt.dataset.result = other;
            None
        }
    }
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
    check_located(host, request, resolver).0
}
/// [`check_with_resolver`], with what locates its issues (P-ISSUE-1): the
/// specification as far as it was read and the project files read.
pub fn check_located(
    host: Option<Language>,
    request: Request<'_>,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
) -> (Failure, crate::issue_location::Locator) {
    located(select(host, request, resolver))
}
/// [`check_located`] of a fragment checked on the parent a caller names,
/// `parent` as given (#3694): the issues of the fragment composed as it would
/// be in a file that names that parent, includes the fragment and writes
/// nothing else, with what locates them in the fragment and the parent's
/// layers. A specification that declares `parent` or `include` is no
/// fragment: the given parent is refused, as `Err` with the issue that says
/// so (LAYER-4).
pub fn check_fragment_located(
    host: Option<Language>,
    request: Request<'_>,
    parent: &str,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
) -> Result<(Failure, crate::issue_location::Locator), Failure> {
    let selected = match resolver {
        Some(resolver) => {
            crate::file_workflow::select_fragment_with_resolver(request, parent, host, resolver)
        }
        None => crate::file_workflow::select_fragment(request, parent, host),
    };
    let (issues, locator) = located(selected);
    match issues.as_slice() {
        [refusal] if refusal.code == ConditionCode::InheritanceNotFragment => Err(issues),
        _ => Ok((issues, locator)),
    }
}
/// The issues of a selection, prepared or refused, with what locates them.
fn located(
    selected: Result<FileSpecification, crate::file_workflow::SelectionError>,
) -> (Failure, crate::issue_location::Locator) {
    match selected {
        Ok(prepared) => (
            collect(crate::specification_check::issues(prepared.run())),
            crate::issue_location::prepared(&prepared),
        ),
        Err(error) => (
            collect(crate::project_application::selection_failure(&error)),
            crate::issue_location::selection(&error),
        ),
    }
}
/// The type the file `written` names declares, read before any call reads
/// it: what the command line dispatches on (#3030 D122). The file is
/// captured once, through `resolver` or else through the local file resolver
/// from its parent directory, so a path that reaches no regular file, or
/// reaches a link, fails as any read does (FILE-7, FILE-8), naming the path
/// as given (FILE-11). Its header is read as [`crate::document::file_type`]
/// reads it; a refused header, or text that is not YAML, reports the issues
/// a call that reads the file as its entry reports (SCHEMA-2, SCHEMA-3).
pub fn file_type(
    written: &str,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
) -> Result<yamaa_core::language::FileType, Failure> {
    file_type_located(written, resolver).map_err(|(issues, _)| issues)
}
/// [`file_type`], whose refusal keeps what locates its issues: the file as
/// read (P-ISSUE-1).
pub fn file_type_located(
    written: &str,
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
) -> Result<yamaa_core::language::FileType, (Failure, crate::issue_location::Locator)> {
    let unreadable = |error| {
        (
            vec![crate::file_resources::caller_read_failure(written, &error)],
            crate::issue_location::Locator::none(),
        )
    };
    let (mut resources, name) = match resolver {
        Some(resolver) => (
            crate::file_resources::Resources::with_resolver(resolver, written),
            written.to_owned(),
        ),
        None => crate::file_configuration::resources(written)
            .map_err(|crate::file_configuration::Error::Path(error)| unreadable(error))?,
    };
    let (bytes, _) = resources.capture_metadata(&name).map_err(unreadable)?;
    let source = crate::specification_source::Source {
        identity: written.to_owned(),
        bytes: bytes.to_vec(),
    };
    crate::document::file_type(source, yamaa_core::language::LANGUAGE_VERSION).map_err(|error| {
        let mut locator = crate::issue_location::Locator::none();
        locator.sources(&resources.retain_metadata_sources());
        (
            collect(crate::specification_diagnostics::capture_failure(&error).into_issues()),
            locator,
        )
    })
}
#[cfg(test)]
mod collect_tests {
    use super::*;

    #[test]
    fn the_findings_limit_keeps_the_first_ten_thousand_across_stages() {
        use yamaa_core::application_issue::{rejected, Fault, Step};
        fn row(index: usize) -> Diagnostic {
            let step = if index < 6_000 {
                Step::Ingest
            } else {
                Step::Output
            };
            let mut finding = rejected(step, Fault::Internal);
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
    use yamaa_core::host_functions::Empty;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    /// Prepare and run one request on the route, through the empty port of an
    /// engine that supplies no project functions, with phases observed in
    /// `scope`.
    fn domain_observed(
        request: Request<'_>,
        identity: Identity<'_>,
        scope: Scope<'_>,
    ) -> Result<Domain, Failure> {
        yamaa_core::domain_entry::domain(
            scope,
            || {
                select(Some(Language::Python), request, None)
                    .map_err(|error| crate::project_application::selection_failure(&error))
            },
            |prepared| {
                let mut scoped = prepared.run_scope(None);
                let mut empty = Empty::<std::convert::Infallible>::new();
                let classify = |_: &_| Classification::Opaque;
                run(prepared, &mut scoped, &mut empty, classify, identity, scope).1
            },
        )
        .map_err(collect)
    }
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
        fs::write(&entry, "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input/input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE], decimals: 18, report: checks.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: float\n    derivation: SRC.VALUE\n    verifications:\n      - not_missing\n      - range: {max: 0, severity: warning}\n").unwrap();
        let written = entry.to_str().unwrap();
        let (mut resources, entry) = file_configuration::resources(written).unwrap();
        let document = crate::file_preparation::prepare_document(&mut resources, &entry).unwrap();
        let prepared = crate::specification_run::PreparedRun::prepare(document).unwrap();
        let mut empty = Empty::<std::convert::Infallible>::new();
        let attempt = prepared.execute_with_ports(&mut empty, &mut resources, Scope::default());
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
                .unwrap();
            resources
                .publication_target_from(origin.base_source, origin.written)
                .map_err(|error| resource_failure(Step::Output, &error))
        });
        assert!(result.output().is_some());
        assert!(result.result.publication_items().last().unwrap().1.len() > 8 * 1024 * 1024);
        // One run, input and base row, two column rows and two check rows.
        assert_eq!(result.run_report().rows(), 7);
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
        let ordinary = domain_observed(request(), identity(), Scope::default()).unwrap();
        let result = if let Some(measurements) = measurements {
            let observed = domain_observed(request(), identity(), Scope::new(measurements))
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
            .save_with(
                false,
                |_, _| Ok(()),
                |_| Err(crate::file_publication::Error::Io(std::io::Error::other(
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
                    .map(|(_, _, _, value)| value.clone())
                    .collect(),
            );
            let marker = Arc::new(());
            let mut entered = 0;
            assert!(!result
                .save_with(
                    false,
                    |_, _| Ok(()),
                    |index| {
                        entered += 1;
                        if index == fail_at {
                            return Err(crate::file_publication::Error::Io(std::io::Error::other(
                                Marker(Arc::clone(&marker)),
                            )));
                        }
                        Ok(())
                    }
                )
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
        let columns = result.run_report().names().collect::<Vec<_>>();
        assert_eq!(columns, yamaa_core::run_report::COLUMNS);
        let original = result.observations();
        let marker = Arc::new(());
        assert!(result
            .save_with(
                false,
                |_, _| Ok(()),
                |_| {
                    Err(crate::file_publication::Error::Io(std::io::Error::other(
                        Marker(Arc::clone(&marker)),
                    )))
                }
            )
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
                .map(|(_, _, _, value)| value.clone())
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
            .save_with(
                false,
                |_, _| Ok(()),
                |_| panic!("linked parent entered publisher")
            )
            .unwrap());
        assert_eq!(result.publication_attempt.entered, 0);
        assert_eq!(result.publication_attempt.completed, 0);
        assert!(matches!(
            result.publication_attempt.failure,
            Some(crate::file_publication::Error::Unreachable(
                yamaa_core::resource::ResourceFailure::Symlink
            ))
        ));
        // FILE-8 at the field that declares the file, naming it as written.
        let issue = result.issues().pop().unwrap();
        assert_eq!(
            issue.code,
            yamaa_core::diagnostic::ConditionCode::PublicationTargetSymlink
        );
        assert_eq!(issue.spec_paths, ["output.report"]);
        assert_eq!(
            Value::Object(issue.context),
            crate::json!({"path": "nested/adsl-checks.csv"})
        );
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
            .save_with(
                false,
                |_, _| Ok(()),
                |_| {
                    entered += 1;
                    if entered == 2 {
                        std::panic::panic_any(Arc::clone(&marker));
                    }
                    Ok(())
                }
            )
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
                "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: out.csv, columns: [{}]{} }}\ncolumns:\n",
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
            let document =
                crate::file_preparation::prepare_document(&mut resources, &entry).unwrap();
            let prepared = crate::specification_run::PreparedRun::prepare(document).unwrap();
            let mut empty = Empty::<std::convert::Infallible>::new();
            let attempt = prepared.execute_with_ports(&mut empty, &mut resources, Scope::default());
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
            let public = result.output().unwrap();
            assert_eq!(public.width(), 40);
            assert_eq!(public.rows(), 1);
            let domain = Domain::from_build(result, |field| {
                let origin = prepared
                    .document()
                    .written_output_field_origin(field)
                    .unwrap();
                resources
                    .publication_target_from(origin.base_source, origin.written)
                    .map_err(|error| resource_failure(Step::Output, &error))
            });
            let mut domain = domain;
            assert!(domain.output().is_some());
            assert!(domain.issues().is_empty());
            assert_eq!(domain.result.report_file(), verification);
            // One run, input and base row and one row for each of the 40 columns.
            assert_eq!(domain.run_report().rows(), 43);
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

    /// A save that stops between two partitions of a run's output (D264) on
    /// the file publisher publishes nothing, stopped by an I/O failure there,
    /// as a full disk would give, which no facade input reaches, or by a
    /// cancellation. The file publisher removes the output's partial staged
    /// file, through `Writing::fail` for the failure and as the unfinished
    /// `Writing` is dropped for the cancellation, and the save removes the
    /// staged report, so the directory holds only what it held. The save's
    /// one issue is the publication failure, or the cancelled publication,
    /// at the output, in the output step, after entering both files and
    /// completing neither. Its three records are three one-record
    /// partitions: the save asks before the output's second and third, and
    /// stops at the third, asking nothing after it. The same result then
    /// saves whole.
    #[test]
    fn a_failure_or_a_cancellation_between_two_partitions_of_the_output_publishes_nothing() {
        use crate::publisher::Interruption;
        for (label, cancelled, fault) in [
            ("an I/O failure", false, Fault::PublicationFailed),
            ("a cancellation", true, Fault::PublicationCancelled),
        ] {
            let root = std::env::temp_dir().join(format!(
                "yamaa-partitioned-save-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            let study = Study(crate::canonical_path::canonicalize(root).unwrap());
            fs::write(study.0.join("input.csv"), "ID,NAME\n1,a\n2,b\n3,c\n").unwrap();
            fs::write(
                study.0.join("spec.yaml"),
                "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID, NAME], report: report.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: NAME, type: str, derivation: SRC.NAME}\n",
            )
            .unwrap();
            let entry = study.0.join("spec.yaml").to_str().unwrap().to_owned();
            let partitions = yamaa_core::dataset::Partitioning {
                rows: std::num::NonZeroUsize::MIN,
                leading_keys: None,
                whole_table_fallback: false,
                whole_output: false,
            };
            let mut result = domain_observed(
                Request {
                    specification: &entry,
                    environment: None,
                },
                Identity::new("test", "test", "partitioned", &entry),
                Scope::default().partitioned(Some(partitions)),
            )
            .unwrap();
            let mut asked = Vec::new();
            let saved = result.save_with(
                false,
                |file, partition| {
                    asked.push((file, partition));
                    match (file, partition) {
                        (1, 2) if cancelled => Err(Interruption::Cancelled),
                        (1, 2) => Err(Interruption::Failed(crate::file_publication::Error::Io(
                            std::io::Error::other("disk full"),
                        ))),
                        _ => Ok(()),
                    }
                },
                |_| Ok(()),
            );
            let mut entries = fs::read_dir(&study.0)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>();
            entries.sort();
            let issues = result
                .issues()
                .iter()
                .map(|issue| {
                    (
                        issue.code,
                        issue.spec_paths.clone(),
                        crate::json_wire::encode_object(&issue.context),
                    )
                })
                .collect::<Vec<_>>();
            // The interruption's publication fault, by the registry's code.
            let context = format!(
                r#"{{"code":"{}","completed":0,"entered":2,"path":"out.csv","step":"output"}}"#,
                fault.as_str()
            );
            assert_eq!(
                (saved.ok(), asked, issues, entries),
                (
                    Some(false),
                    vec![(1, 1), (1, 2)],
                    vec![(
                        ConditionCode::ApplicationRejected,
                        vec!["output.path".to_owned()],
                        context,
                    )],
                    vec!["input.csv".to_owned(), "spec.yaml".to_owned()],
                ),
                "{label}: the save stopped before the output's third partition"
            );
            assert!(result.save().unwrap(), "{label}: the later save");
            assert_eq!(
                fs::read_to_string(study.0.join("out.csv")).unwrap(),
                "ID,NAME\n1,a\n2,b\n3,c\n",
                "{label}: the output saved whole"
            );
        }
    }
}
