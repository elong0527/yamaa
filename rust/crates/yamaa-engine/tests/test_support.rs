//! Exercise the shared opt-in fake ports from a downstream integration boundary.
#![cfg(feature = "test-support")]
extern crate alloc;
use yamaa_engine::{
    dataset, function_invocation, specification_output as output, specification_run as run,
    test_support::*,
};
#[path = "../src/test_support/tests.rs"]
mod tests;
