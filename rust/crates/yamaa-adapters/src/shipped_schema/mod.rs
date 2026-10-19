//! Package-owned schema closure; no directory search or caller-provided schema.
//! The current release ships one version. Shared schema admission checks the
//! entry and each parent's declared version before preparing an executable model.
//!
//! Each closure is fixed when the package is built, so the process admits it
//! once, on first use, and every later check, compilation and run shares that
//! immutable admitted closure.
use crate::specification_source::{
    CapturedSchema, Error, InheritanceError, InheritancePort, PreparedDocument, Source,
};
use std::sync::{Arc, OnceLock};
mod data;

/// Admit the exact generated closure retained inside both native packages.
pub fn capture() -> Result<Arc<CapturedSchema>, Error> {
    static ADMITTED: OnceLock<Arc<CapturedSchema>> = OnceLock::new();
    once(&ADMITTED, || {
        CapturedSchema::admit(sources(data::MODULES), 0)
    })
}

/// Admit the separately versioned environment root with its exact shared module.
pub fn capture_environment() -> Result<Arc<CapturedSchema>, Error> {
    static ADMITTED: OnceLock<Arc<CapturedSchema>> = OnceLock::new();
    once(&ADMITTED, || {
        admit_root(data::ENVIRONMENT_MODULES, "environment_class")
    })
}

/// Admit both separately versioned project roots: the project file with its
/// codelist sources, and the environment file it links with its functions.
pub fn capture_project() -> Result<crate::project_source::ProjectSchemas, Error> {
    static ADMITTED: OnceLock<Arc<CapturedSchema>> = OnceLock::new();
    Ok(crate::project_source::ProjectSchemas {
        project: once(&ADMITTED, || {
            admit_root(data::PROJECT_MODULES, "project_class")
        })?,
        environment: capture_environment()?,
    })
}

/// The closure `admitted` holds, admitting it first if no call has yet. A
/// failed admission is returned and retried by the next call; threads that
/// race on the first admission keep the closure the first of them stored.
fn once(
    admitted: &OnceLock<Arc<CapturedSchema>>,
    admit: impl FnOnce() -> Result<Arc<CapturedSchema>, Error>,
) -> Result<Arc<CapturedSchema>, Error> {
    if let Some(schema) = admitted.get() {
        return Ok(Arc::clone(schema));
    }
    let schema = admit()?;
    Ok(Arc::clone(admitted.get_or_init(|| schema)))
}

fn sources(modules: &[(&str, &str)]) -> Vec<Source> {
    modules
        .iter()
        .map(|(name, source)| Source {
            identity: (*name).into(),
            bytes: source.as_bytes().to_vec(),
        })
        .collect()
}

fn admit_root(modules: &[(&str, &str)], root_class: &str) -> Result<Arc<CapturedSchema>, Error> {
    CapturedSchema::admit_root(sources(modules), 0, root_class)
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

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;

    #[test]
    fn each_shipped_closure_is_admitted_once_and_shared_across_threads() {
        let specification = capture().unwrap();
        assert!(Arc::ptr_eq(&specification, &capture().unwrap()));
        let environment = capture_environment().unwrap();
        let project = capture_project().unwrap();
        assert!(Arc::ptr_eq(&project.environment, &environment));
        assert!(Arc::ptr_eq(
            &project.project,
            &capture_project().unwrap().project
        ));
        let shared = std::thread::spawn(|| capture().unwrap()).join().unwrap();
        assert!(Arc::ptr_eq(&shared, &specification));
        // The shared closure is the one a fresh admission captures.
        let fresh = CapturedSchema::admit(sources(data::MODULES), 0).unwrap();
        assert_eq!(fresh.sources().len(), specification.sources().len());
        for (fresh, kept) in fresh.sources().iter().zip(specification.sources()) {
            assert_eq!(
                (&fresh.identity, &fresh.bytes),
                (&kept.identity, &kept.bytes)
            );
        }
    }
}
