//! Host-independent value contracts and scalar primitives, not a dataset engine.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
pub mod application_issue;

pub mod aggregate_parser;
pub mod bound_expression;
pub mod column_dependencies;
pub mod conversion;
pub mod csv_source;
pub mod dataset;
pub mod dataset_checks;
mod decimal_rounding;
pub mod dependency_analysis;
pub mod diagnostic;
pub mod evaluation;
pub mod function_signature;
pub mod intermediate_reference;
pub mod key_relation;
pub mod match_value;
pub mod numeric;
pub mod numeric_compiler;
pub mod numeric_parser;
pub mod parquet_source;
pub mod predicate;
pub mod predicate_compiler;
pub mod predicate_parser;
pub mod producer_admission;
pub mod producer_contract;
pub mod reference_binding;
pub mod reference_scope;
pub mod resource;
pub mod schema;
pub mod specification;
pub mod temporal;
pub mod typed_csv;
pub mod value;

/// Version of the shared core compiled into a native installation.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod regex;

pub mod reduction;
pub mod table;

pub mod project_call_binding;
pub mod project_call_document;
pub mod project_calls;
pub mod project_environment;

pub mod project_environment_document;

pub mod project_function;

pub mod project_function_document;

pub mod project_function_result;

pub mod project_limits;

pub mod project_terminology;

pub mod project_terminology_document;

pub mod project_environment_diagnostics;

pub mod project_codelist_binding;
