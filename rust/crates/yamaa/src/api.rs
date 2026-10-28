//! The host API both bindings expose: operations on a specification or a
//! project path. [`Engine::spec`] and [`Engine::project`] give one request
//! for each, a [`SpecRequest`] or a [`ProjectRequest`], which holds what a
//! host's lazy object holds and reads nothing until one of its operations
//! runs. Every operation rereads the files, and every result is a fixed
//! snapshot that writes nothing until `save`. A lazy object also keeps a
//! [`Plans`] holder, which its requests reuse, so a later run reuses what an
//! earlier run compiled once the run-start check finds the files unchanged.
//!
//! Every operation produces one outcome: its members in run order and the
//! project-level issues that refused the whole operation. A specification run
//! is a producer graph of one member; a project run is a graph of every
//! specification in its scope, producer first. [`Check`], [`Run`],
//! [`ProjectRun`] and [`Submission`] are views of the outcome, and each
//! computes its [`Status`] from the issues and the build, never storing it.
//! Each prints its [`Summary`] (D37), and a refusal's message is D39's error
//! text, both rendered by [`crate::text`].

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use yamaa_core::application_issue::{Fault, Step};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::project_environment::Submission as Section;

use crate::cancel::{CancellationToken, Cancelled};
use crate::diagnostic::{context_value_from_core, ContextValue, Locator, Report};
use crate::engine::{Build, Engine, Request};
use crate::error::Failure;
use crate::outcome::{fails, DefineBuild, Member, Outcome, Work};
use crate::plan::Plan;
use crate::progress::{Measured, Reporter, Timings};
#[cfg(feature = "arrow")]
use crate::stream::Streamed;
use crate::table::Table;
use crate::text::{self, Action, Head, MemberLine, Summary};

/// The status of a result or of one member of it (P-STATUS-1). Rust computes
/// it from the issues and the build; it is never stored. A warning never
/// changes it, and `status` describes the operation, not a later save.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Everything the operation set out to build was built, with no
    /// error-severity issue.
    Succeeded,
    /// Anything else.
    Failed,
    /// A project-run member that did not run because a run-scope producer
    /// failed or was skipped (PROJECT-11).
    Skipped,
}

/// The statuses as written, one for each [`Status`] in declaration order: the
/// closed vocabulary of every `status` a host returns (P-STATUS-1).
pub const STATUSES: [&str; 3] = ["succeeded", "failed", "skipped"];

/// The sections a project file declares, as written, in run order: the closed
/// vocabulary of the `section` that [`ProjectRequest::section`] takes
/// (PROJECT-3).
pub const SECTIONS: [&str; 3] = ["sdtm", "adam", "send"];

impl Status {
    /// The status as written: `succeeded`, `failed` or `skipped`, one of
    /// [`STATUSES`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    /// `succeeded` exactly when every member succeeded and no issue fails
    /// the whole operation.
    pub(crate) fn of_all(issues: &[Diagnostic], members: impl IntoIterator<Item = Status>) -> Self {
        if !fails(issues) && members.into_iter().all(|status| status == Self::Succeeded) {
            Self::Succeeded
        } else {
            Self::Failed
        }
    }
}

/// The static check of a specification or a project: what is wrong, read
/// without study data or project code.
pub struct Check {
    outcome: Outcome,
    /// The path as given.
    subject: String,
}

impl Check {
    /// The check of the one specification at `subject`, the path as given,
    /// whose `issues` `locator` locates.
    /// `measured` holds the time its preparation took, and `reporter` says
    /// whether its progress callback failed.
    fn of(
        subject: &str,
        (issues, locator): (Vec<yamaa_core::diagnostic::Diagnostic>, Arc<Locator>),
        measured: Measured,
        reporter: &Reporter,
    ) -> Self {
        Self {
            outcome: Outcome {
                members: vec![Member {
                    name: None,
                    identity: None,
                    work: Work::Checked(issues, locator),
                    measured: Some(measured),
                }],
                issues: progress_failure(reporter, Step::Check),
                locator: Arc::new(Locator::none()),
            },
            subject: subject.to_owned(),
        }
    }

    /// What is wrong, one row per finding; empty when nothing is. A project
    /// check lists the project findings first, then each specification's.
    pub fn issues(&self) -> Table {
        self.outcome.report().table()
    }

    /// The issues as typed records, in the table's order.
    pub fn diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        self.outcome.report().diagnostics
    }

    /// `succeeded` when the check found no error-severity issue.
    pub fn status(&self) -> Status {
        self.outcome.status()
    }

    /// The time each specification's preparation took, in the order the
    /// check read them (#3762).
    pub fn timings(&self) -> Timings {
        self.outcome.timings()
    }

    /// The printed text (D37, P-PRINT-1): `yamaa check of <path>: <status>,
    /// <n> issues`, naming the path as given, then the first issue.
    pub fn summary(&self) -> Summary {
        let head = Head {
            kind: "check",
            subject: &self.subject,
            status: self.status(),
            size: None,
        };
        text::summary(head, &[], &self.diagnostics())
    }
}

/// One specification run, or one member of a project run: its output
/// dataset, issues and run report.
pub struct Run {
    pub(crate) build: Build,
}

impl Run {
    /// The output dataset, missing when the run failed.
    pub fn output(&self) -> Option<&Table> {
        self.build.output()
    }

    /// What is wrong, one row per finding; empty when nothing is.
    pub fn issues(&self) -> Table {
        self.build.report().table()
    }

    /// The issues as typed records, in the table's order.
    pub fn diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        self.build.report().diagnostics.clone()
    }

    /// What the run read, counted and checked; missing when the run never
    /// began or, in a producer graph, when the entry never ran.
    pub fn report(&self) -> Option<&Table> {
        self.build.run_report()
    }

    /// `succeeded` when the run built its output with no error-severity
    /// issue; `skipped` for a project-run member that did not run because a
    /// run-scope producer failed or was skipped; `failed` otherwise.
    pub fn status(&self) -> Status {
        self.build.status()
    }

    /// The time each phase of the run took, in the order the phases started
    /// (#3762): from `preparation`, which a run of a kept plan skips, to
    /// `output`. No saved file holds them.
    pub fn timings(&self) -> Timings {
        self.build.timings()
    }

    /// The printed text (D37, P-PRINT-1): `yamaa run of <subject>: <status>,
    /// <size>, <n> issues`, then the first issue. The subject is a project
    /// run member's name as [`ProjectRun::members`] gives it, or the dataset
    /// name once read, or else the path as given; the size is `<r> rows x
    /// <c> columns`, or `no output`.
    pub fn summary(&self) -> Summary {
        let head = Head {
            kind: "run",
            subject: self.build.subject(),
            status: self.status(),
            size: Some(text::output_size(self.build.dimensions())),
        };
        text::summary(head, &[], &self.build.report().diagnostics)
    }

    /// Write the declared report file, then the output dataset (OUTPUT-10),
    /// and return the paths written in that order (D38). A failed run writes
    /// only its report file and fails with the run's issues; a refusal carries
    /// the paths written before it (P-API-1). See [`ProjectRun::save`] for the
    /// one save every result shares, and for how it polls `token`.
    pub fn save(&mut self, token: &CancellationToken) -> Result<Vec<String>, Failure> {
        self.build.save(token)
    }

    /// What the run observed, as JSON: what it read and what it built, the
    /// project functions it called and the files its saves wrote; missing
    /// when the run never began. This is installed-run evidence: the Python
    /// and R qualification suites read it through the engine to prove a run
    /// against its authored report. Its members are not part of the
    /// interface.
    pub fn observations(&self) -> Option<String> {
        self.build.observations()
    }

    /// The run's [`observations`](Run::observations) beside the measurements
    /// its operation took, as one JSON document: the time of each stage, the
    /// project-function calls and save attempts it counted, and how the run
    /// got its plan, which the hosts' plan-reuse suites read. Like the
    /// observations, it is installed-run evidence, and its members are not
    /// part of the interface.
    pub fn measured_observations(&self) -> String {
        self.build.measured_observations()
    }
}

/// The issue that fails an operation whose progress callback failed, at its
/// `step`, or none (#3762).
fn progress_failure(reporter: &Reporter, step: Step) -> Vec<Diagnostic> {
    match reporter.failed() {
        true => vec![Reporter::failure(step)],
        false => Vec::new(),
    }
}

fn request<'a>(specification: &'a str, project: Option<&'a str>) -> Request<'a> {
    match project {
        Some(project) => Request::new(specification).with_environment(project),
        None => Request::new(specification),
    }
}

/// The identities of a specification and of its project file, resolved as
/// compilation resolves them: what a kept plan was compiled from.
pub(crate) type PlanKey = (String, Option<String>);

/// The compiled plans a host's lazy object keeps between runs (#2821), one
/// for each specification and project identity its runs compiled.
///
/// The run of a request given a holder with [`SpecRequest::reusing`] or
/// [`ProjectRequest::reusing`] first resolves the request's paths exactly as
/// compilation does: absolute against the current working directory, with
/// each parent directory canonical. A kept plan is reused only when those
/// identities equal the ones it was compiled from, so a relative path after
/// a working-directory change, or a path through a retargeted link, compiles
/// anew (#3069). Its run-start check must then find every file it was
/// compiled from unchanged: the specification, its layers, dictionaries and
/// linked producing specifications, the project file, and the lock, function
/// files and codelist files it declares. Otherwise the run compiles again,
/// exactly as a one-shot run does, and keeps the new plan; a refused
/// compilation keeps nothing. Every run reads its inputs and activates the
/// project functions afresh, so a reused run differs from a one-shot run
/// only in time. A request given no holder runs through an empty one: each
/// of its runs is a one-shot run.
///
/// A kept plan holds no project functions or observer: it runs through the
/// calling engine's. A holder is `Send` and `Sync`. Its lock is held only to
/// read or replace a kept plan, never across a compilation, a run or a
/// callback, and a replaced plan is dropped after the lock is released.
#[derive(Default)]
pub struct Plans {
    kept: Mutex<BTreeMap<PlanKey, Plan>>,
}

impl Plans {
    /// An empty holder: the first run of each specification compiles it.
    pub fn new() -> Self {
        Self::default()
    }

    fn kept(&self) -> MutexGuard<'_, BTreeMap<PlanKey, Plan>> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The plan kept for `identity`, if any.
    pub(crate) fn get(&self, identity: &PlanKey) -> Option<Plan> {
        self.kept().get(identity).cloned()
    }

    /// Keep `plan` for `identity`, or forget what `identity` kept when there
    /// is no plan to keep or its compilation was refused.
    pub(crate) fn replace(&self, identity: PlanKey, plan: Option<&Plan>) {
        let replaced = {
            let mut kept = self.kept();
            match plan.filter(|plan| plan.report().is_empty()) {
                Some(plan) => kept.insert(identity, plan.detached()),
                None => kept.remove(&identity),
            }
        };
        drop(replaced);
    }

    /// Forget every kept plan whose identity `keep` rejects.
    pub(crate) fn retain(&self, keep: impl Fn(&PlanKey) -> bool) {
        let dropped = {
            let mut kept = self.kept();
            let rejected = kept
                .keys()
                .filter(|identity| !keep(identity))
                .cloned()
                .collect::<Vec<_>>();
            rejected
                .into_iter()
                .filter_map(|identity| kept.remove(&identity))
                .collect::<Vec<_>>()
        };
        drop(dropped);
    }

    #[cfg(all(test, feature = "yaml"))]
    pub(crate) fn identities(&self) -> Vec<PlanKey> {
        self.kept().keys().cloned().collect()
    }
}

/// One member of a producer graph: the specification to build, with its
/// project, and what decides whether it builds.
struct Node<'a> {
    /// The specification's name in its project, when it has one.
    name: Option<String>,
    /// What a project run names the member by: its listed name, or the path
    /// the project file lists when the listing could not be read. A
    /// specification run has none and names its dataset or the path as
    /// given.
    label: Option<String>,
    identity: Option<String>,
    request: Request<'a>,
    /// What the request resolves to now, for a kept plan.
    plan: Option<PlanKey>,
    /// Findings that stop the member before it builds.
    refused: Vec<Diagnostic>,
    /// The producers it reads, by node, each with the finding the member
    /// reports when that producer did not complete.
    producers: Vec<(usize, Diagnostic)>,
    /// The producer identity and location under which consumers read the
    /// member's artifact.
    artifact: Option<(String, String)>,
    /// What locates the findings that stop the member before it builds: the
    /// project's documents, or none for a specification run.
    locator: Arc<Locator>,
}

impl Engine {
    /// The request of the specification `path`, which reads nothing until
    /// one of its operations runs, as a host's specification object reads
    /// nothing until a method runs. Its builders name the project file the
    /// specification is read with, the [`Plans`] holder its runs reuse and
    /// the [`CancellationToken`] its operations poll.
    pub fn spec<'a>(&'a self, path: &'a str) -> SpecRequest<'a> {
        SpecRequest {
            engine: self,
            path: Cow::Borrowed(path),
            project: None,
            plans: None,
            token: None,
        }
    }

    /// The request of the project file `path`, which reads nothing until one
    /// of its operations runs, as a host's project object reads nothing
    /// until a method runs. Its builders name the section its run and
    /// submission take, the [`Plans`] holder its runs reuse and the
    /// [`CancellationToken`] its operations poll.
    pub fn project<'a>(&'a self, path: &'a str) -> ProjectRequest<'a> {
        ProjectRequest {
            engine: self,
            path,
            section: None,
            plans: None,
            token: None,
        }
    }

    /// Build every member of a producer graph once, in order. A producer
    /// hands its artifact to its consumers in memory; a member whose own
    /// findings refuse it, or whose producer did not complete, does not
    /// build. Only cancellation fails the call: `token` is polled before
    /// each member and as each builds, and a user interrupt in project code
    /// cancels the call too (D46), so no later member builds or calls
    /// project code. Each build reports its progress to `reporter`, and once
    /// its callback has failed no later member builds (#3762).
    fn run_graph(
        &self,
        nodes: Vec<Node<'_>>,
        plans: &Plans,
        reporter: &Reporter,
        token: &CancellationToken,
    ) -> Result<Outcome, Cancelled> {
        let mut held = yamaa_adapters::file_resources::HeldArtifacts::new();
        let mut succeeded = vec![false; nodes.len()];
        let mut members = Vec::with_capacity(nodes.len());
        for (position, node) in nodes.into_iter().enumerate() {
            if token.is_cancelled() {
                return Err(Cancelled);
            }
            let blocked = node
                .producers
                .into_iter()
                .filter(|(producer, _)| !succeeded[*producer])
                .map(|(_, finding)| finding)
                .collect::<Vec<_>>();
            if reporter.failed() {
                break;
            }
            let build = if !node.refused.is_empty() {
                self.refused_build(node.refused, node.request.specification, node.locator)
            } else if !blocked.is_empty() {
                self.refused_build(blocked, node.request.specification, node.locator)
            } else {
                let name = node.label.as_deref().unwrap_or(node.request.specification);
                self.build_reusing(
                    plans,
                    &node.request,
                    node.plan,
                    Some(Arc::new(held.clone())),
                    &reporter.observe(name),
                    token,
                )?
            };
            let artifact = build.artifact();
            succeeded[position] = artifact.is_some();
            if let (Some(bytes), Some((producer, location))) = (artifact, node.artifact) {
                held.insert(producer, location, bytes);
            }
            // A project run names each member by its label.
            let build = match &node.label {
                Some(label) => build.with_subject(label),
                None => build,
            };
            members.push(Member {
                name: node.name,
                identity: node.identity,
                work: Work::Built(Box::new(Run { build })),
                measured: None,
            });
        }
        Ok(Outcome {
            members,
            issues: Vec::new(),
            locator: Arc::new(Locator::none()),
        })
    }
}

/// One specification's operations: its path and the project file it is read
/// with, the [`Plans`] holder its runs reuse and the [`CancellationToken`]
/// its operations poll, which [`Engine::spec`] or [`ProjectRequest::spec`]
/// gives and the builders below add to. A request reads nothing until an
/// operation runs, and each operation rereads the files, so one request can
/// run any of them, any number of times.
///
/// Without [`SpecRequest::project`] the specification is read alone; without
/// [`SpecRequest::reusing`] each run compiles it, through an empty holder;
/// without [`SpecRequest::cancel_with`] its operations poll a token that
/// nothing cancels. [`SpecRequest::check`], [`SpecRequest::check_on`],
/// [`SpecRequest::run`] and [`SpecRequest::stream`] poll the token;
/// [`SpecRequest::resolve`] polls none and keeps no plan.
#[non_exhaustive]
#[must_use = "a request reads nothing until one of its operations runs"]
#[derive(Clone)]
pub struct SpecRequest<'a> {
    engine: &'a Engine,
    /// The specification's path: as given, or the listed specification's
    /// path resolved against the project file.
    path: Cow<'a, str>,
    project: Option<&'a str>,
    plans: Option<&'a Plans>,
    token: Option<&'a CancellationToken>,
}

impl<'a> SpecRequest<'a> {
    /// The same request, read with the project file `project`: its
    /// codelists, functions and environment.
    pub fn project(mut self, project: &'a str) -> Self {
        self.project = Some(project);
        self
    }

    /// The same request, whose runs reuse the plans `plans` keeps, as a
    /// host's specification object runs: the first run compiles the
    /// specification and keeps its plan, and a later run reuses that plan
    /// when its paths still resolve to the files it was compiled from and the
    /// run-start check finds every one of them unchanged, and compiles again
    /// otherwise, so no run uses stale or moved files. The holder keeps this
    /// specification's plan alone. See [`Plans`].
    pub fn reusing(mut self, plans: &'a Plans) -> Self {
        self.plans = Some(plans);
        self
    }

    /// The same request, whose operations poll `token`: a cancelled
    /// operation fails with a
    /// [`FailureKind::Cancelled`](crate::FailureKind::Cancelled) failure, with no
    /// result and no issue. See [`CancellationToken`] for where an operation
    /// polls it.
    pub fn cancel_with(mut self, token: &'a CancellationToken) -> Self {
        self.token = Some(token);
        self
    }

    /// The specification's path: as given to [`Engine::spec`], or, for a
    /// request [`ProjectRequest::spec`] gives, the listed specification's
    /// path resolved against the project file, as checks and builds receive
    /// it.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The request as compilation and the check read it.
    fn request(&self) -> Request<'_> {
        request(&self.path, self.project)
    }

    /// The token the operations poll: the request's, or else one that
    /// nothing cancels. A clone shares the request's flag and probe.
    fn token(&self) -> CancellationToken {
        self.token.cloned().unwrap_or_default()
    }

    /// The progress of one operation, reported to the engine's callback.
    fn reporter(&self) -> Reporter {
        Reporter::new(self.engine.shared().progress.clone())
    }

    /// Check the specification without reading study data or running project
    /// code, with the project's codelists and functions when the request
    /// names a project. Admission and preparation failures are the check's
    /// issues, never a second error channel; only cancellation fails the
    /// call.
    ///
    /// # Example
    ///
    /// The specification's bytes enter through a memory resolver; no
    /// filesystem path is touched. Lanes without the `yaml` feature report
    /// issues for the same call.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use yamaa::{Engine, EngineOptions, HostLanguage, MemoryResolver};
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n"[..],
    /// ));
    /// let engine = Engine::new(
    ///     EngineOptions::new(HostLanguage::Python, "doctest".into(), "0".into())
    ///         .with_resolver(resolver),
    /// );
    /// let check = engine.spec("spec.yaml").check().unwrap();
    /// if cfg!(feature = "yaml") {
    ///     assert!(check.diagnostics().is_empty());
    /// } else {
    ///     assert!(!check.diagnostics().is_empty());
    /// }
    /// ```
    pub fn check(&self) -> Result<Check, Failure> {
        let reporter = self.reporter();
        let observed = reporter.observe(&self.path);
        let checked = crate::internal::run_check(
            self.engine.shared(),
            &self.path,
            self.project,
            &observed,
            &self.token(),
        )
        .map_err(|Cancelled| self.cancelled())?;
        Ok(Check::of(
            &self.path,
            checked,
            observed.measured().clone(),
            &reporter,
        ))
    }

    /// [`SpecRequest::check`] of a fragment, a specification that declares
    /// neither `parent` nor `include`, on the specification `parent` names
    /// (#3694). The fragment composes as it would in a file that names
    /// `parent` as its parent, includes the fragment and writes nothing else:
    /// the parent's layers first, then the fragment (LAYER-3). The fragment
    /// stands as the entry, so every issue is located in it or in a layer of
    /// the parent (P-ISSUE-1), and `parent` is a caller path like the
    /// request's: one that cannot be read is the check's issue, naming it as
    /// given at no field (FILE-1). A specification that declares `parent` or
    /// `include` refuses the given parent: the call fails as
    /// `cannot read <path>`, with `value_not_permitted` at `parent`
    /// (LAYER-4). Otherwise only cancellation fails the call.
    pub fn check_on(&self, parent: &str) -> Result<Check, Failure> {
        let reporter = self.reporter();
        let observed = reporter.observe(&self.path);
        let checked = crate::internal::run_check_on(
            self.engine.shared(),
            &self.path,
            self.project,
            parent,
            &observed,
            &self.token(),
        )?;
        Ok(Check::of(
            &self.path,
            checked,
            observed.measured().clone(),
            &reporter,
        ))
    }

    /// Build the specification in memory; nothing is written until `save`.
    /// Every adjudicated outcome is a [`Run`] carrying its output, issues and
    /// run report; only cancellation fails the call.
    ///
    /// # Example
    ///
    /// The specification bytes and one Arrow input table enter through a
    /// memory resolver, and Arrow tables and typed issues come out. Lanes
    /// without the `yaml` feature keep a run carrying issues but no output.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use arrow_array::RecordBatchIterator;
    /// use arrow_schema::Schema;
    /// use yamaa::{
    ///     CancellationToken, DataType, Engine, EngineOptions, FFI_ArrowArrayStream, Field,
    ///     Float64Array, HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch,
    /// };
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n"[..],
    /// ));
    /// let schema = Arc::new(Schema::new(vec![
    ///     Field::new("ID", DataType::Int64, true),
    ///     Field::new("VALUE", DataType::Float64, true),
    /// ]));
    /// let batch = RecordBatch::try_new(
    ///     Arc::clone(&schema),
    ///     vec![
    ///         Arc::new(Int64Array::from(vec![1, 2])),
    ///         Arc::new(Float64Array::from(vec![2.5, 3.5])),
    ///     ],
    /// )
    /// .unwrap();
    /// let frame = InputFrame::import(FFI_ArrowArrayStream::new(Box::new(
    ///     RecordBatchIterator::new(vec![Ok(batch)], schema),
    /// )))
    /// .unwrap();
    /// resolver.insert_frame("/spec.yaml", "input.csv", "data:1", &frame);
    /// let engine = Engine::new(
    ///     EngineOptions::new(HostLanguage::Python, "doctest".into(), "0".into())
    ///         .with_resolver(resolver.clone()),
    /// );
    /// let token = CancellationToken::new();
    /// let mut run = engine.spec("spec.yaml").cancel_with(&token).run().unwrap();
    /// if cfg!(feature = "yaml") {
    ///     assert_eq!(run.output().unwrap().rows(), 2);
    ///     assert!(run.diagnostics().is_empty());
    ///     assert_eq!(run.save(&token).unwrap(), ["memory:/out.csv"]);
    ///     assert!(resolver.published("memory:/out.csv").is_some());
    /// } else {
    ///     assert!(run.output().is_none());
    ///     assert!(!run.diagnostics().is_empty());
    /// }
    /// ```
    pub fn run(&self) -> Result<Run, Failure> {
        let once = Plans::new();
        let plans = self.plans.unwrap_or(&once);
        let request = self.request();
        let identity = crate::internal::request_identity(self.engine.shared(), &request);
        // A spec object names one specification: a plan kept for another
        // identity, such as the one its relative path named before the
        // working directory changed, is dropped.
        plans.retain(|kept| identity.as_ref() == Some(kept));
        let node = Node {
            name: None,
            label: None,
            identity: None,
            request,
            plan: identity,
            refused: Vec::new(),
            producers: Vec::new(),
            artifact: None,
            locator: Arc::new(Locator::none()),
        };
        let reporter = self.reporter();
        let outcome = self
            .engine
            .run_graph(vec![node], plans, &reporter, &self.token())
            .map_err(|Cancelled| self.cancelled())?;
        let member = outcome
            .members
            .into_iter()
            .next()
            .expect("a graph of one member builds it");
        match member.work {
            Work::Built(mut run) => {
                let failure = progress_failure(&reporter, Step::Build);
                if !failure.is_empty() {
                    run.build.fail(failure);
                }
                Ok(*run)
            }
            Work::Checked(..) | Work::Composed(..) => unreachable!("a graph builds its members"),
        }
    }

    /// [`SpecRequest::run`] with its output read as a stream (#3697): a
    /// [`Streamed`] run, which reads no study data and calls no project
    /// function until its output is read or its result asked for. The call
    /// compiles the specification, or reuses the plan the request's holder
    /// keeps, as [`SpecRequest::run`] does; a refused compilation's run has
    /// already ended, with its issues and no output. The run polls the
    /// request's token from then on: at every checkpoint of its schedule,
    /// and before each batch. See [`crate::stream`] for how its output is
    /// produced and how it ends.
    ///
    /// # Example
    ///
    /// The output of a specification served from memory, read as a stream,
    /// then saved. Lanes without the `yaml` feature get a run that ended
    /// with issues.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, MemoryResolver, Plans};
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [ID, NAME]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: NAME, type: str, derivation: SRC.NAME}\n"[..],
    /// ));
    /// resolver.insert("/spec.yaml", "input.csv", "input:1", Arc::from(&b"ID,NAME\n1,alpha\n2,beta\n"[..]));
    /// let engine = Engine::new(
    ///     EngineOptions::new(HostLanguage::Python, "doctest".into(), "0".into())
    ///         .with_resolver(resolver.clone()),
    /// );
    /// let (plans, token) = (Plans::new(), CancellationToken::new());
    /// let streamed = engine
    ///     .spec("spec.yaml")
    ///     .reusing(&plans)
    ///     .cancel_with(&token)
    ///     .stream()
    ///     .unwrap();
    /// if cfg!(feature = "yaml") {
    ///     // Nothing is read until the first pull.
    ///     assert!(resolver.published("memory:/out.csv").is_none());
    ///     let rows: usize = streamed.reader().map(|batch| batch.unwrap().num_rows()).sum();
    ///     assert_eq!(rows, 2);
    ///     let mut run = streamed.run().unwrap();
    ///     assert_eq!(run.summary().to_string(), "yamaa run of TEST: succeeded, 2 rows x 2 columns, 0 issues");
    ///     assert_eq!(run.save(&token).unwrap(), ["memory:/out.csv"]);
    /// } else {
    ///     assert!(streamed.schema().is_none());
    ///     assert!(!streamed.run().unwrap().diagnostics().is_empty());
    /// }
    /// ```
    #[cfg(feature = "arrow")]
    pub fn stream(&self) -> Result<Streamed, Failure> {
        let token = self.token();
        if token.is_cancelled() {
            return Err(self.cancelled());
        }
        let once = Plans::new();
        let plans = self.plans.unwrap_or(&once);
        let request = self.request();
        let identity = crate::internal::request_identity(self.engine.shared(), &request);
        // As a run reusing the holder keeps only this specification's plan.
        plans.retain(|kept| identity.as_ref() == Some(kept));
        let reporter = Arc::new(self.reporter());
        let observed = reporter.observe(&self.path);
        let (plan, compiled) =
            self.engine
                .plan_reusing(plans, &request, identity.as_ref(), &observed);
        if compiled {
            self.engine.keep(plans, &request, identity, &plan);
        }
        let measured = observed.measured().clone();
        drop(observed);
        Ok(Streamed::new(
            self.engine,
            plan,
            &self.path,
            (measured, reporter),
            &token,
        ))
    }

    /// The specification after its layers are composed, as one structured
    /// tree: the normalized model, each field's origin and each resource.
    pub fn resolve(&self) -> Result<ContextValue, Failure> {
        let resolved = self
            .engine
            .resolve_specification(&self.request())
            .map_err(|report| Failure::rejected(Action::Resolve(&self.path), &report))?;
        resolved_value(&self.path, resolved.to_value())
    }

    /// The cancellation of an operation that reads the specification:
    /// `cannot read <path>: operation cancelled`.
    fn cancelled(&self) -> Failure {
        Failure::cancelled(Action::Read(&self.path))
    }
}

impl std::fmt::Debug for SpecRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpecRequest")
            .field("path", &self.path)
            .field("project", &self.project)
            .field("reusing", &self.plans.is_some())
            .field("token", &self.token)
            .finish_non_exhaustive()
    }
}

/// The resolved tree as the artifact's codec reads it. Facts the engine
/// cannot represent are an engine fault, refused with `engine_rejected`.
fn resolved_value(
    specification: &str,
    tree: Option<yamaa_core::json::Value>,
) -> Result<ContextValue, Failure> {
    let tree = tree.ok_or_else(|| {
        Failure::from_issues(
            Action::Resolve(specification),
            &yamaa_application::file_application::rejected(
                Step::Resolve,
                Fault::ArtifactRepresentation,
            ),
        )
    })?;
    Ok(context_value_from_core(&tree))
}

/// The section a caller names, which must be one `read` declares. Any other
/// text, a name outside `sdtm`, `adam` and `send` included, refuses the call
/// as `cannot select <section>`, with `unknown_section` naming the sections
/// the project declares (PROJECT-14).
fn select(
    read: &yamaa_application::project_scope::Project,
    section: Option<&str>,
) -> Result<Option<Section>, Failure> {
    let Some(key) = section else {
        return Ok(None);
    };
    yamaa_core::project_scope::section(key)
        .filter(|section| read.sections().any(|declared| declared == *section))
        .map(Some)
        .ok_or_else(|| {
            let declared = read
                .sections()
                .map(yamaa_core::project_scope::key)
                .collect::<Vec<_>>();
            let finding = yamaa_core::project_scope::unknown_section(key, &declared);
            Failure::from_finding(Action::Select(key), finding)
        })
}

/// What a project operation's summary names: the project path as given,
/// then `, section <s>` when a section was named.
fn project_subject(project: &str, section: Option<&str>) -> String {
    match section {
        Some(section) => format!("{project}, section {section}"),
        None => project.to_owned(),
    }
}

/// Every specification of one project run scope, built once in run order.
pub struct ProjectRun {
    outcome: Outcome,
    subject: String,
}

impl ProjectRun {
    /// The names of the run's specifications, in run order. A listing that
    /// could not be read has no name and is left out;
    /// [`ProjectRun::members`] includes it.
    pub fn names(&self) -> Vec<String> {
        self.outcome
            .members
            .iter()
            .filter_map(|member| member.name.clone())
            .collect()
    }

    /// Every member of the run in run order, each with the name its member
    /// line prints: the specification's listed name or, for a listing that
    /// could not be read, which has none, its path as the project file
    /// writes it (D130). Only a listed name selects a member with
    /// [`ProjectRun::member`].
    pub fn members(&self) -> Vec<(&str, &Run)> {
        self.outcome
            .members
            .iter()
            .filter_map(Member::run)
            .map(|run| (run.build.subject(), run))
            .collect()
    }

    /// The run of the specification named `name`.
    pub fn get(&self, name: &str) -> Option<&Run> {
        self.outcome
            .members
            .iter()
            .find(|member| member.name.as_deref() == Some(name))
            .and_then(Member::run)
    }

    /// The run of the specification named `name`, or the refusal every host
    /// reports for a name the run does not have: `unknown_specification_name`,
    /// naming every name the run has (PROJECT-6).
    pub fn member(&self, name: &str) -> Result<&Run, Failure> {
        self.get(name).ok_or_else(|| {
            let names = self.names();
            let available = names.iter().map(String::as_str).collect::<Vec<_>>();
            Failure::from_finding(
                Action::Select(name),
                yamaa_core::project_scope::unknown_name(name, &available),
            )
        })
    }

    /// Every run's issues in run order, after any project finding that
    /// refused the run. Each names its specification as its `entry`.
    pub fn issues(&self) -> Table {
        self.outcome.report().table()
    }

    /// The issues as typed records, in the table's order.
    pub fn diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        self.outcome.report().diagnostics
    }

    /// The printed text (D37, P-PRINT-1): `yamaa project run of <path>[,
    /// section <s>]: <status>, <k> specifications, <n> issues`, then one line
    /// per member of [`ProjectRun::members`], then the first issue. A member
    /// line gives the name, the status, the size and the issue count, and a
    /// member's own summary is its run's, [`Run::summary`].
    pub fn summary(&self) -> Summary {
        let members = self
            .members()
            .into_iter()
            .map(|(name, run)| MemberLine {
                name: name.to_owned(),
                status: run.status(),
                size: text::output_size(run.build.dimensions()),
                issues: run.build.report().len(),
            })
            .collect::<Vec<_>>();
        let head = Head {
            kind: "project run",
            subject: &self.subject,
            status: self.status(),
            size: Some(text::specifications(self.outcome.members.len())),
        };
        text::summary(head, &members, &self.diagnostics())
    }

    /// The runs' reports stacked in run order; each row names its
    /// specification's artifact. A specification that did not run adds none.
    pub fn report(&self) -> Table {
        let reports = self
            .outcome
            .members
            .iter()
            .filter_map(|member| member.run()?.report().map(Table::public))
            .collect::<Vec<_>>();
        Table::from_public(
            yamaa_application::project_scope::stack_reports(&reports)
                .expect("every run report has the run report columns"),
        )
    }

    /// `succeeded` when every specification succeeded and no project finding
    /// refused the run; a run whose scope is empty succeeds.
    pub fn status(&self) -> Status {
        self.outcome.status()
    }

    /// The time each phase of each specification's run took, the
    /// specifications in run order, each named as [`ProjectRun::members`]
    /// names it (#3762). A specification that did not run adds none.
    pub fn timings(&self) -> Timings {
        self.outcome.timings()
    }

    /// Save the run as one save (PROJECT-12, D38, D74) and return the
    /// absolute paths written, with forward slashes, in publication order.
    ///
    /// The run plans every file first: in run order, each specification's
    /// declared report and then its primary artifact (OUTPUT-10), or, when any
    /// specification failed, only the report of each one that ran. The save
    /// then opens every target, so a target that cannot be written or two
    /// files that name one target refuse it before anything is written; it
    /// stages every file beside its target, and only then replaces each
    /// target in order. The first file that cannot replace its target stops
    /// the save there. When any specification failed, the save fails with
    /// every run's issues once the reports are written.
    ///
    /// A refusal carries its issues and the paths written before it
    /// (P-API-1). A run refused by a project finding fails with that finding,
    /// and a run that built no specification with `nothing_to_save`.
    ///
    /// The save polls `token` once, after every file is staged and before
    /// the first commit (D74), so a cancellation never stops it partway.
    /// Replacement is atomic per file, not across the save: when a file
    /// cannot replace its target, the files already replaced stay
    /// (OUTPUT-10, PROJECT-12). A cancelled save removes what it staged and
    /// fails with the cancellation's error text, `cannot save <subject>:
    /// operation cancelled`, with no issue and no path; `token` then reports
    /// itself cancelled, which tells a cancellation from a refusal.
    pub fn save(&mut self, token: &CancellationToken) -> Result<Vec<String>, Failure> {
        self.outcome
            .save(crate::outcome::Operation::ProjectRun, &self.subject, token)
    }
}

/// One Define-XML composition per section, with its Dataset-JSON files.
pub struct Submission {
    outcome: Outcome,
    subject: String,
    /// The creation time every section was composed with.
    created: String,
}

impl Submission {
    /// Every section's issues in section order, after any project finding.
    pub fn issues(&self) -> Table {
        self.outcome.report().table()
    }

    /// The issues as typed records, in the table's order.
    pub fn diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        self.outcome.report().diagnostics
    }

    /// The printed text (D37, P-PRINT-1): `yamaa submission of <path>[,
    /// section <s>]: <status>, <d> define.xml and <t> transport files, <n>
    /// issues`, then the first issue. The counts are the documents of the
    /// sections that composed: each holds one define.xml and its transport
    /// files.
    pub fn summary(&self) -> Summary {
        let (defines, transports) = self
            .outcome
            .members
            .iter()
            .filter_map(|member| match &member.work {
                Work::Composed(define) if define.composed().is_empty() => Some(define.transports()),
                _ => None,
            })
            .fold((0, 0), |(defines, transports), files| {
                (defines + 1, transports + files)
            });
        let head = Head {
            kind: "submission",
            subject: &self.subject,
            status: self.status(),
            size: Some(text::submission_size(defines, transports)),
        };
        text::summary(head, &[], &self.diagnostics())
    }

    /// `succeeded` when every selected section composed with no issue and no
    /// project finding refused the submission.
    pub fn status(&self) -> Status {
        self.outcome.status()
    }

    /// The time each section's composition took, in section order, each
    /// named by its section (#3762).
    pub fn timings(&self) -> Timings {
        self.outcome.timings()
    }

    /// The creation time the documents carry, as ISO 8601 text (D69, #3030
    /// section 2.8): the `created` argument as given or, when it was
    /// omitted, the UTC time the submission read once and gave every
    /// section, written `YYYY-MM-DDTHH:MM:SS` (D129). It is recorded even
    /// when nothing composed: an argument DEFINE-3 refuses is kept as given,
    /// and each section fails with `value_not_permitted` at `created`. It is
    /// empty only when the argument was omitted and the system clock gave
    /// no time the calendar admits, which each section reports the same way.
    pub fn created(&self) -> &str {
        &self.created
    }

    /// Save the submission as one save (PROJECT-13, JSON-14, D38, D74) and
    /// return the absolute paths written, with forward slashes, in
    /// publication order: section by section in the order `sdtm`, `adam`,
    /// `send`, each section's transport files and then its define.xml.
    ///
    /// The save opens every target of every section first, so a target that
    /// cannot be written or two files that name one target refuse it before
    /// anything is written; it stages every file, then replaces each target in
    /// order, and the first file that cannot replace its target stops the
    /// save. A section that did not compose publishes nothing and stops the
    /// save at its turn, after the sections before it. A refusal carries its
    /// issues and the paths written before it (P-API-1); a submission that
    /// composed no section fails with `nothing_to_save`. The save polls
    /// `token` as [`ProjectRun::save`] does.
    pub fn save(&mut self, token: &CancellationToken) -> Result<Vec<String>, Failure> {
        self.outcome
            .save(crate::outcome::Operation::Submission, &self.subject, token)
    }
}

/// One project file's operations: its path, the section its run and
/// submission take, the [`Plans`] holder its runs reuse and the
/// [`CancellationToken`] its operations poll, which [`Engine::project`] gives
/// and the builders below add to. A request reads nothing until an operation
/// runs, and each operation reads the project file afresh, so the scope and
/// order of each follow the current file; one request can run any of them,
/// any number of times.
///
/// Without [`ProjectRequest::section`] a run builds, and a submission
/// composes, every section; without [`ProjectRequest::reusing`] each run
/// compiles every specification, through an empty holder; without
/// [`ProjectRequest::cancel_with`] its operations poll a token that nothing
/// cancels. [`ProjectRequest::check`], [`ProjectRequest::run`] and
/// [`ProjectRequest::submission`] poll the token; [`ProjectRequest::names`]
/// and [`ProjectRequest::spec`] read the project file alone and poll none.
#[non_exhaustive]
#[must_use = "a request reads nothing until one of its operations runs"]
#[derive(Clone)]
pub struct ProjectRequest<'a> {
    engine: &'a Engine,
    path: &'a str,
    section: Option<&'a str>,
    plans: Option<&'a Plans>,
    token: Option<&'a CancellationToken>,
}

impl<'a> ProjectRequest<'a> {
    /// The same request, whose run builds, and whose submission composes,
    /// the one section `section`, one of [`SECTIONS`]. Once the project file
    /// is read, a section it does not declare, or any other text, fails the
    /// run or the submission as `cannot select <section>`, with
    /// `unknown_section` (PROJECT-14). [`ProjectRequest::check`],
    /// [`ProjectRequest::names`] and [`ProjectRequest::spec`] read every
    /// section, whichever the request names.
    pub fn section(mut self, section: &'a str) -> Self {
        self.section = Some(section);
        self
    }

    /// The same request, whose runs reuse the plans `plans` keeps, as a
    /// host's project object runs. The project file is read afresh every
    /// time, so the run scope and order follow the current file, and each
    /// listed specification is identified from that read: the plan an
    /// earlier run kept for the same specification and project identities is
    /// reused under the rule of [`SpecRequest::reusing`]. Plans kept for
    /// anything the file no longer lists, or for another project file, are
    /// dropped. See [`Plans`].
    pub fn reusing(mut self, plans: &'a Plans) -> Self {
        self.plans = Some(plans);
        self
    }

    /// The same request, whose operations poll `token`: a cancelled
    /// operation fails with a
    /// [`FailureKind::Cancelled`](crate::FailureKind::Cancelled) failure, with no
    /// result and no issue. See [`CancellationToken`] for where an operation
    /// polls it.
    pub fn cancel_with(mut self, token: &'a CancellationToken) -> Self {
        self.token = Some(token);
        self
    }

    /// The token the operations poll: the request's, or else one that
    /// nothing cancels. A clone shares the request's flag and probe.
    fn token(&self) -> CancellationToken {
        self.token.cloned().unwrap_or_default()
    }

    /// The progress of one operation, reported to the engine's callback.
    fn reporter(&self) -> Reporter {
        Reporter::new(self.engine.shared().progress.clone())
    }

    /// The cancellation of an operation that reads the project file:
    /// `cannot read <path>: operation cancelled`.
    fn cancelled(&self) -> Failure {
        Failure::cancelled(Action::Read(self.path))
    }

    /// The request of the listed specification whose `domain` is `name`,
    /// read with this project file, as a host's project object gives a
    /// specification object: its [`SpecRequest::path`] is the listed
    /// specification's, and its operations poll this request's token. It
    /// reuses none of this request's plans: [`SpecRequest::reusing`] gives it
    /// a holder of its own, as each host object keeps its own.
    ///
    /// Every refusal names the call as `cannot select <name>` (D39): a
    /// project that cannot be read, one that lists a name twice, or a name it
    /// does not list, whose issue names the listed names.
    pub fn spec(&self, name: &str) -> Result<SpecRequest<'a>, Failure> {
        let action = Action::Select(name);
        let read = crate::internal::read_project(self.engine.shared(), self.path).map_err(
            |(issues, locator)| Failure::rejected(action, &Report::located(&issues, &locator)),
        )?;
        let duplicates = read.duplicates();
        if !duplicates.is_empty() {
            return Err(Failure::rejected(
                action,
                &Report::located(&duplicates, read.locator()),
            ));
        }
        match read.find(name) {
            Ok(listed) => Ok(SpecRequest {
                engine: self.engine,
                path: Cow::Owned(listed.path().to_owned()),
                project: Some(self.path),
                plans: None,
                token: self.token,
            }),
            Err(issues) => Err(Failure::from_issues(action, &issues)),
        }
    }

    /// The names of the listed specifications in run order, producer first,
    /// as a project run of every section orders them (D45): the names that
    /// [`ProjectRun::names`] of such a run gives, read from the project file
    /// and the specifications' metadata alone, so no input data is read. A
    /// listing that could not be read has no name and is left out, as a run
    /// leaves it out. A project whose run another project finding refuses,
    /// such as two listings that write one file, still names its
    /// specifications, each of which [`ProjectRequest::spec`] selects; the
    /// refusal stays the run's, which names nothing (D180).
    ///
    /// Every refusal names the call as `cannot read <project>` (D39): a
    /// project that cannot be read, or one that lists a name twice, whose
    /// issue names both listings.
    pub fn names(&self) -> Result<Vec<String>, Failure> {
        let action = Action::Read(self.path);
        let read = crate::internal::read_project(self.engine.shared(), self.path).map_err(
            |(issues, locator)| Failure::rejected(action, &Report::located(&issues, &locator)),
        )?;
        let duplicates = read.duplicates();
        if !duplicates.is_empty() {
            return Err(Failure::rejected(
                action,
                &Report::located(&duplicates, read.locator()),
            ));
        }
        Ok(read
            .plan(None)
            .steps()
            .iter()
            .filter_map(|step| read.listed()[step.listed()].name().map(str::to_owned))
            .collect())
    }

    /// Static check of every listed specification with the project's codelists
    /// and functions, plus the project findings: shared names, inputs that
    /// read a listed artifact without a `schema` link, and colliding files.
    /// Only cancellation fails the call, with no result: a check cancelled
    /// between specifications returns none of the findings it gathered.
    pub fn check(&self) -> Result<Check, Failure> {
        let (project, token) = (self.path, self.token());
        if token.is_cancelled() {
            return Err(self.cancelled());
        }
        let shared = self.engine.shared();
        let read = match crate::internal::read_project(shared, project) {
            Ok(read) => read,
            Err((issues, locator)) => {
                return Ok(Check {
                    outcome: Outcome::refused(issues, locator),
                    subject: project.to_owned(),
                })
            }
        };
        let reporter = self.reporter();
        let mut members = Vec::with_capacity(read.listed().len());
        for listed in read.listed() {
            if token.is_cancelled() {
                return Err(self.cancelled());
            }
            // Once the progress callback failed, no later specification is read.
            if reporter.failed() {
                break;
            }
            let (checked, measured) = if listed.failure().is_empty() {
                let observed = reporter.observe(listed.name().unwrap_or(listed.written()));
                let checked = crate::internal::run_check(
                    shared,
                    listed.path(),
                    Some(project),
                    &observed,
                    &token,
                )
                .map_err(|Cancelled| self.cancelled())?;
                (checked, Some(observed.measured().clone()))
            } else {
                (
                    (listed.failure().to_vec(), Arc::clone(read.locator())),
                    None,
                )
            };
            members.push(Member {
                name: listed.name().map(str::to_owned),
                identity: Some(listed.identity().to_owned()),
                work: Work::Checked(checked.0, checked.1),
                measured,
            });
        }
        let mut issues = read.check_findings();
        issues.extend(progress_failure(&reporter, Step::Check));
        Ok(Check {
            outcome: Outcome {
                members,
                issues,
                locator: Arc::clone(read.locator()),
            },
            subject: project.to_owned(),
        })
    }

    /// Build every listed specification of the request's section, or of all
    /// sections in the order of [`SECTIONS`]. Each builds once, producer
    /// first; a producer in the run scope hands its artifact to its consumers
    /// in memory, and one outside it is read from its saved artifact file. A
    /// failed specification stops only its consumers. Project problems come
    /// back as issues of a run that built nothing; only an unknown section
    /// and cancellation fail the call, and a user interrupt in project code
    /// cancels it (D46), so no later specification builds. Once the project
    /// file is read, a section it does not declare, or any other text, fails
    /// as `cannot select <section>`, with `unknown_section` (PROJECT-14).
    pub fn run(&self) -> Result<ProjectRun, Failure> {
        let (project, section, token) = (self.path, self.section, self.token());
        if token.is_cancelled() {
            return Err(self.cancelled());
        }
        let once = Plans::new();
        let plans = self.plans.unwrap_or(&once);
        let shared = self.engine.shared();
        let subject = project_subject(project, section);
        let read = match crate::internal::read_project(shared, project) {
            Ok(read) => read,
            Err((issues, locator)) => {
                return Ok(ProjectRun {
                    outcome: Outcome::refused(issues, locator),
                    subject,
                })
            }
        };
        let section = select(&read, section)?;
        // Identify every listed specification from this read, with the
        // project path resolved now rather than its raw spelling.
        let identities = read
            .listed()
            .iter()
            .map(|listed| {
                (listed.failure().is_empty())
                    .then(|| {
                        let request = Request::new(listed.path()).with_environment(project);
                        crate::internal::request_identity(shared, &request)
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        plans.retain(|kept| identities.iter().flatten().any(|identity| identity == kept));
        let plan = read.plan(section);
        if !plan.refusal().is_empty() {
            return Ok(ProjectRun {
                outcome: Outcome::refused(plan.refusal().to_vec(), Arc::clone(read.locator())),
                subject,
            });
        }
        // The run scope as a producer graph, in run order.
        let position = |listing: usize| {
            plan.steps()
                .iter()
                .position(|step| step.listed() == listing)
                .expect("an in-scope producer is a step of the run")
        };
        let nodes = plan
            .steps()
            .iter()
            .map(|step| {
                let listed = &read.listed()[step.listed()];
                Node {
                    name: listed.name().map(str::to_owned),
                    label: Some(listed.name().unwrap_or(listed.written()).to_owned()),
                    identity: Some(listed.identity().to_owned()),
                    request: Request::new(listed.path()).with_environment(project),
                    plan: identities[step.listed()].clone(),
                    refused: step.refused().to_vec(),
                    producers: step
                        .links(&read)
                        .into_iter()
                        .map(|(producer, finding)| (position(producer), finding))
                        .collect(),
                    artifact: listed
                        .artifact_location()
                        .map(|location| (listed.identity().to_owned(), location.to_owned())),
                    locator: Arc::clone(read.locator()),
                }
            })
            .collect();
        let reporter = self.reporter();
        let mut outcome = self
            .engine
            .run_graph(nodes, plans, &reporter, &token)
            .map_err(|Cancelled| self.cancelled())?;
        // A failed progress callback refuses the whole run, so its save
        // writes nothing (#3762).
        outcome.issues = progress_failure(&reporter, Step::Build);
        Ok(ProjectRun { outcome, subject })
    }

    /// One define.xml for the request's section, or for every declared
    /// section that names `define`, plus Dataset-JSON where the section sets
    /// `transport: dataset_json` or `dataset_ndjson`. Every section is
    /// composed with one creation time, [`Submission::created`]: the
    /// options' `created` as given, or else the current UTC time, read once
    /// before the project file (D129).
    /// Project problems come back as issues; only an unknown section and
    /// cancellation fail the call. Once the project file is read, a section
    /// it does not declare, or any other text, fails as
    /// `cannot select <section>`, with `unknown_section` (PROJECT-14).
    pub fn submission(&self, options: &SubmissionOptions) -> Result<Submission, Failure> {
        let token = self.token();
        if token.is_cancelled() {
            return Err(self.cancelled());
        }
        // One creation time for every section (D129). A clock that gives no
        // time leaves each section to read it and refuse it (DEFINE-3).
        let created = options
            .created
            .clone()
            .or_else(yamaa_application::file_define::current_time);
        let outcome =
            self.compose_sections(created.as_deref(), &options.installed_version, &token)?;
        Ok(Submission {
            outcome,
            subject: project_subject(self.path, self.section),
            created: created.unwrap_or_default(),
        })
    }

    /// What [`ProjectRequest::submission`] composes, every section with
    /// `created` and `installed_version`.
    fn compose_sections(
        &self,
        created: Option<&str>,
        installed_version: &str,
        token: &CancellationToken,
    ) -> Result<Outcome, Failure> {
        let (project, shared) = (self.path, self.engine.shared());
        let read = match crate::internal::read_project(shared, project) {
            Ok(read) => read,
            Err((issues, locator)) => return Ok(Outcome::refused(issues, locator)),
        };
        let selected = select(&read, self.section)?;
        let duplicates = read.duplicates();
        if !duplicates.is_empty() {
            return Ok(Outcome::refused(duplicates, Arc::clone(read.locator())));
        }
        let sections = match selected {
            Some(section) => vec![section],
            None => read.define_sections().collect(),
        };
        let reporter = self.reporter();
        let mut members = Vec::with_capacity(sections.len());
        for section in sections {
            if token.is_cancelled() {
                return Err(self.cancelled());
            }
            // Once the progress callback failed, no later section composes.
            if reporter.failed() {
                break;
            }
            let key = yamaa_core::project_scope::key(section);
            let observed = reporter.observe(key);
            let define = observed.phase(
                yamaa_core::application_observer::Phase::Composition,
                || DefineBuild::compose(shared, project, section, created, installed_version),
                |define| fails(define.composed()),
            );
            members.push(Member {
                name: Some(key.to_owned()),
                identity: None,
                work: Work::Composed(Box::new(define)),
                measured: Some(observed.measured().clone()),
            });
        }
        Ok(Outcome {
            members,
            // A failed progress callback refuses the whole submission, so its
            // save writes nothing (#3762).
            issues: progress_failure(&reporter, Step::Define),
            locator: Arc::clone(read.locator()),
        })
    }
}

impl std::fmt::Debug for ProjectRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectRequest")
            .field("path", &self.path)
            .field("section", &self.section)
            .field("reusing", &self.plans.is_some())
            .field("token", &self.token)
            .finish_non_exhaustive()
    }
}

/// What [`ProjectRequest::submission`] composes with. The struct is
/// `#[non_exhaustive]`: start from [`SubmissionOptions::new`] and the
/// `with_*` methods.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionOptions {
    /// The source-system version Define-XML and Dataset-JSON carry for a
    /// section that names none (DEFINE-12): the version of the package that
    /// calls the engine, such as [`crate::VERSION`] for the hosts and the
    /// command line. An empty version refuses each section with
    /// `source_system_incomplete` at `source_system_version`, whether or not
    /// the section names a version of its own.
    pub installed_version: String,
    /// The creation time every section is composed with, as ISO 8601 text,
    /// or none to read the current UTC time once for every section (D69,
    /// D129). A time DEFINE-3 refuses is kept as given, and each section
    /// fails with `value_not_permitted` at `created`.
    pub created: Option<String>,
}

impl SubmissionOptions {
    /// Compose with `installed_version` as the source-system version, at
    /// the current UTC time.
    pub fn new(installed_version: impl Into<String>) -> Self {
        Self {
            installed_version: installed_version.into(),
            created: None,
        }
    }

    /// Compose every section with the creation time `created`.
    pub fn with_created(mut self, created: impl Into<String>) -> Self {
        self.created = Some(created.into());
        self
    }
}

#[cfg(all(test, feature = "yaml"))]
mod plan_tests {
    use super::*;
    use crate::host::functions::{
        Argument, CallError, FunctionError, FunctionHandle, FunctionIdentity, HostLanguage,
        LockContext, ProjectFunctions, Signature,
    };
    use crate::scalar::Scalar;
    use crate::EngineOptions;

    struct NoFunctions;
    impl ProjectFunctions for NoFunctions {
        fn verify_lock(
            &self,
            _: &LockContext<'_>,
            _: &[FunctionIdentity],
        ) -> Result<(), FunctionError> {
            Ok(())
        }
        fn bind(
            &self,
            _: &FunctionIdentity,
            _: &Signature,
        ) -> Result<FunctionHandle, FunctionError> {
            unreachable!("these specifications call no project function")
        }
        fn invoke(&self, _: &FunctionHandle, _: &[Argument<'_>]) -> Result<Scalar, CallError> {
            unreachable!("these specifications call no project function")
        }
    }

    #[test]
    fn a_project_run_drops_the_plans_of_specifications_the_project_no_longer_lists() {
        const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: dm.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n";
        let root = std::env::temp_dir().join(format!("yamaa-plan-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let root = yamaa_adapters::canonical_path::canonicalize(root).unwrap();
        std::fs::write(root.join("dm.yaml"), SPEC).unwrap();
        std::fs::write(
            root.join("ae.yaml"),
            SPEC.replace("domain: DM", "domain: AE")
                .replace("dm.csv", "ae.csv"),
        )
        .unwrap();
        std::fs::write(root.join("input.csv"), "ID\n1\n").unwrap();
        let listing = |specs: &str| {
            std::fs::write(
                root.join("project.yaml"),
                format!("yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [{specs}]\n"),
            )
            .unwrap();
        };
        let project = root.join("project.yaml").to_str().unwrap().to_owned();
        let engine = Engine::new(
            EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
                .with_functions(Arc::new(NoFunctions)),
        );
        let plans = Plans::new();
        let run = || {
            let runs = engine.project(&project).reusing(&plans).run().unwrap();
            assert_eq!(runs.issues().rows(), 0);
        };
        listing("dm.yaml, ae.yaml");
        run();
        let names = |plans: &Plans| {
            plans
                .identities()
                .into_iter()
                .map(|(specification, project)| {
                    let name = |path: &str| path.rsplit('/').next().unwrap().to_owned();
                    (name(&specification), project.as_deref().map(name))
                })
                .collect::<Vec<_>>()
        };
        let kept = |name: &str| (name.to_owned(), Some("project.yaml".to_owned()));
        assert_eq!(names(&plans), [kept("ae.yaml"), kept("dm.yaml")]);
        // AE leaves the project: its plan goes, and DM's plan is compiled
        // again because the project file changed.
        listing("dm.yaml");
        run();
        assert_eq!(names(&plans), [kept("dm.yaml")]);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text cells of one column of an issues table, in row order.
    fn cells(table: &Table, column: &str) -> Vec<Option<String>> {
        let public = table.public();
        let index = public.names().position(|name| name == column).unwrap();
        let yamaa_adapters::public_table::PublicColumn::Str(values) = public.column(index) else {
            panic!("{column} is a text column")
        };
        (0..values.len())
            .map(|row| values.get(row).map(str::to_owned))
            .collect()
    }

    #[test]
    fn statuses_are_the_statuses_in_order() {
        let statuses = [Status::Succeeded, Status::Failed, Status::Skipped];
        for status in statuses {
            // Exhaustive: a new status fails to compile here until it is
            // listed above, and so in STATUSES.
            match status {
                Status::Succeeded | Status::Failed | Status::Skipped => {}
            }
        }
        assert_eq!(statuses.map(Status::as_str), STATUSES);
    }

    /// A request and the submission options are `Send` and `Sync`, as the
    /// engine, the holder and the token they hold are.
    #[test]
    fn requests_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SpecRequest<'_>>();
        assert_send_sync::<ProjectRequest<'_>>();
        assert_send_sync::<SubmissionOptions>();
    }

    #[test]
    fn sections_are_the_project_sections_in_run_order() {
        use yamaa_core::project_scope::{key, SECTIONS as CORE};
        assert_eq!(CORE.map(key), SECTIONS);
    }

    /// Resolved facts the engine cannot represent are refused as the
    /// registered engine fault, whose message names the step and code.
    #[test]
    fn unrepresentable_resolved_facts_are_an_engine_fault() {
        let fault = yamaa_core::diagnostic::ConditionCode::ApplicationRejected.definition();
        let failure = resolved_value("adam/adsl.yaml", None).unwrap_err();
        assert!(matches!(failure.kind, crate::error::FailureKind::Rejected));
        assert_eq!(
            failure.message,
            format!(
                "cannot resolve adam/adsl.yaml: the engine refused the operation in its \
                 resolve step: artifact_representation [{}]",
                fault.condition
            )
        );
        assert_eq!(
            cells(&failure.issues, "phase"),
            [Some(fault.phase.to_owned())]
        );
        assert_eq!(
            cells(&failure.issues, "condition"),
            [Some(fault.condition.to_owned())]
        );
        assert_eq!(cells(&failure.issues, "rule"), [None]);
        assert_eq!(
            cells(&failure.issues, "context"),
            [Some(
                r#"{"code":"artifact_representation","step":"resolve"}"#.to_owned()
            )]
        );
        let tree = yamaa_core::json::Value::Object(
            [(
                "entry".to_owned(),
                yamaa_core::json::Value::String("spec.yaml".to_owned()),
            )]
            .into(),
        );
        assert_eq!(
            resolved_value("adam/adsl.yaml", Some(tree)).unwrap(),
            ContextValue::Map(
                [(
                    "entry".to_owned(),
                    ContextValue::Text("spec.yaml".to_owned())
                )]
                .into()
            )
        );
    }
}
