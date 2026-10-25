//! Infrastructure adapters. The bootstrap resource is embedded at build time.
#![deny(unsafe_code)]
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_file;

pub mod application_measurements;
#[cfg(feature = "arrow")]
pub mod arrow_bridge;
pub mod column_table;
pub mod columns;
pub mod epoch;
pub mod file_define;
pub mod json_wire;
pub mod public_table;
pub mod report_storage;
mod storage_api;
pub mod submission_json;
pub mod submission_xml;
pub mod submission_xpt;

pub mod scalar_bytes;

pub mod lint;
pub mod style_values;
pub mod yaml_decode;
#[cfg(feature = "yaml")]
mod yaml_source;

pub mod csv_artifact;
pub mod csv_source;
pub mod dataset_json_source;
pub mod decoder;
pub mod odm_xml;
pub mod parquet_artifact;
pub mod parquet_source;
pub mod sas_source;

pub mod document;
pub mod shipped_schema;
pub mod specification_source;

pub mod source_decoding;
mod source_profile_inspection;
pub mod specification_run;

pub mod resource_diagnostics;
pub mod source_diagnostics;
mod source_ordinal;
pub mod specification_diagnostics;

pub(crate) mod run_report;
pub mod specification_check;
pub mod specification_report;

pub mod typed_csv;

pub mod canonical_path;
mod changed_metadata;
pub mod file_application;
pub mod file_configuration;
pub mod file_entry;
mod file_metadata;
pub mod file_preparation;
pub mod file_project;
#[cfg(unix)]
pub mod file_publication;
// File descriptors and retained Windows handles use platform-selected
// implementations of the same publication boundary.
#[cfg(windows)]
#[path = "file_publication_windows.rs"]
pub mod file_publication;
pub mod file_resolver;
pub mod file_resources;
pub mod file_workflow;
pub mod host_resources;
pub mod issue_location;
pub mod publication;
pub mod publisher;
pub mod resolved_artifact;
pub mod table_artifact;

pub mod project_source;

pub mod project_lock;
pub mod project_lock_diagnostics;
mod python_packaging;

pub mod project_environment_diagnostics;

pub mod project_attempt;
pub mod project_report;

pub mod project_function_diagnostics;

pub mod project_codelist_diagnostics;

pub mod project_activation_diagnostics;

pub mod project_activation_observations;

pub mod project_application;
pub mod project_scope;

mod project_diagnostic_projection;

mod file_inventory;

pub mod callback;
pub mod dataset_result;
mod execution_diagnostics;
mod report_scalar;
mod schema_outcomes;
mod yaml_diagnostics;
