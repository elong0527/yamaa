//! R composition roots and lossless public result conversion.
use extendr_api::prelude::*;
mod api_entry;
mod context_values;
mod function_callback;
mod handle;
mod interface;
mod interrupt;
mod project_activation;
mod public_table;
mod thread_bound;

#[extendr]
fn core_version() -> &'static str {
    yamaa::VERSION
}

include!("generated/yamaa.rs");
