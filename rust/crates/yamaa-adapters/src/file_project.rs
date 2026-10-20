//! Owned project preparation using the same approved native roots and snapshots
//! as ordinary file builds. Project metadata capture grants no runtime authority.
use crate::{
    file_preparation,
    file_resources::{Error as ResourceError, Resources, RunScope},
    project_run::{Attempt, PreparedRun, RejectedRun},
    project_source,
    specification_source::CapturedSchema,
};
use std::sync::Arc;
use yamaa_core::application_observer::Scope;
use yamaa_core::project_activation::ActivationPort;
use yamaa_core::project_function::Language;

#[derive(Debug)]
pub enum Error {
    Resource(ResourceError),
    Environment(project_source::Failure<ResourceError>),
    Document(file_preparation::Error),
    Compile(Box<RejectedRun>),
}

pub struct FileProject {
    run: Arc<PreparedRun>,
    resources: Resources,
    /// Inputs a project run may serve from artifacts it already built.
    linked: Vec<crate::file_resources::LinkedInput>,
    /// The entry path as written and the producing specifications its
    /// `schema` links name: the declarations a run-start refusal names.
    entry: String,
    links: Vec<crate::specification_source::LinkedSpecification>,
}
/// Ordinary compilation rejection beside the original native resource owner.
/// Projection refusal and file deletion never consume or replace this evidence.
pub struct RejectedOrdinary {
    run: Box<RejectedRun>,
    entry: String,
    resources: Resources,
}
impl RejectedOrdinary {
    pub fn run(&self) -> &RejectedRun {
        &self.run
    }
    pub fn entry(&self) -> &str {
        &self.entry
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    pub fn retained_sources(&self) -> crate::file_resources::RetainedSources {
        self.resources.retain_sources()
    }
}
impl std::fmt::Debug for RejectedOrdinary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedOrdinary")
            .field("entry", &self.entry)
            .field("error", &self.run.rejection.error)
            .field("capture_reads", &self.capture_reads())
            .finish_non_exhaustive()
    }
}
impl FileProject {
    pub fn resolved_specification(
        &self,
    ) -> Result<crate::resolved_artifact::ResolvedArtifact, crate::resolved_artifact::Error> {
        crate::resolved_artifact::single(
            self.run.document(),
            &self.resources,
            self.run
                .captured_environment()
                .root_optional()
                .map(|root| root.source().identity.clone()),
        )
    }
    /// Consume only the complete sealed native capture. No externally supplied
    /// standalone document or replacement environment can enter this transfer.
    pub(crate) fn from_workflow(
        entry: crate::file_entry::CapturedEntry,
    ) -> Result<Self, Box<RejectedOrdinary>> {
        let (resources, entry, document, links, environment) = entry.into_parts();
        let linked = crate::file_resources::linked_inputs(&resources, &document, &links);
        match PreparedRun::prepare_with_profile_inspection(
            document,
            &links,
            environment,
            |document, dataset, path| {
                crate::source_profile_inspection::inspect(document, &resources, dataset, path)
            },
        )
        .and_then(|run| {
            let findings = crate::run_report::location_findings(&resources, &run, &links);
            if findings.is_empty() {
                Ok(run)
            } else {
                Err(run.reject(yamaa_core::specification::PrepareError::Invalid(findings)))
            }
        }) {
            Ok(run) => {
                // All capture, selection and compilation has completed. Study
                // authority uses ordinary build admission, never metadata quota.
                Ok(Self {
                    run: Arc::new(run),
                    resources,
                    linked,
                    entry,
                    links,
                })
            }
            Err(run) => Err(Box::new(RejectedOrdinary {
                run,
                entry,
                resources,
            })),
        }
    }
    /// Schemas are retained capabilities; public entry points use the shipped
    /// domain and project closures. `environment` names the project file.
    pub fn prepare(
        mut resources: Resources,
        specification: &str,
        environment: &str,
        host: Language,
        specification_schema: Arc<CapturedSchema>,
        project_schemas: project_source::ProjectSchemas,
    ) -> Result<Self, Error> {
        let mut environment =
            capture_environment(&mut resources, environment, host, project_schemas)?;
        let document = file_preparation::prepare_document(
            &mut resources,
            specification,
            Some(Arc::clone(&specification_schema)),
        )
        .map_err(Error::Document)?;
        crate::file_inventory::prepare(
            &mut resources,
            &specification_schema,
            &mut environment,
            yamaa_core::submission_inventory::requested(document.model().document()),
            &document.source().identity,
        )
        .map_err(Error::Document)?;
        let links = file_preparation::capture_links(
            &mut resources,
            &document,
            Some(&specification_schema),
            &mut None,
        )
        .map_err(Error::Document)?;
        let linked = crate::file_resources::linked_inputs(&resources, &document, &links);
        let run = PreparedRun::prepare_with_profile_inspection(
            document,
            &links,
            environment,
            |document, dataset, path| {
                crate::source_profile_inspection::inspect(document, &resources, dataset, path)
            },
        )
        .map_err(Error::Compile)?;
        let findings = crate::run_report::location_findings(&resources, &run, &links);
        if !findings.is_empty() {
            return Err(Error::Compile(run.reject(
                yamaa_core::specification::PrepareError::Invalid(findings),
            )));
        }
        Ok(Self {
            run: Arc::new(run),
            resources,
            linked,
            entry: specification.into(),
            links,
        })
    }
    pub fn run(&self) -> &PreparedRun {
        &self.run
    }
    /// A retained attempt keeps original documents and metadata independently
    /// of this file capability, without acquiring its study-resource authority.
    pub fn retained_run(&self) -> Arc<PreparedRun> {
        Arc::clone(&self.run)
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    /// Open one run's study scope; see [`Resources::run_scope`].
    pub fn run_scope(
        &self,
        resolver: Option<Arc<dyn crate::host_resources::Resolver>>,
    ) -> RunScope {
        self.resources.run_scope(resolver)
    }
    /// Verify every held metadata snapshot at run start, before activation or
    /// any study read: the entry, its layers, dictionaries and linked
    /// producing specifications, the project file, the environment file it
    /// links, and the lock, function definitions and codelists they declare.
    /// A changed or missing file refuses the run naming its declaration; see
    /// [`crate::file_application::verify_held`].
    pub fn verify_metadata(&self) -> Result<(), crate::file_application::Failure> {
        use crate::changed_metadata as changed;
        crate::file_application::verify_held(&self.resources, |identity, failure| {
            let document = self.run.document();
            let captured = self.run.captured_environment();
            changed::specification("specification", &self.entry, document, identity, failure)
                .or_else(|| changed::links(document, &self.links, identity, failure))
                .or_else(|| {
                    let root = captured.root_optional()?;
                    let project = captured.project()?;
                    changed::inventory(project, captured.inventory_documents(), identity, failure)
                        .or_else(|| {
                            let environment = changed::Environment {
                                written: captured.written()?,
                                host: self.run.environment().host(),
                                root,
                                project,
                                environment_file: captured.environment_file(),
                                lock: captured.lock(),
                                captures: captured.captures(),
                            };
                            changed::environment(&environment, identity, failure)
                        })
                })
        })
    }
    /// Bounded aliases of the held metadata bytes; no file access or replay.
    pub fn retained_sources(&self) -> crate::file_resources::RetainedSources {
        self.resources.retain_sources()
    }
    pub(crate) fn publication_target(
        &self,
        run: &RunScope,
    ) -> Result<crate::resource_publication::AnchoredTarget, ResourceError> {
        self.publication_field_target(run, "path")
    }
    pub(crate) fn publication_field_target(
        &self,
        run: &RunScope,
        field: &str,
    ) -> Result<crate::resource_publication::AnchoredTarget, ResourceError> {
        let origin = self
            .run
            .document()
            .written_output_field_origin(field)
            .ok_or(ResourceError::InvalidPath)?;
        run.resources()
            .publication_target_from(origin.base_source, origin.written)
    }
    /// Activate, capture and execute in `run`; this value is unchanged.
    pub fn build<A: ActivationPort>(
        &self,
        run: &mut RunScope,
        activation: &mut A,
        scope: Scope<'_>,
    ) -> Attempt<ResourceError, A::Error> {
        let mut port = crate::file_resources::HeldPort::new(run, &self.linked);
        self.run.execute_with_ports(activation, &mut port, scope)
    }
    /// Let an installed port borrow the exact retained lock without copying it
    /// or exposing study authority. Port construction performs no activation.
    pub fn build_with<'run, A: ActivationPort>(
        &'run self,
        run: &mut RunScope,
        factory: impl FnOnce(&'run PreparedRun) -> A,
        scope: Scope<'_>,
    ) -> Attempt<ResourceError, A::Error> {
        let mut activation = factory(&self.run);
        let mut port = crate::file_resources::HeldPort::new(run, &self.linked);
        self.run
            .execute_with_ports(&mut activation, &mut port, scope)
    }
}

/// Capture the original project file, the environment file it links for the
/// host and their declared metadata through the same approved reader. Static
/// workflow preparation reuses this transfer without introducing a second
/// capture or any activation or study-source port.
pub(crate) fn capture_environment(
    resources: &mut Resources,
    written: &str,
    host: Language,
    schemas: project_source::ProjectSchemas,
) -> Result<project_source::OwnedEnvironment, Error> {
    let mut reader = crate::file_metadata::Reader { resources };
    let prepared = match yamaa_core::build::prepare_environment(
        &mut reader,
        &mut project_source::Decoder(schemas),
        written,
        host,
    ) {
        Ok(prepared) => Ok(prepared),
        Err(yamaa_core::build::EnvironmentFailure::Resource(error)) => {
            return Err(Error::Resource(error));
        }
        Err(yamaa_core::build::EnvironmentFailure::Preparation(error)) => Err(error),
    };
    project_source::adapt_preparation(prepared)
        .map_err(Error::Environment)
        .map(|prepared| prepared.into_owned().with_written(written))
}
