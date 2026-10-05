//! Host-independent value contracts and scalar primitives, not a dataset engine.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod column_dependencies;
pub mod conversion;
mod decimal_rounding;
pub mod dependency_analysis;
pub mod evaluation;
pub mod numeric;
pub mod numeric_compiler;
pub mod numeric_parser;
pub mod predicate;
pub mod reference_binding;
pub mod temporal;
pub mod value;

/// Version of the shared core compiled into a native installation.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod reduction;
pub mod table;
