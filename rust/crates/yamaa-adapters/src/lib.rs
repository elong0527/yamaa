//! Infrastructure adapters. The bootstrap resource is embedded at build time.
#![deny(unsafe_code)]
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_file;

pub mod column_dependency_transport;
pub mod dataset_profile;
pub mod dataset_transport;
pub mod dependency_transport;
mod function_signature;
pub mod function_transport;
pub mod inheritance_transport;
pub mod numeric_transport;
mod predicate_transport;
pub mod reference_transport;
pub mod scalar_transport;

/// Exercise resource inclusion without depending on the checkout at runtime.
pub fn installation_resource() -> &'static str {
    include_str!("../resources/installation.txt")
}

pub mod arrow_table;
mod arrow_temporal;
pub mod public_table;
pub mod table_transport;

pub mod scalar_bytes;

pub mod aggregate_transport;

pub mod numeric_syntax;

pub mod regex_transport;

pub mod predicate_syntax;
mod schema_diagnostic_kinds;
pub mod schema_transport;
pub mod yaml_decode;
mod yaml_source;
pub mod yaml_transport;

pub mod csv_artifact;
pub mod csv_source;
pub mod parquet_artifact;
pub mod parquet_source;

pub mod shipped_schema;
pub mod specification_source;

pub mod specification_run;

pub mod specification_diagnostics;

pub mod issue_rows;
pub mod specification_check;
pub mod specification_report;

pub mod typed_csv;

#[cfg(any(unix, windows))]
pub mod file_application;
#[cfg(any(unix, windows))]
pub mod file_configuration;
#[cfg(any(unix, windows))]
pub mod file_preparation;
#[cfg(unix)]
pub mod file_publication;
#[cfg(windows)]
#[path = "file_publication_windows.rs"]
pub mod file_publication;
#[cfg(any(unix, windows))]
pub mod file_resources;

pub mod project_source;

pub mod project_lock;

pub mod project_environment_diagnostics;

pub mod project_source_decoder;

pub mod project_function_diagnostics;
