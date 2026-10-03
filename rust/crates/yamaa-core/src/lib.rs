//! Host-independent value contracts and scalar primitives, not a dataset engine.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod numeric;
pub mod temporal;
pub mod value;

/// Version of the shared core compiled into a native installation.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
