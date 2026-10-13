//! Application boundary; the installation probe does not execute specifications.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod build;
pub mod dataset;
mod dataset_budget;
pub mod dataset_predicate;
mod dataset_verification;
pub mod dictionary_preparation;
pub mod document_preparation;
pub mod domain;
pub mod domain_entry;
pub mod file_preparation;
pub mod function_invocation;
pub mod host_values;
pub mod inheritance;
pub mod inheritance_capture;
pub mod inheritance_outcome;
pub mod inheritance_preparation;
pub mod numeric_lifecycle;
pub mod schema_capture;
pub mod standalone_preparation;
pub mod structural_preparation;
pub mod table_grouping;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

/// Bootstrap capability information, independent of Python and R representations.
#[derive(Debug, PartialEq, Eq)]
pub struct EngineInfo {
    pub core_version: &'static str,
    pub protocol_version: &'static str,
    pub execution_supported: bool,
}

/// Report only capabilities actually implemented by this revision.
pub fn engine_info() -> EngineInfo {
    EngineInfo {
        core_version: yamaa_core::VERSION,
        protocol_version: "installation-probe/1",
        execution_supported: false,
    }
}

pub mod table_reduction;

pub mod source_ingestion;
pub mod specification;
pub mod specification_output;
pub mod specification_run;

pub mod project_activation;
pub mod project_preparation;

pub mod producer_admission;
pub mod producer_build;
pub mod producer_graph;
pub mod producer_publication;
pub mod project_domain;

pub mod project_diagnostics;

pub mod project_codelist_diagnostics;
pub mod verification_diagnostics;

pub mod define;

pub mod submission_inventory;
