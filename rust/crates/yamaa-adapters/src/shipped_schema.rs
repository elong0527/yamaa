//! Package-owned schema closure; no directory search or caller-provided schema.
//! The current release ships one version. Shared schema admission checks the
//! entry and each parent's declared version before preparing an executable model.
use crate::specification_source::{
    CapturedSchema, Error, InheritanceError, InheritancePort, PreparedDocument, Source,
};
use std::sync::Arc;
#[path = "shipped_schema_data.rs"]
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
        Default::default(),
    )
}

/// Admit the separately versioned environment root with its exact shared module.
pub fn capture_environment() -> Result<Arc<CapturedSchema>, Error> {
    CapturedSchema::admit_root(
        data::ENVIRONMENT_MODULES
            .iter()
            .map(|(name, source)| Source {
                identity: (*name).into(),
                bytes: source.as_bytes().to_vec(),
            })
            .collect(),
        0,
        "environment_class",
        Default::default(),
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
