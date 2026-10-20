//! Package-owned schema closures; no directory search or caller-provided schema.
//! The package ships the 0.5 closures the engine reads and, beside them, the
//! 1.0 closure of each file type (`yaml/v1/`), which a document's header
//! selects (SCHEMA-2). Shared schema admission checks the entry and each
//! parent's declared version before preparing an executable model.
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

/// Admit the 1.0 closure of one file type (`yaml/v1/`): its entry module and
/// every module it includes, with the type's root class (SCHEMA-2).
pub fn capture_versioned(
    file_type: yamaa_core::language::FileType,
) -> Result<Arc<CapturedSchema>, Error> {
    use yamaa_core::language::FileType;
    static SPEC: OnceLock<Arc<CapturedSchema>> = OnceLock::new();
    static PROJECT: OnceLock<Arc<CapturedSchema>> = OnceLock::new();
    static CODELISTS: OnceLock<Arc<CapturedSchema>> = OnceLock::new();
    static FUNCTION: OnceLock<Arc<CapturedSchema>> = OnceLock::new();
    let admitted = match file_type {
        FileType::Spec => &SPEC,
        FileType::Project => &PROJECT,
        FileType::Codelists => &CODELISTS,
        FileType::Function => &FUNCTION,
    };
    once(admitted, || {
        let (_, names) = data::V1_CLOSURES
            .iter()
            .find(|(name, _)| *name == file_type.name())
            .expect("every file type has a closure");
        let modules = names
            .iter()
            .map(|name| {
                *data::V1_MODULES
                    .iter()
                    .find(|(module, _)| module == name)
                    .expect("a closure names embedded modules")
            })
            .collect::<Vec<_>>();
        admit_root(&modules, file_type.root_class())
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
        for file_type in yamaa_core::language::FileType::ALL {
            let versioned = capture_versioned(file_type).unwrap();
            assert!(Arc::ptr_eq(
                &versioned,
                &capture_versioned(file_type).unwrap()
            ));
            assert_eq!(
                versioned.structure().root_class().name,
                file_type.root_class()
            );
            // The 1.0 bundle names the version its files declare.
            assert_eq!(versioned.structure().version(), "1.0");
        }
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
