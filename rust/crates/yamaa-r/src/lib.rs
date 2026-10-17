//! R composition roots and lossless public result conversion.
use extendr_api::prelude::*;
mod arrow_bridge;
mod define_entry;
mod diagnostic_records;
mod domain_entry;
mod function_callback;
mod handle;
mod issue_frame;
mod project_activation;
mod public_table;
mod resolved_artifact;
mod resource_resolver;
mod scalars;
mod thread_bound;

#[extendr]
fn core_version() -> &'static str {
    yamaa::VERSION
}

include!("generated/yamaa.rs");
