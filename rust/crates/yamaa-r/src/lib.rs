//! R composition roots and lossless public result conversion.
use extendr_api::prelude::*;
mod api_entry;
mod arrow_stream;
mod context_values;
mod function_callback;
mod handle;
mod input_frames;
mod interface;
mod interrupt;
mod progress;
mod project_activation;
mod public_table;
mod thread_bound;

/// The engine's release: every workspace crate carries the workspace's
/// version, as the engine's core does.
#[extendr]
fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

include!("generated/yamaa.rs");
