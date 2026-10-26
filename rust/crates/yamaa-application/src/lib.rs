//! The engine's application layer: the use cases that prepare, bind, execute
//! and publish over the ports of `yamaa-core`, through the readers, writers
//! and resolvers of `yamaa-adapters`. The facade is its one caller.
//!
//! - **Files and projects.** One specification's lifecycle on the file route
//!   (`file_application`): its capture (`file_entry`, `file_workflow`), its
//!   preparation (`file_preparation`, `changed_metadata`), a project's
//!   (`file_project`, `project_application`, `project_scope`), and the
//!   locations of the issues they report (`issue_location`).
//! - **Runs.** A prepared specification's build (`specification_run`), its
//!   static check (`specification_check`) and the attempt it leaves
//!   (`project_attempt`, `project_activation_observations`).
//! - **Reports.** The result builder (`specification_report`), the run report
//!   (`run_report`), a project's half of it (`project_report` and the
//!   `project_*_diagnostics` it renders) and the measurements of one
//!   application (`application_measurements`).
//! - **Saves.** A save of every file an outcome writes (`publication`), a
//!   table saved on its own (`table_artifact`), a submission section's
//!   composition (`file_define`) and the versioned facts of a preparation
//!   (`resolved_artifact`).
#![deny(unsafe_code)]

// The adapters' modules, at the paths the modules below name them by.
#[cfg(all(test, feature = "yaml"))]
pub(crate) use yamaa_adapters::canonical_path;
pub(crate) use yamaa_adapters::{
    callback, column_table, csv_artifact, csv_source, dataset_result, decoder, document,
    file_configuration, file_publication, file_resolver, file_resources, host_resources, json,
    json_wire, parquet_artifact, parquet_source, project_diagnostic_projection, project_lock,
    project_source, public_table, publisher, report_storage, resource_diagnostics, shipped_schema,
    source_decoding, source_diagnostics, specification_diagnostics, specification_source,
    submission_json, submission_xml, submission_xpt, yaml_decode,
};

pub mod application_measurements;
mod source_profile_inspection;
pub mod specification_run;

pub(crate) mod run_report;
pub mod specification_check;
pub mod specification_report;

mod changed_metadata;
pub mod file_application;
pub mod file_define;
pub mod file_entry;
mod file_inventory;
pub mod file_preparation;
pub mod file_project;
pub mod file_workflow;
pub mod issue_location;
pub mod publication;
pub mod resolved_artifact;
pub mod table_artifact;

pub mod project_lock_diagnostics;

pub mod project_environment_diagnostics;

pub mod project_attempt;
pub mod project_report;

pub mod project_function_diagnostics;

pub mod project_codelist_diagnostics;

pub mod project_activation_diagnostics;

pub mod project_activation_observations;

pub mod project_application;
pub mod project_scope;

mod report_scalar;
