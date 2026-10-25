//! The engine's application layer: the use cases that prepare, bind, execute
//! and publish over the ports of `yamaa-core`, through the readers, writers
//! and resolvers of `yamaa-adapters`. The facade is its one caller.
//!
//! Its integration suite already lives here and drives those use cases where
//! `yamaa-adapters` still holds them; the modules follow (#3761).
#![deny(unsafe_code)]
