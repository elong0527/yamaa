//! The engine's C ABI over the facade: the function table of
//! [`yamaa_boundary::abi`], its handle table, the callback adapters and the
//! Arrow stream export (#3702; the design on #2870, sections 3.4 to 3.7).
//! Both bindings link it and call it through `yamaa_boundary::client`.
//!
//! A host gets the table from [`negotiate`], which answers a request in the
//! check order of [`yamaa_boundary`] and returns the table only on
//! acceptance. In the shipped packages the binding links this crate and
//! calls [`negotiate`] as a plain Rust function; a test configuration will
//! load the same crate as a library and look the entry up by name, so both
//! modes run the same table.
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
