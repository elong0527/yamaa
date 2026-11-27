//! The project file a preparation captures, read through the same approved
//! native roots and snapshots as a specification alone. Project metadata
//! capture grants no runtime authority.
use crate::{
    document::Closures,
    file_preparation,
    file_resources::{Error as ResourceError, Resources},
    project_source,
};
use yamaa_core::project_function::Language;

#[derive(Debug)]
pub enum Error {
    Resource(ResourceError),
    Environment(project_source::Failure<ResourceError>),
    Document(file_preparation::Error),
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
