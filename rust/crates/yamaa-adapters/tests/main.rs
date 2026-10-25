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

// The replay of the expected outcomes written as data (#3691, #3798), apart
// from the families below.
mod case_api_contracts;
mod diagnostics;

mod arrow_host_bridge;
mod ast_lowering;
mod check_oracle;
mod declaration_contract_vectors;
mod declaration_language_header;
mod declaration_layer_composition;
mod declaration_migration_hints;
mod declaration_one_spelling;
mod declaration_project_files;
mod declaration_schema_capture;
mod declaration_v1_lowering;
mod declaration_v1_schema;
mod execution_aggregate_key_reads;
mod execution_aggregate_record_operations;
mod execution_column_driver_bindings;
mod execution_column_window_drivers;
mod execution_completed_intermediates;
mod execution_date_text_forms;
mod execution_date_text_reads;
mod execution_declarative_case_predicates;
mod execution_declarative_self;
mod execution_filtered_mapping_source;
mod execution_grouped_driver_dataset;
mod execution_implicit_join_absence;
mod execution_intermediate_driver_columns;
mod execution_no_rows_record_reads;
mod execution_retired_odm_context;
mod execution_window_group_sources;
mod execution_window_semantics;
mod file_diagnostic_origins;
mod file_project;
mod file_resources;
mod file_workflow;
mod host_resources;
mod network_shares;
mod odm_xml;
mod project_diagnostics;
mod project_domain_submission;
mod project_environment;
mod project_run;
mod project_scope;
mod project_source;
mod resolved_artifact;
mod resolved_artifact_wire;
mod resolved_plan;
mod run_scopes;
mod scalar_bytes;
mod scalar_declarative_text_case;
mod scalar_declarative_text_decimal;
mod scalar_declarative_text_extract;
mod scalar_declarative_text_template;
mod scalar_language;
mod scalar_text_positions;
mod specification_reports;
mod specification_sources;
// The file wraps its tests in a module of its own name, which kept their
// paths when each file was its own binary.
#[allow(clippy::module_inception)]
mod submission_encoders;
mod submission_service;
mod submission_staleness;
mod submission_transport;
mod table_arrow_table;
mod table_csv_source;
mod table_dataset_json_source;
mod table_parquet_bytes;
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
