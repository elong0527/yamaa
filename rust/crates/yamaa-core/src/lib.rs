//! Host-independent language contracts. Evaluation is not implemented yet.
#![no_std]
#![forbid(unsafe_code)]

/// Version of the shared core compiled into a native installation.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
