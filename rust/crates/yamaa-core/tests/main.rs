//! Every integration test of `yamaa-core`, in one binary.
//!
//! Each file under `tests/` holds one family and is a module here, so the
//! families share one build of the support tree and link into one
//! executable (#3556). A new family file gets its `mod` line below, which
//! `every_family_file_is_declared` requires.

// The copied modules and the feature-gated families leave some helpers
// unused in a given build.
#[allow(dead_code)]
mod support;

// The file wraps its tests in a module of its own name, which kept their
// paths when each file was its own binary.
#[allow(clippy::module_inception)]
mod diagnostics;
mod diagnostics_aggregate;
mod execution_case_predicates;
mod execution_declarative_text_replace;
mod execution_numeric_lifecycle;
mod execution_scalar_language;
mod host_function_registry;
mod numeric_aggregate_parser;
mod portable_math_golden;
mod ports;
mod preparation_dictionary_preparation;
mod preparation_project_preparation;
mod preparation_standalone;
mod project_activation;
mod project_execution;
mod project_function_invocation;
mod project_producer_contract;
mod project_rows;
mod project_specification;
mod relations_aggregate_compiler;
mod relations_reference_scope;
mod scalar_aggregate_reduction;
mod scalar_declarative_temporal;
mod scalar_sqrt;
mod schema_inheritance_dependency_diagnostics;
mod schema_references;
mod schema_style_values;
mod schema_submission;
mod schema_validation;
mod schema_windows;
mod service_build_metadata;
mod service_partitioned_reading;
mod service_specification_run;
mod source_csv_precision;
mod submission_limits;
mod table_grouping;
// The file wraps its tests in a module of its own name, which kept their
// paths when each file was its own binary.
#[allow(clippy::module_inception)]
mod temporal_date_text;

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
