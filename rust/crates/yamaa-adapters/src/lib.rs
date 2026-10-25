//! The engine's infrastructure adapters: what reads and writes bytes, files
//! and host objects behind the ports of `yamaa-core`. Nothing here
//! orchestrates a run: `yamaa-application` prepares, binds, executes and
//! publishes over these modules, and the facade composes both.
//!
//! - **Formats.** The physical readers and writers: CSV (`csv_source`,
//!   `typed_csv`, `csv_artifact`), Parquet (`parquet_source`,
//!   `parquet_artifact`), SAS (`sas_source`), ODM XML (`odm_xml`),
//!   Dataset-JSON (`dataset_json_source`), the submission encoders
//!   (`submission_json`, `submission_xml`, `submission_xpt`), YAML
//!   (`yaml_decode`), and the lint of a file's written form (`lint`,
//!   `style_values`).
//! - **Decoding.** The decoder port (`decoder`), the decoding of a
//!   specification's held sources and the run error it reports
//!   (`source_decoding`), and the documents a run reads: specifications
//!   (`document`, `specification_source`), project files (`project_source`)
//!   and their packaging locks (`project_lock`).
//! - **Tables.** The engine's columnar tables (`column_table`, `columns`,
//!   `epoch`), host scalars (`scalar_bytes`), the public form of a table
//!   (`public_table`) and its Arrow bridge (`arrow_bridge`).
//! - **Resources.** The resolver port: the local file resolver
//!   (`file_resolver`, `canonical_path`, `file_configuration`), a caller's
//!   resolver (`host_resources`) and the snapshots an operation holds
//!   (`file_resources`); and the publisher port a save writes through
//!   (`publisher`), over the file publisher (`file_publication`).
//! - **Records.** The diagnostics these report (`resource_diagnostics`,
//!   `source_diagnostics`, `specification_diagnostics`,
//!   `project_diagnostic_projection`), the typed execution results they
//!   render (`dataset_result`, `callback`), a report's encoded rows
//!   (`report_storage`) and the JSON wire form (`json_wire`).
//! - **Schema.** The shipped schema closures (`shipped_schema`); the
//!   bootstrap resource is embedded at build time.
#![deny(unsafe_code)]
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_file;

#[cfg(feature = "arrow")]
pub mod arrow_bridge;
pub mod column_table;
pub mod columns;
pub mod epoch;
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

pub mod resource_diagnostics;
pub mod source_diagnostics;
mod source_ordinal;
pub mod specification_diagnostics;

pub mod typed_csv;

pub mod canonical_path;
pub mod file_configuration;
mod file_metadata;
#[cfg(unix)]
pub mod file_publication;
// File descriptors and retained Windows handles use platform-selected
// implementations of the same publication boundary.
#[cfg(windows)]
#[path = "file_publication_windows.rs"]
pub mod file_publication;
pub mod file_resolver;
pub mod file_resources;
pub mod host_resources;
pub mod publisher;

pub mod project_source;

pub mod project_lock;
mod python_packaging;

pub mod project_diagnostic_projection;

pub mod callback;
pub mod dataset_result;
mod execution_diagnostics;
mod schema_outcomes;
mod yaml_diagnostics;
