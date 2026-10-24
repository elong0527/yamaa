//! The engine's C ABI over the facade: the function table of
//! [`yamaa_boundary::abi`], its handle table, the callback adapters and the
//! Arrow stream export (#3702; the design on #2870, sections 3.4 to 3.7).
//! Both bindings link it and call it through `yamaa_boundary::client`.
//!
//! A host gets the table from [`negotiate`], which answers a request in the
//! check order of [`yamaa_boundary`] and returns the table only on
//! acceptance. In the shipped packages the binding links this crate and
//! calls [`negotiate`] as a plain Rust function. In the test-only loaded
//! mode, this crate built as a library with the `dynamic-engine` feature
//! exports it as `yamaa_engine_negotiate`, which the binding's loader looks
//! up beside its own module, so both modes run the same table. The loading
//! tests' fixture engines, in `examples/`, answer as engines of another
//! build would.
//!
//! Every entry builds the facade's engine for the call, from the call's host
//! and its callbacks, and runs inside `catch_unwind`: a panic is a boundary
//! refusal, and nothing unwinds across the ABI. Results, plans holders and
//! tables live in a generation-checked handle table, so a released handle or
//! one of another kind is refused instead of being dereferenced, and
//! releasing twice does nothing. Each result sits behind its own lock: a call
//! that reenters a busy result, as a probe may during a save, is refused,
//! while every other result stays usable. A plans holder is shared without a
//! lock, so a run started from a project function can use it too.
//!
//! The unsafe code sits in two files, each with its safety argument:
//! `entries.rs`, the table's entries and the memory they hand out, and
//! `host.rs`, the calls into a host's callbacks.
#![deny(unsafe_code)]

#[allow(unsafe_code)]
mod entries;
mod handles;
#[allow(unsafe_code)]
mod host;

pub use entries::negotiate;
#[doc(hidden)]
pub use entries::negotiate_withholding;
#[cfg(feature = "dynamic-engine")]
pub use entries::yamaa_engine_negotiate;
