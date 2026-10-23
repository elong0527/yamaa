#![cfg(all(
    feature = "yaml",
    feature = "parquet",
    feature = "readstat",
    feature = "odm"
))]
//! Partitioned and streamed runs reproduce the benchmark corpus (#3697): the
//! whole-table results each benchmark states, independently authored, in
//! `expected/`.
//!
//! Every benchmark with one specification and no project runs on an engine
//! that divides its driver input into one-record partitions, the division
//! with the most partition boundaries. A plan whose steps are all
//! partition-safe runs divided; a plan with a step that needs the whole
//! table is refused, and runs again with the whole-table fallback. Either
//! way, a positive benchmark's saved files equal its expected files byte for
//! byte. A negative benchmark's first issue has the phase, condition, spec
//! paths and rule its `expected/error.yaml` states, and every issue, context
//! included, equals the whole-table run's. A Parquet file has no byte
//! guarantee (PARQUET-4), so it equals the whole-table run's file instead,
//! which the hosts' conformance checks compare with the expected file's
//! content.
//!
//! Each benchmark is also read as a stream on the same engine (#3697, step
//! 4), whose batches then hold one record each: a positive benchmark's
//! batches hold its run's output table, record for record, under the schema
//! the stream gave before its first pull, and a negative benchmark's stream
//! ends at its first pull with its run's issues. Either way the streamed
//! run's issues are its run's.
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use crate::support::scratch::Scratch;
use yamaa::{CancellationToken, Engine, EngineOptions, Error, HostLanguage, Partitions, Plans};

/// Benchmarks that must divide, read from their specifications: each
/// derives only from its input's own records (source reads, mappings, date
/// arithmetic and a range check), with no window, aggregate, lookup or
/// project function.
const DIVIDED: [&str; 3] = [
    "adam-adsl-analysis-age",
    "schema-adsl-age-quality",
    "sdtm-dm-basic",
];

fn benchmarks() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks")
}

/// The benchmarks one specification run reproduces: one `spec.yaml`, no
/// project file and no declared entries, in name order.
fn cases() -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = std::fs::read_dir(benchmarks())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|case| case.join("benchmark.yaml").is_file() && case.join("spec.yaml").is_file())
        .filter(|case| {
            let metadata = std::fs::read_to_string(case.join("benchmark.yaml")).unwrap();
            let specifications = std::fs::read_dir(case)
                .unwrap()
                .filter(|entry| {
                    let name = entry.as_ref().unwrap().file_name();
                    let name = name.to_string_lossy();
                    name.starts_with("spec") && name.ends_with(".yaml")
                })
                .count();
            specifications == 1
                && !case.join("project.yaml").exists()
                && !metadata.lines().any(|line| line.starts_with("entries:"))
        })
        .collect();
    cases.sort();
    cases
}

/// Copy the benchmark into `to` without what it states as truth, keeping
/// its symbolic links; false where this platform cannot make one.
fn stage(from: &Path, to: &Path) -> bool {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let target = to.join(&name);
        let kind = entry.file_type().unwrap();
        if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(entry.path()).unwrap(), &target).unwrap();
            #[cfg(not(unix))]
            return false;
        } else if kind.is_dir() {
            if name == "expected" {
                continue;
            }
            std::fs::create_dir_all(&target).unwrap();
            if !stage(&entry.path(), &target) {
                return false;
            }
        } else if name != "README.md" && name != "benchmark.yaml" {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
    true
}

fn engine(partitions: Option<Partitions>) -> Engine {
    let options = EngineOptions::new(HostLanguage::Python, "yamaa".into(), "0".into());
    Engine::new(match partitions {
        Some(partitions) => options.with_partitions(partitions),
        None => options,
    })
}

/// A run's first issue: its phase, condition, spec paths and rule.
type Issue = (String, String, Vec<String>, Option<String>);

/// One run of a staged specification: the first issue of a run with no
/// output, or else the bytes of each file its save wrote, by file name.
enum Outcome {
    Issue(Option<Issue>),
    Saved(Result<BTreeMap<String, Vec<u8>>, String>),
}

fn outcome(mut run: yamaa::Run) -> Outcome {
    if run.output().is_none() {
        return Outcome::Issue(run.diagnostics().first().map(|issue| {
            (
                issue.phase.clone(),
                issue.code.clone(),
                issue.spec_path.clone(),
                issue.rule.clone(),
            )
        }));
    }
    Outcome::Saved(
        run.save(&CancellationToken::new())
            .map(|saved| {
                saved
                    .into_iter()
                    .map(|path| {
                        let bytes = std::fs::read(&path).unwrap();
                        let name = Path::new(&path).file_name().unwrap();
                        (name.to_string_lossy().into_owned(), bytes)
                    })
                    .collect()
            })
            .map_err(|failure| failure.message),
    )
}

/// A `key: value` scalar of a flat `expected/error.yaml`.
fn scalar(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .map(|value| value.trim().trim_matches('"').to_owned())
}

/// The `spec_paths` of an `expected/error.yaml`, written inline or as a
/// block list.
fn spec_paths(text: &str) -> Vec<String> {
    let mut lines = text
        .lines()
        .skip_while(|line| !line.starts_with("spec_paths:"));
    let head = lines.next().unwrap_or("");
    let unquote = |path: &str| path.trim().trim_matches('"').to_owned();
    match head.strip_prefix("spec_paths: ") {
        Some(inline) => inline
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(unquote)
            .filter(|path| !path.is_empty())
            .collect(),
        None => lines
            .map_while(|line| line.strip_prefix("  - "))
            .map(unquote)
            .collect(),
    }
}

/// The whole-table run's saved file `name`, for a file with no byte
/// guarantee.
fn whole_table_file(case: &Path, name: &str) -> Option<Vec<u8>> {
    let whole = Scratch::new("partitioned-corpus-whole");
    stage(case, &whole);
    let spec = whole.join("spec.yaml");
    let run = engine(None)
        .spec_run(spec.to_str().unwrap(), None, &CancellationToken::new())
        .unwrap();
    match outcome(run) {
        Outcome::Saved(Ok(mut files)) => files.remove(name),
        _ => None,
    }
}

/// What differs between a case's partitioned run and what it states.
fn problems(case: &Path, outcome: Outcome) -> Vec<String> {
    let error = case.join("expected/error.yaml");
    let mut problems = Vec::new();
    match outcome {
        Outcome::Issue(first) if error.is_file() => {
            let text = std::fs::read_to_string(&error).unwrap();
            let expected = (
                scalar(&text, "phase").unwrap(),
                scalar(&text, "condition").unwrap(),
                spec_paths(&text),
                scalar(&text, "rule"),
            );
            if first.as_ref() != Some(&expected) {
                problems.push(format!("first issue {first:?}, expected {expected:?}"));
            }
        }
        Outcome::Saved(Ok(saved)) if !error.is_file() => {
            for file in std::fs::read_dir(case.join("expected")).unwrap() {
                let file = file.unwrap().path();
                let name = file.file_name().unwrap().to_string_lossy().into_owned();
                let expected = if name.ends_with(".parquet") {
                    whole_table_file(case, &name)
                } else {
                    Some(std::fs::read(&file).unwrap())
                };
                if expected.is_none() || saved.get(&name) != expected.as_ref() {
                    problems.push(format!("{name} differs from what the benchmark states"));
                }
            }
        }
        Outcome::Issue(first) => problems.push(format!("no output: {first:?}")),
        Outcome::Saved(Ok(_)) => problems.push("an output where an issue is expected".into()),
        Outcome::Saved(Err(message)) => problems.push(format!("save refused: {message}")),
    }
    problems
}

/// What differs between `run` and the same specification read as a stream
/// on an engine with one-record partitions.
fn stream_problems(run: &yamaa::Run, streamed: yamaa::Streamed) -> Vec<String> {
    let mut problems = Vec::new();
    match run.output() {
        Some(table) => {
            if streamed.schema() != Some(table.schema()) {
                problems.push("the stream's schema differs from the output's".into());
            }
            let rows = table
                .batches()
                .iter()
                .flat_map(|batch| (0..batch.num_rows()).map(|row| batch.slice(row, 1)))
                .collect::<Vec<_>>();
            match streamed.reader().collect::<Result<Vec<_>, _>>() {
                Ok(batches) if batches == rows => {}
                Ok(_) => problems.push("streamed records differ from the output's".into()),
                Err(error) => problems.push(format!("stream ended with {error}")),
            }
        }
        None => match streamed.reader().next() {
            Some(Err(Error::Rejected(report))) if report.diagnostics == run.diagnostics() => {}
            other => problems.push(format!("stream did not end with the issues: {other:?}")),
        },
    }
    match streamed.run() {
        Ok(streamed) if streamed.diagnostics() == run.diagnostics() => {}
        Ok(_) => problems.push("the streamed run's issues differ".into()),
        Err(error) => problems.push(format!("no streamed run: {error}")),
    }
    problems
}

#[test]
fn partitioned_and_streamed_runs_reproduce_every_single_specification_benchmark() {
    let token = CancellationToken::new();
    let one = Partitions::new(NonZeroUsize::MIN);
    let mut mismatched = BTreeMap::new();
    let mut divided = Vec::new();
    for case in cases() {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        let scratch = Scratch::new("partitioned-corpus");
        if !stage(&case, &scratch) {
            continue;
        }
        let spec = scratch.join("spec.yaml");
        let spec = spec.to_str().unwrap();
        let mut run = engine(Some(one)).spec_run(spec, None, &token).unwrap();
        let refused = run.diagnostics().first().is_some_and(|issue| {
            issue.code == "engine_rejected"
                && issue
                    .context
                    .to_json()
                    .contains(r#""code":"partition_global""#)
        });
        let partitions = if refused {
            one.with_whole_table_fallback()
        } else {
            one
        };
        if refused {
            run = engine(Some(partitions))
                .spec_run(spec, None, &token)
                .unwrap();
        } else {
            divided.push(name.clone());
        }
        // Read as a stream before the run's save writes its files.
        let streamed = engine(Some(partitions))
            .spec_stream(spec, None, &Plans::new(), &token)
            .unwrap();
        let streaming = stream_problems(&run, streamed);
        // A negative benchmark states its first issue's phase, condition,
        // paths and rule; every issue, context included, is the whole-table
        // run's, over the same staged files.
        let issues = if case.join("expected/error.yaml").is_file() {
            let whole = engine(None).spec_run(spec, None, &token).unwrap();
            (run.diagnostics() != whole.diagnostics())
                .then(|| "issues differ from the whole-table run's".to_owned())
        } else {
            None
        };
        let mut problems = problems(&case, outcome(run));
        problems.extend(issues);
        problems.extend(streaming);
        if !problems.is_empty() {
            mismatched.insert(name, problems);
        }
    }
    assert!(mismatched.is_empty(), "{mismatched:#?}");
    for name in DIVIDED {
        assert!(divided.iter().any(|case| case == name), "{name} divides");
    }
}
