//! The supported engine surface: one [`Engine`] over the existing prepare,
//! bind, execute and publish workflow. There is no second admission or
//! evaluation route; all physical access goes through a resolver, and local
//! files are just the default resolver.
//!
//! A rejected build that still owns diagnostics, evidence or retained sources
//! returns an owned [`Build`] carrying them, exactly as the host
//! failure-handles do. [`Error::Rejected`] is reserved for operations without
//! a handle: unsupported routes and failed specification resolution. A
//! result saves through [`crate::Run::save`], [`crate::ProjectRun::save`] or
//! [`crate::Submission::save`], one save over its outcome.

use std::sync::Arc;

use yamaa_adapters::application_measurements::Measurements;
use yamaa_adapters::file_application::Domain;
use yamaa_core::diagnostic::Diagnostic;

use crate::api::Failure;
use crate::cancel::CancellationToken;
use crate::diagnostic::Report;
use crate::error::{Error, FunctionError};
use crate::functions::{HostLanguage, ProjectFunctions};
use crate::internal::{run_check, EngineShared, ObserverBridge};
use crate::observe::Progress;
use crate::plan::Plan;
use crate::resolver::ResourceResolver;
use crate::table::Table;

/// One retained host failure with its failure stage.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct AttemptFailure {
    /// The stage that retained the failure.
    pub stage: &'static str,
    /// The retained function failure with its host payload.
    pub error: FunctionError,
}

/// Engine configuration. Options and request types are `#[non_exhaustive]`
/// structs, so the `new`, `neutral` and `with_*` constructors below are the
/// only way to build them, keeping additive field changes non-breaking. The
/// default options are neutral, with an empty runtime identity.
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
    pub observer: Option<Arc<dyn Progress>>,
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
            observer: None,
        }
    }

    /// Serve resources through a caller-supplied resolver.
    pub fn with_resolver(mut self, resolver: Arc<dyn ResourceResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Activate project functions through a caller-supplied capability.
    /// Without one, a build that calls a project function reports
    /// unsupported before it reads data; every other build runs, with or
    /// without a project. An engine that claims no host language never calls
    /// them.
    pub fn with_functions(mut self, functions: Arc<dyn ProjectFunctions>) -> Self {
        self.functions = Some(functions);
        self
    }

    /// Observe operation stages through a caller-supplied progress port.
    pub fn with_observer(mut self, observer: Arc<dyn Progress>) -> Self {
        self.observer = Some(observer);
        self
    }
}

/// One build or check request: the entry path or resolver reference plus an
/// optional environment.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub specification: &'a str,
    pub environment: Option<&'a str>,
}

impl<'a> Request<'a> {
    /// Request the specification entry with no environment.
    pub fn new(specification: &'a str) -> Self {
        Self {
            specification,
            environment: None,
        }
    }

    /// Request the specification entry with an environment.
    pub fn with_environment(mut self, environment: &'a str) -> Self {
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

    /// Static checks only: admission and preparation failures come back as
    /// diagnostics in the returned report, never as a second error channel.
    /// Only cancellation fails the call: a cancelled token returns
    /// [`Error::Cancelled`], with no report.
    ///
    /// # Example
    ///
    /// The specification bytes and one Arrow input table enter through a
    /// memory resolver; no filesystem path is touched. Lanes without the
    /// `yaml` feature report diagnostics for the same call.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use arrow_array::RecordBatchIterator;
    /// use arrow_schema::Schema;
    /// use yamaa::{
    ///     CancellationToken, DataType, Engine, EngineOptions, FFI_ArrowArrayStream, Field,
    ///     Float64Array, HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch,
    ///     Request,
    /// };
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n"[..],
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
    /// let report = engine.check(&Request::new("spec.yaml"), &token).unwrap();
    /// if cfg!(feature = "yaml") {
    ///     assert!(report.is_empty());
    /// } else {
    ///     assert!(!report.is_empty());
    /// }
    /// ```
    pub fn check(&self, request: &Request<'_>, token: &CancellationToken) -> Result<Report, Error> {
        run_check(
            &self.shared,
            request.specification,
            request.environment,
            token,
        )
        .map(|issues| Report::from_issues(&issues))
    }

    /// Prepare a specification once for many runs. Compilation captures and
    /// admits the metadata through the engine's resolver and compiles the
    /// selected route; it reads no study data and activates nothing. A refused
    /// compilation still returns a plan that owns its report and the sources
    /// it captured; only cancellation fails the call itself.
    ///
    /// # Example
    ///
    /// The compile resolver holds only the specification: a valid
    /// specification makes no input request. The plan's run reads the input
    /// from the same resolver. Lanes without the `yaml` feature return a
    /// refused plan carrying diagnostics.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, MemoryResolver, Request,
    ///     RunRequest};
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n"[..],
    /// ));
    /// let engine = Engine::new(
    ///     EngineOptions::new(HostLanguage::Python, "doctest".into(), "0".into())
    ///         .with_resolver(resolver.clone()),
    /// );
    /// let token = CancellationToken::new();
    /// let plan = engine.compile(&Request::new("spec.yaml"), &token).unwrap();
    /// resolver.insert("/spec.yaml", "input.csv", "data:1", Arc::from(&b"ID\n1\n2\n"[..]));
    /// let build = plan.run(&RunRequest::new(), &token).unwrap();
    /// if cfg!(feature = "yaml") {
    ///     assert!(plan.report().is_empty());
    ///     assert_eq!(build.output().unwrap().rows(), 2);
    /// } else {
    ///     assert!(!plan.report().is_empty());
    ///     assert!(build.output().is_none());
    /// }
    /// ```
    pub fn compile(&self, request: &Request<'_>, token: &CancellationToken) -> Result<Plan, Error> {
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let measurements = Arc::new(Measurements::new());
        Ok(self.compile_observed(request, &measurements, self.shared.observer.as_deref()))
    }

    fn compile_observed(
        &self,
        request: &Request<'_>,
        measurements: &Arc<Measurements>,
        progress: Option<&dyn Progress>,
    ) -> Plan {
        let bridge = ObserverBridge::new(measurements, progress);
        let compiled = crate::internal::compile(
            &self.shared,
            request.specification,
            request.environment,
            yamaa_core::application_observer::Scope::new(&bridge),
        );
        Plan::new(
            Arc::clone(&self.shared),
            request.specification.to_owned(),
            compiled,
        )
    }

    /// Prepare, bind, execute and retain one build: exactly one
    /// [`Engine::compile`] followed by one [`Plan::run`], with the same
    /// events. Every adjudicated outcome returns an owned build carrying its
    /// output, run report, observations and diagnostics; only cancellation
    /// fails the call itself.
    ///
    /// # Example
    ///
    /// The same in-memory setup as [`Engine::check`]: Arrow tables and typed
    /// diagnostics come out. Lanes without the `yaml` feature keep a build
    /// carrying diagnostics but no output table.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use arrow_array::RecordBatchIterator;
    /// use arrow_schema::Schema;
    /// use yamaa::{
    ///     CancellationToken, DataType, Engine, EngineOptions, FFI_ArrowArrayStream, Field,
    ///     Float64Array, HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch,
    ///     Request,
    /// };
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n"[..],
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
    /// let build = engine.build(&Request::new("spec.yaml"), &token).unwrap();
    /// if cfg!(feature = "yaml") {
    ///     let output = build.output().unwrap();
    ///     assert_eq!(output.rows(), 2);
    ///     assert!(!output.ipc().unwrap().is_empty());
    ///     assert!(build.report().is_empty());
    /// } else {
    ///     assert!(build.output().is_none());
    ///     assert!(!build.report().is_empty());
    /// }
    /// ```
    pub fn build(&self, request: &Request<'_>, token: &CancellationToken) -> Result<Build, Error> {
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        // One measurement owner sees both halves, so a build's evidence is
        // the compilation's events followed by the run's.
        let measurements = Arc::new(Measurements::new());
        let progress = self.shared.observer.as_deref();
        let plan = self.compile_observed(request, &measurements, progress);
        measurements.plan(if plan.report().is_empty() {
            "compiled"
        } else {
            "refused"
        });
        // The compilation above has just verified the metadata it captured.
        let build = plan.run_observed(None, false, &measurements, progress, token);
        if build.interrupted() || token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        measurements.complete(build.accepted());
        let subject = plan.name().unwrap_or(request.specification);
        Ok(build.with_subject(subject))
    }

    /// Build `request` through `plans`, as a host's lazy object runs (#2821).
    /// `identity` is what the request resolves to now. A plan kept for that
    /// identity, whose files the run-start check finds unchanged, runs again;
    /// otherwise the request compiles and runs exactly as [`Engine::build`],
    /// and `plans` keeps the new plan if the request still resolves to
    /// `identity` once compiled. Either way the run uses this engine's
    /// project functions and observer. In a project run, an input whose
    /// `schema` link names a producer the run already built reads that `held`
    /// artifact; every other input reads its saved file.
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
        let progress = self.shared.observer.as_deref();
        // The run-start check runs outside the holder's lock.
        let kept = identity
            .as_ref()
            .and_then(|identity| plans.get(identity))
            .filter(|plan| plan.reusable_by(&self.shared));
        let (build, name) = match kept {
            Some(plan) => {
                measurements.plan("reused");
                let build = plan.run_through(&self.shared, held, &measurements, progress, token);
                (build, plan.name().map(str::to_owned))
            }
            None => {
                let plan = self.compile_observed(request, &measurements, progress);
                measurements.plan(if plan.report().is_empty() {
                    "compiled"
                } else {
                    "refused"
                });
                // The compilation has just captured the files it holds.
                let build = plan.run_through(&self.shared, held, &measurements, progress, token);
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
    /// that explain why, named by `subject` in what it prints.
    pub(crate) fn refused_build(&self, issues: Vec<Diagnostic>, subject: &str) -> Build {
        let measurements = Arc::new(Measurements::new());
        measurements.complete(false);
        Build::new(Err(issues), Vec::new(), None, measurements, &self.shared).with_subject(subject)
    }

    pub(crate) fn shared(&self) -> &EngineShared {
        &self.shared
    }

    /// Resolve one specification entry into its owned inspection artifact,
    /// reading through the engine's resolver. The artifact carries no
    /// reusable executable code; loading it performs no source access, path
    /// resolution, callbacks or recompilation.
    ///
    /// # Example
    ///
    /// Unknown entries reject with their report in every lane, and unknown
    /// text fails loading with a stable code. With the `yaml` feature the
    /// artifact round-trips through JSON.
    ///
    /// ```
    /// use yamaa::{Engine, EngineOptions, HostLanguage, Request, ResolvedSpecification};
    ///
    /// let engine = Engine::new(EngineOptions::new(
    ///     HostLanguage::Python,
    ///     "doctest".into(),
    ///     "0".into(),
    /// ));
    /// let missing = std::env::temp_dir().join("yamaa-doctest-resolve-missing.yaml");
    /// let _ = std::fs::remove_file(&missing);
    /// let error = engine
    ///     .resolve_specification(&Request::new(missing.to_str().unwrap()))
    ///     .unwrap_err();
    /// assert!(matches!(error, yamaa::Error::Rejected(_)));
    /// assert!(ResolvedSpecification::from_json("not json").is_err());
    ///
    /// if cfg!(feature = "yaml") {
    ///     let dir = std::env::temp_dir()
    ///         .join(format!("yamaa-doctest-resolve-{}", std::process::id()));
    ///     std::fs::create_dir_all(&dir).unwrap();
    ///     std::fs::write(
    ///         dir.join("spec.yaml"),
    ///         b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n",
    ///     )
    ///     .unwrap();
    ///     std::fs::write(dir.join("input.csv"), b"ID,VALUE\n1,2.5\n2,3.5\n").unwrap();
    ///     let entry = dir.join("spec.yaml");
    ///     let resolved = engine
    ///         .resolve_specification(&Request::new(entry.to_str().unwrap()))
    ///         .unwrap();
    ///     assert!(resolved.entry().ends_with("spec.yaml"));
    ///     assert_eq!(resolved.project(), None);
    ///     let roundtrip =
    ///         ResolvedSpecification::from_json(&resolved.to_json().unwrap()).unwrap();
    ///     assert_eq!(roundtrip.entry(), resolved.entry());
    ///     std::fs::remove_dir_all(&dir).unwrap();
    /// }
    /// ```
    pub fn resolve_specification(
        &self,
        request: &Request<'_>,
    ) -> Result<ResolvedSpecification, Error> {
        match yamaa_adapters::resolved_artifact::resolve(
            request.specification,
            request.environment,
            self.shared.language,
            crate::internal::resolver_bridge(&self.shared),
        ) {
            Ok(artifact) => Ok(ResolvedSpecification { inner: artifact }),
            Err(issues) => Err(Error::Rejected(Report::from_issues(&issues))),
        }
    }
}

/// Named retained byte sources: capture names with their original bytes.
pub type SourceBytes = Vec<(String, Arc<[u8]>)>;

/// An owned build: one attempt with its output, run report and observations.
/// Only the save of the result that holds it writes, through the resolver.
pub struct Build {
    /// The member's result, or the findings that explain why it has none: a
    /// refusal before it ran, or the refusal of a result its run could not
    /// build.
    result: Result<Box<Domain>, Vec<Diagnostic>>,
    /// The sources a refused compilation captured, or a run refused because
    /// its files changed since compilation; empty for any other build.
    sources: SourceBytes,
    measurements: Arc<Measurements>,
    /// The issues of the build as it was made: a later save's refusal joins
    /// the report, never these.
    built: Vec<Diagnostic>,
    report: Report,
    output: Option<Table>,
    run_report: Option<Table>,
    project_failures: Option<Vec<AttemptFailure>>,
    interrupt: Option<FunctionError>,
    /// What the build's printed text and refusals name (P-PRINT-1): a
    /// project run member's listed name, or the path the project file lists
    /// for a member whose listing could not be read (D130); otherwise the
    /// dataset name once read, or else the path as given.
    subject: String,
}

impl Build {
    /// One build of a member: its `result` and `sources`, the measurements
    /// its operation observed and, for a member with a project, the host
    /// failures its run retained, the first interrupt among them classified
    /// by the engine's project functions.
    pub(crate) fn new(
        result: Result<Box<Domain>, Vec<Diagnostic>>,
        sources: SourceBytes,
        failures: Option<Vec<AttemptFailure>>,
        measurements: Arc<Measurements>,
        shared: &EngineShared,
    ) -> Self {
        let interrupt = failures.as_ref().and_then(|failures| {
            let functions = shared.functions.as_ref()?;
            failures
                .iter()
                .find(|failure| functions.is_interrupt(&failure.error))
                .map(|failure| failure.error.clone())
        });
        let mut build = Self {
            result,
            sources,
            measurements,
            built: Vec::new(),
            report: Report::default(),
            output: None,
            run_report: None,
            project_failures: failures,
            interrupt,
            subject: String::new(),
        };
        build.built = build.issues();
        build.refresh();
        build
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
        self.report = Report::from_issues(&self.issues());
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
    pub fn output(&self) -> Option<&Table> {
        self.output.as_ref()
    }

    /// Borrow the output bytes, when the build produced them.
    pub fn output_bytes(&self) -> Option<&[u8]> {
        self.domain()?.output()
    }

    /// Borrow the run report table, which every executed build holds,
    /// success or failure. A build that never reached execution has none. A
    /// producer graph returns its entry specification's report, which a graph
    /// stopped before the entry ran does not have.
    pub fn run_report(&self) -> Option<&Table> {
        self.run_report.as_ref()
    }

    /// Borrow the run report's Arrow IPC bytes.
    pub fn run_report_bytes(&self) -> Option<&[u8]> {
        self.domain().map(|domain| domain.run_report())
    }

    /// Borrow the build report with every diagnostic.
    pub fn report(&self) -> &Report {
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
    /// one save every result shares: the declared report file, then the
    /// output, returning the paths written (D38). A failed build writes only
    /// its report file and fails with its issues. See [`crate::Run::save`].
    pub fn save(&mut self, token: &CancellationToken) -> Result<Vec<String>, Failure> {
        let subject = self.subject.clone();
        crate::outcome::save(
            &[],
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
    pub fn observations(&self, include_measurements: bool) -> Option<String> {
        let observed = self.domain().map(|domain| domain.observations_json());
        if include_measurements {
            Some(self.measurements.envelope(observed))
        } else {
            observed
        }
    }

    /// Borrow retained selection sources by name. Selection failures retain
    /// their originally captured bytes; other builds retain nothing here.
    pub fn preparation_sources(&self) -> &SourceBytes {
        &self.sources
    }

    /// Borrow the retained project host failures, when this is a project build.
    pub fn project_failures(&self) -> Option<&[AttemptFailure]> {
        self.project_failures.as_deref()
    }

    /// Borrow the first interrupt-classified project host failure, if any.
    /// Hosts propagate the original payload from this record.
    pub fn interrupt(&self) -> Option<&FunctionError> {
        self.interrupt.as_ref()
    }

    /// Whether a project function raised the host's interrupt, which cancels
    /// the call that built this (D46).
    pub(crate) fn interrupted(&self) -> bool {
        self.interrupt.is_some()
    }
}

/// An owned resolved-specification artifact: inspection facts without source
/// access, path resolution, callbacks or recompilation.
#[derive(Clone, Debug)]
pub struct ResolvedSpecification {
    inner: yamaa_adapters::resolved_artifact::ResolvedArtifact,
}

impl ResolvedSpecification {
    /// Serialize the artifact as compact UTF-8 JSON.
    pub fn to_json(&self) -> Result<String, Error> {
        self.inner.to_json().map_err(|_| Error::Internal {
            code: "resolved_serialization",
        })
    }

    /// The facts `to_json` writes, as one owned tree.
    pub(crate) fn to_value(&self) -> Result<yamaa_core::json::Value, Error> {
        self.inner.to_value().map_err(|_| Error::Internal {
            code: "resolved_serialization",
        })
    }

    /// Read back already-serialized facts. Loading never authenticates
    /// caller-edited JSON beyond format and version checks.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        yamaa_adapters::resolved_artifact::ResolvedArtifact::from_json(text)
            .map(|inner| Self { inner })
            .map_err(|_| Error::Internal {
                code: "resolved_syntax",
            })
    }

    /// Borrow the canonical identity of the requested specification entry.
    pub fn entry(&self) -> &str {
        self.inner.entry()
    }

    /// Borrow the explicit project identity, when present.
    pub fn project(&self) -> Option<&str> {
        self.inner.project()
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
    fn request_builders_keep_additive_fields_non_breaking() {
        let request = Request::new("spec");
        assert_eq!(request.environment, None);
        assert_eq!(request.with_environment("env").environment, Some("env"));
        let options = EngineOptions::new(HostLanguage::R, "r".into(), "4.3".into())
            .with_resolver(Arc::new(MemoryResolver::new()))
            .with_observer(Arc::new(NoopProgress));
        assert!(options.resolver.is_some());
        assert!(options.observer.is_some());
        assert!(options.functions.is_none());
    }

    struct NoopProgress;
    impl Progress for NoopProgress {
        fn observe(&self, _event: crate::observe::Event) {}
    }

    /// A cancelled call returns `Error::Cancelled`, with no result and no
    /// issue; a check is no exception.
    #[test]
    fn cancelled_tokens_fail_operations_before_entry() {
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        token.cancel();
        let request = Request::new("spec");
        assert!(matches!(
            engine.build(&request, &token),
            Err(Error::Cancelled)
        ));
        assert!(matches!(
            engine.check(&request, &token),
            Err(Error::Cancelled)
        ));
    }

    #[test]
    fn missing_entries_report_diagnostics_without_a_build_error() {
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        let missing = missing_spec();
        let request = Request::new(missing.to_str().unwrap());
        let report = engine.check(&request, &token).unwrap();
        assert!(!report.is_empty());
        let build = engine.build(&request, &token).unwrap();
        assert!(build.output().is_none());
        assert!(build.run_report().is_none());
        assert!(!build.report().is_empty());
        assert!(build.observations(false).is_none());
        assert!(build.preparation_sources().is_empty());
        assert!(build.project_failures().is_none());
        assert!(build.interrupt().is_none());
    }

    #[test]
    fn unsupported_project_routes_reject_without_functions() {
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        let missing = missing_spec();
        // Selection fails before any route dispatch, so a missing entry
        // stays a diagnostic build even without functions configured.
        let request = Request::new(missing.to_str().unwrap());
        assert!(engine.build(&request, &token).is_ok());
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

    #[test]
    fn unknown_resolved_text_fails_with_a_stable_code() {
        let error = ResolvedSpecification::from_json("not json").unwrap_err();
        assert!(matches!(
            error,
            Error::Internal {
                code: "resolved_syntax"
            }
        ));
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
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input/input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n",
        )
        .unwrap();
        Study(root)
    }

    #[test]
    fn plain_builds_carry_output_and_save_through_the_facade() {
        let study = study();
        let entry = study.0.join("spec.yaml");
        let engine = Engine::new(EngineOptions::new(
            HostLanguage::Python,
            "test".into(),
            "test".into(),
        ));
        let token = CancellationToken::new();
        let request = Request::new(entry.to_str().unwrap());
        let report = engine.check(&request, &token).unwrap();
        assert!(report.is_empty());
        let build = engine.build(&request, &token).unwrap();
        assert!(build.report().is_empty());
        let output = build.output().unwrap();
        assert_eq!(output.rows(), 2);
        assert!(build.output_bytes().is_some());
        // One run, input, base and two column rows; no handler or check.
        let report = build.run_report().unwrap();
        assert_eq!(report.rows(), 5);
        assert!(build.run_report_bytes().is_some());
        assert!(build.observations(false).is_some());
        assert!(build.observations(true).is_some());
        // The specification's run saves its one file and returns its path.
        let mut run = engine
            .spec_run(entry.to_str().unwrap(), None, &token)
            .unwrap();
        let root = yamaa_adapters::canonical_path::canonicalize(&study.0).unwrap();
        let path = root.join("out.csv").to_str().unwrap().replace('\\', "/");
        assert_eq!(run.save(&CancellationToken::new()).unwrap(), [path]);
        let written = std::fs::read(study.0.join("out.csv")).unwrap();
        assert!(written.starts_with(b"ID,VALUE\n1,"));
    }
}
