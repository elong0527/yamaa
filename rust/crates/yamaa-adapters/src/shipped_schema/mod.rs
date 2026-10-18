//! Package-owned schema closure; no directory search or caller-provided schema.
//! The current release ships one version. Shared schema admission checks the
//! entry and each parent's declared version before preparing an executable model.
use crate::specification_source::{
    CapturedSchema, Error, InheritanceError, InheritancePort, PreparedDocument, Source,
};
use std::sync::Arc;
mod data;

/// Admit the exact generated closure retained inside both native packages.
pub fn capture() -> Result<Arc<CapturedSchema>, Error> {
    CapturedSchema::admit(
        data::MODULES
            .iter()
            .map(|(name, source)| Source {
                identity: (*name).into(),
                bytes: source.as_bytes().to_vec(),
            })
            .collect(),
        0,
    )
}

/// Admit the separately versioned environment root with its exact shared module.
pub fn capture_environment() -> Result<Arc<CapturedSchema>, Error> {
    admit_root(data::ENVIRONMENT_MODULES, "environment_class")
}

/// Admit both separately versioned project roots: the project file with its
/// codelist sources, and the environment file it links with its functions.
pub fn capture_project() -> Result<crate::project_source::ProjectSchemas, Error> {
    Ok(crate::project_source::ProjectSchemas {
        project: admit_root(data::PROJECT_MODULES, "project_class")?,
        environment: capture_environment()?,
    })
}

fn admit_root(modules: &[(&str, &str)], root_class: &str) -> Result<Arc<CapturedSchema>, Error> {
    CapturedSchema::admit_root(
        modules
            .iter()
            .map(|(name, source)| Source {
                identity: (*name).into(),
                bytes: source.as_bytes().to_vec(),
            })
            .collect(),
        0,
        root_class,
    )
}

/// Prepare either original document form without host semantic selection.
pub fn prepare<P: InheritancePort>(
    source: Source,
    display_path: String,
    port: &mut P,
) -> Result<PreparedDocument, InheritanceError<P::Error>> {
    capture()
        .map_err(InheritanceError::Entry)?
        .prepare_document(source, display_path, port)
}
