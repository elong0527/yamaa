//! Points the selftest table (`src/selftest/cases.rs`) at the case files it
//! embeds (#3030 D108).
//!
//! Every source archive carries the cases in this crate's `selftest`
//! directory, staged from the repository's `benchmarks/` by
//! tools/staging/engine_bundle.py, so a build from an archive reads nothing
//! outside its package. Only a build in the repository, where no staged copy
//! exists, reads them from `benchmarks/` itself, their one source. The path
//! is relative to the table's own file, so it is the same wherever the
//! package is unpacked, and a later installation that shares the Cargo build
//! of an earlier one rebuilds nothing.

use std::path::Path;

fn main() {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo names the package");
    let cases = if Path::new(&manifest).join("selftest").is_dir() {
        "../../selftest"
    } else {
        "../../../../../benchmarks"
    };
    println!("cargo:rustc-env=YAMAA_SELFTEST_CASES={cases}");
    println!("cargo:rerun-if-changed=build.rs");
}
