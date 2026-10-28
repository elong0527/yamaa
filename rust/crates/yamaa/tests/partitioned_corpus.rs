#![cfg(all(
    feature = "yaml",
    feature = "parquet",
    feature = "readstat",
    feature = "odm"
))]
//! Partitioned and streamed runs reproduce the benchmark corpus (#3697,
//! #4092): the whole-table results each benchmark states, independently
//! authored, in `expected/`.
//!
//! Every benchmark runs as its corpus readers run it (`tools/benchmark.py`):
//! in a copy staged without its truth, each entry its `benchmark.yaml` lists
//! for Python builds in order, read with the project file the entry names,
//! and saves before the next one builds, so a later entry reads an earlier
//! entry's artifact; then the submission it declares composes over the
//! saved artifacts. A benchmark that lists no entries builds its one
//! specification, and an entry that names no environment is read with the
//! benchmark's `project.yaml` when it has one. Only the benchmarks whose
//! projects supply project functions are left out, each listed in
//! [`FUNCTIONS`] with the reason.
//!
//! Every build runs on an engine that divides its driver input into
//! one-record partitions, the division with the most partition boundaries.
//! A plan whose steps are all partition-safe runs divided; a plan with a step
//! that needs the whole table is refused, and runs again with the
//! whole-table fallback. Either way, a positive benchmark reports no issue
//! and saves exactly its expected files, each equal byte for byte. A negative
//! benchmark's first issue, its entries' or else its submission's, holds
//! every field its `expected/error.yaml` states, context included, and every
//! issue equals the whole-table run's, its staged path aside. A Parquet
//! file has no byte guarantee (PARQUET-4), so it equals the whole-table run's
//! file instead, which the hosts' conformance checks compare with the
//! expected file's content. Each engine builds in a staged copy of its own,
//! so a later entry and the submission read the artifacts that engine's
//! builds saved; a submission composes from them, so partitions reach a
//! Define or transport file only through the builds.
//!
//! Each build is also read as a stream on the same engine (#3697, step
//! 4), whose batches then hold one record each: a positive build's
//! batches hold its run's output table, record for record, under the schema
//! the stream gave before its first pull, and a refused build's stream
//! ends at its first pull with its run's issues. Either way the streamed
//! run's issues are its run's.
//!
//! Each benchmark runs again on an engine with key-bounded partitions over
//! its two leading declared keys (#3763), one key group per partition, and
//! the same holds of that run. Every benchmark the specifications show to
//! group its windows and aggregates by those keys is admitted: it runs with
//! no fallback, divided where its input is grouped by those keys and as one
//! partition where it is not.
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use crate::support::scratch::Scratch;
use yamaa::{
    CancellationToken, ContextValue, Diagnostic, Engine, EngineOptions, Failure, FailureKind,
    HostLanguage, Partitions, SpecRequest, SubmissionOptions,
};
use yamaa_core::schema::{Document, DocumentNode};

/// Benchmarks that must divide, read from their specifications: each
/// derives only from its input's own records (source reads, mappings, date
/// arithmetic and a range check), with no window, aggregate, lookup or
/// project function.
const DIVIDED: [&str; 3] = [
    "adam-adsl-analysis-age",
    "schema-adsl-age-quality",
    "sdtm-dm-basic",
];

/// ADaM benchmarks that key-bounded partitions over their two leading
/// declared keys must admit, read from their specifications: every window
/// `group_by`, unqualified aggregate `group_by`, grouped row template and
/// qualified aggregate key names both keys, which the driver input holds as
/// fields the specification copies unchanged, every row comes from the
/// driver input, and nothing else needs the whole table (no named
/// intermediate, implicit join, structured source or project function).
/// Each is keyed STUDYID, USUBJID and more, except adam-adlb-lymphocytes and
/// adam-advs-locf, keyed USUBJID first, and adam-clinsite-site-level, keyed
/// STUDYID and SITEID. adam-adlb-lymphocytes is admitted but runs as one
/// partition: its `lb.csv` returns to LYM-001's CYCLE 1 DAY 1 records after
/// other subjects, so the input is not grouped by USUBJID and VISIT.
const KEY_BOUNDED: [&str; 34] = [
    "adam-adae-occurrence-flags",
    "adam-adae-serious-sequence",
    "adam-adae-severity-rank",
    "adam-adae-worst-severity",
    "adam-adce-worst-toxicity",
    "adam-adeg-derived-intervals",
    "adam-adeg-time-matched-baseline",
    "adam-adex-cumulative-dose",
    "adam-adex-dose-reduction",
    "adam-adex-uncollected",
    "adam-adfa-fever",
    "adam-adlb-closest-visit",
    "adam-adlb-lymphocytes",
    "adam-adlb-mean",
    "adam-adlb-ordered-sum",
    "adam-adlb-shift-criteria",
    "adam-adpc-blq-imputation",
    "adam-adpp-carryover-exclusion",
    "adam-adqs-multi-scale-scoring",
    "adam-adqs-subscale-score",
    "adam-adrs-confirmed-response",
    "adam-adsl-alive-date",
    "adam-adsl-dose-adjustment",
    "adam-adsl-new-therapy",
    "adam-adsl-period-dates",
    "adam-adtr-nadir",
    "adam-adtr-sum",
    "adam-advs-bsa",
    "adam-advs-first-observed-carry",
    "adam-advs-locf",
    "adam-advs-map",
    "adam-advs-orthostatic-criteria",
    "adam-advs-prior-result",
    "adam-clinsite-site-level",
];

/// The benchmarks this test leaves out: exactly those whose project lists
/// function files, which the test requires. Their functions are Python or R
/// code run under the project's uv and renv locks. The engine here has no
/// functions port to call them (see `engine`), and the hosts, which do call
/// them, run every build over key-bounded partitions of up to 624,640
/// records (#3794), with no setting for smaller ones, so a benchmark's input
/// is one partition there. Running these partitioned needs a private
/// partition override in each host, which is host work outside #4092.
const FUNCTIONS: [&str; 12] = [
    "adam-adsl-bmi",
    "adam-advs-percentiles",
    "negative-donor-function-unkept",
    "negative-function-contract",
    "negative-function-host-error",
    "negative-function-name-twice",
    "negative-function-non-finite-result",
    "negative-function-test-mismatch",
    "schema-functions",
    "schema-keys-record-reads",
    "schema-non-finite",
    "schema-window-key-records",
];

fn benchmarks() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks")
}

/// The YAML file at `path` as the engine's YAML reader decodes it, or none
/// when it cannot be read.
fn yaml(path: &Path) -> Option<ContextValue> {
    fn value(document: &Document, node: usize) -> ContextValue {
        match &document.nodes()[node] {
            DocumentNode::Null => ContextValue::Null,
            DocumentNode::Boolean(value) => ContextValue::Bool(*value),
            DocumentNode::Integer(value) => value
                .parse()
                .map_or_else(|_| ContextValue::Wide(value.clone()), ContextValue::Int),
            DocumentNode::Float(value) => ContextValue::Float(*value),
            DocumentNode::Text(value) => ContextValue::Text(value.clone()),
            DocumentNode::Sequence(items) => {
                ContextValue::List(items.iter().map(|&item| value(document, item)).collect())
            }
            DocumentNode::Mapping(pairs) => ContextValue::Map(
                pairs
                    .iter()
                    .map(|&(key, item)| {
                        let key = match value(document, key) {
                            ContextValue::Text(key) => key,
                            key => key.to_json(),
                        };
                        (key, value(document, item))
                    })
                    .collect(),
            ),
        }
    }
    let bytes = std::fs::read(path).ok()?;
    let document = yamaa_adapters::yaml_decode::decode_yaml(&bytes)
        .ok()?
        .document;
    Some(value(&document, document.root()))
}

/// The field `key` of a mapping, or none.
fn field<'a>(value: &'a ContextValue, key: &str) -> Option<&'a ContextValue> {
    match value {
        ContextValue::Map(fields) => fields.get(key),
        _ => None,
    }
}

/// The text of a field, or none.
fn text(value: Option<&ContextValue>) -> Option<String> {
    match value {
        Some(ContextValue::Text(text)) => Some(text.clone()),
        _ => None,
    }
}

/// One build a benchmark lists: its specification, and the project file it
/// is read with, if any, each a path in the benchmark.
struct Entry {
    specification: String,
    project: Option<String>,
}

/// The submission a benchmark declares, its `define`: the project file, the
/// section it composes and the creation time it composes with.
struct Define {
    project: String,
    section: String,
    created: String,
}

/// A benchmark as its corpus readers run it.
struct Case {
    path: PathBuf,
    name: String,
    entries: Vec<Entry>,
    define: Option<Define>,
}

impl Case {
    /// The benchmark at `path`: the entries its `benchmark.yaml` lists for
    /// Python, or else its one `spec*.yaml`, and the submission it declares.
    fn read(path: PathBuf) -> Self {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let metadata = yaml(&path.join("benchmark.yaml"))
            .unwrap_or_else(|| panic!("{name}: benchmark.yaml does not read"));
        let project = path
            .join("project.yaml")
            .is_file()
            .then(|| "project.yaml".to_owned());
        let listed = field(&metadata, "entries").and_then(|entries| field(entries, "python"));
        let entries = match listed {
            Some(ContextValue::List(calls)) => calls
                .iter()
                .map(|call| Entry {
                    specification: text(field(call, "specification"))
                        .unwrap_or_else(|| panic!("{name}: an entry names no specification")),
                    project: match field(call, "environment") {
                        None => project.clone(),
                        Some(ContextValue::Null) => None,
                        environment => Some(
                            text(environment)
                                .unwrap_or_else(|| panic!("{name}: an environment is a path")),
                        ),
                    },
                })
                .collect(),
            _ => {
                let specifications: Vec<String> = std::fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .filter(|file| file.starts_with("spec") && file.ends_with(".yaml"))
                    .collect();
                assert_eq!(
                    specifications.len(),
                    1,
                    "{name}: a benchmark without entries has one specification"
                );
                specifications
                    .into_iter()
                    .map(|specification| Entry {
                        specification,
                        project: project.clone(),
                    })
                    .collect()
            }
        };
        let define = field(&metadata, "define").map(|define| Define {
            project: text(field(define, "environment")).unwrap(),
            section: text(field(define, "standard")).unwrap(),
            created: text(field(define, "created")).unwrap(),
        });
        Self {
            path,
            name,
            entries,
            define,
        }
    }

    /// Whether a project file the benchmark reads lists function files, read
    /// as the conformance planner reads it: a file that cannot be read lists
    /// none.
    fn supplies_functions(&self) -> bool {
        self.entries
            .iter()
            .filter_map(|entry| entry.project.as_deref())
            .chain(self.define.as_ref().map(|define| define.project.as_str()))
            .filter_map(|project| yaml(&self.path.join(project)))
            .any(|project| match field(&project, "functions") {
                None | Some(ContextValue::Null | ContextValue::Bool(false)) => false,
                Some(ContextValue::List(items)) => !items.is_empty(),
                Some(ContextValue::Map(items)) => !items.is_empty(),
                Some(_) => true,
            })
    }

    /// What the benchmark states of its first issue, its
    /// `expected/error.yaml`, or none for a positive benchmark.
    fn stated_issue(&self) -> Option<ContextValue> {
        let error = self.path.join("expected/error.yaml");
        error
            .is_file()
            .then(|| yaml(&error).unwrap_or_else(|| panic!("{}: expected/error.yaml", self.name)))
    }

    /// Whether the comparison needs the whole-table run: a negative
    /// benchmark's issues, or a Parquet file's content.
    fn needs_whole_table(&self) -> bool {
        std::fs::read_dir(self.path.join("expected"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .any(|file| file == "error.yaml" || file.to_string_lossy().ends_with(".parquet"))
    }
}

/// Every benchmark, in name order: each directory of `benchmarks/` that
/// holds a `benchmark.yaml`.
fn cases() -> Vec<Case> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(benchmarks())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|case| case.join("benchmark.yaml").is_file())
        .collect();
    paths.sort();
    paths.into_iter().map(Case::read).collect()
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

/// The engine of a Rust embedder that names the Python host: with no
/// functions port, so a build that calls a project function reports it
/// unsupported, and with `partitions`, or whole tables.
fn engine(partitions: Option<Partitions>) -> Engine {
    let options = EngineOptions::new(HostLanguage::Python, "yamaa".into(), "0".into());
    Engine::new(match partitions {
        Some(partitions) => options.with_partitions(partitions),
        None => options,
    })
}

/// An entry's specification and project file in a staged copy.
struct Staged {
    specification: String,
    project: Option<String>,
}

impl Staged {
    fn new(staged: &Path, entry: &Entry) -> Self {
        let path = |file: &str| staged.join(file).to_str().unwrap().to_owned();
        Self {
            specification: path(&entry.specification),
            project: entry.project.as_deref().map(path),
        }
    }
}

/// The request of `entry` on `engine`, polling `token`.
fn request<'a>(
    engine: &'a Engine,
    entry: &'a Staged,
    token: &'a CancellationToken,
) -> SpecRequest<'a> {
    let request = engine.spec(&entry.specification).cancel_with(token);
    match &entry.project {
        Some(project) => request.project(project),
        None => request,
    }
}

/// What differs between `run` and the same specification read as a stream
/// on an engine with one-record partitions: its batches hold the output's
/// records one each, or, for an output with no record, are the output's
/// one empty batch, as the whole table is.
fn stream_problems(run: &yamaa::Run, streamed: yamaa::Streamed) -> Vec<String> {
    let mut problems = Vec::new();
    match run.output() {
        Some(table) => {
            if streamed.schema() != Some(table.schema()) {
                problems.push("the stream's schema differs from the output's".into());
            }
            let rows = match table.rows() {
                0 => table.batches().to_vec(),
                _ => table
                    .batches()
                    .iter()
                    .flat_map(|batch| (0..batch.num_rows()).map(|row| batch.slice(row, 1)))
                    .collect::<Vec<_>>(),
            };
            match streamed.reader().collect::<Result<Vec<_>, _>>() {
                Ok(batches) if batches == rows => {}
                Ok(_) => problems.push("streamed records differ from the output's".into()),
                Err(error) => problems.push(format!("stream ended with {error}")),
            }
        }
        None => match streamed.reader().next() {
            Some(Err(Failure {
                kind: FailureKind::Rejected,
                diagnostics,
                ..
            })) if diagnostics == run.diagnostics() => {}
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

/// A run of `entry` on an engine with `partitions`, run again with the
/// whole-table fallback when a step that needs the whole table refuses it:
/// the run, the partitions it ran on, and whether the partitions admitted
/// it, so it ran with no fallback.
fn partitioned_run(
    entry: &Staged,
    partitions: Partitions,
    token: &CancellationToken,
) -> (yamaa::Run, Partitions, bool) {
    let run = request(&engine(Some(partitions)), entry, token)
        .run()
        .unwrap();
    let refused = run.diagnostics().first().is_some_and(|issue| {
        issue.code.as_str() == "engine_rejected"
            && issue
                .context
                .to_json()
                .contains(r#""code":"partition_global""#)
    });
    if !refused {
        return (run, partitions, true);
    }
    let partitions = partitions.with_whole_table_fallback();
    let run = request(&engine(Some(partitions)), entry, token)
        .run()
        .unwrap();
    (run, partitions, false)
}

/// One engine's builds of a case, entry by entry, then its submission, in a
/// staged copy of its own, so each build and the submission read only the
/// artifacts this chain saved: the issues of each, in that order, the bytes
/// of each file their saves wrote, by file name, whether the partitions
/// admitted every build, and what a save or a stream showed wrong.
struct Chain {
    staged: Scratch,
    partitions: Option<Partitions>,
    issues: Vec<Vec<Diagnostic>>,
    saved: BTreeMap<String, Vec<u8>>,
    admitted: bool,
    problems: Vec<String>,
}

impl Chain {
    /// A chain of `case` on `partitions`, or the whole table, in a fresh
    /// staged copy; none where this platform cannot stage the case.
    fn new(case: &Case, partitions: Option<Partitions>) -> Option<Self> {
        let staged = Scratch::new("partitioned-corpus");
        stage(&case.path, &staged).then(|| Self {
            staged,
            partitions,
            issues: Vec::new(),
            saved: BTreeMap::new(),
            admitted: true,
            problems: Vec::new(),
        })
    }

    /// Build `entry` in the chain's staged copy on the chain's engine: the
    /// run, the entry's staged paths and the partitions it ran on.
    fn build(
        &mut self,
        entry: &Entry,
        token: &CancellationToken,
    ) -> (yamaa::Run, Staged, Option<Partitions>) {
        let entry = Staged::new(&self.staged, entry);
        match self.partitions {
            Some(partitions) => {
                let (run, partitions, admitted) = partitioned_run(&entry, partitions, token);
                self.admitted &= admitted;
                (run, entry, Some(partitions))
            }
            None => {
                let run = request(&engine(None), &entry, token).run().unwrap();
                (run, entry, None)
            }
        }
    }

    /// The issues as values, the chain's staged path written `<staged>`
    /// wherever a location, message or context holds it, so chains staged
    /// apart compare.
    fn issue_values(&self) -> Vec<Vec<ContextValue>> {
        fn unstaged(value: ContextValue, staged: &str) -> ContextValue {
            match value {
                ContextValue::Text(text) => ContextValue::Text(text.replace(staged, "<staged>")),
                ContextValue::List(items) => ContextValue::List(
                    items
                        .into_iter()
                        .map(|item| unstaged(item, staged))
                        .collect(),
                ),
                ContextValue::Map(fields) => ContextValue::Map(
                    fields
                        .into_iter()
                        .map(|(key, item)| (key, unstaged(item, staged)))
                        .collect(),
                ),
                other => other,
            }
        }
        let staged = self.staged.to_str().unwrap();
        self.issues
            .iter()
            .map(|issues| {
                issues
                    .iter()
                    .map(|issue| unstaged(issue.to_value(), staged))
                    .collect()
            })
            .collect()
    }

    /// Keep `run`'s issues, and save its output when it built one, as the
    /// corpus readers do before the next entry builds.
    fn keep(&mut self, mut run: yamaa::Run, token: &CancellationToken) {
        self.issues.push(run.diagnostics());
        if run.output().is_some() {
            self.save(run.save(token));
        }
    }

    /// Keep the bytes of each file a save wrote, by file name.
    fn save(&mut self, saved: Result<Vec<String>, Failure>) {
        match saved {
            Ok(paths) => {
                for path in paths {
                    let bytes = std::fs::read(&path).unwrap();
                    let name = Path::new(&path).file_name().unwrap();
                    self.saved
                        .insert(name.to_string_lossy().into_owned(), bytes);
                }
            }
            Err(failure) => self
                .problems
                .push(format!("save refused: {}", failure.message)),
        }
    }

    /// Compose the submission `define` declares over the artifacts this
    /// chain saved, keep its issues, and save it when it reports none.
    fn submit(&mut self, define: &Define, token: &CancellationToken) {
        let project = self.staged.join(&define.project);
        let options = SubmissionOptions::new(yamaa::VERSION).with_created(define.created.as_str());
        let submission = engine(self.partitions)
            .project(project.to_str().unwrap())
            .section(&define.section)
            .cancel_with(token)
            .submission(&options);
        let issues = match submission {
            Ok(mut submission) => {
                let issues = submission.diagnostics();
                if issues.is_empty() {
                    self.save(submission.save(token));
                }
                issues
            }
            Err(failure) => failure.diagnostics,
        };
        self.issues.push(issues);
    }
}

/// A case's chains, each in its own staged copy: on one-record partitions,
/// whose problems hold what each build's stream showed, on key-bounded
/// partitions, and over the whole table when the comparison needs it.
struct Chains {
    one: Chain,
    keyed: Chain,
    whole: Option<Chain>,
}

/// Run `case` on each chain, or none where this platform cannot stage it.
/// In each chain, every entry's run, and its stream, comes before its save
/// writes its files, and the next entry and the submission read the
/// artifacts that chain saved.
fn reproduce(case: &Case, token: &CancellationToken) -> Option<Chains> {
    let keyed = Partitions::by_keys(NonZeroUsize::new(2).unwrap(), NonZeroUsize::MIN);
    let mut chains = Chains {
        one: Chain::new(case, Some(Partitions::new(NonZeroUsize::MIN)))?,
        keyed: Chain::new(case, Some(keyed))?,
        whole: match case.needs_whole_table() {
            true => Some(Chain::new(case, None)?),
            false => None,
        },
    };
    let Chains { one, keyed, whole } = &mut chains;
    for entry in &case.entries {
        let (run, staged, partitions) = one.build(entry, token);
        let streamed = request(&engine(partitions), &staged, token)
            .stream()
            .unwrap();
        one.problems.extend(stream_problems(&run, streamed));
        one.keep(run, token);
        for chain in [&mut *keyed].into_iter().chain(whole.as_mut()) {
            let (run, ..) = chain.build(entry, token);
            chain.keep(run, token);
        }
    }
    if let Some(define) = &case.define {
        for chain in [one, keyed].into_iter().chain(whole.as_mut()) {
            chain.submit(define, token);
        }
    }
    Some(chains)
}

/// Whether `reported` holds every field `stated` writes: a mapping states
/// the fields it writes, recursively, and any other value is compared
/// whole.
fn states(stated: &ContextValue, reported: &ContextValue) -> bool {
    match (stated, reported) {
        (ContextValue::Map(stated), ContextValue::Map(reported)) => {
            stated.iter().all(|(key, stated)| {
                reported
                    .get(key)
                    .is_some_and(|reported| states(stated, reported))
            })
        }
        (ContextValue::Map(_), _) => false,
        _ => stated == reported,
    }
}

/// What differs between `chain` and what `case` states: a negative
/// benchmark's first issue, and every issue, context included, against the
/// whole-table run's over its own copy of the same files; a positive
/// benchmark's issues, none; and the files the chain saved, exactly the
/// expected files.
/// A negative benchmark without a submission states its issue alone: its
/// builds save nothing, and, as for its corpus readers, no other expected
/// file is compared.
fn problems(case: &Case, chain: &Chain, whole: Option<&Chain>) -> Vec<String> {
    let mut problems = chain.problems.clone();
    let first = chain.issues.iter().flatten().next();
    let stated = case.stated_issue();
    let compared = stated.is_none() || case.define.is_some();
    match stated {
        Some(stated) => {
            if !first.is_some_and(|first| states(&stated, &first.to_value())) {
                let first = first.map(Diagnostic::to_value);
                problems.push(format!("first issue {first:?}, stated {stated:?}"));
            }
            if whole.is_some_and(|whole| whole.issue_values() != chain.issue_values()) {
                problems.push("issues differ from the whole-table run's".into());
            }
        }
        None => problems
            .extend(first.map(|first| format!("an issue where none is stated: {}", first.text()))),
    }
    let mut unstated: BTreeSet<&String> = chain.saved.keys().collect();
    for file in std::fs::read_dir(case.path.join("expected")).unwrap() {
        let file = file.unwrap().path();
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        if name == "error.yaml" || !compared {
            continue;
        }
        unstated.remove(&name);
        let expected = if name.ends_with(".parquet") {
            whole.and_then(|whole| whole.saved.get(&name).cloned())
        } else {
            Some(std::fs::read(&file).unwrap())
        };
        if expected.is_none() || chain.saved.get(&name) != expected.as_ref() {
            problems.push(format!("{name} differs from what the benchmark states"));
        }
    }
    problems.extend(
        unstated
            .into_iter()
            .map(|name| format!("{name} was saved, which the benchmark does not state")),
    );
    problems
}

#[test]
fn partitioned_and_streamed_runs_reproduce_every_benchmark_without_project_functions() {
    let token = CancellationToken::new();
    let mut mismatched = BTreeMap::new();
    let mut exempt = Vec::new();
    let mut divided = Vec::new();
    let mut key_admitted = Vec::new();
    for case in cases() {
        if case.supplies_functions() {
            exempt.push(case.name);
            continue;
        }
        let Some(Chains { one, keyed, whole }) = reproduce(&case, &token) else {
            continue;
        };
        // One-record partitions divide every run they admit over more than
        // one record.
        if one.admitted {
            divided.push(case.name.clone());
        }
        if keyed.admitted {
            key_admitted.push(case.name.clone());
        }
        let mut found = problems(&case, &one, whole.as_ref());
        found.extend(
            problems(&case, &keyed, whole.as_ref())
                .into_iter()
                .map(|problem| format!("key-bounded partitions: {problem}")),
        );
        if !found.is_empty() {
            mismatched.insert(case.name, found);
        }
    }
    assert_eq!(
        exempt, FUNCTIONS,
        "the benchmarks whose projects supply project functions"
    );
    assert!(mismatched.is_empty(), "{mismatched:#?}");
    for name in DIVIDED {
        assert!(divided.iter().any(|case| case == name), "{name} divides");
    }
    let not_admitted: Vec<&str> = KEY_BOUNDED
        .into_iter()
        .filter(|name| !key_admitted.iter().any(|case| case == name))
        .collect();
    assert!(
        not_admitted.is_empty(),
        "not admitted under key-bounded partitions: {not_admitted:?}"
    );
}
