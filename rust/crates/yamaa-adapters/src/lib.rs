//! Infrastructure adapters. The bootstrap resource is embedded at build time.
#![deny(unsafe_code)]
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_file;

pub mod application_measurements;
pub mod arrow_table;
mod arrow_temporal;
#[cfg(any(unix, windows))]
pub mod file_define;
pub mod public_table;
mod report_publication;
pub mod report_storage;

pub mod scalar_bytes;

pub mod style_values;
pub mod yaml_decode;
mod yaml_source;

pub mod csv_artifact;
pub mod csv_source;
pub mod parquet_artifact;
pub mod parquet_source;

pub mod shipped_schema;
pub mod specification_source;

pub mod source_metadata;
mod source_profile_inspection;
pub mod specification_run;
pub mod specification_run_view;

pub mod source_diagnostics;
mod source_ordinal;
pub mod specification_diagnostics;

pub mod issue_rows;
pub mod specification_check;
pub mod specification_report;
mod verification_logs;

pub mod typed_csv;

#[cfg(any(unix, windows))]
pub mod file_application;
#[cfg(any(unix, windows))]
pub mod file_configuration;
#[cfg(any(unix, windows))]
pub mod file_graph;
#[cfg(any(unix, windows))]
mod file_metadata;
#[cfg(any(unix, windows))]
pub mod file_preparation;
#[cfg(any(unix, windows))]
pub mod file_producer_build;
#[cfg(any(unix, windows))]
pub mod file_project;
#[cfg(unix)]
pub mod file_publication;
#[cfg(windows)]
#[path = "file_publication_windows.rs"]
pub mod file_publication;
#[cfg(any(unix, windows))]
pub mod file_resources;
#[cfg(any(unix, windows))]
pub mod file_workflow;

pub mod project_source;

pub mod project_lock;
pub mod project_lock_diagnostics;

pub mod project_environment_diagnostics;

pub mod project_source_decoder;

pub mod project_attempt;
pub mod project_report;
pub mod project_run;
mod report_projection;

pub mod project_function_diagnostics;

pub mod project_codelist_diagnostics;

pub mod project_activation_diagnostics;

pub mod project_activation_observations;

pub mod producer_admission;
#[cfg(any(unix, windows))]
pub mod producer_attempt;
#[cfg(any(unix, windows))]
pub mod producer_check;
#[cfg(any(unix, windows))]
mod producer_diagnostics;
pub mod producer_graph;
#[cfg(any(unix, windows))]
mod producer_log_publication;
#[cfg(any(unix, windows))]
pub mod producer_publication;
#[cfg(any(unix, windows))]
pub mod producer_report;
#[cfg(any(unix, windows))]
pub mod producer_result;
#[cfg(any(unix, windows))]
pub mod project_application;
pub mod project_check;

mod project_diagnostic_projection;

mod file_inventory;

pub mod host_limits;

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
