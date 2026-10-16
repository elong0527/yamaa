//! Infrastructure adapters. The bootstrap resource is embedded at build time.
#![deny(unsafe_code)]
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_file;

pub mod application_measurements;
pub mod arrow_batch;
pub mod arrow_bridge;
pub mod arrow_table;
mod arrow_temporal;
pub mod file_define;
pub mod json_wire;
pub mod public_table;
mod report_publication;
pub mod report_storage;
mod storage_api;
pub mod submission_json;

pub mod scalar_bytes;

pub mod style_values;
pub mod yaml_decode;
#[cfg(feature = "yaml")]
mod yaml_source;

pub mod csv_artifact;
pub mod csv_source;
pub mod parquet_artifact;
pub mod parquet_source;
pub mod sas_source;

pub mod shipped_schema;
pub mod specification_source;

mod source_profile_inspection;
pub mod specification_run;
pub mod specification_run_view;

pub mod resource_diagnostics;
pub mod source_diagnostics;
mod source_ordinal;
pub mod specification_diagnostics;

pub mod diagnostic_record;
pub mod issue_rows;
pub mod specification_check;
pub mod specification_report;
mod verification_logs;

pub mod typed_csv;

pub mod file_application;
pub mod file_configuration;
pub mod file_graph;
mod file_metadata;
pub mod file_preparation;
pub mod file_producer_build;
pub mod file_project;
#[cfg(unix)]
pub mod file_publication;
// File descriptors and retained Windows handles use platform-selected
// implementations of the same publication boundary.
#[cfg(windows)]
#[path = "file_publication_windows.rs"]
pub mod file_publication;
pub mod file_resources;
pub mod file_workflow;
pub mod host_resources;
pub mod resolved_artifact;
mod resource_publication;

pub mod project_source;

pub mod project_lock;
pub mod project_lock_diagnostics;

pub mod project_environment_diagnostics;

pub mod project_source_decoder;

pub mod project_attempt;
pub mod project_report;
pub mod project_run;

pub mod project_function_diagnostics;

pub mod project_codelist_diagnostics;

pub mod project_activation_diagnostics;

pub mod project_activation_observations;

pub mod producer_admission;
pub mod producer_attempt;
pub mod producer_check;
mod producer_diagnostics;
pub mod producer_graph;
mod producer_log_publication;
pub mod producer_publication;
pub mod producer_report;
pub mod producer_result;
pub mod project_application;
pub mod project_check;

mod project_diagnostic_projection;

mod file_inventory;

pub mod callback;
pub mod dataset_result;
mod dataset_snapshot;
mod execution_diagnostics;
pub mod ipc_table;
mod report_scalar;
mod schema_outcomes;
mod yaml_diagnostics;

#[cfg(test)]
mod test_diagnostic_truth;
