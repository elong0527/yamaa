//! Project preparation using the same approved native roots and snapshots as
//! a specification alone. Project metadata capture grants no runtime authority.
use crate::{
    file_preparation::{self, FileSpecification},
    file_resources::{Error as ResourceError, Resources},
    project_source,
    specification_run::RejectedRun,
    specification_source::CapturedSchema,
};
use std::sync::Arc;
use yamaa_core::project_function::Language;

#[derive(Debug)]
pub enum Error {
    Resource(ResourceError),
    Environment(project_source::Failure<ResourceError>),
    Document(file_preparation::Error),
    Compile(Box<RejectedRun>),
}

/// Prepare `specification` with the project file `environment` from
/// `resources`. Schemas are retained capabilities; public entry points use
/// the shipped domain and project closures.
pub fn prepare(
    mut resources: Resources,
    specification: &str,
    environment: &str,
    host: Language,
    specification_schema: Arc<CapturedSchema>,
    project_schemas: project_source::ProjectSchemas,
) -> Result<FileSpecification, Error> {
    let mut environment = capture_environment(&mut resources, environment, host, project_schemas)?;
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
    FileSpecification::compile(
        resources,
        specification.into(),
        document,
        links,
        environment,
    )
    .map_err(|rejected| Error::Compile(rejected.into_run()))
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
