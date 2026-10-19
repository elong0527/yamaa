//! Internal engine bridges. Adapters and core types never cross the facade
//! boundary; every public type in the facade surface is facade-owned and the
//! conversions below are the only place adapters or core types appear.
//!
//! Build outcomes mirror the host failure-handles: every adjudicated outcome,
//! including selection failures and projection refusals, returns an owned
//! build carrying its diagnostics, evidence and retained sources. `Rejected`
//! errors are reserved for operations without a handle (unsupported routes,
//! failed saves, failed specification resolution).

use std::convert::Infallible;
use std::sync::Arc;

use crate::cancel::CancellationToken;
use crate::diagnostic::Report;
use crate::engine::SourceBytes;
use crate::error::{FunctionError, ResolverFailure};
use crate::functions::{
    Argument, CallError, FunctionHandle, FunctionIdentity, HostLanguage, LockContext, LockKind,
    ProjectFunctions, Signature,
};
use crate::observe::{Event, Progress};
use crate::resolver::{ResolverReply, ResolverRequest, ResourceResolver};
use crate::scalar::ScalarRef;

pub(crate) type RetainedSources = SourceBytes;

/// One retained host failure with its failure stage.
#[derive(Clone, Debug)]
pub(crate) struct AttemptFailure {
    pub stage: &'static str,
    pub error: FunctionError,
}

/// The internal host error carrier. Raised and invalid-result failures stay
/// distinct through the outer invocation wrapper, preserving engine
/// diagnostics.
#[derive(Clone, Debug)]
pub(crate) struct HostFailure {
    pub error: FunctionError,
}

pub(crate) type ProjectAttempt =
    yamaa_adapters::project_run::Attempt<yamaa_adapters::file_resources::Error, HostFailure>;

pub(crate) struct ResolverBridge {
    inner: Arc<dyn ResourceResolver>,
}

impl yamaa_adapters::host_resources::Resolver for ResolverBridge {
    fn request(
        &self,
        request: yamaa_adapters::host_resources::Request<'_>,
    ) -> Result<yamaa_adapters::host_resources::Reply, yamaa_adapters::host_resources::Failure>
    {
        use yamaa_adapters::host_resources::{Kind, Operation};
        let operation = match request.operation {
            Operation::Inspect => crate::resolver::ResolverOperation::Inspect,
            Operation::Capture => crate::resolver::ResolverOperation::Capture,
            Operation::Target => crate::resolver::ResolverOperation::Target,
            Operation::Stage => crate::resolver::ResolverOperation::Stage,
            Operation::Commit => crate::resolver::ResolverOperation::Commit,
            Operation::Abort => crate::resolver::ResolverOperation::Abort,
        };
        let kind = match request.kind {
            Kind::Metadata => crate::resolver::ResolverKind::Metadata,
            Kind::Input => crate::resolver::ResolverKind::Input,
            Kind::Output => crate::resolver::ResolverKind::Output,
        };
        self.inner
            .request(ResolverRequest {
                operation,
                kind,
                declaring: request.declaring,
                written: request.written,
                reference: request.reference,
                bytes: request.bytes,
            })
            .map(
                |reply: ResolverReply| yamaa_adapters::host_resources::Reply {
                    reference: reply.reference,
                    bytes: reply.bytes,
                    frame: reply.frame.map(crate::table::InputFrame::into_adapter),
                    location: None,
                },
            )
            .map_err(|failure: ResolverFailure| {
                yamaa_adapters::host_resources::Failure::from_name(failure.code())
            })
    }
}

/// One observed operation: its measurements always, plus the caller's
/// progress port when there is one.
pub(crate) struct ObserverBridge<'a> {
    measurements: &'a yamaa_adapters::application_measurements::Measurements,
    progress: Option<&'a dyn Progress>,
}

impl<'a> ObserverBridge<'a> {
    pub(crate) fn new(
        measurements: &'a yamaa_adapters::application_measurements::Measurements,
        progress: Option<&'a dyn Progress>,
    ) -> Self {
        Self {
            measurements,
            progress,
        }
    }
}

impl yamaa_core::application_observer::Observer for ObserverBridge<'_> {
    fn observe(&self, event: yamaa_core::application_observer::Event) {
        self.measurements.observe(event);
        if let Some(progress) = self.progress {
            progress.observe(Event::from_core(event));
        }
    }
}

pub(crate) struct EngineShared {
    pub resolver: Option<Arc<ResolverBridge>>,
    pub language: yamaa_core::project_function::Language,
    pub runtime_name: String,
    pub runtime_version: String,
    pub functions: Option<Arc<dyn ProjectFunctions>>,
    pub observer: Option<Arc<dyn Progress>>,
}

impl EngineShared {
    pub(crate) fn from_options(options: &crate::engine::EngineOptions) -> Self {
        Self {
            resolver: options
                .resolver
                .clone()
                .map(|resolver| Arc::new(ResolverBridge { inner: resolver })),
            language: options.host_language.into_core(),
            runtime_name: options.runtime_name.clone(),
            runtime_version: options.runtime_version.clone(),
            functions: options.functions.clone(),
            observer: options.observer.clone(),
        }
    }

    fn identity<'a>(
        &'a self,
        example: &'a str,
        specification: &'a str,
    ) -> yamaa_adapters::specification_report::Identity<'a> {
        yamaa_adapters::specification_report::Identity {
            runtime: &self.runtime_name,
            runtime_version: &self.runtime_version,
            engine_version: crate::VERSION,
            example,
            specification,
            base_directory: ".",
        }
    }
}

pub(crate) fn resolver_bridge(
    shared: &EngineShared,
) -> Option<Arc<dyn yamaa_adapters::host_resources::Resolver>> {
    shared
        .resolver
        .clone()
        .map(|bridge| bridge as Arc<dyn yamaa_adapters::host_resources::Resolver>)
}

/// Read a project file and its listed specifications through the engine's
/// resolver: the caller's, or the local file resolver.
pub(crate) fn read_project(
    shared: &EngineShared,
    project: &str,
) -> Result<yamaa_adapters::project_scope::Project, Vec<yamaa_adapters::issue_rows::Issue>> {
    yamaa_adapters::project_scope::Project::read(project, shared.language, resolver_bridge(shared))
}

/// Bridge one caller resolver into the adapters' resource port.
pub(crate) fn bridge(
    resolver: Arc<dyn ResourceResolver>,
) -> Arc<dyn yamaa_adapters::host_resources::Resolver> {
    Arc::new(ResolverBridge { inner: resolver })
}

struct HostPort<'a> {
    functions: &'a dyn ProjectFunctions,
    lock_bytes: &'a [u8],
}

impl yamaa_core::project_activation::ActivationPort for HostPort<'_> {
    type Handle = FunctionHandle;
    type Error = HostFailure;

    fn verify_lock(
        &mut self,
        language: yamaa_core::project_function::Language,
        lock: &yamaa_core::project_environment::LockReference,
        functions: &[yamaa_core::function_signature::ProjectFunctionIdentity],
    ) -> Result<(), Self::Error> {
        let host = match language {
            yamaa_core::project_function::Language::Python => HostLanguage::Python,
            yamaa_core::project_function::Language::R => HostLanguage::R,
        };
        let context = LockContext {
            language: host,
            kind: LockKind::from_core(lock.kind),
            bytes: self.lock_bytes,
        };
        let identities: Vec<FunctionIdentity> =
            functions.iter().map(FunctionIdentity::from_core).collect();
        self.functions
            .verify_lock(&context, &identities)
            .map_err(|error| HostFailure { error })
    }

    fn bind(
        &mut self,
        identity: &yamaa_core::function_signature::ProjectFunctionIdentity,
        signature: &yamaa_core::function_signature::LogicalSignature,
    ) -> Result<Self::Handle, Self::Error> {
        self.functions
            .bind(
                &FunctionIdentity::from_core(identity),
                &Signature::from_core(signature),
            )
            .map_err(|error| HostFailure { error })
    }

    fn is_interrupt(&self, error: &Self::Error) -> bool {
        self.functions.is_interrupt(&error.error)
    }

    fn invoke(
        &mut self,
        handle: &Self::Handle,
        arguments: &[yamaa_core::function_invocation::Argument<'_>],
    ) -> Result<yamaa_core::value::Value, yamaa_core::function_invocation::HostError<Self::Error>>
    {
        use yamaa_core::function_invocation::HostError;
        let borrowed: Vec<Argument<'_>> = arguments
            .iter()
            .map(|argument| Argument {
                name: argument.name,
                value: ScalarRef::from_core(argument.value),
            })
            .collect();
        match self.functions.invoke(handle, &borrowed) {
            Ok(value) => Ok(value.into_core()),
            Err(CallError::Raised(error)) => Err(HostError::Raised(HostFailure { error })),
            Err(CallError::InvalidResult(error)) => {
                Err(HostError::InvalidResult(HostFailure { error }))
            }
        }
    }
}

/// One classified host failure: ordinary facts project into diagnostics while
/// opaque failures stay held without classification facts.
enum ClassifiedFailure {
    Host(yamaa_adapters::callback::CallbackError),
    Lock(Vec<yamaa_core::diagnostic::LockFinding>),
    Opaque,
}

fn classify(error: &FunctionError) -> ClassifiedFailure {
    use yamaa_adapters::callback::CallbackError;
    match error {
        FunctionError::Exception { class, message, .. } => {
            ClassifiedFailure::Host(CallbackError::Exception {
                class: class.clone(),
                message: message.clone(),
            })
        }
        FunctionError::Rejected { reason, returned } => {
            ClassifiedFailure::Host(CallbackError::Rejected {
                reason: reason.clone(),
                returned: returned.clone(),
            })
        }
        FunctionError::Representation { message, .. } => {
            ClassifiedFailure::Host(CallbackError::Rejected {
                reason: message.clone(),
                returned: None,
            })
        }
        FunctionError::Scalar { reason } => ClassifiedFailure::Host(CallbackError::Rejected {
            reason: (*reason).to_owned(),
            returned: None,
        }),
        FunctionError::Lock(findings) => ClassifiedFailure::Lock(
            findings
                .iter()
                .map(|finding| finding.clone().into_core())
                .collect(),
        ),
        _ => ClassifiedFailure::Opaque,
    }
}

/// Snapshot borrowed attempt failures into owned facade records.
pub(crate) fn snapshot_failures(
    failures: Vec<(&'static str, &HostFailure)>,
) -> Vec<AttemptFailure> {
    failures
        .into_iter()
        .map(|(stage, failure)| AttemptFailure {
            stage,
            error: failure.error.clone(),
        })
        .collect()
}

/// Format one retained project attempt into its public result candidate.
/// Save targets resolve from the attempt's run scope.
pub(crate) fn format_project_attempt(
    attempt: &ProjectAttempt,
    run: &Arc<yamaa_adapters::project_run::PreparedRun>,
    identity: yamaa_adapters::specification_report::Identity<'_>,
    project: &yamaa_adapters::file_project::FileProject,
    run_scope: &yamaa_adapters::file_resources::RunScope,
) -> Result<yamaa_adapters::file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
    use yamaa_adapters::project_report::Classified;
    let mut borrowed = Vec::new();
    yamaa_adapters::project_attempt::visit_host_failures(attempt, |stage, error| {
        borrowed.push((stage, error));
    });
    let classified: Vec<ClassifiedFailure> = borrowed
        .iter()
        .map(|(_, failure)| classify(&failure.error))
        .collect();
    let facts: Vec<Classified<'_, HostFailure>> = borrowed
        .iter()
        .zip(&classified)
        .filter_map(|((_, failure), item)| match item {
            ClassifiedFailure::Host(error) => {
                yamaa_adapters::callback::details(error).map(|details| Classified::Host {
                    failure: *failure,
                    details,
                })
            }
            ClassifiedFailure::Lock(findings) => Some(Classified::Lock {
                failure: *failure,
                findings,
            }),
            ClassifiedFailure::Opaque => None,
        })
        .collect();
    let formatted = yamaa_adapters::project_report::build_result(run, attempt, identity, &facts)
        .map_err(|error| error.refusal());
    match formatted {
        Ok(result) => {
            yamaa_adapters::file_application::Domain::from_project(result, project, run_scope)
        }
        Err(refusal) => Err(refusal.issues()),
    }
}

pub(crate) enum BuildOutcome {
    Plain {
        result: Result<
            yamaa_adapters::file_application::Domain,
            Vec<yamaa_adapters::issue_rows::Issue>,
        >,
    },
    Project {
        attempt: Box<ProjectAttempt>,
        result: Result<
            yamaa_adapters::file_application::Domain,
            Vec<yamaa_adapters::issue_rows::Issue>,
        >,
    },
    SelectionFailed {
        issues: Vec<yamaa_adapters::issue_rows::Issue>,
        sources: RetainedSources,
    },
    Unsupported {
        issues: Vec<yamaa_adapters::issue_rows::Issue>,
    },
    /// A project-run specification that did not build: a project finding
    /// failed it, or a producer it reads failed or did not run.
    Refused {
        issues: Vec<yamaa_adapters::issue_rows::Issue>,
    },
}

/// Compilation reads through the engine's resolver. Each run opens its own
/// resource scope: through the compile authority, or through the run's own
/// resolver when it brings one. A plan's run first verifies the compiled
/// metadata; a build's run does not, because its compilation just did.
struct EngineRuntime<'a> {
    shared: &'a EngineShared,
    specification: &'a str,
    run_resolver: Option<Arc<dyn yamaa_adapters::host_resources::Resolver>>,
    /// Artifacts a project run already built, offered to this run's inputs
    /// whose `schema` links name their producers.
    held: Option<Arc<yamaa_adapters::file_resources::HeldArtifacts>>,
    verify: bool,
}

impl EngineRuntime<'_> {
    /// Refuse a run whose held metadata changed since compilation, before
    /// activation or any study read. The refusal keeps the compiled sources.
    fn refusal(
        &self,
        verify: impl FnOnce() -> Result<(), Vec<yamaa_adapters::issue_rows::Issue>>,
        sources: impl FnOnce() -> RetainedSources,
    ) -> Option<BuildOutcome> {
        if !self.verify {
            return None;
        }
        verify().err().map(|issues| BuildOutcome::SelectionFailed {
            issues,
            sources: sources(),
        })
    }
}

impl yamaa_adapters::file_application::RuntimePort for EngineRuntime<'_> {
    type Result = BuildOutcome;
    type Error = Infallible;

    fn resolver(&self) -> Option<Arc<dyn yamaa_adapters::host_resources::Resolver>> {
        resolver_bridge(self.shared)
    }

    fn plain(
        &mut self,
        prepared: &yamaa_adapters::file_preparation::FileSpecification,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Result<Self::Result, Self::Error> {
        if let Some(refused) = self.refusal(
            || prepared.verify_metadata(),
            || prepared.retained_sources(),
        ) {
            return Ok(refused);
        }
        let identity = self.shared.identity("domain", self.specification);
        let mut run_scope = prepared.run_scope(self.run_resolver.clone());
        if let Some(held) = &self.held {
            run_scope.hold(Arc::clone(held));
        }
        let result =
            yamaa_adapters::file_application::plain(prepared, &mut run_scope, identity, scope);
        Ok(BuildOutcome::Plain { result })
    }

    fn project(
        &mut self,
        project: &yamaa_adapters::file_project::FileProject,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Result<Self::Result, Self::Error> {
        if let Some(refused) =
            self.refusal(|| project.verify_metadata(), || project.retained_sources())
        {
            return Ok(refused);
        }
        let Some(functions) = self.shared.functions.clone() else {
            return Ok(BuildOutcome::Unsupported {
                issues: yamaa_adapters::file_application::unsupported(
                    "environment",
                    Some("environment"),
                ),
            });
        };
        let mut run_scope = project.run_scope(self.run_resolver.clone());
        if let Some(held) = &self.held {
            run_scope.hold(Arc::clone(held));
        }
        let inner = project.build_with(
            &mut run_scope,
            |run| {
                let bytes = run
                    .captured_environment()
                    .lock()
                    .map_or(&[][..], |lock| &lock.source.bytes[..]);
                HostPort {
                    functions: functions.as_ref(),
                    lock_bytes: bytes,
                }
            },
            scope,
        );
        let run = project.retained_run();
        scope.phase(yamaa_core::application_observer::Phase::Output);
        let identity = self.shared.identity("domain", self.specification);
        let result = format_project_attempt(&inner, &run, identity, project, &run_scope);
        scope.exit(
            yamaa_core::application_observer::Phase::Output,
            if result.is_ok() {
                yamaa_core::application_observer::Outcome::Succeeded
            } else {
                yamaa_core::application_observer::Outcome::Failed
            },
        );
        Ok(BuildOutcome::Project {
            attempt: Box::new(inner),
            result,
        })
    }
}

pub(crate) fn cancelled_issue() -> yamaa_adapters::issue_rows::Issue {
    yamaa_adapters::issue_rows::Issue {
        phase: "check".into(),
        condition: "cancelled".into(),
        requirement: None,
        spec_paths: Vec::new(),
        context: yamaa_adapters::json!({}),
        source_span: None,
        operand_route: None,
    }
}

pub(crate) fn run_check(
    shared: &EngineShared,
    specification: &str,
    environment: Option<&str>,
    token: &CancellationToken,
) -> Vec<yamaa_adapters::issue_rows::Issue> {
    if token.is_cancelled() {
        return vec![cancelled_issue()];
    }
    let identity = shared.identity("check", specification);
    yamaa_adapters::file_application::check_with_resolver(
        shared.language,
        yamaa_core::domain_entry::Request {
            specification,
            environment,
        },
        identity,
        resolver_bridge(shared),
    )
}

/// One compiled request: the selected prepared route, or a refused
/// compilation that still owns its diagnostics and the sources it captured.
pub(crate) enum Compiled {
    Ready(Box<yamaa_adapters::file_workflow::Selected>),
    Refused {
        issues: Vec<yamaa_adapters::issue_rows::Issue>,
        sources: RetainedSources,
    },
}

fn selection_failed(
    error: &yamaa_adapters::file_workflow::SelectionError,
) -> (Vec<yamaa_adapters::issue_rows::Issue>, RetainedSources) {
    (
        yamaa_adapters::project_application::selection_failure(error),
        error.retained_sources(),
    )
}

/// The Preparation half of a build: capture metadata through the engine's
/// resolver, admit and compile. No study input is captured or read.
pub(crate) fn compile(
    shared: &EngineShared,
    specification: &str,
    environment: Option<&str>,
    scope: yamaa_core::application_observer::Scope<'_>,
) -> Compiled {
    let mut application = yamaa_adapters::file_application::Application {
        host: shared.language,
        runtime: EngineRuntime {
            shared,
            specification,
            run_resolver: None,
            held: None,
            verify: false,
        },
    };
    match yamaa_core::domain_entry::compile(
        yamaa_core::domain_entry::Request {
            specification,
            environment,
        },
        &mut application,
        scope,
    ) {
        Ok(selected) => Compiled::Ready(Box::new(selected)),
        Err(yamaa_adapters::file_application::ApplicationError::Preparation(error)) => {
            let (issues, sources) = selection_failed(&error);
            Compiled::Refused { issues, sources }
        }
        Err(yamaa_adapters::file_application::ApplicationError::Runtime(never)) => match never {},
    }
}

/// The Build and Finished half: one run of a compiled request in its own
/// resource scope. With `verify`, the run first verifies the compiled metadata
/// and refuses if it changed. A refused compilation's run returns its held
/// refusal and performs no resource, activation or study work. A project run
/// offers its `held` artifacts to the inputs whose `schema` links name their
/// producers.
pub(crate) fn run(
    shared: &EngineShared,
    specification: &str,
    compiled: &Compiled,
    run_resolver: Option<Arc<dyn yamaa_adapters::host_resources::Resolver>>,
    held: Option<Arc<yamaa_adapters::file_resources::HeldArtifacts>>,
    verify: bool,
    scope: yamaa_core::application_observer::Scope<'_>,
) -> BuildOutcome {
    let selected = match compiled {
        Compiled::Ready(selected) => selected,
        Compiled::Refused { issues, sources } => {
            yamaa_core::domain_entry::finish(scope);
            return BuildOutcome::SelectionFailed {
                issues: issues.clone(),
                sources: sources.clone(),
            };
        }
    };
    let mut application = yamaa_adapters::file_application::Application {
        host: shared.language,
        runtime: EngineRuntime {
            shared,
            specification,
            run_resolver,
            held,
            verify,
        },
    };
    match yamaa_core::domain_entry::run(selected, &mut application, scope) {
        Ok(outcome) => outcome,
        // A run selects nothing; keep any selection refusal typed regardless.
        Err(yamaa_adapters::file_application::ApplicationError::Preparation(error)) => {
            let (issues, sources) = selection_failed(&error);
            BuildOutcome::SelectionFailed { issues, sources }
        }
        Err(yamaa_adapters::file_application::ApplicationError::Runtime(never)) => match never {},
    }
}

/// The specification and project identities a request compiles to, resolved
/// now exactly as compilation resolves them (#3069). A local path is made
/// absolute against the current working directory and its parent directory
/// canonical, so a relative path after a working-directory change, or a path
/// through a retargeted link, resolves to another identity. A caller resolver
/// names files by the references it is given, so those are the identities.
/// `None` when a local path cannot be resolved; such a request compiles and
/// reports why, and nothing is kept for it.
pub(crate) fn request_identity(
    shared: &EngineShared,
    request: &crate::engine::Request<'_>,
) -> Option<crate::api::PlanKey> {
    if shared.resolver.is_some() {
        return Some((
            request.specification.to_owned(),
            request.environment.map(str::to_owned),
        ));
    }
    let identity = |written| yamaa_adapters::file_configuration::caller_identity(written).ok();
    let specification = identity(request.specification)?;
    let environment = match request.environment {
        Some(environment) => Some(identity(environment)?),
        None => None,
    };
    Some((specification, environment))
}

/// Whether two engine configurations compile and run alike: the same host
/// language, and no resolver or the same caller resolver.
pub(crate) fn same_authority(left: &EngineShared, right: &EngineShared) -> bool {
    left.language == right.language
        && match (&left.resolver, &right.resolver) {
            (None, None) => true,
            (Some(left), Some(right)) => Arc::ptr_eq(&left.inner, &right.inner),
            _ => false,
        }
}

/// The run-start check outside a run: whether every file a compiled request
/// holds still has the bytes it was compiled from. A refused compilation is
/// never reused, so it reports `false`.
pub(crate) fn unchanged(compiled: &Compiled) -> bool {
    let Compiled::Ready(selected) = compiled else {
        return false;
    };
    match selected.as_ref() {
        yamaa_core::build::Prepared::Plain(prepared) => prepared.verify_metadata().is_ok(),
        yamaa_core::build::Prepared::Ordinary(project) => project.verify_metadata().is_ok(),
    }
}

pub(crate) fn outcome_accepted(outcome: &BuildOutcome) -> bool {
    match outcome {
        BuildOutcome::Plain { result } => result
            .as_ref()
            .is_ok_and(|domain| domain.output().is_some()),
        BuildOutcome::Project { result, .. } => result
            .as_ref()
            .is_ok_and(|domain| domain.output().is_some()),
        BuildOutcome::SelectionFailed { .. }
        | BuildOutcome::Unsupported { .. }
        | BuildOutcome::Refused { .. } => false,
    }
}

/// The issue rows of one outcome, live from its current state.
pub(crate) fn outcome_issues(outcome: &BuildOutcome) -> Vec<yamaa_adapters::issue_rows::Issue> {
    match outcome {
        BuildOutcome::Plain { result } | BuildOutcome::Project { result, .. } => match result {
            Ok(domain) => domain.issues(),
            Err(issues) => issues.clone(),
        },
        BuildOutcome::SelectionFailed { issues, .. }
        | BuildOutcome::Unsupported { issues }
        | BuildOutcome::Refused { issues } => issues.clone(),
    }
}

/// Build the public report for one outcome, live from its current state.
pub(crate) fn outcome_report(outcome: &BuildOutcome) -> Report {
    Report::from_issues(&outcome_issues(outcome))
}
