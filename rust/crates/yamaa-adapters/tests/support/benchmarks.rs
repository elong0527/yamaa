//! The live benchmarks, in either language (#3030 D120).
//!
//! The corpus moves to 1.0 one prefix at a time (P2-5 to P2-8), so a test
//! that reads a benchmark reads what the benchmark itself states, such as its
//! `expected/error.yaml`, rather than a 0.5 spelling of it (#3422).
use std::path::{Path, PathBuf};
use yamaa_core::json::Value;
use yamaa_core::schema::{Document, DocumentNode as N};

/// The directory of the benchmark `name`.
pub fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../benchmarks")
        .join(name)
}

/// Every benchmark, in name order: each directory of `benchmarks/` that
/// holds a `benchmark.yaml`. The corpus tests narrow it to the cases they read.
pub fn all() -> Vec<PathBuf> {
    let mut cases: Vec<_> = std::fs::read_dir(path(""))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|case| case.join("benchmark.yaml").is_file())
        .collect();
    cases.sort();
    cases
}

/// The value of the YAML file at `path`, as JSON.
pub fn yaml(path: &Path) -> Value {
    fn value(d: &Document, id: usize) -> Value {
        match &d.nodes()[id] {
            N::Null => Value::Null,
            N::Boolean(v) => (*v).into(),
            N::Integer(v) => yamaa_adapters::json_wire::decode(v).unwrap(),
            N::Float(v) => yamaa_adapters::json!(v),
            N::Text(v) => v.clone().into(),
            N::Sequence(ids) => ids.iter().map(|&id| value(d, id)).collect(),
            N::Mapping(pairs) => pairs
                .iter()
                .map(|&(key, item)| (value(d, key).as_str().unwrap().to_owned(), value(d, item)))
                .collect(),
        }
    }
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let d = yamaa_adapters::yaml_decode::decode_yaml(&bytes)
        .unwrap()
        .document;
    value(&d, d.root())
}

/// The specification of the benchmark `name` as 1.0 writes it, from
/// `tests/fixtures/converted/`, for a test that edits its text: the benchmark
/// moves to 1.0 in its own pull request, and the copy keeps the test's edits
/// in one language. Its inputs and expected files are the benchmark's own.
pub fn converted(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/converted")
        .join(name)
        .join("spec.yaml");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// What the negative benchmark `name` states in its `expected/error.yaml`.
pub fn error(name: &str) -> Value {
    yaml(&path(name).join("expected/error.yaml"))
}

/// A finding's context without the layer a refused layer names. A schema
/// finding in a 1.x specification names the files the run loaded, `source`
/// and `entry` together, as an inherited 0.5 finding does; a standalone 0.5
/// specification's finding names none, so neither a benchmark's
/// `expected/error.yaml` nor a capture from 0.5 holds them.
pub fn without_layer(mut context: yamaa_core::json::Map<String, Value>) -> Value {
    if context.remove("entry").is_some() {
        context.remove("source");
    }
    Value::Object(context)
}

/// Whether `bytes` are a YAML file that declares the 1.x header, as the
/// engine routes a root (SCHEMA-2, D115).
pub fn declares_header(bytes: &[u8]) -> bool {
    yamaa_adapters::yaml_decode::decode_yaml(bytes)
        .is_ok_and(|decoded| yamaa_core::language::declares_header(&decoded.document))
}
