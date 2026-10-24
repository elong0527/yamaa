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

/// Run the benchmark `name` on the file route for the Python host and compare
/// the run with what its `expected/` files state, as public conformance reads
/// them (`tools/conformance/public_conformance.py`, #3691).
///
/// The run reads a scratch copy of the case without its expected files,
/// README and `benchmark.yaml`, and builds each entry `benchmark.yaml` lists
/// for Python, in order: an entry that names no environment passes the case's
/// `project.yaml` when it has one, and `null` passes none. A case without
/// entries runs its one specification. A Define benchmark, whose expected
/// failure belongs to its submission, a case conformance excludes and a
/// symbolic link in the case fail the call.
///
/// A positive benchmark's entries check clean and build without an issue, each
/// CSV under `expected/` equals the one file of its name the entries saved,
/// byte for byte, and each file they saved has its expected file; any other
/// expected file fails the call. A negative benchmark's
/// `expected/error.yaml` is a partial assertion about the first issue the
/// entries report: its phase, condition and spec paths, its rule when the
/// file writes one, and each context field the file writes, recursively. A
/// validation error's spec paths are each owned by a validation issue of its
/// condition, whose spec paths lie at or below them, and no entry of a
/// negative benchmark exposes or saves an output.
#[track_caller]
pub fn reproduce(name: &str) {
    fn stage(name: &str, from: &Path, to: &Path) {
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let file = entry.file_name();
            if ["expected", "README.md", "benchmark.yaml"].contains(&file.to_str().unwrap()) {
                continue;
            }
            let kind = entry.file_type().unwrap();
            assert!(
                !kind.is_symlink(),
                "{name}: {} is a symbolic link, which this copy does not keep",
                entry.path().display()
            );
            let target = to.join(&file);
            if kind.is_dir() {
                std::fs::create_dir(&target).unwrap();
                stage(name, &entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    /// Every file below `root`.
    fn files(root: &Path) -> std::collections::BTreeSet<PathBuf> {
        let mut found = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                found.extend(files(&path));
            } else {
                found.insert(path);
            }
        }
        found
    }
    /// The path below `at` of the first field `stated` writes that `reported`
    /// lacks or holds another value at: a mapping states the fields it writes,
    /// recursively, and any other value is compared whole.
    fn differing(stated: &Value, reported: &Value, at: &str) -> Option<String> {
        let Some(fields) = stated.as_object() else {
            return (stated != reported).then(|| at.to_owned());
        };
        if !reported.is_object() {
            return Some(at.to_owned());
        }
        fields
            .iter()
            .find_map(|(field, value)| match reported.get(field) {
                Some(reported) => differing(value, reported, &format!("{at}.{field}")),
                None => Some(format!("{at}.{field}")),
            })
    }
    let case = path(name);
    let metadata = yaml(&case.join("benchmark.yaml"));
    assert!(
        metadata["define"].is_null(),
        "{name}: a Define benchmark's expected files belong to its submission"
    );
    assert!(
        metadata["conformance"]["excluded"].is_null(),
        "{name}: conformance excludes it"
    );
    let project = case
        .join("project.yaml")
        .is_file()
        .then(|| "project.yaml".to_owned());
    let entries: Vec<(String, Option<String>)> = match metadata["entries"]["python"].as_array() {
        Some(entries) => entries
            .iter()
            .map(|entry| {
                let environment = match entry.get("environment") {
                    None => project.clone(),
                    Some(Value::Null) => None,
                    Some(file) => Some(file.as_str().unwrap().to_owned()),
                };
                (
                    entry["specification"].as_str().unwrap().to_owned(),
                    environment,
                )
            })
            .collect(),
        None => {
            let mut specifications: Vec<String> = std::fs::read_dir(&case)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .filter(|file| file.starts_with("spec") && file.ends_with(".yaml"))
                .collect();
            assert_eq!(specifications.len(), 1, "{name}: {specifications:?}");
            vec![(specifications.remove(0), project)]
        }
    };
    let study = super::Scratch::new(name);
    stage(name, &case, &study);
    let staged = files(&study);
    let negative = case.join("expected/error.yaml").is_file();
    let host = yamaa_core::project_function::Language::Python;
    let mut issues = Vec::new();
    for (specification, environment) in &entries {
        let path = study.join(specification).to_str().unwrap().to_owned();
        let environment = environment
            .as_ref()
            .map(|file| study.join(file).to_str().unwrap().to_owned());
        let request = || yamaa_core::domain_entry::Request {
            specification: &path,
            environment: environment.as_deref(),
        };
        let identity = || super::study::identity(name, &path);
        if !negative {
            let checked = yamaa_adapters::file_application::check(host, request(), identity());
            let checked: Vec<_> = checked.iter().map(super::study::issue_row).collect();
            assert!(
                checked.is_empty(),
                "{name}: check of {specification}: {checked:?}"
            );
        }
        let reported = match super::study::domain(host, request(), identity()) {
            Ok(mut domain) if negative => {
                assert!(
                    domain.output().is_none(),
                    "{name}: {specification} exposed an output"
                );
                assert!(domain.save().is_err(), "{name}: {specification} saved");
                domain.issues()
            }
            Ok(mut domain) => {
                let reported = domain.issues();
                if reported.is_empty() {
                    assert!(
                        domain.save().unwrap(),
                        "{name}: {specification} saved nothing"
                    );
                }
                reported
            }
            Err(refused) => refused,
        };
        let reported: Vec<_> = reported.iter().map(super::study::issue_row).collect();
        assert!(
            negative || reported.is_empty(),
            "{name}: {specification}: {reported:?}"
        );
        issues.extend(reported);
    }
    if negative {
        let stated = error(name);
        let Some(first) = issues.first() else {
            panic!("{name}: no entry reported an issue");
        };
        for field in ["phase", "condition", "spec_paths"] {
            assert_eq!(
                first[field], stated[field],
                "{name}: the first issue's {field}"
            );
        }
        if let Some(rule) = stated.get("rule") {
            assert_eq!(&first["rule"], rule, "{name}: the first issue's rule");
        }
        if !stated["context"].is_null() {
            if let Some(at) = differing(&stated["context"], &first["context"], "context") {
                panic!(
                    "{name}: the first issue's {at} is not what expected/error.yaml states\n  \
                     stated: {:?}\nreported: {:?}",
                    stated["context"], first["context"]
                );
            }
        }
        if stated["phase"] == "validation" {
            let declared: Vec<&str> = stated["spec_paths"]
                .as_array()
                .unwrap()
                .iter()
                .map(|path| path.as_str().unwrap())
                .collect();
            let below = |actual: &str, path: &str| {
                actual
                    .strip_prefix(path)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '[']))
            };
            let mut owned = std::collections::BTreeSet::<&str>::new();
            for issue in &issues {
                let paths: Vec<&str> = issue["spec_paths"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                if issue["phase"] == "validation"
                    && issue["condition"] == stated["condition"]
                    && !paths.is_empty()
                    && paths
                        .iter()
                        .all(|actual| declared.iter().any(|path| below(actual, path)))
                {
                    owned.extend(
                        declared
                            .iter()
                            .copied()
                            .filter(|path| paths.iter().any(|actual| below(actual, path))),
                    );
                }
            }
            let unowned: Vec<&str> = declared
                .iter()
                .copied()
                .filter(|path| !owned.contains(path))
                .collect();
            assert!(
                unowned.is_empty(),
                "{name}: no validation issue of the stated condition owns {unowned:?}: {issues:?}"
            );
        }
        return;
    }
    let mut written = files(&study);
    written.retain(|path| !staged.contains(path));
    let mut expected: Vec<PathBuf> = std::fs::read_dir(case.join("expected"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    expected.sort();
    for truth in &expected {
        let file = truth.file_name().unwrap().to_str().unwrap();
        assert!(
            file.ends_with(".csv"),
            "{name}: expected/{file} is not a CSV, which is all this compares"
        );
        let saved: Vec<&PathBuf> = written
            .iter()
            .filter(|path| path.file_name() == Some(std::ffi::OsStr::new(file)))
            .collect();
        let [saved] = saved[..] else {
            panic!("{name}: the entries saved {file} as {saved:?}");
        };
        assert_eq!(
            String::from_utf8(std::fs::read(saved).unwrap()),
            String::from_utf8(std::fs::read(truth).unwrap()),
            "{name}: the saved {file} against expected/{file}"
        );
    }
    for path in &written {
        assert!(
            expected
                .iter()
                .any(|truth| truth.file_name() == path.file_name()),
            "{name}: {} has no expected file",
            path.display()
        );
    }
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
