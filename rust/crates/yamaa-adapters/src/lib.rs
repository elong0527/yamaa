//! Infrastructure adapters. The bootstrap resource is embedded at build time.
#![forbid(unsafe_code)]

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
pub mod table_transport;

pub mod scalar_bytes;

pub mod aggregate_transport;

pub mod numeric_syntax;

pub mod regex_transport;

pub mod predicate_syntax;
pub mod schema_transport;
pub mod yaml_decode;
mod yaml_source;
pub mod yaml_transport;
