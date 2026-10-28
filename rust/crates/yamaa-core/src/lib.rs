//! Shared language semantics, preparation and execution services, and the
//! submission metadata model. The adapters read and write files, and the
//! application layer runs the use cases over both.
#![forbid(unsafe_code)]

pub mod application_issue;
pub mod json;

pub mod ast;
pub mod front_end;

pub mod aggregate_reduction;
pub mod bound_expression;
pub mod cdisc_vocabulary;
pub mod column_dependencies;
pub mod conversion;
pub mod copy_expansion;
pub mod csv_precision;
pub mod csv_source;
pub mod dataset;
pub mod dataset_checks;
pub mod date_forms;
mod decimal_rounding;
pub mod decoder;
pub mod define_diagnostics;
pub mod dependency_analysis;
pub mod diagnostic;
pub mod expression;
pub mod expression_table;
pub mod function_signature;
pub mod intermediate_reference;
pub mod iso8601;
pub mod key_relation;
pub mod language;
pub mod limits;
pub mod mapping_cut;
pub mod numeric;
pub mod odm_items;
pub mod odm_source;
pub mod parquet_source;
pub mod portable_regex;
pub mod producer_contract;
pub mod project_scope;
pub mod reference_binding;
pub mod reference_scope;
pub mod resolver;
pub mod resource;
pub mod result_type;
pub mod run_report;
pub mod scalar_language;
mod scalar_verb;
pub mod schema;
pub mod source_ordinal;
pub mod specification;
pub mod specification_submission_document;
pub mod submission_composition;
pub mod submission_inventory;
mod submission_origin;
pub mod submission_staleness;
pub mod submission_transport;
mod submission_values;
pub mod temporal;
pub mod text_case;
pub mod text_decimal;
pub mod text_extract;
pub mod text_position;
pub mod text_replace;
pub mod text_template;
pub mod typed_csv;
pub mod value;

/// Version of the shared core compiled into a native installation.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod reduction;
pub mod table;
pub mod text_pad;

pub mod project_call_binding;
pub mod project_call_document;
pub mod project_calls;
pub mod project_environment;

pub mod project_function;

pub mod project_function_document;

pub mod project_function_result;

pub mod project_terminology;

pub mod project_environment_diagnostics;

pub mod project_codelist_binding;

pub mod project_diagnostics;

pub mod project_codelist_diagnostics;
pub mod verification_diagnostics;

pub mod yaml_diagnostics;

pub mod scalar_operator;

pub mod schema_capture;

pub mod table_reduction;

pub mod domain;

pub mod specification_run;

pub mod project_preparation;

pub mod project_layers;

pub mod dictionary_preparation;

pub mod domain_entry;

pub mod inheritance;

pub mod application_observer;

pub mod define;

pub mod project_domain;

pub mod inheritance_preparation;

pub mod resolution;

pub mod build;

pub mod table_grouping;

pub mod numeric_lifecycle;

pub mod function_batch;
pub mod function_invocation;
pub mod host_function_registry;
pub mod host_functions;

pub mod inheritance_outcome;

pub mod file_preparation;

pub mod project_activation;

pub mod inheritance_capture;

pub mod source_profile_inspection;

#[cfg(test)]
use crate::schema as document_schema;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/document.rs"]
mod test_document;
#[cfg(test)]
mod test_specification;
