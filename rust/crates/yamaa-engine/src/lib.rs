//! Application boundary; the installation probe does not execute specifications.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod dataset;
mod dataset_budget;
pub mod dataset_predicate;
mod dataset_verification;
pub mod domain;
pub mod domain_entry;
pub mod function_invocation;
pub mod inheritance;
pub mod inheritance_preparation;
pub mod numeric_lifecycle;
pub mod table_grouping;

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

pub mod specification;
pub mod specification_output;
pub mod specification_run;

pub mod project_activation;

pub mod project_domain;
