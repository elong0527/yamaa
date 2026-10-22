//! Internal engine bridges. Adapters and core types never cross the facade
//! boundary; every public type in the facade surface is facade-owned and the
//! conversions below are the only place adapters or core types appear.
//!
//! Every build, of a specification alone or of one member of a project run,
//! takes one route: compile the member, then run it through the one
//! host-functions port, the embedder's [`ProjectFunctions`] or the empty
//! port. Each adjudicated build, including a refused compilation, returns an
//! owned [`Build`] carrying its diagnostics and evidence. `Rejected` errors
//! are reserved for refusals without a result, such as a failed
//! specification resolution.

use std::convert::Infallible;
use std::sync::Arc;

use yamaa_adapters::application_measurements::Measurements;
use yamaa_core::host_functions::{Empty, HostFunctions, Lock};

use crate::cancel::CancellationToken;
use crate::diagnostic::Locator;
use crate::engine::Build;
use crate::error::{Error, FunctionError, ResolverFailure};
use crate::functions::{
    Argument, BatchArgument, CallError, FunctionHandle, FunctionIdentity, HostLanguage,
    LockContext, LockKind, ProjectFunctions, Signature,
};
use crate::resolver::{ResolverReply, ResolverRequest, ResourceResolver};
use crate::scalar::{Scalar, ScalarRef};

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
                    bytes: reply.bytes,
                    frame: reply.frame.map(crate::table::InputFrame::into_adapter),
                    ..yamaa_adapters::host_resources::Reply::reference(reply.reference)
                },
            )
            .map_err(|failure: ResolverFailure| {
                yamaa_adapters::host_resources::Failure::from_name(failure.code())
            })
    }
}

pub(crate) struct EngineShared {
    pub resolver: Option<Arc<ResolverBridge>>,
    /// The host language, or none for an engine that claims no host language
    /// (#3030 section 2.10).
    pub language: Option<yamaa_core::project_function::Language>,
    pub runtime_name: String,
    pub runtime_version: String,
    pub functions: Option<Arc<dyn ProjectFunctions>>,
}

impl EngineShared {
    pub(crate) fn from_options(options: &crate::engine::EngineOptions) -> Self {
        Self {
            resolver: options
                .resolver
                .clone()
                .map(|resolver| Arc::new(ResolverBridge { inner: resolver })),
            language: options.host_language.map(HostLanguage::into_core),
            runtime_name: options.runtime_name.clone(),
            runtime_version: options.runtime_version.clone(),
            // Implementations are named per host language, so an engine that
            // claims none has no function to call: its port stays empty.
            functions: options.host_language.and(options.functions.clone()),
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

/// The findings of a project file that could not be read, with what locates
/// them: the files read before it stopped.
pub(crate) type Unread = (Vec<yamaa_core::diagnostic::Diagnostic>, Arc<Locator>);

/// Read a project file and its listed specifications through the engine's
/// resolver: the caller's, or the local file resolver.
pub(crate) fn read_project(
    shared: &EngineShared,
    project: &str,
) -> Result<yamaa_adapters::project_scope::Project, Unread> {
    yamaa_adapters::project_scope::Project::read_located(
        project,
        shared.language,
        resolver_bridge(shared),
    )
    .map_err(|(issues, locator)| (issues, Arc::new(locator)))
}

/// The embedder's project functions are the build's host-functions port: a
/// host's (Python's or R's), the memory route's or another embedder's. Each
/// call converts between the core's values and the facade's on the calling
/// thread; a host failure is retained as the `FunctionError` it returned.
impl HostFunctions for &dyn ProjectFunctions {
    type Handle = FunctionHandle;
    type Error = FunctionError;

    fn verify_lock(
        &mut self,
        language: yamaa_core::project_function::Language,
        lock: Lock<'_>,
        functions: &[yamaa_core::function_signature::ProjectFunctionIdentity],
    ) -> Result<(), FunctionError> {
        let language = match language {
            yamaa_core::project_function::Language::Python => HostLanguage::Python,
            yamaa_core::project_function::Language::R => HostLanguage::R,
        };
        let context = LockContext {
            language,
            kind: LockKind::from_core(lock.reference.kind),
            bytes: lock.bytes,
        };
        let identities: Vec<FunctionIdentity> =
            functions.iter().map(FunctionIdentity::from_core).collect();
        (**self).verify_lock(&context, &identities)
    }

    fn bind(
        &mut self,
        identity: &yamaa_core::function_signature::ProjectFunctionIdentity,
        signature: &yamaa_core::function_signature::LogicalSignature,
    ) -> Result<FunctionHandle, FunctionError> {
        (**self).bind(
            &FunctionIdentity::from_core(identity),
            &Signature::from_core(signature),
        )
    }

    fn is_interrupt(&self, error: &FunctionError) -> bool {
        (**self).is_interrupt(error)
    }

    fn invoke(
        &mut self,
        handle: &FunctionHandle,
        arguments: &[yamaa_core::function_invocation::Argument<'_>],
    ) -> Result<yamaa_core::value::Value, yamaa_core::function_invocation::HostError<FunctionError>>
    {
        let borrowed: Vec<Argument<'_>> = arguments
            .iter()
            .map(|argument| Argument {
                name: argument.name,
                value: ScalarRef::from_core(argument.value),
            })
            .collect();
        (**self)
            .invoke(handle, &borrowed)
            .map(Scalar::into_core)
            .map_err(host_error)
    }

    fn call_batch(
        &mut self,
        handle: &FunctionHandle,
        rows: usize,
        arguments: &[yamaa_core::function_batch::BatchArgument],
    ) -> Result<
        Vec<yamaa_core::value::Value>,
        yamaa_core::function_invocation::HostError<FunctionError>,
    > {
        let arguments: Vec<BatchArgument> = arguments
            .iter()
            .map(|argument| BatchArgument {
                name: argument.name.clone(),
                values: argument
                    .values
                    .iter()
                    .cloned()
                    .map(Scalar::from_core)
                    .collect(),
            })
            .collect();
        (**self)
            .call_batch(handle, rows, &arguments)
            .map(|values| values.into_iter().map(Scalar::into_core).collect())
            .map_err(host_error)
    }
}

/// A raised host failure stays distinct from a rejected host representation,
/// preserving the engine's diagnostics.
fn host_error(error: CallError) -> yamaa_core::function_invocation::HostError<FunctionError> {
    use yamaa_core::function_invocation::HostError;
    match error {
        CallError::Raised(error) => HostError::Raised(error),
        CallError::InvalidResult(error) => HostError::InvalidResult(error),
    }
}

/// Classify one retained host failure: ordinary facts project into
/// diagnostics while opaque failures stay held without classification facts.
fn classify(failure: &FunctionError) -> yamaa_adapters::file_application::Classification {
    use yamaa_adapters::callback::CallbackError;
    use yamaa_adapters::file_application::Classification;
    match failure {
        FunctionError::Exception { class, message, .. } => {
            Classification::Host(CallbackError::Exception {
                class: class.clone(),
                message: message.clone(),
            })
        }
        FunctionError::Rejected { reason, returned } => {
            Classification::Host(CallbackError::Rejected {
                reason: reason.clone(),
                returned: returned.clone(),
            })
        }
        FunctionError::Representation { message, .. } => {
            Classification::Host(CallbackError::Rejected {
                reason: message.clone(),
                returned: None,
            })
        }
        FunctionError::Scalar { reason } => Classification::Host(CallbackError::Rejected {
            reason: (*reason).to_owned(),
            returned: None,
        }),
        FunctionError::Lock(findings) => Classification::Lock(
            findings
                .iter()
                .map(|finding| finding.clone().into_core())
                .collect(),
        ),
        _ => Classification::Opaque,
    }
}

/// The static check of one specification: its findings, which are issues,
/// and what locates them. Only a cancelled token fails the call, with no
/// issue.
pub(crate) fn run_check(
    shared: &EngineShared,
    specification: &str,
    environment: Option<&str>,
    token: &CancellationToken,
) -> Result<(Vec<yamaa_core::diagnostic::Diagnostic>, Arc<Locator>), Error> {
    if token.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let (issues, locator) = yamaa_adapters::file_application::check_located(
        shared.language,
        yamaa_core::domain_entry::Request {
            specification,
            environment,
        },
        resolver_bridge(shared),
    );
    Ok((issues, Arc::new(locator)))
}

/// One compiled request: the prepared member, or a refused compilation that
/// still owns its diagnostics and the dataset name it read. Either keeps
/// what locates the issues of its runs (P-ISSUE-1).
pub(crate) enum Compiled {
    Ready(
        Box<yamaa_adapters::file_preparation::FileSpecification>,
        Arc<Locator>,
    ),
    Refused {
        issues: Vec<yamaa_core::diagnostic::Diagnostic>,
        name: Option<String>,
        locator: Arc<Locator>,
    },
}

impl Compiled {
    /// The specification's dataset name, its `domain`, once its document was
    /// read: what a run's printed summary names.
    pub(crate) fn name(&self) -> Option<&str> {
        match self {
            Self::Ready(prepared, _) => prepared.run().document().domain(),
            Self::Refused { name, .. } => name.as_deref(),
        }
    }

    /// What locates the issues of this compilation and of its runs: the
    /// documents it read.
    pub(crate) fn locator(&self) -> &Arc<Locator> {
        match self {
            Self::Ready(_, locator) | Self::Refused { locator, .. } => locator,
        }
    }
}

/// The Preparation half of a build: capture metadata through the engine's
/// resolver, admit and compile. No study input is captured or read.
pub(crate) fn compile(
    shared: &EngineShared,
    specification: &str,
    environment: Option<&str>,
    scope: yamaa_core::application_observer::Scope<'_>,
) -> Compiled {
    let prepared = yamaa_core::domain_entry::compile(scope, || {
        yamaa_adapters::file_application::select(
            shared.language,
            yamaa_core::domain_entry::Request {
                specification,
                environment,
            },
            resolver_bridge(shared),
        )
    });
    match prepared {
        Ok(prepared) => {
            let locator = Arc::new(yamaa_adapters::issue_location::prepared(&prepared));
            Compiled::Ready(Box::new(prepared), locator)
        }
        Err(error) => Compiled::Refused {
            issues: yamaa_adapters::project_application::selection_failure(&error),
            name: match &error {
                yamaa_adapters::file_workflow::SelectionError::Compile(rejected) => {
                    rejected.run().document.domain().map(str::to_owned)
                }
                yamaa_adapters::file_workflow::SelectionError::Preparation(_) => None,
            },
            locator: Arc::new(yamaa_adapters::issue_location::selection(&error)),
        },
    }
}

/// The Build and Finished half: one run of a compiled request in its own
/// resource scope, whose phase and stage events reach `measurements`, which
/// the build keeps. A refused compilation's run returns its held refusal and performs
/// no resource, activation or study work. A run offers its `held` artifacts
/// to the inputs whose `schema` links name their producers, and polls
/// `token` at every checkpoint of its dataset's schedule and before each
/// batch of project-function calls (#2822).
pub(crate) fn run(
    shared: &EngineShared,
    specification: &str,
    compiled: &Compiled,
    held: Option<Arc<yamaa_adapters::file_resources::HeldArtifacts>>,
    measurements: &Arc<Measurements>,
    token: &CancellationToken,
) -> Build {
    let cancelled = || token.is_cancelled();
    let scope =
        yamaa_core::application_observer::Scope::new(&**measurements).cancellable(&cancelled);
    let measurements = Arc::clone(measurements);
    let locator = Arc::clone(compiled.locator());
    let prepared = match compiled {
        Compiled::Ready(prepared, _) => prepared,
        Compiled::Refused { issues, .. } => {
            yamaa_core::domain_entry::finish(scope);
            return Build::new(
                Err(issues.clone()),
                Vec::new(),
                measurements,
                shared,
                locator,
            );
        }
    };
    let built = yamaa_core::domain_entry::run(scope, || {
        Ok::<_, Infallible>(run_member(shared, specification, prepared, held, scope))
    });
    let (result, failures) = match built {
        Ok(built) => built,
        Err(never) => match never {},
    };
    Build::new(result, failures, measurements, shared, locator)
}

/// What one run of a member yields: its result, or the findings that explain
/// why it has none; and, with a project, the host failures the run retained.
type Ran = (
    Result<Box<yamaa_adapters::file_application::Domain>, Vec<yamaa_core::diagnostic::Diagnostic>>,
    Vec<FunctionError>,
);

/// One run of a prepared member through the engine's host-functions port:
/// the embedder's project functions, or the empty port when it supplies none.
/// An engine that claims no host language keeps no functions, so it runs
/// through the empty port too. A member that calls no project function runs
/// as a host runs it, with or without a project; through the empty port, one
/// that calls a project function is refused before it reads any input
/// (#3030 D110, D123).
fn run_member(
    shared: &EngineShared,
    specification: &str,
    prepared: &yamaa_adapters::file_preparation::FileSpecification,
    held: Option<Arc<yamaa_adapters::file_resources::HeldArtifacts>>,
    scope: yamaa_core::application_observer::Scope<'_>,
) -> Ran {
    let mut run_scope = prepared.run_scope(None);
    if let Some(held) = held {
        run_scope.hold(held);
    }
    let identity = shared.identity("domain", specification);
    let Some(mut functions) = shared.functions.as_deref() else {
        let (_, result) = yamaa_adapters::file_application::run(
            prepared,
            &mut run_scope,
            &mut Empty::<FunctionError>::new(),
            classify,
            identity,
            scope,
        );
        return (result.map(Box::new), Vec::new());
    };
    let (attempt, result) = yamaa_adapters::file_application::run(
        prepared,
        &mut run_scope,
        &mut functions,
        classify,
        identity,
        scope,
    );
    // A member with a project reports the host failures it retained.
    let mut failures = Vec::new();
    if prepared.run().has_project() {
        yamaa_adapters::project_attempt::visit_host_failures(&attempt, |_, failure| {
            failures.push(failure.clone());
        });
    }
    (result.map(Box::new), failures)
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
    let Compiled::Ready(prepared, _) = compiled else {
        return false;
    };
    prepared.verify_metadata().is_ok()
}
