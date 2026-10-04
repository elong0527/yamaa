//! Infrastructure adapters. The bootstrap resource is embedded at build time.
#![forbid(unsafe_code)]

pub mod function_transport;
pub mod numeric_transport;
pub mod scalar_transport;

/// Exercise resource inclusion without depending on the checkout at runtime.
pub fn installation_resource() -> &'static str {
    include_str!("../resources/installation.txt")
}

pub mod arrow_table;
mod arrow_temporal;
pub mod table_transport;

pub mod scalar_bytes;
