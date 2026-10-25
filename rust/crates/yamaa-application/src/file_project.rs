//! Project preparation using the same approved native roots and snapshots as
//! a specification alone. Project metadata capture grants no runtime authority.
use crate::{
    document::Closures,
    file_preparation::{self, FileSpecification},
    file_resources::{Error as ResourceError, Resources},
    project_source,
    specification_run::RejectedRun,
};
use yamaa_core::project_function::Language;

#[derive(Debug)]
pub enum Error {
    Resource(ResourceError),
    Environment(project_source::Failure<ResourceError>),
    Document(file_preparation::Error),
    Compile(Box<RejectedRun>),
}

/// Prepare `specification` with the project file `environment` from
/// `resources`. The closures are retained capabilities; public entry points
/// use the shipped 1.0 closures.
pub fn prepare(
    mut resources: Resources,
    specification: &str,
    environment: &str,
    host: Language,
    closures: Closures,
) -> Result<FileSpecification, Error> {
    let mut environment = capture_environment(&mut resources, environment, Some(host), closures)?;
    let document = file_preparation::prepare_document(&mut resources, specification)
        .map_err(Error::Document)?;
    crate::file_inventory::prepare(
        &mut resources,
        &mut environment,
        yamaa_core::submission_inventory::requested(document.document()),
        &document.source().identity,
    )
    .map_err(Error::Document)?;
    let links = file_preparation::capture_links(&mut resources, &document, &mut None)
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

/// Capture the original project file and the metadata it declares through the
/// same approved reader: the host's lock, every function file it lists and
/// its codelist files (D8). An engine that claims no host language (`host`
/// is none) reads no lock. Static workflow preparation reuses this transfer
/// without introducing a second capture, any activation or any study read.
///
/// Public calls pass the shipped `closures`, which read 1.0 files (#3030
/// D76).
pub(crate) fn capture_environment(
    resources: &mut Resources,
    written: &str,
    host: Option<Language>,
    closures: Closures,
) -> Result<project_source::OwnedEnvironment, Error> {
    let prepared = match yamaa_core::build::prepare_environment(
        resources,
        &mut crate::decoder::Decoder::<std::convert::Infallible>::with_closures(closures),
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
