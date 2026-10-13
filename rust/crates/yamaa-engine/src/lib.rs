//! Typed application services independent of host representations.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod application_observer;
pub mod build;
pub mod capacity;
pub mod dataset;
mod dataset_budget;
pub mod dataset_predicate;
pub mod dictionary_preparation;
pub mod document_preparation;
pub mod domain;
pub mod domain_entry;
pub mod execution_work;
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
pub mod style_values;
pub mod table_grouping;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub mod table_reduction;

pub mod source_ingestion;
pub mod source_metadata;
pub mod source_profile_inspection;
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

pub mod source_header;

pub mod source_decode;
