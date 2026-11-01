//! The supported engine surface: one [`Engine`] over the existing prepare,
//! bind, execute and publish workflow. There is no second admission or
//! evaluation route; all physical access goes through a resolver, and local
//! files are just the default resolver.
//!
//! The use cases are the operations of the requests [`Engine::spec`] and
//! [`Engine::project`] give, in [`crate::api`], and [`Engine`]'s methods in
//! [`crate::file_type`], [`crate::odm`] and [`crate::table`]. Each build, of a
//! specification alone or of one member of a project run, is an owned build
//! internal to the facade; a [`crate::Run`] is its public view, and a result
//! saves through [`crate::Run::save`], [`crate::ProjectRun::save`] or
//! [`crate::Submission::save`], one save over its outcome.

use std::num::NonZeroUsize;
use std::sync::Arc;

use yamaa_application::file_application::Domain;
use yamaa_core::diagnostic::Diagnostic;

use crate::cancel::{CancellationToken, Cancelled};
use crate::diagnostic::{ContextValue, Locator, Report};
use crate::error::Failure;
use crate::host::functions::{FunctionError, HostLanguage, ProjectFunctions};
use crate::host::resolver::ResourceResolver;
use crate::internal::EngineShared;
use crate::plan::Plan;
use crate::progress::{Measured, Observed, ProgressCallback, ProgressEvent, Timings};
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
    /// How each run divides its driver input, or none to run it whole, the
    /// default (see [`EngineOptions::with_partitions`]).
    pub partitions: Option<Partitions>,
    /// The callback every operation reports its progress to, or none (see
    /// [`EngineOptions::with_progress`]).
    pub progress: Option<ProgressCallback>,
}

/// Partitioned execution: each run divides its driver input into partitions
/// and runs each partition through every derivation step, holding only the
/// decoded partitions it is deriving (#3830), up to four at once on threads
/// of their own by default ([`Partitions::with_threads`], #4193); the output
/// keys, checks and order then run once over every partition's rows. A run
/// polls its token while it derives its partitions as well. Its result,
/// output, issues and run report are the whole-table run's, and neither the
/// partition boundaries nor the threads change any of them.
///
/// There are two strategies:
///
/// - [`Partitions::new`] divides the input into consecutive partitions of
///   `rows` records, the last holding the rest (#3697). When the
///   specification reads records together, in a grouped row template, its
///   key grain or an ODM read of the input, a partition ends only between
///   runs of the input fields all of those reads share, so it holds as many
///   whole runs as fit in `rows` records, a longer run alone; an input not
///   grouped by those fields is one partition (#3830).
/// - [`Partitions::by_keys`] divides it into key-bounded partitions (#3763):
///   consecutive partitions that never split a group of the specification's
///   leading `keys` declared keys, each holding as many whole groups as fit
///   in `rows` records, a larger group alone. The input is one partition
///   when its records are not grouped by those keys, or when a row
///   template that reads it derives one of those keys from anything but one
///   of its fields, copied unchanged. A specification that declares no key
///   runs over consecutive partitions of `rows` records.
///
/// The partitions divide only what the partition-safety inventory lets
/// them divide. A step that needs the whole table refuses the run before
/// derivation: the run fails with `engine_rejected` (`step: execute`,
/// `code: partition_global`) at that step's spec path. Under consecutive
/// partitions that is a step that reads a window, an aggregate, a named or
/// joined relation, SELF rows or a project function, or rows driven by an
/// intermediate whose derivations do. Under key-bounded partitions a window
/// or an aggregate reads only rows of its own partition, so it does not
/// refuse, when its grouping includes every partition key, each one the
/// input holds as a field the row templates copy unchanged, and every row it
/// reads is built from the input's records: a window whose `group_by` names
/// each key, an aggregate whose `group_by` names each, a grouped row
/// template's aggregate when the template groups by each, or a qualified
/// aggregate whose `key` pairs each. Every other step that needs the whole
/// table refuses as it does under consecutive partitions.
///
/// The records a step reads together must sit in one partition (#3830), so
/// these steps refuse the run too: rows driven by an intermediate over the
/// input, and an intermediate over the input with a `unique` check; under
/// key-bounded partitions, a grouped row template, the key grain or an ODM
/// read whose fields omit a partition key, or any of them when the input
/// does not hold every partition key; under consecutive partitions, the
/// first of those reads that shares no input field with the ones before it.
/// With [`Partitions::with_whole_table_fallback`], a step that needs the
/// whole table runs over the whole input instead.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partitions {
    /// The driver records of each partition; under key-bounded partitions,
    /// the most a partition holds unless one group holds more.
    pub rows: NonZeroUsize,
    /// How many leading declared keys no partition splits a group of, or
    /// none for consecutive partitions.
    pub leading_keys: Option<NonZeroUsize>,
    /// Whether a step that needs the whole table runs over the whole input
    /// instead of refusing the run.
    pub whole_table_fallback: bool,
    /// How many partitions of one run derive at once, each on a thread of
    /// its own, and how many threads a project run's members and their
    /// partitions share (#4193); see [`Partitions::with_threads`].
    pub threads: NonZeroUsize,
}

impl Partitions {
    /// Consecutive partitions of `rows` driver records, refusing a run that
    /// has a step that needs the whole table.
    pub fn new(rows: NonZeroUsize) -> Self {
        Self {
            rows,
            leading_keys: None,
            whole_table_fallback: false,
            threads: Self::default_threads(),
        }
    }

    /// Key-bounded partitions over the leading `keys` declared keys, each
    /// holding at most `rows` driver records unless one group holds more,
    /// refusing a run that has a step that needs the whole table (#3763).
    ///
    /// # Example
    ///
    /// The row number restarts for each `ID`, so the window groups by the
    /// one partition key and each subject's records run as a partition of
    /// their own.
    ///
    /// ```
    /// use std::num::NonZeroUsize;
    /// use std::sync::Arc;
    ///
    /// use yamaa::{Engine, EngineOptions, HostLanguage, MemoryResolver, Partitions};
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID, SEQ]\ninput: {SRC: {path: input.csv, types: {SEQ: int}}}\noutput: {path: out.csv, columns: [ID, SEQ, N]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: SEQ, type: int, derivation: SRC.SEQ}\n  - {name: N, type: int, derivation: {row_number: {window: {group_by: [ID], order_by: [SEQ]}}}}\n"[..],
    /// ));
    /// resolver.insert("/spec.yaml", "input.csv", "input:1", Arc::from(&b"ID,SEQ\na,2\na,1\nb,1\n"[..]));
    /// let engine = Engine::new(
    ///     EngineOptions::new(HostLanguage::Python, "doctest".into(), "0".into())
    ///         .with_resolver(resolver)
    ///         .with_partitions(Partitions::by_keys(NonZeroUsize::MIN, NonZeroUsize::MIN)),
    /// );
    /// let run = engine.spec("spec.yaml").run().unwrap();
    /// if cfg!(feature = "yaml") {
    ///     assert!(run.diagnostics().is_empty(), "{:?}", run.diagnostics());
    ///     assert_eq!(run.output().unwrap().rows(), 3);
    /// }
    /// ```
    pub fn by_keys(keys: NonZeroUsize, rows: NonZeroUsize) -> Self {
        Self {
            rows,
            leading_keys: Some(keys),
            whole_table_fallback: false,
            threads: Self::default_threads(),
        }
    }

    /// The same partitions, running a step that needs the whole table over
    /// the whole input instead of refusing the run.
    pub fn with_whole_table_fallback(mut self) -> Self {
        self.whole_table_fallback = true;
        self
    }

    /// The same partitions, up to `threads` of one run deriving at once,
    /// each on a thread of its own (#4193), as the hosts' `threads` argument
    /// and the command line's `--threads` ask ([`Self::with_host_threads`]).
    /// Each thread decodes the partitions it derives from the held bytes of
    /// a CSV or Parquet driver input; the calling thread reports progress
    /// and polls the cancellation token. The result, its issues, its report
    /// and the progress events, in order, are those of a run on one thread.
    /// A run with a step that calls a project function runs on the calling
    /// thread alone, as does a run with one partition.
    ///
    /// A project run builds its members at once on the same threads: a
    /// member starts once every member whose artifact it reads has
    /// finished, while a thread is free, and its partitions take an equal
    /// share of the free threads among the members ready to start, at least
    /// one, so the members and their partitions never hold more than
    /// `threads`. The calling thread publishes the members in run order,
    /// so the result, its issues, its reports and the progress events, in
    /// order, are those of a run on one thread; a member's events reach the
    /// callback once it has built. A member that calls a project function
    /// runs on the calling thread in its turn, as does every member of a
    /// run that reads through a caller resolver.
    ///
    /// Without it, up to four partitions derive at once, and no more than
    /// the threads the process may run on
    /// ([`std::thread::available_parallelism`]). Each partition deriving at
    /// once holds its decoded records and its run's working state, so peak
    /// memory grows with the threads.
    pub fn with_threads(mut self, threads: NonZeroUsize) -> Self {
        self.threads = threads;
        self
    }

    /// The same partitions, with the threads a host's `threads` argument
    /// gives (#4193): a positive whole number, which R may give as a whole
    /// double and which saturates at the most this platform counts, or a
    /// missing value, which keeps the default ([`Self::with_threads`]).
    /// `None` for any other value, which the host refuses as an argument it
    /// cannot take.
    pub fn with_host_threads(self, threads: &ContextValue) -> Option<Self> {
        let count = match threads {
            ContextValue::Null => return Some(self),
            ContextValue::Int(count) => {
                (*count >= 1).then(|| usize::try_from(*count).unwrap_or(usize::MAX))
            }
            // An integer beyond 64 bits, as Python gives it.
            ContextValue::Wide(count) => (count.bytes().all(|digit| digit.is_ascii_digit())
                && count.bytes().any(|digit| digit != b'0'))
            .then_some(usize::MAX),
            ContextValue::Float(count) if count.is_finite() && count.fract() == 0.0 => {
                (*count >= 1.0).then_some(*count as usize)
            }
            _ => None,
        };
        count
            .and_then(NonZeroUsize::new)
            .map(|count| self.with_threads(count))
    }

    /// The most partitions of one run that derive at once by default
    /// (#4193): each one more holds one more decoded partition and its
    /// run's working state, and a run on a shared machine keeps to a few of
    /// its cores unless asked for more.
    const DEFAULT_THREADS: NonZeroUsize = match NonZeroUsize::new(4) {
        Some(threads) => threads,
        None => unreachable!(),
    };

    /// The default threads, or fewer when the process may run on fewer.
    fn default_threads() -> NonZeroUsize {
        std::thread::available_parallelism().map_or(NonZeroUsize::MIN, |available| {
            available.min(Self::DEFAULT_THREADS)
        })
    }

    pub(crate) fn into_core(self) -> yamaa_core::dataset::Partitioning {
        yamaa_core::dataset::Partitioning {
            rows: self.rows,
            leading_keys: self.leading_keys,
            whole_table_fallback: self.whole_table_fallback,
            // The run sets it for a SAS or ODM driver input (D264).
            whole_output: false,
            threads: self.threads,
        }
    }
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
    /// `unsupported_operation` with operation `environment` (D110) at the
    /// first derivation that calls one (D221), and functions given to
    /// [`EngineOptions::with_functions`] are never called.
    /// Every function file a 1.0 project lists is still read and admitted,
    /// so a check or resolve of a specification that calls one runs as a
    /// host's does. Every build that calls no project function runs as a
    /// host runs it, with or without a project.
    pub fn neutral(runtime_name: String, runtime_version: String) -> Self {
        Self {
            resolver: None,
            host_language: None,
            runtime_name,
            runtime_version,
            functions: None,
            partitions: None,
            progress: None,
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

    /// Run every build divided into `partitions` (#3697, #3763), an engine
    /// mode with no host surface. The Python and R bindings and the command
    /// line run every build over key-bounded partitions with the whole-table
    /// fallback (#3794); an engine that sets none runs every build whole.
    ///
    /// # Example
    ///
    /// A specification whose steps are all partition-safe builds the same
    /// output one record at a time as it does whole.
    ///
    /// ```
    /// use std::num::NonZeroUsize;
    /// use std::sync::Arc;
    ///
    /// use yamaa::{Engine, EngineOptions, HostLanguage, MemoryResolver, Partitions};
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [ID, NAME]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: NAME, type: str, derivation: SRC.NAME}\n"[..],
    /// ));
    /// resolver.insert("/spec.yaml", "input.csv", "input:1", Arc::from(&b"ID,NAME\n1,alpha\n2,beta\n3,gamma\n"[..]));
    /// let engine = Engine::new(
    ///     EngineOptions::new(HostLanguage::Python, "doctest".into(), "0".into())
    ///         .with_resolver(resolver)
    ///         .with_partitions(Partitions::new(NonZeroUsize::MIN)),
    /// );
    /// let run = engine.spec("spec.yaml").run().unwrap();
    /// if cfg!(feature = "yaml") {
    ///     assert!(run.diagnostics().is_empty(), "{:?}", run.diagnostics());
    ///     assert_eq!(run.output().unwrap().rows(), 3);
    /// }
    /// ```
    pub fn with_partitions(mut self, partitions: Partitions) -> Self {
        self.partitions = Some(partitions);
        self
    }

    /// Report every operation's progress to `progress` (#3762): each phase
    /// of a check, a run or a submission as it starts and ends, each
    /// partition a partitioned run enters and each batch a streamed run's
    /// reader hands out. See [`crate::progress`] for when it is called and
    /// what a callback that fails does to the result.
    ///
    /// # Example
    ///
    /// A run of a specification served from memory reports its phases, in
    /// order, and its result gives the time each took.
    ///
    /// ```
    /// use std::sync::{Arc, Mutex};
    ///
    /// use yamaa::{Engine, EngineOptions, HostLanguage, MemoryResolver, ProgressEvent};
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n"[..],
    /// ));
    /// resolver.insert("/spec.yaml", "input.csv", "input:1", Arc::from(&b"ID\n1\n"[..]));
    /// let heard = Arc::new(Mutex::new(Vec::new()));
    /// let kept = Arc::clone(&heard);
    /// let engine = Engine::new(
    ///     EngineOptions::new(HostLanguage::Python, "doctest".into(), "0".into())
    ///         .with_resolver(resolver)
    ///         .with_progress(move |event: &ProgressEvent| {
    ///             kept.lock().unwrap().push(format!("{} {}", event.kind.as_str(), event.phase.as_str()));
    ///             Ok(())
    ///         }),
    /// );
    /// let run = engine.spec("spec.yaml").run().unwrap();
    /// let heard = heard.lock().unwrap();
    /// assert_eq!(heard.first().map(String::as_str), Some("start preparation"));
    /// // Without the `yaml` feature the specification cannot be read.
    /// let last = if cfg!(feature = "yaml") { "end output" } else { "end preparation" };
    /// assert_eq!(heard.last().map(String::as_str), Some(last));
    /// assert_eq!(run.timings().entries().len() * 2, heard.len());
    /// ```
    pub fn with_progress(
        mut self,
        progress: impl Fn(&ProgressEvent) -> Result<(), FunctionError> + Send + Sync + 'static,
    ) -> Self {
        self.progress = Some(Arc::new(progress));
        self
    }
}

/// One build or check request: the entry path or resolver reference plus an
/// optional project file.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Request<'a> {
    pub(crate) specification: &'a str,
    pub(crate) environment: Option<&'a str>,
    pub(crate) inputs: Option<&'a std::collections::BTreeMap<String, crate::InputFrame>>,
}

impl<'a> Request<'a> {
    /// Request the specification entry with no project.
    pub(crate) fn new(specification: &'a str) -> Self {
        Self {
            specification,
            environment: None,
            inputs: None,
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
    fn compile(&self, request: &Request<'_>, observed: &Observed<'_>) -> Plan {
        let compiled = crate::internal::compile(
            &self.shared,
            request.specification,
            request.environment,
            yamaa_core::application_observer::Scope::new(observed),
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
    /// it), stops the call as [`Cancelled`] and is dropped. `observed` takes
    /// the build's events and its measurements.
    pub(crate) fn build_reusing(
        &self,
        plans: &crate::api::Plans,
        request: &Request<'_>,
        identity: Option<crate::api::PlanKey>,
        held: Option<Arc<yamaa_adapters::file_resources::HeldArtifacts>>,
        observed: &Observed<'_>,
        token: &CancellationToken,
    ) -> Result<Build, Cancelled> {
        let (plan, compiled) = self.plan_reusing(plans, request, identity.as_ref(), observed);
        // A compilation has just captured the files it holds.
        let build = plan.run_through(
            &self.shared,
            crate::internal::RunInputs {
                held,
                frames: request.inputs.cloned().unwrap_or_default(),
                threads: None,
            },
            observed,
            token,
        );
        if compiled {
            self.keep(plans, request, identity, &plan);
        }
        Self::finish(build, &plan, request, observed, token)
    }

    /// The end of a build of `plan` for `request`: a build that `token`
    /// finds cancelled, or whose project function raised the host's
    /// interrupt, stops the call as [`Cancelled`]; any other completes its
    /// measurements and is named by its dataset, or else by the path.
    pub(crate) fn finish(
        build: Build,
        plan: &Plan,
        request: &Request<'_>,
        observed: &Observed<'_>,
        token: &CancellationToken,
    ) -> Result<Build, Cancelled> {
        if build.interrupted() || token.is_cancelled() {
            return Err(Cancelled);
        }
        observed.measurements().complete(build.accepted());
        let subject = plan.name().unwrap_or(request.specification);
        Ok(build.with_subject(subject))
    }

    /// The plan `request` runs: the one `plans` keeps for `identity`, what
    /// the request resolves to now, when the run-start check finds its files
    /// unchanged, or else a new compilation, with `true`. The choice is
    /// counted in the measurements `observed` takes, which observes a
    /// compilation.
    pub(crate) fn plan_reusing(
        &self,
        plans: &crate::api::Plans,
        request: &Request<'_>,
        identity: Option<&crate::api::PlanKey>,
        observed: &Observed<'_>,
    ) -> (Plan, bool) {
        let measurements = observed.measurements();
        // The run-start check runs outside the holder's lock.
        let kept = identity
            .and_then(|identity| plans.get(identity))
            .filter(|plan| plan.reusable_by(&self.shared));
        if let Some(plan) = kept {
            measurements.plan("reused");
            return (plan, false);
        }
        let plan = self.compile(request, observed);
        measurements.plan(if plan.report().is_empty() {
            "compiled"
        } else {
            "refused"
        });
        (plan, true)
    }

    /// Keep the newly compiled `plan` in `plans` for `identity`, the identity
    /// it was compiled from: a request that resolves elsewhere meanwhile, or
    /// a refused compilation, keeps nothing, so its next run compiles again.
    pub(crate) fn keep(
        &self,
        plans: &crate::api::Plans,
        request: &Request<'_>,
        identity: Option<crate::api::PlanKey>,
        plan: &Plan,
    ) {
        if let Some(identity) = identity {
            let stable = crate::internal::request_identity(&self.shared, request).as_ref()
                == Some(&identity);
            plans.replace(identity, stable.then_some(plan));
        }
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
        let measured = Measured::unobserved(subject);
        measured.measurements().complete(false);
        Build::new(Err(issues), Vec::new(), measured, &self.shared, locator).with_subject(subject)
    }

    pub(crate) fn shared(&self) -> &EngineShared {
        &self.shared
    }

    /// The engine's shared configuration, owned, for a run that outlives
    /// the call that started it.
    #[cfg(feature = "arrow")]
    pub(crate) fn shared_owned(&self) -> Arc<EngineShared> {
        Arc::clone(&self.shared)
    }

    /// Resolve one specification entry into its owned inspection artifact,
    /// reading through the engine's resolver; a refusal is the report of its
    /// issues.
    pub(crate) fn resolve_specification(
        &self,
        request: &Request<'_>,
    ) -> Result<ResolvedSpecification, Report> {
        match yamaa_application::resolved_artifact::resolve_located(
            request.specification,
            request.environment,
            self.shared.language,
            crate::internal::resolver_bridge(&self.shared),
        ) {
            Ok(artifact) => Ok(ResolvedSpecification { inner: artifact }),
            Err((issues, locator)) => Err(Report::located(&issues, &locator)),
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
    /// What the build's timings are read from: its measurements, which also
    /// count its saves.
    measured: Measured,
    /// The issues of the build as it was made: a later save's refusal joins
    /// the report, never these.
    built: Vec<Diagnostic>,
    report: Report,
    /// The output table, built when first read (#4201).
    output: std::sync::OnceLock<Option<Table>>,
    /// The rows and columns of an accepted output read as a stream, which
    /// builds no table (#3697).
    streamed: Option<(usize, usize)>,
    run_report: Option<Table>,
    /// The host failures the attempt retained. The build owns them, and so
    /// their original host objects, for as long as it lives, as the host
    /// interface's ownership rules ask; nothing reads them once built.
    _failures: Vec<FunctionError>,
    progress_cause: Option<crate::host::functions::HostPayload>,
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
        measured: Measured,
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
            measured,
            built: Vec::new(),
            report: Report::default(),
            output: std::sync::OnceLock::new(),
            streamed: None,
            run_report: None,
            _failures: failures,
            progress_cause: None,
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
        self.output = std::sync::OnceLock::new();
        self.run_report = self
            .domain()
            .map(|domain| Table::from_adapter(domain.run_report()));
    }

    /// Whether the build produced its primary output, as a table or to be
    /// read as a stream.
    pub(crate) fn accepted(&self) -> bool {
        self.domain().is_some_and(Domain::accepted)
    }

    /// The accepted output's columns among the accepted dataset's, which the
    /// batches of a stream of it project.
    #[cfg(feature = "arrow")]
    pub(crate) fn projection(&self) -> Option<&[usize]> {
        self.domain()?.projection()
    }

    /// Record that the accepted output, of `rows` rows and `columns`
    /// columns, is read as a stream instead of a table.
    #[cfg(feature = "arrow")]
    pub(crate) fn streamed(&mut self, rows: usize, columns: usize) {
        self.streamed = Some((rows, columns));
    }

    /// The accepted output's rows and columns, whether it is a table or read
    /// as a stream; none when the build has no output.
    pub(crate) fn dimensions(&self) -> Option<(usize, usize)> {
        match self.domain().and_then(Domain::output_dimensions) {
            Some(dimensions) => Some(dimensions),
            None => self.streamed.filter(|_| self.accepted()),
        }
    }

    /// Borrow the output table, when the build produced one; the first
    /// borrow builds it.
    pub(crate) fn output(&self) -> Option<&Table> {
        self.output
            .get_or_init(|| {
                self.domain()
                    .and_then(Domain::output)
                    .map(Table::from_adapter)
            })
            .as_ref()
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
        } else if self.accepted() && !crate::outcome::fails(issues) {
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
            Vec<yamaa_application::publication::File<'_>>,
            yamaa_application::file_application::Admitted,
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
        admitted: yamaa_application::file_application::Admitted,
        committed: usize,
        stopped: Option<(usize, yamaa_application::publication::Cause)>,
    ) {
        if let Ok(domain) = &mut self.result {
            domain.record(admitted, committed, stopped);
        }
        self.refresh();
    }

    /// The phases the build ran, in the order they started (#3762).
    pub(crate) fn timings(&self) -> Timings {
        self.measured.timings()
    }

    /// Fail the build with its issues and then `failure`, as when its
    /// operation's progress callback failed (#3762): it keeps no output or
    /// run report, so its save writes nothing, and its timings stay.
    pub(crate) fn fail(&mut self, failure: Vec<Diagnostic>) {
        let mut issues = self.issues();
        issues.extend(failure);
        self.result = Err(issues);
        self.streamed = None;
        self.built = self.issues();
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
        self.measured.measurements().save_entered();
        self.measured.measurements().save_completed(outcome);
    }

    /// Save this build as a run of one specification saves it, through the
    /// one save every result shares; see [`crate::Run::save`].
    pub(crate) fn save(&mut self, token: &CancellationToken) -> Result<Vec<String>, Failure> {
        let subject = self.subject.clone();
        let cause = self.progress_cause.clone();
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
        .map_err(|failure| failure.with_cause(cause))
    }

    pub(crate) fn fail_progress(
        &mut self,
        reporter: &crate::progress::Reporter,
        step: yamaa_core::application_issue::Step,
    ) {
        if reporter.failed() {
            self.progress_cause = reporter.cause();
            self.fail(vec![reporter.failure(step)]);
        }
    }

    /// The build's observations as JSON; none when it never ran.
    pub(crate) fn observations(&self) -> Option<String> {
        self.domain().map(|domain| domain.observations_json())
    }

    /// The build's observations enveloped with its operation's measurements.
    pub(crate) fn measured_observations(&self) -> String {
        self.measured.measurements().envelope(self.observations())
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
    inner: yamaa_application::resolved_artifact::ResolvedArtifact,
}

impl ResolvedSpecification {
    /// The facts the artifact holds, as one owned tree, or none when the
    /// engine cannot represent them.
    pub(crate) fn to_value(&self) -> Option<yamaa_core::json::Value> {
        self.inner.to_value().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::resolver::MemoryResolver;

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

    /// A cancelled call fails as cancelled, with no result and no issue; a
    /// check is no exception.
    #[test]
    fn cancelled_tokens_fail_operations_before_entry() {
        use crate::error::FailureKind;
        let engine = Engine::new(options());
        let token = CancellationToken::new();
        token.cancel();
        let request = engine.spec("spec").cancel_with(&token);
        for failure in [request.run().err(), request.check().err()] {
            let failure = failure.expect("a cancelled call fails");
            assert!(matches!(failure.kind, FailureKind::Cancelled));
            assert_eq!(failure.message, "cannot read spec: operation cancelled");
            assert!(failure.diagnostics.is_empty());
        }
    }

    #[test]
    fn missing_entries_report_diagnostics_without_a_build_error() {
        let engine = Engine::new(options());
        let missing = missing_spec();
        let request = engine.spec(missing.to_str().unwrap());
        let check = request.check().unwrap();
        assert!(!check.diagnostics().is_empty());
        let run = request.run().unwrap();
        assert!(run.output().is_none());
        assert!(run.report().is_none());
        assert!(!run.diagnostics().is_empty());
        assert!(run.observations().is_none());
        assert!(!run.build.interrupted());
    }

    #[test]
    fn a_failed_run_saves_nothing_and_refuses_with_its_issues() {
        let engine = Engine::new(options());
        let missing = missing_spec();
        let mut run = engine.spec(missing.to_str().unwrap()).run().unwrap();
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
        let request = engine.spec(entry.to_str().unwrap()).cancel_with(&token);
        let check = request.check().unwrap();
        assert!(check.diagnostics().is_empty());
        let mut run = request.run().unwrap();
        assert!(run.diagnostics().is_empty());
        assert_eq!(run.output().unwrap().rows(), 2);
        // One run, input, base and two column rows; no handler or check.
        assert_eq!(run.report().unwrap().rows(), 5);
        // The measured observations hold the observations as they are.
        let observed = run.observations().expect("a run that began observes");
        assert!(run.measured_observations().contains(&observed));
        // The specification's run saves its one file and returns its path.
        let root = yamaa_adapters::canonical_path::canonicalize(&study.0).unwrap();
        let out = root.join("out.csv").to_str().unwrap().replace('\\', "/");
        assert_eq!(run.save(&token).unwrap(), [out]);
        let written = std::fs::read(study.0.join("out.csv")).unwrap();
        assert!(written.starts_with(b"ID,VALUE\n1,"));
    }
}
