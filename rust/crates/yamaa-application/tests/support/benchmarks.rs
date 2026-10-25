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

/// Copy the files of the benchmark `name` into the directory `into`, each at
/// its path in the benchmark, as a run of the case reads them: every file but
/// the case's truth and its description, its `expected/` directory,
/// `README.md` and `benchmark.yaml`. A symbolic link fails the call, since
/// the copy does not keep one. Returns each copied file's path in `into`,
/// with forward slashes.
#[track_caller]
pub fn stage(name: &str, into: &Path) -> Vec<String> {
    fn copy(name: &str, from: &Path, to: &Path, at: &str, staged: &mut Vec<String>) {
        let entries = std::fs::read_dir(from)
            .unwrap_or_else(|error| panic!("{name}: {}: {error}", from.display()));
        for entry in entries {
            let entry = entry.unwrap_or_else(|error| panic!("{name}: {}: {error}", from.display()));
            let (path, file) = (entry.path(), entry.file_name());
            if at.is_empty()
                && ["expected", "README.md", "benchmark.yaml"]
                    .iter()
                    .any(|skipped| file == *skipped)
            {
                continue;
            }
            let kind = entry
                .file_type()
                .unwrap_or_else(|error| panic!("{name}: {}: {error}", path.display()));
            assert!(
                !kind.is_symlink(),
                "{name}: {} is a symbolic link, which this copy does not keep",
                path.display()
            );
            let (target, written) = (to.join(&file), format!("{at}{}", file.to_string_lossy()));
            if kind.is_dir() {
                std::fs::create_dir(&target)
                    .unwrap_or_else(|error| panic!("{name}: {}: {error}", target.display()));
                copy(name, &path, &target, &format!("{written}/"), staged);
            } else {
                std::fs::copy(&path, &target)
                    .unwrap_or_else(|error| panic!("{name}: {}: {error}", path.display()));
                staged.push(written);
            }
        }
    }
    let mut staged = Vec::new();
    copy(name, &path(name), into, "", &mut staged);
    staged
}

/// Run the benchmark `name` on the file route for the Python host and compare
/// the run with what its `expected/` files state, as public conformance reads
/// them (`tools/conformance/public_conformance.py`, #3691).
///
/// The run reads a scratch copy of the case without its expected files,
/// README and `benchmark.yaml`, and builds each entry `benchmark.yaml` lists
/// for Python, in order: an entry that names no environment passes the case's
/// `project.yaml` when it has one, and `null` passes none. A case without
/// entries runs its one specification. These fail the call: a Define
/// benchmark, whose expected failure belongs to its submission; a case
/// conformance excludes; a project that supplies project functions, which
/// this route's empty host-functions double cannot call and the command-line
/// conformance excludes too; a symbolic link in the case; and an issue of
/// phase `engine` or condition `unsupported_operation` from any entry, which
/// conformance judges an error or unsupported whatever the benchmark states.
///
/// A positive benchmark's entries check clean and build without an issue.
/// Each output an entry declares, the `output.report` and `output.path` of
/// its composed layers, is saved at that path beside the entry and equals its
/// expected file byte for byte; every expected file is a declared CSV output,
/// and the entries save nothing else. This compares the saved bytes, not the
/// typed host view of the table that conformance also compares, nor the order
/// in which a save publishes its files.
///
/// A negative benchmark's `expected/error.yaml` is a partial assertion about
/// the first issue the entries report: its phase, condition and spec paths,
/// its rule when the file writes one, and each context field the file writes,
/// recursively. A validation error's spec paths are each owned by a
/// validation issue of its condition, whose spec paths lie at or below them,
/// and no entry of a negative benchmark exposes or saves an output.
#[track_caller]
pub fn reproduce(name: &str) {
    let case = path(name);
    let entries = python_entries(name, &case);
    let study = super::Scratch::new(name);
    stage(name, &study);
    let staged = files(name, &study);
    let negative = case.join("expected/error.yaml").is_file();
    let issues = run_entries(name, &study, &entries, negative);
    // Conformance judges every issue of every entry before the first one.
    for issue in &issues {
        assert!(
            issue["phase"] != "engine",
            "{name}: an engine issue, which conformance judges an error: {issue:?}"
        );
        assert!(
            issue["condition"] != "unsupported_operation",
            "{name}: an unsupported operation, which conformance judges unsupported: {issue:?}"
        );
    }
    if negative {
        first_issue_is_stated(name, &issues);
        return;
    }
    assert!(issues.is_empty(), "{name}: {issues:?}");
    saved_outputs_are_expected(name, &case, &study, &entries, &staged);
}

/// Every file below `root`.
fn files(name: &str, root: &Path) -> std::collections::BTreeSet<PathBuf> {
    let mut found = std::collections::BTreeSet::new();
    let entries = std::fs::read_dir(root)
        .unwrap_or_else(|error| panic!("{name}: {}: {error}", root.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|error| panic!("{name}: {}: {error}", root.display()))
            .path();
        if path.is_dir() {
            found.extend(files(name, &path));
        } else {
            found.insert(path);
        }
    }
    found
}

/// The layers of the composition whose entry is `path`, in contribution
/// order: its parent chain, then the fragments it includes, then the
/// layer itself (LAYER-3).
fn layers(path: &Path) -> Vec<Value> {
    let layer = yaml(path);
    let directory = path.parent().unwrap_or(Path::new(""));
    let mut layers = match layer["parent"].as_str() {
        Some(parent) => layers(&directory.join(parent)),
        None => Vec::new(),
    };
    let fragments = layer["include"].as_array().into_iter().flatten();
    for fragment in fragments.filter_map(Value::as_str) {
        layers.push(yaml(&directory.join(fragment)));
    }
    layers.push(layer);
    layers
}

/// Whether the project file at `project` lists function files, read as
/// conformance reads it: a file that cannot be read lists none.
fn supplies_functions(project: &Path) -> bool {
    let Some(document) = std::fs::read(project)
        .ok()
        .and_then(|bytes| yamaa_adapters::yaml_decode::decode_yaml(&bytes).ok())
        .map(|decoded| decoded.document)
    else {
        return false;
    };
    document
        .field(document.root(), "functions")
        .is_some_and(|functions| match &document.nodes()[functions] {
            N::Null | N::Boolean(false) => false,
            N::Sequence(items) => !items.is_empty(),
            N::Mapping(pairs) => !pairs.is_empty(),
            _ => true,
        })
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

/// The entries the benchmark at `case` lists for Python, each its
/// specification and its environment, once the case is one this route runs:
/// not a Define benchmark, not excluded from conformance, and with no
/// project that supplies project functions.
#[track_caller]
fn python_entries(name: &str, case: &Path) -> Vec<(String, Option<String>)> {
    let metadata = yaml(&case.join("benchmark.yaml"));
    assert!(
        metadata["define"].is_null(),
        "{name}: a Define benchmark's expected files belong to its submission"
    );
    assert!(
        metadata["conformance"]["excluded"].is_null(),
        "{name}: conformance excludes it"
    );
    let text = |value: &Value, what: &str| -> String {
        value
            .as_str()
            .unwrap_or_else(|| panic!("{name}: {what} is not a file name: {value:?}"))
            .to_owned()
    };
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
                    Some(file) => Some(text(file, "an entry's environment")),
                };
                (
                    text(&entry["specification"], "an entry's specification"),
                    environment,
                )
            })
            .collect(),
        None => {
            let mut specifications: Vec<String> = std::fs::read_dir(case)
                .unwrap_or_else(|error| panic!("{name}: {error}"))
                .map(|entry| {
                    let entry = entry.unwrap_or_else(|error| panic!("{name}: {error}"));
                    entry.file_name().to_string_lossy().into_owned()
                })
                .filter(|file| file.starts_with("spec") && file.ends_with(".yaml"))
                .collect();
            assert_eq!(
                specifications.len(),
                1,
                "{name}: a case without entries needs exactly one specification, not \
                 {specifications:?}"
            );
            vec![(specifications.remove(0), project)]
        }
    };
    for project in entries.iter().filter_map(|(_, project)| project.as_deref()) {
        assert!(
            !supplies_functions(&case.join(project)),
            "{name}: {project} supplies project functions, which this route's empty \
             host-functions double cannot call (the command-line conformance excludes such \
             a benchmark too)"
        );
    }
    entries
}

/// Run each of `entries` in the staged copy `study`, in order, and return
/// the issue rows they report: a positive benchmark's entry checks clean
/// and saves when it builds without an issue, and a negative benchmark's
/// entry neither exposes nor saves an output.
#[track_caller]
fn run_entries(
    name: &str,
    study: &super::Scratch,
    entries: &[(String, Option<String>)],
    negative: bool,
) -> Vec<Value> {
    let host = yamaa_core::project_function::Language::Python;
    let mut issues = Vec::new();
    for (specification, environment) in entries {
        let path = study.join(specification).to_string_lossy().into_owned();
        let environment = environment
            .as_ref()
            .map(|file| study.join(file).to_string_lossy().into_owned());
        let request = || yamaa_core::domain_entry::Request {
            specification: &path,
            environment: environment.as_deref(),
        };
        let identity = || super::study::identity(name, &path);
        if !negative {
            let checked = yamaa_application::file_application::check(host, request(), identity());
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
                    let saved = domain
                        .save()
                        .unwrap_or_else(|_| panic!("{name}: {specification} failed to save"));
                    assert!(saved, "{name}: {specification} saved nothing");
                }
                reported
            }
            Err(refused) => refused,
        };
        issues.extend(reported.iter().map(super::study::issue_row));
    }
    issues
}

/// Compare the first of a negative benchmark's `issues` with its
/// `expected/error.yaml`: its phase, condition and spec paths, its rule and
/// context as the file writes them, and, for a validation error, that a
/// validation issue of its condition owns each spec path it states.
#[track_caller]
fn first_issue_is_stated(name: &str, issues: &[Value]) {
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
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let below = |actual: &str, path: &str| {
            actual
                .strip_prefix(path)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '[']))
        };
        let mut owned = std::collections::BTreeSet::<&str>::new();
        for issue in issues {
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
}

/// Each entry's declared report, then its declared output, beside the
/// entry in the staged copy `study`, as its composed layers declare them; a
/// later layer's `clear` removes an earlier layer's output field (LAYER-9).
fn declared_outputs(
    case: &Path,
    study: &super::Scratch,
    entries: &[(String, Option<String>)],
) -> Vec<PathBuf> {
    let mut declared = Vec::new();
    for (specification, _) in entries {
        let mut output = yamaa_core::json::Map::new();
        for layer in layers(&case.join(specification)) {
            let cleared = layer["clear"].as_array().into_iter().flatten();
            for field in cleared.filter_map(|field| field.as_str()?.strip_prefix("output.")) {
                output.remove(field);
            }
            if let Some(fields) = layer["output"].as_object() {
                output.extend(fields.clone());
            }
        }
        let entry = study.join(specification);
        let directory = entry.parent().unwrap_or(study.path());
        for field in ["report", "path"] {
            if let Some(file) = output.get(field).and_then(Value::as_str) {
                declared.push(directory.join(file));
            }
        }
    }
    declared
}

/// Compare what a positive benchmark's `entries` saved in `study` with its
/// `expected/` files: every expected file is a declared CSV output, each
/// declared output equals its expected file byte for byte, and the entries
/// wrote no file beside the `staged` copy that no entry declares.
#[track_caller]
fn saved_outputs_are_expected(
    name: &str,
    case: &Path,
    study: &super::Scratch,
    entries: &[(String, Option<String>)],
    staged: &std::collections::BTreeSet<PathBuf>,
) {
    let declared = declared_outputs(case, study, entries);
    let truths = case.join("expected");
    let mut expected: Vec<PathBuf> = std::fs::read_dir(&truths)
        .unwrap_or_else(|error| panic!("{name}: {}: {error}", truths.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("{name}: {}: {error}", truths.display()))
                .path()
        })
        .collect();
    expected.sort();
    for truth in &expected {
        let file = truth.file_name().unwrap_or_default().to_string_lossy();
        assert!(
            file.ends_with(".csv"),
            "{name}: expected/{file} is not a CSV, which is all this compares"
        );
        assert!(
            declared
                .iter()
                .any(|output| output.file_name() == truth.file_name()),
            "{name}: expected/{file} is no output an entry declares: {declared:?}"
        );
    }
    for output in &declared {
        let file = output.file_name().unwrap_or_default().to_string_lossy();
        let truth = truths.join(&*file);
        assert!(
            truth.is_file(),
            "{name}: the declared output {} has no expected file",
            output.display()
        );
        let saved = std::fs::read(output)
            .unwrap_or_else(|error| panic!("{name}: {} was not saved: {error}", output.display()));
        let truth = std::fs::read(&truth)
            .unwrap_or_else(|error| panic!("{name}: expected/{file}: {error}"));
        assert_eq!(
            String::from_utf8(saved),
            String::from_utf8(truth),
            "{name}: the saved {} against expected/{file}",
            output.display()
        );
    }
    let mut written = files(name, study);
    written.retain(|path| !staged.contains(path));
    for path in &written {
        assert!(
            declared.contains(path),
            "{name}: the entries saved {}, which no entry declares: {declared:?}",
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
