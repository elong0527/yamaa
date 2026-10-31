//! Every integration test of `yamaa`, in one binary.
//!
//! Each file under `tests/` holds one family and is a module here, so the
//! families share one build of the support tree and link into one
//! executable (#3556). A new family file gets its `mod` line below, which
//! `every_family_file_is_declared` requires.

#[allow(dead_code)]
mod support;

mod caller_paths;
mod cancellation;
mod client_partitions;
mod conditions;
mod copy_entry;
mod csv_byte_order_mark;
mod diagnostic_records;
mod end_states;
mod file_type;
mod info;
mod issue_locations;
mod language_header;
mod lint;
mod lint_inheritance;
mod lint_restated;
mod memory;
mod memory_inputs;
mod memory_route;
mod odm_input;
mod partitioned_confinement;
mod partitioned_corpus;
mod partitioned_lookups;
mod partitioned_memory;
mod partitioned_publication;
mod partitioned_reading;
mod plan;
mod plan_reuse;
mod printed_text;
mod progress;
mod project_binding;
mod project_operations;
mod read_odm;
mod resolver_access;
mod resolver_kinds;
mod save_table;
mod save_targets;
mod schema;
mod streaming;
mod vocabulary;

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
