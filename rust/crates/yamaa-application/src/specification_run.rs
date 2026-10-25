//! Execute prepared YAML specifications over captured schema and typed snapshots.
//! The resolver port supplies snapshots; publication belongs to the public Save service.
//!
//! [`PreparedRun`] is the one prepared specification: alone, it carries an
//! empty environment, and with a project it carries the project's admitted
//! environment. Every build of it activates the project functions it calls
//! through one host-functions port, then captures, decodes, binds and executes.
use crate::{
    dataset_result::{self, DatasetError, DatasetResponse},
    decoder::{Decoder, Snapshot},
    project_source::{CapturedEnvironment, OwnedEnvironment},
    specification_source::{LinkedSpecification, PreparedDocument},
};
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};
pub use yamaa_adapters::source_decoding::Error;
use yamaa_core::host_functions::HostFunctions;
use yamaa_core::project_activation::{Failure, Observations};
use yamaa_core::project_domain::{self, CheckedOwnedProject, RejectedOwnedProject};
use yamaa_core::project_environment::ExecutionEnvironment;
use yamaa_core::specification::{PrepareError, PreparedSpecification, SourceDeclaration};
use yamaa_core::specification_run as application;
pub use yamaa_core::specification_run::{InspectionFailure, PortError, SourceRead};
/// One captured source of an attempt: the exact owned snapshot used for
/// execution; reporting never reopens or reparses data.
pub type CapturedSource<E> = application::CapturedSource<Snapshot<E>>;
/// The dataset half of one attempt: every source it entered and the
/// execution or the port failure that ended it.
pub type CapturedAttempt<C, E> = application::CapturedAttempt<C, Error, Snapshot<E>>;

/// One prepared specification: its document, the environment it was compiled
/// against (empty without a project) and its compilation. Preparation has no
/// runtime or study authority; every build retains its own observations and
/// original host failures, including an unwound host boundary.
#[derive(Debug)]
pub struct PreparedRun {
    document: PreparedDocument,
    captured: CapturedEnvironment,
    checked: CheckedOwnedProject,
}

/// Failed static checking still owns the original documents, lock and metadata.
/// A diagnostic renderer must use these held origins instead of recapturing them.
#[derive(Debug)]
pub struct RejectedRun {
    pub document: PreparedDocument,
    pub captured: CapturedEnvironment,
    pub rejection: Box<RejectedOwnedProject>,
}

pub enum BoundaryFailure<E> {
    Activation(Failure<E>),
    Unwind(Box<dyn Any + Send>),
}
impl<E: std::fmt::Debug> std::fmt::Debug for BoundaryFailure<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Activation(error) => formatter.debug_tuple("Activation").field(error).finish(),
            Self::Unwind(_) => formatter.write_str("Unwind(..)"),
        }
    }
}

/// Boundary success alone does not imply dataset success. The engine's dataset
/// attempt independently owns capture, ingestion, execution and check outcomes.
pub struct Attempt<C, E> {
    pub boundary: Result<(), BoundaryFailure<E>>,
    pub activation: Observations,
    pub dataset: CapturedAttempt<C, E>,
}

impl PreparedRun {
    /// Compile a document alone, without a project. Entire vocabulary
    /// admission precedes obtaining a study-data snapshot. A specification
    /// without a project never reads its environment's host language.
    pub fn prepare(document: PreparedDocument) -> Result<Self, Error> {
        let environment = OwnedEnvironment::without_environment(Some(
            yamaa_core::project_function::Language::Python,
        ));
        Self::prepare_with_environment(document, environment)
            .map_err(|rejected| Error::Prepare(rejected.rejection.error))
    }
    /// Compile a document against an admitted environment.
    pub fn prepare_with_environment(
        document: PreparedDocument,
        environment: OwnedEnvironment,
    ) -> Result<Self, Box<RejectedRun>> {
        Self::prepare_with_profile_inspection(document, &[], environment, |_, _, _| None)
    }
    /// Compile against the admitted environment and the producing
    /// specifications that the document's `schema` links name; each supplies
    /// only its output contract.
    pub(crate) fn prepare_with_profile_inspection(
        document: PreparedDocument,
        links: &[LinkedSpecification],
        environment: OwnedEnvironment,
        mut inspect: impl FnMut(
            &PreparedDocument,
            &str,
            &str,
        ) -> Option<(yamaa_core::resource::ResourceFailure, String)>,
    ) -> Result<Self, Box<RejectedRun>> {
        let (environment, captured) = environment.into_parts();
        let checked = {
            let metadata = crate::specification_source::link_metadata(&document, links);
            let links = yamaa_core::producer_contract::Links {
                consumer: &document.source().identity,
                links: &metadata,
            };
            // A specification binds from the tree its front end lowered
            // (#3030 D76).
            project_domain::check_owned_tree_linked(
                document.tree().clone(),
                document.document(),
                environment,
                links,
            )
        };
        match checked {
            Ok(checked) => Ok(Self {
                document,
                captured,
                checked,
            }),
            Err(mut rejection) => {
                rejection.error = yamaa_core::source_profile_inspection::refine(
                    rejection.error,
                    |dataset, path| inspect(&document, dataset, path),
                );
                Err(Box::new(RejectedRun {
                    document,
                    captured,
                    rejection,
                }))
            }
        }
    }
    pub fn document(&self) -> &PreparedDocument {
        &self.document
    }
    pub(crate) fn reject(self, error: PrepareError) -> Box<RejectedRun> {
        Box::new(RejectedRun {
            document: self.document,
            captured: self.captured,
            rejection: self.checked.reject(error),
        })
    }
    /// The captured project documents, empty without a project.
    pub fn captured_environment(&self) -> &CapturedEnvironment {
        &self.captured
    }
    /// Whether a project file was supplied for this specification.
    pub fn has_project(&self) -> bool {
        self.captured.root_optional().is_some()
    }
    pub fn environment(&self) -> &ExecutionEnvironment {
        self.checked.environment()
    }
    pub fn key_names(&self) -> impl Iterator<Item = &str> {
        self.compiled().key_names()
    }
    pub fn compiled(&self) -> &PreparedSpecification {
        self.checked.compiled()
    }
    /// Read static findings from the same engine capability used by build.
    pub fn check_diagnostics(&self) -> Vec<yamaa_core::diagnostic::Diagnostic> {
        self.checked.check_diagnostics()
    }
    pub fn source(&self) -> &SourceDeclaration {
        self.compiled().source()
    }
    /// The shared application service decodes, binds and executes held bytes.
    pub fn execute_csv(&self, bytes: &[u8]) -> Result<DatasetResponse, Error> {
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            application::execute_bytes(
                self.compiled(),
                bytes,
                &mut Decoder::<crate::callback::CallbackError>::default(),
            )
        }))
        .map_err(|_| Error::Execution(DatasetError::Internal))?;
        let execution = attempt
            .result
            .map_err(|error| run_error(error, self.compiled()))?;
        response(execution)
    }

    /// Own evidence outside the host boundary before granting activation or
    /// study authority. No prior attempt, binding or successful test is
    /// reused here. A specification that calls no project function never
    /// enters `functions`, the build's host-functions port; the empty port
    /// refuses one that does before it binds anything or reads any input.
    pub fn execute_with_ports<A: HostFunctions, P: yamaa_core::resolver::Resolver>(
        &self,
        functions: &mut A,
        source: &mut P,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Attempt<P::Error, A::Error> {
        let mut attempt = Attempt {
            boundary: Ok(()),
            activation: Observations::default(),
            dataset: application::CapturedAttempt::new(self.compiled().source()),
        };
        let lock = self
            .captured
            .lock()
            .map_or(&[][..], |lock| &lock.source.bytes[..]);
        attempt.boundary = match catch_unwind(AssertUnwindSafe(|| {
            self.checked.build_into(
                (functions, lock),
                source,
                &mut Decoder::<A::Error>::default(),
                &mut attempt.dataset,
                (Some(&mut attempt.activation), scope),
            )
        })) {
            Ok(result) => result.map_err(BoundaryFailure::Activation),
            Err(payload) => Err(BoundaryFailure::Unwind(payload)),
        };
        attempt
    }
}

fn response(
    execution: yamaa_core::dataset::ExecutionAttempt<crate::callback::CallbackError>,
) -> Result<DatasetResponse, Error> {
    catch_unwind(AssertUnwindSafe(|| {
        dataset_result::specification_response(execution)
    }))
    .map_err(|_| Error::Execution(DatasetError::Internal))?
    .map_err(Error::Execution)
}

fn run_error(error: application::RunError<Error>, prepared: &PreparedSpecification) -> Error {
    match error {
        application::RunError::Decode(error) => error,
        application::RunError::Sources(mut errors) if prepared.sources().len() == 1 => {
            errors.pop().expect("source failure").1
        }
        application::RunError::Sources(errors) => Error::Sources(
            errors
                .into_iter()
                .map(|(index, error)| (prepared.sources()[index].clone(), error))
                .collect(),
        ),
        application::RunError::AdmittedSources(errors) => Error::Sources(
            errors
                .into_iter()
                .map(|(index, error)| {
                    (
                        prepared.sources()[index].clone(),
                        match error {
                            application::SourceIssue::Decode(error) => error,
                            application::SourceIssue::OdmValues(values) => Error::OdmValues(values),
                        },
                    )
                })
                .collect(),
        ),
        application::RunError::Bind(error) => Error::Bind(error),
        error @ application::RunError::Boundary { .. } => Error::ResourceLimit(
            crate::resource_diagnostics::run_limit(&error, prepared.sources())
                .expect("source boundary owner"),
        ),
    }
}
