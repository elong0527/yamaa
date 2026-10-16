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
use crate::error::{Error, FunctionError, ResolverFailure};
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

pub(crate) type ProducerAttempt = yamaa_adapters::file_producer_build::OwnedAttempt<HostFailure>;

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
                },
            )
            .map_err(|failure: ResolverFailure| {
                use yamaa_adapters::host_resources::Failure;
                match failure.code() {
                    "missing" => Failure::Missing,
                    "changed" => Failure::Changed,
                    "cancelled" => Failure::Cancelled,
                    "publication" => Failure::Publication,
                    "cleanup" => Failure::Cleanup,
                    _ => Failure::Rejected,
                }
            })
    }
}

struct ObserverBridge<'a> {
    measurements: &'a yamaa_adapters::application_measurements::Measurements,
    progress: Option<&'a dyn Progress>,
}

impl yamaa_core::application_observer::Observer for ObserverBridge<'_> {
    fn observe(&self, event: yamaa_core::application_observer::Event<'_>) {
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
pub(crate) fn format_project_attempt(
    attempt: &ProjectAttempt,
    run: &Arc<yamaa_adapters::project_run::PreparedRun>,
    identity: yamaa_adapters::specification_report::Identity<'_>,
    project: &yamaa_adapters::file_project::FileProject,
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
        Ok(result) => yamaa_adapters::file_application::Domain::from_project(result, project),
        Err(refusal) => Err(refusal.issues()),
    }
}

/// Project one retained owned producer attempt into its public projection.
pub(crate) fn format_producer_attempt(
    attempt: &ProducerAttempt,
    identity: yamaa_adapters::specification_report::Identity<'_>,
) -> yamaa_adapters::producer_result::RootProjection {
    use yamaa_adapters::project_report::Classified;
    let mut borrowed = Vec::new();
    yamaa_adapters::producer_attempt::visit_host_failures(attempt, |stage, error| {
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
    yamaa_adapters::producer_result::project_owned(attempt, identity, &facts)
}

pub(crate) enum BuildOutcome {
    Plain {
        result: Result<
            yamaa_adapters::file_application::Domain,
            Vec<yamaa_adapters::issue_rows::Issue>,
        >,
    },
    Project {
        attempt: ProjectAttempt,
        result: Result<
            yamaa_adapters::file_application::Domain,
            Vec<yamaa_adapters::issue_rows::Issue>,
        >,
    },
    Producer {
        attempt: ProducerAttempt,
        projection: yamaa_adapters::producer_result::RootProjection,
    },
    SelectionFailed {
        issues: Vec<yamaa_adapters::issue_rows::Issue>,
        sources: RetainedSources,
    },
    Unsupported {
        issues: Vec<yamaa_adapters::issue_rows::Issue>,
    },
}

struct EngineRuntime<'a> {
    shared: &'a EngineShared,
    specification: &'a str,
}

impl yamaa_adapters::file_application::RuntimePort for EngineRuntime<'_> {
    type Result = BuildOutcome;
    type Error = Infallible;

    fn resolver(&self) -> Option<Arc<dyn yamaa_adapters::host_resources::Resolver>> {
        resolver_bridge(self.shared)
    }

    fn plain(
        &mut self,
        prepared: yamaa_adapters::file_preparation::FileSpecification,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Result<Self::Result, Self::Error> {
        let identity = self.shared.identity("domain", self.specification);
        let result = yamaa_adapters::file_application::plain(prepared, identity, scope);
        Ok(BuildOutcome::Plain { result })
    }

    fn project(
        &mut self,
        mut project: Box<yamaa_adapters::file_project::FileProject>,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Result<Self::Result, Self::Error> {
        let Some(functions) = self.shared.functions.clone() else {
            return Ok(BuildOutcome::Unsupported {
                issues: yamaa_adapters::file_application::unsupported(
                    "environment",
                    Some("environment"),
                ),
            });
        };
        let inner = project.build_with(
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
        let result = format_project_attempt(&inner, &run, identity, &project);
        scope.exit(
            yamaa_core::application_observer::Phase::Output,
            if result.is_ok() {
                yamaa_core::application_observer::Outcome::Succeeded
            } else {
                yamaa_core::application_observer::Outcome::Failed
            },
        );
        Ok(BuildOutcome::Project {
            attempt: inner,
            result,
        })
    }

    fn producer(
        &mut self,
        graph: Box<yamaa_adapters::file_graph::FileGraph>,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Result<Self::Result, Self::Error> {
        let Some(functions) = self.shared.functions.clone() else {
            return Ok(BuildOutcome::Unsupported {
                issues: yamaa_adapters::file_application::unsupported("producer_host", None),
            });
        };
        let mut build = graph.into_build();
        let provenance = Arc::clone(build.provenance());
        let lock_bytes = provenance
            .environment()
            .lock()
            .map_or(Vec::new(), |lock| lock.source.bytes.clone());
        let mut port = HostPort {
            functions: functions.as_ref(),
            lock_bytes: &lock_bytes,
        };
        let identity = self.shared.identity("domain", self.specification);
        scope.phase(yamaa_core::application_observer::Phase::Output);
        let inner = build.build_owned_reported(&mut port, identity, scope);
        let projection = {
            let identity = self.shared.identity("domain", self.specification);
            format_producer_attempt(&inner, identity)
        };
        scope.exit(
            yamaa_core::application_observer::Phase::Output,
            if projection.result.is_ok() {
                yamaa_core::application_observer::Outcome::Succeeded
            } else {
                yamaa_core::application_observer::Outcome::Failed
            },
        );
        Ok(BuildOutcome::Producer {
            attempt: inner,
            projection,
        })
    }
}

pub(crate) fn cancelled_issue() -> yamaa_adapters::issue_rows::Issue {
    yamaa_adapters::issue_rows::Issue {
        phase: "check".into(),
        condition: "cancelled".into(),
        requirement: None,
        spec_paths: Vec::new(),
        context: "{}".into(),
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
        yamaa_core::domain_entry::Request {
            specification,
            environment,
        },
        identity,
        resolver_bridge(shared),
    )
}

pub(crate) fn run_build(
    shared: &EngineShared,
    specification: &str,
    environment: Option<&str>,
    token: &CancellationToken,
    measurements: &Arc<yamaa_adapters::application_measurements::Measurements>,
) -> Result<BuildOutcome, Error> {
    if token.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let bridge = ObserverBridge {
        measurements,
        progress: shared.observer.as_deref(),
    };
    let scope = yamaa_core::application_observer::Scope::new(&bridge);
    let mut application = yamaa_adapters::file_application::Application {
        host: shared.language,
        runtime: EngineRuntime {
            shared,
            specification,
        },
    };
    match yamaa_core::domain_entry::domain(
        yamaa_core::domain_entry::Request {
            specification,
            environment,
        },
        &mut application,
        scope,
    ) {
        Ok(outcome) => Ok(outcome),
        Err(yamaa_adapters::file_application::ApplicationError::Preparation(error)) => {
            let issues =
                yamaa_adapters::project_application::selection_failure(&error, shared.language);
            let sources = error.retained_sources();
            Ok(BuildOutcome::SelectionFailed { issues, sources })
        }
        Err(yamaa_adapters::file_application::ApplicationError::Runtime(never)) => match never {},
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
        BuildOutcome::Producer { projection, .. } => projection
            .result
            .as_ref()
            .is_ok_and(|root| root.output().is_some()),
        BuildOutcome::SelectionFailed { .. } | BuildOutcome::Unsupported { .. } => false,
    }
}

/// Build the public report for one outcome, live from its current state.
pub(crate) fn outcome_report(outcome: &BuildOutcome) -> Report {
    match outcome {
        BuildOutcome::Plain { result } => match result {
            Ok(domain) => Report::from_issues(&domain.issues()),
            Err(issues) => Report::from_issues(issues),
        },
        BuildOutcome::Project { result, .. } => match result {
            Ok(domain) => Report::from_issues(&domain.issues()),
            Err(issues) => Report::from_issues(issues),
        },
        BuildOutcome::Producer { projection, .. } => match &projection.result {
            Ok(root) => Report::from_issues(root.issues()),
            Err(refusal) => Report::from_issues(&refusal.issues()),
        },
        BuildOutcome::SelectionFailed { issues, .. } | BuildOutcome::Unsupported { issues } => {
            Report::from_issues(issues)
        }
    }
}
