//! Owned project preparation using the same approved native roots and snapshots
//! as ordinary file builds. Project metadata capture grants no runtime authority.
use crate::{
    file_preparation,
    file_resources::{Error as ResourceError, Resources},
    project_run::{Attempt, PreparedRun, RejectedRun},
    project_source,
    specification_source::CapturedSchema,
};
use std::sync::Arc;
use yamaa_core::project_function::Language;
use yamaa_engine::project_activation::ActivationPort;

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
    pub fn retained_sources(
        &self,
        aliases: usize,
        text: usize,
    ) -> Result<crate::file_resources::RetainedSources, crate::file_resources::RetentionFailure>
    {
        self.resources.retain_sources(aliases, text)
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
    /// Consume only the complete sealed native capture. No externally supplied
    /// standalone document or replacement environment can enter this transfer.
    pub(crate) fn from_workflow(
        entry: crate::file_graph::CapturedEntry,
    ) -> Result<Self, Box<RejectedOrdinary>> {
        let (mut resources, entry, document, environment) = entry.into_ordinary_parts();
        match PreparedRun::prepare_with_profile_inspection(
            document,
            environment,
            |document, dataset, path| {
                crate::source_profile_inspection::inspect(document, &resources, dataset, path)
            },
        )
        .and_then(|run| {
            let findings = crate::verification_logs::location_findings(&resources, &run);
            if findings.is_empty() {
                Ok(run)
            } else {
                Err(run.reject(yamaa_core::specification::PrepareError::Invalid(findings)))
            }
        }) {
            Ok(run) => {
                // All capture, selection and compilation has completed. Study
                // authority uses ordinary build admission, never metadata quota.
                resources.finish_metadata_budget();
                Ok(Self {
                    run: Arc::new(run),
                    resources,
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
    /// domain and environment closures.
    pub fn prepare(
        mut resources: Resources,
        specification: &str,
        environment: &str,
        host: Language,
        specification_schema: Arc<CapturedSchema>,
        environment_schema: Arc<CapturedSchema>,
    ) -> Result<Self, Error> {
        let mut environment =
            capture_environment(&mut resources, environment, host, environment_schema)?;
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
        let run = PreparedRun::prepare_with_profile_inspection(
            document,
            environment,
            |document, dataset, path| {
                crate::source_profile_inspection::inspect(document, &resources, dataset, path)
            },
        )
        .map_err(Error::Compile)?;
        let findings = crate::verification_logs::location_findings(&resources, &run);
        if !findings.is_empty() {
            return Err(Error::Compile(run.reject(
                yamaa_core::specification::PrepareError::Invalid(findings),
            )));
        }
        Ok(Self {
            run: Arc::new(run),
            resources,
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
    /// Explicit source-byte capture stays in the owner's original native roots.
    pub fn capture_input_metadata(
        &mut self,
        index: usize,
        maximum: usize,
    ) -> Result<std::sync::Arc<[u8]>, Vec<crate::issue_rows::Issue>> {
        let descriptors = crate::source_metadata::inputs(self.run()).map_err(|_| {
            crate::file_application::rejected("source_metadata", "descriptor_admission")
        })?;
        crate::source_metadata::capture(&mut self.resources, &descriptors, index, maximum)
    }

    pub(crate) fn publication_target(
        &self,
    ) -> Result<crate::file_publication::AnchoredTarget, ResourceError> {
        self.publication_field_target("path")
    }
    pub(crate) fn publication_field_target(
        &self,
        field: &str,
    ) -> Result<crate::file_publication::AnchoredTarget, ResourceError> {
        let origin = self
            .run
            .document()
            .written_output_field_origin(field)
            .ok_or(ResourceError::InvalidPath)?;
        self.resources
            .publication_target_from(origin.declaring_source, origin.written)
    }
    pub fn build<A: ActivationPort>(
        &mut self,
        activation: &mut A,
    ) -> Attempt<ResourceError, A::Error> {
        self.run.execute_with_ports(activation, &mut self.resources)
    }
    /// Let an installed port borrow the exact retained lock without copying it
    /// or exposing study authority. Port construction performs no activation.
    pub fn build_with<'run, A: ActivationPort>(
        &'run mut self,
        factory: impl FnOnce(&'run PreparedRun) -> A,
    ) -> Attempt<ResourceError, A::Error> {
        let mut activation = factory(&self.run);
        self.run
            .execute_with_ports(&mut activation, &mut self.resources)
    }
}

/// Capture the original environment and declared metadata through the same
/// approved reader. Static workflow preparation reuses this transfer without
/// introducing a second capture or any activation or study-source port.
pub(crate) fn capture_environment(
    resources: &mut Resources,
    written: &str,
    host: Language,
    schema: Arc<CapturedSchema>,
) -> Result<project_source::OwnedEnvironment, Error> {
    let mut reader = crate::file_metadata::Reader { resources };
    let prepared = match yamaa_engine::build::prepare_environment(
        &mut reader,
        &mut project_source::Decoder(schema),
        written,
        host,
        Default::default(),
    ) {
        Ok(prepared) => Ok(prepared),
        Err(yamaa_engine::build::EnvironmentFailure::Resource(error)) => {
            return Err(Error::Resource(error));
        }
        Err(yamaa_engine::build::EnvironmentFailure::Preparation(error)) => Err(error),
    };
    project_source::adapt_preparation(prepared)
        .map_err(Error::Environment)
        .map(project_source::PreparedEnvironment::into_owned)
}
