//! Every integration test of `yamaa-adapters`, in one binary.
//!
//! Each file under `tests/` holds one family and is a module here, so the
//! families share one build of the support tree and link into one
//! executable (#3556). A new family file gets its `mod` line below, which
//! `every_family_file_is_declared` requires.

// The copied modules and the feature-gated families leave some helpers
// unused in a given build.
#[allow(dead_code)]
mod support;

mod arrow_host_bridge;
mod declaration_language_header;
mod declaration_migration_hints;
mod declaration_v1_lowering;
mod declaration_v1_schema;
mod odm_xml;
mod report_storage;
mod scalar_bytes;
// The file wraps its tests in a module of its own name, which kept their
// paths when each file was its own binary.
#[allow(clippy::module_inception)]
mod submission_encoders;
mod table_column_storage;
mod table_csv_source;
mod table_dataset_json_source;
mod table_parquet_readback;
mod table_parquet_source;
mod table_partitioned_sources;
mod table_public_batches;
mod table_sas_allocation;
mod table_sas_source;
mod uv_lock;

/// A file under `tests/` that is neither a module here nor its own `[[test]]`
/// in the manifest would never run (#3556).
#[test]
fn every_family_file_is_declared() {
    let modules = include_str!("main.rs");
    let manifest = include_str!("../Cargo.toml");
    let tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut undeclared = Vec::new();
    for entry in std::fs::read_dir(tests).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        let module = match name.strip_suffix(".rs") {
            Some("main") => continue,
            Some(stem) => stem.to_owned(),
            None if path.join("mod.rs").is_file() => name.clone(),
            None => continue,
        };
        let here = modules
            .lines()
            .any(|line| line.trim() == format!("mod {module};"));
        let own = manifest
            .lines()
            .any(|line| line.trim() == format!("path = \"tests/{name}\""));
        if here == own {
            undeclared.push(name);
        }
    }
    undeclared.sort();
    assert!(
        undeclared.is_empty(),
        "declare each file once, as a module here or as its own [[test]]: {undeclared:?}"
    );
}
