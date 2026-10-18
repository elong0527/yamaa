//! The supported engine surface: one [`Engine`] over the existing prepare,
//! bind, execute and publish workflow. There is no second admission or
//! evaluation route; all physical access goes through a resolver, and local
//! files are just the default resolver.
//!
//! A rejected build that still owns diagnostics, evidence or retained sources
//! returns an owned [`Build`] carrying them, exactly as the host
//! failure-handles do. [`Error::Rejected`] is reserved for operations without
//! a handle: unsupported routes, failed saves and failed specification
//! resolution.

use std::sync::Arc;

use crate::cancel::CancellationToken;
use crate::diagnostic::Report;
use crate::error::{Error, FunctionError};
use crate::functions::{HostLanguage, ProjectFunctions};
use crate::internal::{
    outcome_accepted, outcome_report, run_check, snapshot_failures,
    AttemptFailure as InternalFailure, BuildOutcome, EngineShared, ObserverBridge,
};
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

impl AttemptFailure {
    pub(crate) fn from_internal(failure: &InternalFailure) -> Self {
        Self {
            stage: failure.stage,
            error: failure.error.clone(),
        }
    }
}

/// Engine configuration. Options and request types are `#[non_exhaustive]`
/// structs, so the `new` plus `with_*` constructors below are the only way
/// to build them, keeping additive field changes non-breaking.
#[non_exhaustive]
#[derive(Default)]
pub struct EngineOptions {
    pub resolver: Option<Arc<dyn ResourceResolver>>,
    pub host_language: HostLanguage,
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
            resolver: None,
            host_language,
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
    /// Project routes without functions report unsupported.
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

/// One define request: environment, standard, installed version and optional
/// creation timestamp.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefineRequest<'a> {
    pub environment: &'a str,
    pub standard: &'a str,
    pub created: Option<&'a str>,
    pub installed: &'a str,
}

impl<'a> DefineRequest<'a> {
    /// Request metadata composition for an environment and standard.
    pub fn new(environment: &'a str, standard: &'a str, installed: &'a str) -> Self {
        Self {
            environment,
            standard,
            created: None,
            installed,
        }
    }

    /// Set the creation timestamp.
    pub fn with_created(mut self, created: &'a str) -> Self {
        self.created = Some(created);
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

    /// Static checks only, infallible at the boundary: admission and
    /// preparation failures come back as diagnostics in the returned report,
    /// never as a second error channel. A cancelled token returns a report
    /// with a single cancelled diagnostic.
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
    /// let report = engine.check(&Request::new("spec.yaml"), &token);
    /// if cfg!(feature = "yaml") {
    ///     assert!(report.is_empty());
    /// } else {
    ///     assert!(!report.is_empty());
    /// }
    /// ```
    pub fn check(&self, request: &Request<'_>, token: &CancellationToken) -> Report {
        Report::from_issues(&run_check(
            &self.shared,
            request.specification,
            request.environment,
            token,
        ))
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
        let measurements = yamaa_adapters::application_measurements::Measurements::new();
        Ok(self.compile_observed(request, &measurements, self.shared.observer.as_deref()))
    }

    fn compile_observed(
        &self,
        request: &Request<'_>,
        measurements: &yamaa_adapters::application_measurements::Measurements,
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
    /// outputs, logs, observations and diagnostics; only cancellation fails
    /// the call itself.
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
        let measurements = Arc::new(yamaa_adapters::application_measurements::Measurements::new());
        let progress = self.shared.observer.as_deref();
        let plan = self.compile_observed(request, &measurements, progress);
        let outcome = plan.run_observed(None, &measurements, progress);
        measurements.complete(outcome_accepted(&outcome));
        Ok(Build::from_outcome(outcome, measurements, &self.shared))
    }

    /// Compose submission metadata without running a specification.
    ///
    /// # Example
    ///
    /// A missing environment reports typed diagnostics in every lane instead
    /// of composing, and its failed save rejects with the same report.
    ///
    /// ```
    /// use yamaa::{CancellationToken, DefineRequest, Engine, EngineOptions, HostLanguage};
    ///
    /// let engine = Engine::new(EngineOptions::new(
    ///     HostLanguage::Python,
    ///     "doctest".into(),
    ///     "0".into(),
    /// ));
    /// let token = CancellationToken::new();
    /// let missing = std::env::temp_dir().join("yamaa-doctest-define-missing.yaml");
    /// let _ = std::fs::remove_file(&missing);
    /// let define = engine
    ///     .define(&DefineRequest::new(missing.to_str().unwrap(), "sdtm", "1.0"), &token)
    ///     .unwrap();
    /// assert!(!define.report().is_empty());
    /// assert!(define.artifacts().is_empty());
    /// let mut define = define;
    /// assert!(define.save().is_err());
    /// ```
    pub fn define(
        &self,
        request: &DefineRequest<'_>,
        token: &CancellationToken,
    ) -> Result<DefineBuild, Error> {
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let inner = yamaa_adapters::file_define::Define::prepare(
            request.environment,
            request.standard,
            request.created,
            request.installed,
            self.shared.language,
        );
        Ok(DefineBuild::from_define(inner))
    }

    /// Resolve one specification entry into its owned inspection artifact.
    /// The artifact carries no reusable executable code; loading it performs
    /// no source access, path resolution, callbacks or recompilation.
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
    ///     assert_eq!(resolved.environment(), None);
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
        ) {
            Ok(artifact) => Ok(ResolvedSpecification { inner: artifact }),
            Err(issues) => Err(Error::Rejected(Report::from_issues(&issues))),
        }
    }
}

/// Named retained byte sources: capture names with their original bytes.
pub type SourceBytes = Vec<(String, Arc<[u8]>)>;

/// An owned build: one attempt with its outputs, logs and observations.
/// `save` is the only mutating operation and publishes through the resolver.
pub struct Build {
    state: BuildOutcome,
    measurements: Arc<yamaa_adapters::application_measurements::Measurements>,
    report: Report,
    output: Option<Table>,
    verification: Option<Table>,
    warning: Option<Table>,
    project_failures: Option<Vec<AttemptFailure>>,
    interrupt: Option<FunctionError>,
}

impl Build {
    pub(crate) fn from_outcome(
        outcome: BuildOutcome,
        measurements: Arc<yamaa_adapters::application_measurements::Measurements>,
        shared: &EngineShared,
    ) -> Self {
        let mut build = Self {
            state: outcome,
            measurements,
            report: Report {
                diagnostics: Vec::new(),
            },
            output: None,
            verification: None,
            warning: None,
            project_failures: None,
            interrupt: None,
        };
        build.refresh();
        build.capture_evidence(shared);
        build
    }

    fn refresh(&mut self) {
        self.report = outcome_report(&self.state);
        let (output, verification, warning) = tables_of(&self.state);
        self.output = output;
        self.verification = verification;
        self.warning = warning;
    }

    fn capture_evidence(&mut self, shared: &EngineShared) {
        let is_interrupt = |error: &FunctionError| {
            shared
                .functions
                .as_ref()
                .is_some_and(|functions| functions.is_interrupt(error))
        };
        if let BuildOutcome::Project { attempt, .. } = &self.state {
            let mut borrowed = Vec::new();
            yamaa_adapters::project_attempt::visit_host_failures(&**attempt, |stage, error| {
                borrowed.push((stage, error));
            });
            let failures: Vec<AttemptFailure> = snapshot_failures(borrowed)
                .iter()
                .map(AttemptFailure::from_internal)
                .collect();
            self.interrupt = failures
                .iter()
                .find(|failure| is_interrupt(&failure.error))
                .map(|failure| failure.error.clone());
            self.project_failures = Some(failures);
        }
    }

    /// Borrow the output table, when the build produced one.
    pub fn output(&self) -> Option<&Table> {
        self.output.as_ref()
    }

    /// Borrow the output bytes, when the build produced them.
    pub fn output_bytes(&self) -> Option<&[u8]> {
        match &self.state {
            BuildOutcome::Plain { result: Ok(domain) } => domain.output(),
            BuildOutcome::Project {
                result: Ok(domain), ..
            } => domain.output(),
            _ => None,
        }
    }

    /// Borrow the verification (`warning == false`) or warning log table.
    pub fn log(&self, warning: bool) -> Option<&Table> {
        if warning {
            self.warning.as_ref()
        } else {
            self.verification.as_ref()
        }
    }

    /// Borrow the verification or warning log bytes.
    pub fn log_bytes(&self, warning: bool) -> Option<&[u8]> {
        match &self.state {
            BuildOutcome::Plain { result: Ok(domain) } => domain_logs(domain, warning),
            BuildOutcome::Project {
                result: Ok(domain), ..
            } => domain_logs(domain, warning),
            _ => None,
        }
    }

    /// Borrow the build report with every diagnostic.
    pub fn report(&self) -> &Report {
        &self.report
    }

    /// Publish held logs and output. A failed build cannot save.
    ///
    /// # Example
    ///
    /// Saving the in-memory build from [`Engine::build`] publishes through
    /// the memory resolver; lanes without the `yaml` feature reject instead.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use arrow_array::RecordBatchIterator;
    /// use arrow_schema::Schema;
    /// use yamaa::{
    ///     CancellationToken, DataType, Engine, EngineOptions, FFI_ArrowArrayStream, Field,
    ///     Float64Array, HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch,
    ///     Request, SaveOutcome,
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
    /// let mut build = engine.build(&Request::new("spec.yaml"), &token).unwrap();
    /// if cfg!(feature = "yaml") {
    ///     assert_eq!(build.save().unwrap(), SaveOutcome::Saved);
    ///     assert!(resolver.published("memory:/out.csv").is_some());
    /// } else {
    ///     assert!(build.save().is_err());
    /// }
    /// ```
    pub fn save(&mut self) -> Result<SaveOutcome, Error> {
        use yamaa_adapters::file_application::FailedBuild;
        self.measurements.save_entered();
        let saved = match &mut self.state {
            BuildOutcome::Plain { result: Ok(domain) } => domain.save(),
            BuildOutcome::Project {
                result: Ok(domain), ..
            } => domain.save(),
            _ => Err(FailedBuild),
        };
        let outcome = match saved {
            Ok(true) => "succeeded",
            Ok(false) => "refused",
            Err(_) => "failed",
        };
        self.measurements.save_completed(outcome);
        self.refresh();
        match saved {
            Ok(true) => Ok(SaveOutcome::Saved),
            Ok(false) => Ok(SaveOutcome::NothingToSave),
            Err(_) => Err(Error::Rejected(self.report.clone())),
        }
    }

    /// Render observations, optionally enveloped with timing measurements.
    pub fn observations(&self, include_measurements: bool) -> Option<String> {
        let observed = match &self.state {
            BuildOutcome::Plain { result: Ok(domain) } => Some(domain.observations_json()),
            BuildOutcome::Project {
                result: Ok(domain), ..
            } => Some(domain.observations_json()),
            _ => None,
        };
        if include_measurements {
            Some(self.measurements.envelope(observed))
        } else {
            observed
        }
    }

    /// Borrow retained selection sources by name. Selection failures retain
    /// their originally captured bytes; other builds retain nothing here.
    pub fn preparation_sources(&self) -> &SourceBytes {
        static EMPTY: SourceBytes = Vec::new();
        match &self.state {
            BuildOutcome::SelectionFailed { sources, .. } => sources,
            _ => &EMPTY,
        }
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
}

fn tables_of(state: &BuildOutcome) -> (Option<Table>, Option<Table>, Option<Table>) {
    let tables = |output: Option<&yamaa_adapters::public_table::PublicTable>,
                  verification: Option<&yamaa_adapters::public_table::PublicTable>,
                  warning: Option<&yamaa_adapters::public_table::PublicTable>| {
        (
            output.map(Table::from_adapter),
            verification.map(Table::from_adapter),
            warning.map(Table::from_adapter),
        )
    };
    match state {
        BuildOutcome::Plain { result: Ok(domain) } => tables(
            domain.output_table(),
            domain.log_table(false),
            domain.log_table(true),
        ),
        BuildOutcome::Project {
            result: Ok(domain), ..
        } => tables(
            domain.output_table(),
            domain.log_table(false),
            domain.log_table(true),
        ),
        _ => (None, None, None),
    }
}

fn domain_logs(domain: &yamaa_adapters::file_application::Domain, warning: bool) -> Option<&[u8]> {
    if warning {
        domain.warning_log()
    } else {
        domain.verification_log()
    }
}

/// Whether saving published anything.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    NothingToSave,
}

/// One retained define artifact: path, bytes and publication state.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefineArtifact {
    pub path: String,
    pub bytes: Vec<u8>,
    pub state: String,
}

/// An owned define composition with its rendered artifacts.
/// `save` is the only mutating operation and publishes through the resolver.
pub struct DefineBuild {
    inner: yamaa_adapters::file_define::Define,
    report: Report,
}

impl DefineBuild {
    pub(crate) fn from_define(inner: yamaa_adapters::file_define::Define) -> Self {
        let report = Report::from_issues(inner.issues());
        Self { inner, report }
    }

    /// Borrow the define report with every diagnostic.
    pub fn report(&self) -> &Report {
        &self.report
    }

    /// Borrow the rendered artifacts with their publication states.
    pub fn artifacts(&self) -> Vec<DefineArtifact> {
        self.inner
            .artifacts()
            .map(|(path, bytes, state)| DefineArtifact {
                path: path.to_owned(),
                bytes: bytes.to_vec(),
                // Explicit tokens, not the adapter `Debug` text: a renamed
                // or restructured adapter variant must break this build
                // rather than silently change a public value.
                state: match state {
                    yamaa_adapters::file_define::PublicationState::Ready => "Ready",
                    yamaa_adapters::file_define::PublicationState::Published => "Published",
                    yamaa_adapters::file_define::PublicationState::Failed => "Failed",
                    yamaa_adapters::file_define::PublicationState::Stopped => "Stopped",
                }
                .to_owned(),
            })
            .collect()
    }

    /// Count resource reads captured while composing.
    pub fn capture_reads(&self) -> usize {
        self.inner.capture_reads()
    }

    /// Publish all preflighted outputs. A failed composition cannot save.
    pub fn save(&mut self) -> Result<SaveOutcome, Error> {
        match self.inner.save() {
            Ok(true) => Ok(SaveOutcome::Saved),
            Ok(false) => Ok(SaveOutcome::NothingToSave),
            Err(_) => {
                self.report = Report::from_issues(self.inner.issues());
                Err(Error::Rejected(self.report.clone()))
            }
        }
    }

    /// Borrow the publication failure message after a failed save, if any.
    pub fn save_error_message(&self) -> Option<String> {
        match self.inner.failure() {
            Some(yamaa_adapters::file_define::Failure::Publication { error, .. }) => {
                Some(error.message())
            }
            _ => None,
        }
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

    /// Borrow the explicit environment identity, when present.
    pub fn environment(&self) -> Option<&str> {
        self.inner.environment()
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
        let define = DefineRequest::new("env", "std", "1.0").with_created("now");
        assert_eq!(define.created, Some("now"));
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
            engine.define(&DefineRequest::new("env", "std", "1.0"), &token),
            Err(Error::Cancelled)
        ));
        let report = engine.check(&request, &token);
        assert_eq!(report.len(), 1);
        assert_eq!(report.diagnostics[0].code, "cancelled");
    }

    #[test]
    fn missing_entries_report_diagnostics_without_a_build_error() {
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        let missing = missing_spec();
        let request = Request::new(missing.to_str().unwrap());
        let report = engine.check(&request, &token);
        assert!(!report.is_empty());
        let build = engine.build(&request, &token).unwrap();
        assert!(build.output().is_none());
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
    fn failed_saves_reject_with_their_report() {
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        let missing = missing_spec();
        let request = Request::new(missing.to_str().unwrap());
        let mut build = engine.build(&request, &token).unwrap();
        let error = build.save().unwrap_err();
        assert!(matches!(error, Error::Rejected(_)));
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

    #[test]
    fn define_without_sources_reports_diagnostics() {
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        let missing = missing_spec();
        let request = DefineRequest::new(missing.to_str().unwrap(), "standard", "1.0");
        let define = engine.define(&request, &token).unwrap();
        assert!(!define.report().is_empty());
        assert!(define.artifacts().is_empty());
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
        let report = engine.check(&request, &token);
        assert!(report.is_empty());
        let mut build = engine.build(&request, &token).unwrap();
        assert!(build.report().is_empty());
        let output = build.output().unwrap();
        assert_eq!(output.rows(), 2);
        assert!(build.output_bytes().is_some());
        assert!(build.log(false).is_none());
        assert!(build.observations(false).is_some());
        assert!(build.observations(true).is_some());
        assert_eq!(build.save().unwrap(), SaveOutcome::Saved);
        let written = std::fs::read(study.0.join("out.csv")).unwrap();
        assert!(written.starts_with(b"ID,VALUE\n1,"));
    }
}
