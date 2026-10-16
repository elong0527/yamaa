//! Metadata inspection preserves the original declaring file and written path.
use crate::{file_resources::Resources, specification_source::PreparedDocument};
use yamaa_core::resource::ResourceFailure;

pub(crate) fn inspect(
    document: &PreparedDocument,
    resources: &Resources,
    dataset: &str,
    path: &str,
) -> Option<(ResourceFailure, String)> {
    let origin = document.written_input_origin(dataset, "path")?;
    let error = resources
        .inspect_from(origin.base_source, origin.written)
        .err()?;
    let failure = yamaa_core::specification_run::SpecificationSourcePort::resource_failure(
        resources, &error,
    )?;
    Some((
        failure,
        document.written_source_path(dataset).unwrap_or(path).into(),
    ))
}
