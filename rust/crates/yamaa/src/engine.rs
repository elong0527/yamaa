//! The supported engine surface: one [`Engine`] over the existing prepare,
//! bind, execute and publish workflow. There is no second admission or
//! evaluation route; all physical access goes through a resolver, and local
//! files are just the default resolver.
//!
//! The use cases are [`Engine`]'s methods in [`crate::api`],
//! [`crate::file_type`], [`crate::odm`] and [`crate::table`]. Each build, of a
//! specification alone or of one member of a project run, is an owned build
//! internal to the facade; a [`crate::Run`] is its public view, and a result
//! saves through [`crate::Run::save`], [`crate::ProjectRun::save`] or
//! [`crate::Submission::save`], one save over its outcome.

use std::sync::Arc;

use yamaa_adapters::application_measurements::Measurements;
use yamaa_adapters::file_application::Domain;
use yamaa_core::diagnostic::Diagnostic;

use crate::api::Failure;
use crate::cancel::CancellationToken;
use crate::diagnostic::{Locator, Report};
use crate::error::{Error, FunctionError};
use crate::functions::{HostLanguage, ProjectFunctions};
use crate::internal::EngineShared;
use crate::plan::Plan;
use crate::resolver::ResourceResolver;
use crate::table::Table;

/// Engine configuration. The struct is `#[non_exhaustive]`, so the `new`,
/// `neutral` and `with_*` constructors below are the only way to build it,
/// keeping additive field changes non-breaking. The default options are
/// neutral, with an empty runtime identity.
#[non_exhaustive]
#[derive(Default)]
pub struct EngineOptions {
    pub resolver: Option<Arc<dyn ResourceResolver>>,
    /// The host language whose project functions the engine runs, or none
    /// for an engine that claims no host language (see
    /// [`EngineOptions::neutral`]).
    pub host_language: Option<HostLanguage>,
    pub runtime_name: String,
    pub runtime_version: String,
    pub functions: Option<Arc<dyn ProjectFunctions>>,
}

impl EngineOptions {
    /// Configure one engine for a host language and runtime identity.
    /// The runtime name folds in the report identity (hosts pass their
    /// language name, as today) alongside the runtime version.
    pub fn new(host_language: HostLanguage, runtime_name: String, runtime_version: String) -> Self {
        Self {
            host_language: Some(host_language),
            ..Self::neutral(runtime_name, runtime_version)
        }
    }

    /// Configure one engine that claims no host language, such as the
    /// command line's or a bare Rust embedder's (#3030 section 2.10, D123). It
    /// reads no project's `environments.<host>` entry, so it verifies no
    /// lock and activates nothing, and it runs no project function: a build
    /// that calls one is refused before any input is read, as
    /// `unsupported_operation` with operation `environment` (D110), and
    /// functions given to [`EngineOptions::with_functions`] are never called.
    /// Every function file a 1.0 project lists is still read and admitted,
    /// so a check or resolve of a specification that calls one runs as a
    /// host's does; a 0.5 project's functions are declared in the environment
    /// files such an engine does not read, so each call to one is refused
    /// while the specification compiles. Every build that calls no project
    /// function runs as a host runs it, with or without a project.
    pub fn neutral(runtime_name: String, runtime_version: String) -> Self {
        Self {
            resolver: None,
            host_language: None,
            runtime_name,
            runtime_version,
            functions: None,
        }
    }

    /// Serve resources through a caller-supplied resolver.
    pub fn with_resolver(mut self, resolver: Arc<dyn ResourceResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Activate project functions through a caller-supplied capability.
    /// Without one, the engine's host-functions port is the empty one: a build
    /// that calls a project function reports unsupported before it reads data,
    /// and every other build runs, with or without a project. An engine that
    /// claims no host language never calls them.
    pub fn with_functions(mut self, functions: Arc<dyn ProjectFunctions>) -> Self {
        self.functions = Some(functions);
        self
    }
}

/// One build or check request: the entry path or resolver reference plus an
/// optional project file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Request<'a> {
    pub(crate) specification: &'a str,
    pub(crate) environment: Option<&'a str>,
}

impl<'a> Request<'a> {
    /// Request the specification entry with no project.
    pub(crate) fn new(specification: &'a str) -> Self {
        Self {
            specification,
            environment: None,
        }
    }

    /// Request the specification entry with a project file.
    pub(crate) fn with_environment(mut self, environment: &'a str) -> Self {
        self.environment = Some(environment);
        self
    }
}

/// An embedder configures one engine and calls operations on it. The engine
/// is cheap to clone over shared state; concurrent calls are independent
/// attempts.
#[derive(Clone)]
pub struct Engine {
    shared: Arc<EngineShared>,
}

impl Engine {
    /// Configure one engine from its options.
    pub fn new(options: EngineOptions) -> Self {
        Self {
            shared: Arc::new(EngineShared::from_options(&options)),
        }
    }

    /// The Preparation half of a build: capture and admit the metadata
    /// through the engine's resolver and compile the selected route, reading
    /// no study data and activating nothing. A refused compilation still
    /// returns a plan that owns its report.
    fn compile(&self, request: &Request<'_>, measurements: &Arc<Measurements>) -> Plan {
        let compiled = crate::internal::compile(
            &self.shared,
            request.specification,
            request.environment,
            yamaa_core::application_observer::Scope::new(&**measurements),
        );
        Plan::new(
            Arc::clone(&self.shared),
            request.specification.to_owned(),
            compiled,
        )
    }

    /// Build `request` through `plans`, as a host's lazy object runs (#2821).
    /// `identity` is what the request resolves to now. A plan kept for that
    /// identity, whose files the run-start check finds unchanged, runs again;
    /// otherwise the request compiles and runs, and `plans` keeps the new
    /// plan if the request still resolves to `identity` once compiled.
    /// Either way the run uses this engine's project functions. In a project
    /// run, an input whose `schema` link names a producer the run already
    /// built reads that `held` artifact; every other input reads its saved
    /// file.
    ///
    /// The run polls `token`, and so does this call once the member is
    /// built: a cancelled build, or one whose project function raised the
    /// host's interrupt (as [`ProjectFunctions::is_interrupt`] classifies
    /// it), fails the call with [`Error::Cancelled`] and is dropped.
    pub(crate) fn build_reusing(
        &self,
        plans: &crate::api::Plans,
        request: &Request<'_>,
        identity: Option<crate::api::PlanKey>,
        held: Option<Arc<yamaa_adapters::file_resources::HeldArtifacts>>,
        token: &CancellationToken,
    ) -> Result<Build, Error> {
        let measurements = Arc::new(Measurements::new());
        // The run-start check runs outside the holder's lock.
        let kept = identity
            .as_ref()
            .and_then(|identity| plans.get(identity))
            .filter(|plan| plan.reusable_by(&self.shared));
        let (build, name) = match kept {
            Some(plan) => {
                measurements.plan("reused");
                let build = plan.run_through(&self.shared, held, &measurements, token);
                (build, plan.name().map(str::to_owned))
            }
            None => {
                let plan = self.compile(request, &measurements);
                measurements.plan(if plan.report().is_empty() {
                    "compiled"
                } else {
                    "refused"
                });
                // The compilation has just captured the files it holds.
                let build = plan.run_through(&self.shared, held, &measurements, token);
                if let Some(identity) = identity {
                    // Keep the plan only for the identity it was compiled
                    // from: a path that resolved elsewhere meanwhile keeps
                    // nothing, so its next run compiles again.
                    let stable = crate::internal::request_identity(&self.shared, request).as_ref()
                        == Some(&identity);
                    plans.replace(identity, stable.then_some(&plan));
                }
                (build, plan.name().map(str::to_owned))
            }
        };
        if build.interrupted() || token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        measurements.complete(build.accepted());
        let subject = name.as_deref().unwrap_or(request.specification);
        Ok(build.with_subject(subject))
    }

    /// A project-run specification that did not build, with the findings
    /// that explain why, named by `subject` in what it prints and located by
    /// `locator`, the project's.
    pub(crate) fn refused_build(
        &self,
        issues: Vec<Diagnostic>,
        subject: &str,
        locator: Arc<Locator>,
    ) -> Build {
        let measurements = Arc::new(Measurements::new());
        measurements.complete(false);
        Build::new(Err(issues), Vec::new(), measurements, &self.shared, locator)
            .with_subject(subject)
    }

    pub(crate) fn shared(&self) -> &EngineShared {
        &self.shared
    }

    /// Resolve one specification entry into its owned inspection artifact,
    /// reading through the engine's resolver; a refusal carries its issues.
    pub(crate) fn resolve_specification(
        &self,
        request: &Request<'_>,
    ) -> Result<ResolvedSpecification, Error> {
        match yamaa_adapters::resolved_artifact::resolve_located(
            request.specification,
            request.environment,
            self.shared.language,
            crate::internal::resolver_bridge(&self.shared),
        ) {
            Ok(artifact) => Ok(ResolvedSpecification { inner: artifact }),
            Err((issues, locator)) => Err(Error::Rejected(Report::located(&issues, &locator))),
        }
    }
}

/// An owned build: one attempt with its output, run report and observations.
/// Only the save of the result that holds it writes, through the resolver.
pub(crate) struct Build {
    /// The member's result, or the findings that explain why it has none: a
    /// refusal before it ran, or the refusal of a result its run could not
    /// build.
    result: Result<Box<Domain>, Vec<Diagnostic>>,
    measurements: Arc<Measurements>,
    /// The issues of the build as it was made: a later save's refusal joins
    /// the report, never these.
    built: Vec<Diagnostic>,
    report: Report,
    output: Option<Table>,
    run_report: Option<Table>,
    /// The host failures the attempt retained. The build owns them, and so
    /// their original host objects, for as long as it lives, as the host
    /// interface's ownership rules ask; nothing reads them once built.
    _failures: Vec<FunctionError>,
    /// Whether a project function raised the host's interrupt (D46).
    interrupted: bool,
    /// What the build's printed text and refusals name (P-PRINT-1): a
    /// project run member's listed name, or the path the project file lists
    /// for a member whose listing could not be read (D130); otherwise the
    /// dataset name once read, or else the path as given.
    subject: String,
    /// What locates the build's issues (P-ISSUE-1): the documents its
    /// compilation read, or a refused project member's project.
    locator: Arc<Locator>,
}

impl Build {
    /// One build of a member: its `result`, the measurements its operation
    /// observed and the host failures its run retained, of which one the
    /// engine's project functions classify as an interrupt cancels the call.
    /// `locator` locates its issues.
    pub(crate) fn new(
        result: Result<Box<Domain>, Vec<Diagnostic>>,
        failures: Vec<FunctionError>,
        measurements: Arc<Measurements>,
        shared: &EngineShared,
        locator: Arc<Locator>,
    ) -> Self {
        let interrupted = shared.functions.as_ref().is_some_and(|functions| {
            failures
                .iter()
                .any(|failure| functions.is_interrupt(failure))
        });
        let mut build = Self {
            result,
            measurements,
            built: Vec::new(),
            report: Report::default(),
            output: None,
            run_report: None,
            _failures: failures,
            interrupted,
            subject: String::new(),
            locator,
        };
        build.built = build.issues();
        build.refresh();
        build
    }

    /// What locates the build's issues.
    pub(crate) fn locator(&self) -> &Arc<Locator> {
        &self.locator
    }

    /// The same build, named by `subject` in what it prints.
    pub(crate) fn with_subject(mut self, subject: &str) -> Self {
        subject.clone_into(&mut self.subject);
        self
    }

    /// What the build's printed text and refusals name.
    pub(crate) fn subject(&self) -> &str {
        &self.subject
    }

    fn domain(&self) -> Option<&Domain> {
        self.result.as_deref().ok()
    }

    fn refresh(&mut self) {
        self.report = Report::located(&self.issues(), &self.locator);
        (self.output, self.run_report) = match self.domain() {
            Some(domain) => (
                domain.output_table().map(Table::from_adapter),
                Some(Table::from_adapter(domain.report_table())),
            ),
            None => (None, None),
        };
    }

    /// Whether the build produced its primary output.
    pub(crate) fn accepted(&self) -> bool {
        self.domain()
            .is_some_and(|domain| domain.output().is_some())
    }

    /// Borrow the output table, when the build produced one.
    pub(crate) fn output(&self) -> Option<&Table> {
        self.output.as_ref()
    }

    /// Borrow the run report table, which every executed build holds,
    /// success or failure. A build that never reached execution has none.
    pub(crate) fn run_report(&self) -> Option<&Table> {
        self.run_report.as_ref()
    }

    /// Borrow the build report with every diagnostic.
    pub(crate) fn report(&self) -> &Report {
        &self.report
    }

    /// The build's issues, live from its current state.
    pub(crate) fn issues(&self) -> Vec<Diagnostic> {
        match &self.result {
            Ok(domain) => domain.issues(),
            Err(issues) => issues.clone(),
        }
    }

    /// The build's status (P-STATUS-1), computed from its issues as built and
    /// what it built: `skipped` when it did not run because every reason is a
    /// run-scope producer that did not complete (PROJECT-11), `succeeded`
    /// when it built its output with no error-severity issue, and `failed`
    /// otherwise. A later save does not change it.
    pub(crate) fn status(&self) -> crate::api::Status {
        use crate::api::Status;
        use yamaa_core::diagnostic::ConditionCode;
        let issues = &self.built;
        let skipped = self.result.is_err()
            && !issues.is_empty()
            && issues
                .iter()
                .all(|issue| issue.code == ConditionCode::ProjectProducerNotCompleted);
        if skipped {
            Status::Skipped
        } else if self.output.is_some() && !crate::outcome::fails(issues) {
            Status::Succeeded
        } else {
            Status::Failed
        }
    }

    /// The accepted primary artifact's serialized bytes, shared, for the
    /// consumers a project run builds later.
    pub(crate) fn artifact(&self) -> Option<Arc<[u8]>> {
        self.domain()?.artifact()
    }

    /// The files a save of this build writes, in publication order, with
    /// what the save admitted for them: the declared report file, then the
    /// primary artifact unless `report_only` or the build failed (OUTPUT-10).
    /// A build that never ran, or a failed one without a report file, writes
    /// nothing.
    pub(crate) fn plan(
        &self,
        report_only: bool,
    ) -> Result<
        Option<(
            Vec<yamaa_adapters::publication::File<'_>>,
            yamaa_adapters::file_application::Admitted,
        )>,
        Vec<Diagnostic>,
    > {
        match self.domain() {
            Some(domain) => domain.plan(report_only),
            None => Ok(None),
        }
    }

    /// Record what a save did with this build's files; a stop joins the
    /// build's issues, never its status.
    pub(crate) fn record(
        &mut self,
        admitted: yamaa_adapters::file_application::Admitted,
        committed: usize,
        stopped: Option<(usize, yamaa_adapters::publication::Cause)>,
    ) {
        if let Ok(domain) = &mut self.result {
            domain.record(admitted, committed, stopped);
        }
        self.refresh();
    }

    /// Record a save refused for `issues` before any file was staged.
    pub(crate) fn refuse(&mut self, issues: Vec<Diagnostic>) {
        if let Ok(domain) = &mut self.result {
            domain.refuse(issues);
        }
        self.refresh();
    }

    /// Count one save of this build in its measurements, ending as `outcome`.
    pub(crate) fn measure_save(&self, outcome: &'static str) {
        self.measurements.save_entered();
        self.measurements.save_completed(outcome);
    }

    /// Save this build as a run of one specification saves it, through the
    /// one save every result shares; see [`crate::Run::save`].
    pub(crate) fn save(&mut self, token: &CancellationToken) -> Result<Vec<String>, Failure> {
        let subject = self.subject.clone();
        crate::outcome::save(
            &[],
            &Locator::none(),
            vec![crate::outcome::Part::Built {
                identity: None,
                build: self,
            }],
            crate::outcome::Operation::Run,
            &subject,
            token,
        )
    }

    /// Render observations, optionally enveloped with timing measurements.
    pub(crate) fn observations(&self, include_measurements: bool) -> Option<String> {
        let observed = self.domain().map(|domain| domain.observations_json());
        if include_measurements {
            Some(self.measurements.envelope(observed))
        } else {
            observed
        }
    }

    /// Whether a project function raised the host's interrupt, which cancels
    /// the call that built this (D46).
    pub(crate) fn interrupted(&self) -> bool {
        self.interrupted
    }
}

/// An owned resolved-specification artifact: inspection facts without source
/// access, path resolution, callbacks or recompilation.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedSpecification {
    inner: yamaa_adapters::resolved_artifact::ResolvedArtifact,
}

impl ResolvedSpecification {
    /// The facts the artifact holds, as one owned tree.
    pub(crate) fn to_value(&self) -> Result<yamaa_core::json::Value, Error> {
        self.inner.to_value().map_err(|_| Error::Internal {
            code: "resolved_serialization",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::MemoryResolver;

    fn options() -> EngineOptions {
        EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
    }

    fn missing_spec() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push("yamaa-facade-test-missing-entry.yaml");
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn option_builders_keep_additive_fields_non_breaking() {
        let request = Request::new("spec");
        assert_eq!(request.environment, None);
        assert_eq!(request.with_environment("env").environment, Some("env"));
        let options = EngineOptions::new(HostLanguage::R, "r".into(), "4.3".into())
            .with_resolver(Arc::new(MemoryResolver::new()));
        assert!(options.resolver.is_some());
        assert!(options.functions.is_none());
    }

    /// A cancelled call returns `Error::Cancelled`, with no result and no
    /// issue; a check is no exception.
    #[test]
    fn cancelled_tokens_fail_operations_before_entry() {
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        token.cancel();
        assert!(matches!(
            engine.spec_run("spec", None, &token),
            Err(Error::Cancelled)
        ));
        assert!(matches!(
            engine.spec_check("spec", None, &token),
            Err(Error::Cancelled)
        ));
    }

    #[test]
    fn missing_entries_report_diagnostics_without_a_build_error() {
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        let missing = missing_spec();
        let path = missing.to_str().unwrap();
        let check = engine.spec_check(path, None, &token).unwrap();
        assert!(!check.diagnostics().is_empty());
        let run = engine.spec_run(path, None, &token).unwrap();
        assert!(run.output().is_none());
        assert!(run.report().is_none());
        assert!(!run.diagnostics().is_empty());
        assert!(run.observations(false).is_none());
        assert!(!run.build.interrupted());
    }

    #[test]
    fn a_failed_run_saves_nothing_and_refuses_with_its_issues() {
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        let missing = missing_spec();
        let mut run = engine
            .spec_run(missing.to_str().unwrap(), None, &token)
            .unwrap();
        let failure = run.save(&CancellationToken::new()).unwrap_err();
        // D39: a run whose file was not read names the path as given.
        assert_eq!(
            failure.message,
            format!(
                "cannot save {}: {}",
                missing.to_str().unwrap(),
                failure.diagnostics[0].text()
            )
        );
        assert!(failure.issues.rows() > 0);
        assert!(failure.paths.is_empty());
    }
}

#[cfg(all(test, feature = "yaml"))]
mod plain_build_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(1);

    struct Study(std::path::PathBuf);
    impl Drop for Study {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn study() -> Study {
        let root = std::env::temp_dir().join(format!(
            "yamaa-facade-plain-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("input")).unwrap();
        std::fs::write(root.join("input/input.csv"), "ID,VALUE\n1,2.5\n2,3.5\n").unwrap();
        std::fs::write(
            root.join("spec.yaml"),
            "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input/input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n",
        )
        .unwrap();
        Study(root)
    }

    #[test]
    fn plain_runs_carry_output_and_save_through_the_facade() {
        let study = study();
        let entry = study.0.join("spec.yaml");
        let engine = Engine::new(EngineOptions::new(
            HostLanguage::Python,
            "test".into(),
            "test".into(),
        ));
        let token = CancellationToken::new();
        let path = entry.to_str().unwrap();
        let check = engine.spec_check(path, None, &token).unwrap();
        assert!(check.diagnostics().is_empty());
        let mut run = engine.spec_run(path, None, &token).unwrap();
        assert!(run.diagnostics().is_empty());
        assert_eq!(run.output().unwrap().rows(), 2);
        // One run, input, base and two column rows; no handler or check.
        assert_eq!(run.report().unwrap().rows(), 5);
        assert!(run.observations(false).is_some());
        assert!(run.observations(true).is_some());
        // The specification's run saves its one file and returns its path.
        let root = yamaa_adapters::canonical_path::canonicalize(&study.0).unwrap();
        let out = root.join("out.csv").to_str().unwrap().replace('\\', "/");
        assert_eq!(run.save(&token).unwrap(), [out]);
        let written = std::fs::read(study.0.join("out.csv")).unwrap();
        assert!(written.starts_with(b"ID,VALUE\n1,"));
    }
}
