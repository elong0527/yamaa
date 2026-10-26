//! Gate for the runnable in-memory example (issue #2735): the exact
//! `examples/memory.rs` program runs here with no filesystem access, so the
//! example cannot rot.
#![cfg(feature = "arrow")]

#[path = "../examples/memory.rs"]
mod memory_example;

#[test]
fn in_memory_example_runs_without_filesystem_access() {
    memory_example::main();
}
