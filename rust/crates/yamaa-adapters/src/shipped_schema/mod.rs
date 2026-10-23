//! Package-owned schema closures; no directory search or caller-provided schema.
//! The package ships the 1.0 closure of each file type (`yaml/v1/`), which a
//! document's header selects (SCHEMA-2), and that type's JSON Schema
//! ([`json_schema`]).
//!
//! Each closure is fixed when the package is built, so the process admits it
//! once, on first use, and every later check, compilation and run shares that
//! immutable admitted closure.
use crate::specification_source::{
    CapturedSchema, Error, InheritanceError, PreparedDocument, Source,
};
use std::sync::{Arc, OnceLock};
use yamaa_core::resolver::Resolver;
mod data;
mod json;
pub use json::json_schema;

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

/// Prepare a specification with the shipped 1.0 closures, without host
/// semantic selection; a file that declares `schema_version` is refused as
/// `unsupported_language_version` (SCHEMA-2).
pub fn prepare<P: Resolver>(
    source: Source,
    display_path: String,
    port: &mut P,
) -> Result<PreparedDocument, InheritanceError<P::Error>> {
    crate::specification_source::prepare_versioned(source, display_path, port)
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;

    #[test]
    fn each_shipped_closure_is_admitted_once_and_shared_across_threads() {
        let spec = yamaa_core::language::FileType::Spec;
        let shared = std::thread::spawn(move || capture_versioned(spec).unwrap())
            .join()
            .unwrap();
        assert!(Arc::ptr_eq(&shared, &capture_versioned(spec).unwrap()));
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
            // The shared closure is the one a fresh admission captures.
            let (_, names) = data::V1_CLOSURES
                .iter()
                .find(|(name, _)| *name == file_type.name())
                .unwrap();
            let modules = names
                .iter()
                .map(|name| *data::V1_MODULES.iter().find(|(m, _)| m == name).unwrap())
                .collect::<Vec<_>>();
            let fresh = admit_root(&modules, file_type.root_class()).unwrap();
            assert_eq!(fresh.sources().len(), versioned.sources().len());
            for (fresh, kept) in fresh.sources().iter().zip(versioned.sources()) {
                assert_eq!(
                    (&fresh.identity, &fresh.bytes),
                    (&kept.identity, &kept.bytes)
                );
            }
        }
    }
}
