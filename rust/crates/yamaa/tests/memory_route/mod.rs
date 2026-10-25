//! The memory route: every benchmark runs through the facade's use cases, as
//! every host runs it, with each file of the case served as held bytes
//! through the facade's memory resolver. The outcome is compared with the
//! authored expected artifacts (positive) or the authored first issue
//! (negative) by the file route's comparison rules (the public conformance
//! runner in `tools/conformance`). No benchmark is exempt: the shipped
//! runtime is the tested runtime (#2845).
//!
//! The resolver serves what the conformance runners stage: every file of the
//! case but its expected truth, its README and its benchmark.yaml. Inputs are
//! bytes, so the engine decodes them as it decodes files, and ingest findings
//! arise from their text. A directory or symbolic link is served as a kind
//! fact: the resolver reports it instead of serving bytes, as the file
//! resolver does, and the engine raises the file rules' conditions from that
//! report.
//!
//! The engine claims R, as the R host does. Its project functions are
//! [`functions::Benchmarks`], a Rust port of the benchmarks' R package, so a
//! project reads its R environment file and that file's renv.lock, and the
//! engine runs every declared function contract test through the port before
//! it reads study data.
//!
//! In a build without the `parquet` feature, cases involving Parquet are
//! skipped (the per-feature lanes exercise everything else).
#![cfg(feature = "yaml")]

mod compare;
mod functions;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use yamaa::host::resolver::ResourceKind;
use yamaa::{
    CancellationToken, Engine, EngineOptions, Failure, HostLanguage, MemoryResolver,
    ProjectFunctions, ResourceResolver, Run,
};
use yamaa_core::json::Value as Json;

use compare::{
    brief_record, declarations_for, diagnostic_matches, issue_records, normalize, parquet_schema,
    report_types, typed_from_csv, typed_from_parquet, typed_from_table, yaml_json,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks")
}

/// One planned benchmark entry, following the conformance plan for the R
/// host: explicit entries win; an entry that does not name a project passes
/// the case's root `project.yaml` when that file exists, and an explicit null
/// passes none; a case without entries has exactly one specification.
#[derive(Debug, Clone)]
struct EntryCall {
    specification: String,
    project: Option<String>,
}

fn plan_entries(case: &Path) -> Vec<EntryCall> {
    let default_project = case
        .join("project.yaml")
        .is_file()
        .then(|| "project.yaml".to_string());
    if let Ok(bytes) = fs::read(case.join("benchmark.yaml")) {
        let doc = yaml_json(&bytes);
        if let Some(entries) = doc
            .get("entries")
            .and_then(|entries| entries.get("r"))
            .and_then(Json::as_array)
        {
            return entries
                .iter()
                .map(|entry| EntryCall {
                    specification: entry
                        .get("specification")
                        .and_then(Json::as_str)
                        .expect("entry names a specification")
                        .to_string(),
                    project: match entry.get("environment") {
                        Some(Json::String(path)) => Some(path.clone()),
                        Some(_) => None,
                        None => default_project.clone(),
                    },
                })
                .collect();
        }
    }
    let mut specs: Vec<String> = fs::read_dir(case)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            (name.starts_with("spec") && name.ends_with(".yaml")).then_some(name)
        })
        .collect();
    specs.sort();
    assert_eq!(
        specs.len(),
        1,
        "case without entries needs exactly one specification: {}",
        case.display()
    );
    vec![EntryCall {
        specification: specs.pop().unwrap(),
        project: default_project,
    }]
}

/// Every entry the conformance runners stage from a case, as a case-relative
/// POSIX path with its kind, judged without following links. Like their
/// `ignore_patterns("expected", "README.md", "benchmark.yaml")`, nothing of
/// those names is staged at any depth.
fn case_entries(case: &Path) -> Vec<(String, PathBuf, ResourceKind)> {
    let mut out = Vec::new();
    let mut stack = vec![(String::new(), case.to_path_buf())];
    while let Some((prefix, dir)) = stack.pop() {
        for item in fs::read_dir(&dir).unwrap().flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            if ["expected", "README.md", "benchmark.yaml"].contains(&name.as_str()) {
                continue;
            }
            let written = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let path = item.path();
            let kind = item.file_type().unwrap();
            if kind.is_symlink() {
                out.push((written, path, ResourceKind::SymbolicLink));
            } else if kind.is_dir() {
                out.push((written.clone(), path.clone(), ResourceKind::Directory));
                stack.push((written, path));
            } else if kind.is_file() {
                out.push((written, path, ResourceKind::File));
            } else {
                out.push((written, path, ResourceKind::Other));
            }
        }
    }
    out.sort_by(|left, right| left.0.cmp(&right.0));
    out
}

/// Declaring identities a capture may use: the case root and every YAML file
/// of the case (entries, parents, producers, projects and environment
/// files), each named by its canonical leading-slash form.
fn declaring_identities(entries: &[(String, PathBuf, ResourceKind)]) -> Vec<String> {
    let mut identities = vec!["/".to_string()];
    for (written, _, kind) in entries {
        if *kind == ResourceKind::File && written.ends_with(".yaml") {
            identities.push(format!("/{written}"));
        }
    }
    identities
}

/// The spellings under which a declaring file may name a target: the
/// case-relative path, and the path relative to the declaring file's own
/// directory (a schema specification inside `input/` names its sibling inputs
/// by bare file name).
fn spellings(identity: &str, target: &str) -> Vec<String> {
    let dir = identity.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let from: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    let to: Vec<&str> = target.split('/').filter(|part| !part.is_empty()).collect();
    let mut common = 0;
    while common < from.len() && common < to.len() && from[common] == to[common] {
        common += 1;
    }
    let mut parts: Vec<&str> = vec![".."; from.len() - common];
    parts.extend(&to[common..]);
    let relative = parts.join("/");
    if relative.is_empty() || relative == target {
        vec![target.to_string()]
    } else {
        vec![target.to_string(), relative]
    }
}

/// One served case: its resolver and the declaring identities its files
/// are served under.
struct Served {
    resolver: Arc<MemoryResolver>,
    identities: Vec<String>,
}

impl Served {
    /// Serve every staged file of a case as held bytes under a per-file
    /// reference, and every directory and link as a kind fact. The resolver
    /// holds one reply per declaring and written pair.
    fn case(case: &Path) -> Self {
        let entries = case_entries(case);
        let served = Self {
            resolver: Arc::new(MemoryResolver::new()),
            identities: declaring_identities(&entries),
        };
        for (written, path, kind) in &entries {
            if *kind == ResourceKind::File {
                served.offer(written, Arc::from(fs::read(path).unwrap()));
            } else {
                for declaring in &served.identities {
                    for spelling in spellings(declaring, written) {
                        served.resolver.insert_kind(declaring, &spelling, *kind);
                    }
                }
            }
        }
        served
    }

    /// Serve `bytes` as the file at the case-relative `written` path. A
    /// repeated reference always carries the same bytes, or a capture fails
    /// as changed.
    fn offer(&self, written: &str, bytes: Arc<[u8]>) {
        let reference = format!("object:{written}");
        for declaring in &self.identities {
            for spelling in spellings(declaring, written) {
                self.resolver
                    .insert(declaring, &spelling, &reference, Arc::clone(&bytes));
            }
        }
    }

    /// Serve what a save published to later entries under the same
    /// declaring identities and spellings as the authored files, as the
    /// file route's runners read an entry's saved artifacts.
    fn offer_saved(&self, paths: &[String]) {
        for path in paths {
            let target = path
                .strip_prefix("memory:/")
                .expect("a memory target names its path");
            let bytes = self
                .resolver
                .published(path)
                .expect("a saved path was published");
            self.offer(target, bytes);
        }
    }

    /// An engine over this case's resolver as the R host configures it, with
    /// `functions`, or with none, as the command line runs.
    fn engine_with(&self, functions: Option<Arc<dyn ProjectFunctions>>) -> Engine {
        let options = EngineOptions::new(HostLanguage::R, "rust".into(), "memory".into())
            .with_resolver(Arc::clone(&self.resolver) as Arc<dyn ResourceResolver>);
        Engine::new(match functions {
            Some(functions) => options.with_functions(functions),
            None => options,
        })
    }
}

/// The benchmarks' project functions, as the R host supplies them.
fn benchmarks() -> Option<Arc<dyn ProjectFunctions>> {
    Some(Arc::new(functions::Benchmarks))
}

fn expected_error(case: &Path) -> Option<Json> {
    let bytes = fs::read(case.join("expected/error.yaml")).ok()?;
    Some(yaml_json(&bytes))
}

/// Whether the case's benchmark.yaml declares a Define call.
fn declares_define(case: &Path) -> bool {
    fs::read(case.join("benchmark.yaml"))
        .ok()
        .and_then(|bytes| yaml_json(&bytes).get("define").cloned())
        .is_some_and(|define| !define.is_null())
}

/// The authored reason a case is excluded from the file route's own
/// conformance plan (benchmark.yaml `conformance.excluded`), if any.
fn conformance_exclusion(case: &Path) -> Option<String> {
    let bytes = fs::read(case.join("benchmark.yaml")).ok()?;
    let doc = yaml_json(&bytes);
    doc.get("conformance")?
        .get("excluded")?
        .as_str()
        .map(str::to_owned)
}

fn matches_path(actual: &str, expected: &str) -> bool {
    actual == expected
        || actual.starts_with(&format!("{expected}."))
        || actual.starts_with(&format!("{expected}["))
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// One entry's run and, for a positive case, its save; or the panic that
/// stopped it.
enum EntryOutcome {
    Ran {
        run: Box<Run>,
        saved: Option<Result<Vec<String>, Failure>>,
    },
    Panicked(String),
}

struct EntryRun {
    call: EntryCall,
    outcome: EntryOutcome,
}

/// Run one entry through the facade, as a host's specification object runs:
/// with its project when it has one. Only cancellation fails the call.
fn run_entry(engine: &Engine, call: &EntryCall) -> Result<Run, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let request = engine.spec(&call.specification);
        match call.project.as_deref() {
            Some(project) => request.project(project),
            None => request,
        }
        .run()
        .expect("only cancellation fails a run")
    }))
    .map_err(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap_or_else(|| "unknown panic".to_string())
    })
}

fn check_negative(name: &str, want: &Json, runs: Vec<EntryRun>, findings: &mut Vec<String>) {
    let mut issues: Vec<Json> = Vec::new();
    // Every negative's first issue is located in a file the case serves
    // (P-ISSUE-1), named by the reference the resolver gave it.
    let mut first_location = None;
    for EntryRun { call, outcome } in runs {
        match outcome {
            EntryOutcome::Ran { mut run, .. } => {
                if first_location.is_none() {
                    first_location = run
                        .diagnostics()
                        .first()
                        .map(|issue| issue.location.clone());
                }
                issues.extend(issue_records(&run.issues()));
                if run.output().is_some() {
                    findings.push(format!(
                        "{name}: negative build exposed output ({})",
                        call.specification
                    ));
                }
                if run.save(&CancellationToken::new()).is_ok() {
                    findings.push(format!(
                        "{name}: negative build saved its output ({})",
                        call.specification
                    ));
                }
            }
            EntryOutcome::Panicked(message) => {
                findings.push(format!(
                    "{name}: entry {} panicked: {message}",
                    call.specification
                ));
            }
        }
    }
    if issues.is_empty() {
        findings.push(format!("{name}: negative benchmark produced no issues"));
        return;
    }
    match first_location.flatten() {
        Some(location) if location.position.is_some() => {}
        located => findings.push(format!(
            "{name}: first issue is not located at a line and column: {located:?}"
        )),
    }
    if !diagnostic_matches(want, &issues[0]) {
        findings.push(format!(
            "{name}: first issue differs: expected [{}], got [{}]",
            brief_record(want),
            brief_record(&issues[0])
        ));
    }
    if want.get("phase").and_then(Json::as_str) == Some("validation") {
        let want_paths: Vec<String> = want
            .get("spec_paths")
            .and_then(Json::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let want_condition = want.get("condition").and_then(Json::as_str).unwrap_or("");
        let mut matched: Vec<&str> = Vec::new();
        for issue in &issues {
            let paths: Vec<&str> = issue
                .get("spec_paths")
                .and_then(Json::as_array)
                .map(|paths| paths.iter().filter_map(Json::as_str).collect())
                .unwrap_or_default();
            let owned = issue.get("phase").and_then(Json::as_str) == Some("validation")
                && issue.get("condition").and_then(Json::as_str) == Some(want_condition)
                && !paths.is_empty()
                && paths
                    .iter()
                    .all(|actual| want_paths.iter().any(|x| matches_path(actual, x)));
            if !owned {
                continue;
            }
            for expected in &want_paths {
                if paths.iter().any(|actual| matches_path(actual, expected)) {
                    matched.push(expected);
                }
            }
        }
        for expected in &want_paths {
            if !matched.contains(&expected.as_str()) {
                findings.push(format!(
                    "{name}: no validation issue covers expected path {expected}"
                ));
            }
        }
    }
}

/// One positive entry under comparison: its case and specification, and the
/// resolver its save published to.
struct Entry<'a> {
    name: &'a str,
    case: &'a Path,
    spec: &'a str,
    resolver: &'a MemoryResolver,
}

/// Compare one declared slot of a positive entry, the output or its run
/// report, with its authored artifact: the saved bytes (CSV) or the saved
/// table (Parquet), and the host table.
fn check_slot(
    entry: &Entry<'_>,
    slot: &str,
    declared: &str,
    host: Option<&yamaa::Table>,
    types: &BTreeMap<String, String>,
    decimals: Option<u32>,
    findings: &mut Vec<String>,
) {
    let Entry {
        name,
        case,
        spec,
        resolver,
    } = *entry;
    let artifact = basename(declared);
    let truth_path = case.join("expected").join(&artifact);
    if !truth_path.is_file() {
        findings.push(format!(
            "{name}: entry {spec} declares {slot} {declared} without an expected artifact"
        ));
        return;
    }
    let truth = fs::read(&truth_path).unwrap();
    // An entry below the case root saves beside its own file.
    let target = format!("memory:/{}", normalize(spec, declared));
    let Some(published) = resolver.published(&target) else {
        findings.push(format!(
            "{name}: entry {spec} save did not publish {declared}"
        ));
        return;
    };
    let slot_types = if slot == "output" {
        types.clone()
    } else {
        match report_types(case, &artifact) {
            Ok(map) => map,
            Err(error) => {
                findings.push(format!("{name}: report types for {artifact}: {error}"));
                return;
            }
        }
    };
    let expected = if artifact.ends_with(".parquet") {
        if !cfg!(feature = "parquet") {
            findings.push(format!(
                "{name}: entry {spec} cannot compare a Parquet slot without the parquet feature"
            ));
            return;
        }
        match (parquet_schema(&truth), parquet_schema(&published)) {
            (Ok(expected), Ok(actual)) if actual != expected => findings.push(format!(
                "{name}: entry {spec} saved Parquet schema differs: {declared}: expected {expected:?}, actual {actual:?}"
            )),
            (Err(error), _) => findings.push(format!("{name}: Parquet schema of {artifact}: {error}")),
            (_, Err(error)) => {
                findings.push(format!("{name}: Parquet schema of saved {declared}: {error}"))
            }
            _ => {}
        }
        let expected = match typed_from_parquet(&truth, &slot_types) {
            Ok(expected) => expected,
            Err(error) => {
                findings.push(format!("{name}: expected Parquet {artifact}: {error}"));
                return;
            }
        };
        match typed_from_parquet(&published, &slot_types) {
            Ok(actual) if actual != expected => findings.push(format!(
                "{name}: entry {spec} typed {slot} Parquet table differs: {}",
                actual.describe_diff(&expected)
            )),
            Err(error) => findings.push(format!("{name}: saved Parquet {declared}: {error}")),
            _ => {}
        }
        expected
    } else {
        if published.as_ref() != truth.as_slice() {
            findings.push(format!(
                "{name}: entry {spec} saved CSV bytes differ: {declared}"
            ));
        }
        match typed_from_csv(&truth, &slot_types) {
            Ok(expected) => expected,
            Err(error) => {
                findings.push(format!("{name}: expected CSV {artifact}: {error}"));
                return;
            }
        }
    };
    // The host table agrees with the expected table too; a Parquet artifact
    // holds unprojected values.
    let decimals = decimals.filter(|_| slot == "output" && artifact.ends_with(".csv"));
    match host.map(|table| typed_from_table(table, decimals)) {
        None => findings.push(format!("{name}: entry {spec} has no host table for {slot}")),
        Some(Ok(actual)) if actual != expected => findings.push(format!(
            "{name}: entry {spec} host {slot} table differs: {}",
            actual.describe_diff(&expected)
        )),
        Some(Err(error)) => {
            findings.push(format!("{name}: entry {spec} host {slot} table: {error}"))
        }
        Some(Ok(_)) => {}
    }
}

fn check_positive(
    name: &str,
    case: &Path,
    resolver: &MemoryResolver,
    runs: Vec<EntryRun>,
    findings: &mut Vec<String>,
) {
    let mut compared: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for EntryRun { call, outcome } in runs {
        let spec = call.specification;
        let (run, saved) = match outcome {
            EntryOutcome::Panicked(message) => {
                findings.push(format!("{name}: entry {spec} panicked: {message}"));
                continue;
            }
            EntryOutcome::Ran { run, saved } => (run, saved),
        };
        let (types, output) = match declarations_for(case, &spec) {
            Ok(pair) => pair,
            Err(error) => {
                findings.push(format!("{name}: entry {spec}: declarations: {error}"));
                continue;
            }
        };
        let issues = issue_records(&run.issues());
        if !issues.is_empty() {
            findings.push(format!(
                "{name}: entry {spec} reported {} issue(s), first: {}",
                issues.len(),
                brief_record(&issues[0])
            ));
            for declared in [&output.path, &output.report].into_iter().flatten() {
                compared.insert(basename(declared));
            }
            continue;
        }
        // The save returns the declared report and then the output (OUTPUT-10),
        // each named by the reference the resolver's target reply gave.
        let planned = [&output.report, &output.path]
            .into_iter()
            .flatten()
            .map(|declared| format!("memory:/{}", normalize(&spec, declared)))
            .collect::<Vec<_>>();
        match saved {
            Some(Ok(paths)) if paths != planned => findings.push(format!(
                "{name}: entry {spec} saved {paths:?}, expected {planned:?}"
            )),
            Some(Err(failure)) => findings.push(format!(
                "{name}: entry {spec} did not save: {failure} (wrote {:?})",
                failure.paths
            )),
            None => findings.push(format!("{name}: entry {spec} was not saved")),
            Some(Ok(_)) => {}
        }
        if output.path.is_none() && run.output().is_some() {
            findings.push(format!(
                "{name}: entry {spec} produced an undeclared output table"
            ));
        }
        for (slot, declared, host) in [
            ("output", &output.path, run.output()),
            ("report", &output.report, run.report()),
        ] {
            // Every build holds its run report; only a declared file has an
            // independent expected artifact to compare.
            let Some(declared) = declared else {
                continue;
            };
            compared.insert(basename(declared));
            let entry = Entry {
                name,
                case,
                spec: &spec,
                resolver,
            };
            check_slot(
                &entry,
                slot,
                declared,
                host,
                &types,
                output.decimals,
                findings,
            );
        }
    }
    if let Ok(entries) = fs::read_dir(case.join("expected")) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if (file_name.ends_with(".csv") || file_name.ends_with(".parquet"))
                && !compared.contains(&file_name)
            {
                findings.push(format!(
                    "{name}: expected artifact {file_name} was not compared"
                ));
            }
        }
    }
}

/// The outcome of checking one case: agreement, disagreement with findings,
/// or a skip when this build lacks a feature the case needs (the per-feature
/// lanes exercise everything else).
enum CaseOutcome {
    Pass,
    Findings(Vec<String>),
    Skipped,
}

/// Whether the case involves Parquet anywhere (inputs or expected
/// artifacts); such cases need the `parquet` feature to run at all.
fn case_needs_parquet(case: &Path) -> bool {
    fn walk(dir: &Path) -> bool {
        let Ok(entries) = fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                walk(&path)
            } else {
                path.extension().is_some_and(|ext| ext == "parquet")
            }
        })
    }
    walk(case)
}

/// Run one benchmark case entirely through the memory resolver and compare
/// the outcome with the authored expectation, the file route's way.
fn check_case(name: &str, case: &Path) -> CaseOutcome {
    check_case_with(name, case, benchmarks())
}

/// [`check_case`] on an engine with `functions`, or with none.
fn check_case_with(
    name: &str,
    case: &Path,
    functions: Option<Arc<dyn ProjectFunctions>>,
) -> CaseOutcome {
    if !cfg!(feature = "parquet") && case_needs_parquet(case) {
        return CaseOutcome::Skipped;
    }
    let mut findings = Vec::new();
    if let Some(reason) = conformance_exclusion(case) {
        findings.push(format!(
            "{name}: excluded from the file route's conformance plan: {reason}"
        ));
        return CaseOutcome::Findings(findings);
    }
    let served = Served::case(case);
    let engine = served.engine_with(functions);
    let entries = plan_entries(case);
    // A Define benchmark's expected failure belongs to its Define call, as in
    // compare_example; its domain builds are compared as a positive case.
    let expected = expected_error(case).filter(|_| !declares_define(case));
    let mut runs = Vec::new();
    for call in entries {
        let outcome = match run_entry(&engine, &call) {
            Ok(mut run) => {
                // As the hosts' runners do, a positive entry without issues
                // saves before the next entry reads its artifacts.
                let saved = (expected.is_none() && run.issues().rows() == 0)
                    .then(|| run.save(&CancellationToken::new()));
                if let Some(Ok(paths)) = &saved {
                    served.offer_saved(paths);
                }
                EntryOutcome::Ran {
                    run: Box::new(run),
                    saved,
                }
            }
            Err(message) => EntryOutcome::Panicked(message),
        };
        runs.push(EntryRun { call, outcome });
    }
    match expected {
        Some(want) => check_negative(name, &want, runs, &mut findings),
        None => check_positive(name, case, &served.resolver, runs, &mut findings),
    }
    if findings.is_empty() {
        CaseOutcome::Pass
    } else {
        CaseOutcome::Findings(findings)
    }
}

fn benchmark_cases() -> Vec<(String, PathBuf)> {
    let mut cases: Vec<(String, PathBuf)> = fs::read_dir(root())
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            (path.is_dir() && path.join("benchmark.yaml").is_file())
                .then(|| (entry.file_name().to_string_lossy().into_owned(), path))
        })
        .collect();
    cases.sort();
    cases
}

fn check_all<'a>(cases: impl IntoIterator<Item = (&'a str, &'a Path)>) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, case) in cases {
        if let CaseOutcome::Findings(findings) = check_case(name, case) {
            failures.extend(findings);
        }
    }
    failures
}

/// A representative subset across every mechanism class (inheritance,
/// schema-linked stored artifacts and ordered entries, metadata documents,
/// ODM, Parquet in and out, reported precision, log artifacts, project
/// functions, kind facts, ingest findings from input text, negatives of
/// several phases), for a fast signal while iterating.
const REPRESENTATIVE: &[&str] = &[
    "adam-adsl-age-group",
    "adam-adlb-reported-precision",
    "adam-adsl-randomization",
    "adam-adsl-bmi",
    "adam-results-ae-incidence",
    "adam-adae-exposure-adjusted-incidence",
    "sdtm-dm-race-ethnicity",
    "sdtm-dm-metadata",
    "sdtm-ae-coding",
    "schema-functions",
    "schema-inheritance",
    "schema-odm-item-sample",
    "schema-parquet",
    "schema-verification-log",
    "negative-function-contract",
    "negative-redefined-date",
    "negative-source-unterminated-quote",
    "negative-zero-division",
    "negative-odm-schema-missing",
];

#[test]
fn memory_route_representative() {
    let cases = benchmark_cases();
    let selected = REPRESENTATIVE.iter().map(|name| {
        let (_, case) = cases
            .iter()
            .find(|(case_name, _)| case_name == name)
            .unwrap_or_else(|| panic!("representative case {name} not found"));
        (*name, case.as_path())
    });
    let failures = check_all(selected);
    assert!(
        failures.is_empty(),
        "the memory route disagrees with the file route:\n{}",
        failures.join("\n")
    );
}

/// The plan passes a case's root project to every call, so the study-level
/// cases build on the project route with their codelists and functions; a
/// case whose project file has another name declares the call that passes
/// it.
#[test]
fn memory_route_plan_passes_the_root_project() {
    let cases = benchmark_cases();
    for (name, specification, project) in [
        ("sdtm-ae-coding", "spec.yaml", "project.yaml"),
        ("sdtm-dm-metadata", "spec.yaml", "project.yaml"),
        (
            "sdtm-vs-epoch-from-subject-elements",
            "spec.yaml",
            "project.yaml",
        ),
        ("schema-functions", "spec.yaml", "project.yaml"),
        (
            "sdtm-lb-metadata",
            "submission-spec.yaml",
            "submission-project.yaml",
        ),
    ] {
        let (_, case) = cases
            .iter()
            .find(|(case_name, _)| case_name == name)
            .unwrap_or_else(|| panic!("planned case {name} not found"));
        let calls = plan_entries(case);
        assert_eq!(calls.len(), 1, "{name}");
        assert_eq!(calls[0].specification, specification, "{name}");
        assert_eq!(calls[0].project.as_deref(), Some(project), "{name}");
    }
}

/// The first issue of a function benchmark's entry, run through `functions`
/// over the case as served, after `edit` changes what it serves; the run
/// builds no output.
fn first_issue(
    name: &str,
    functions: Arc<dyn ProjectFunctions>,
    edit: impl FnOnce(&Served),
) -> Json {
    let served = Served::case(&root().join(name));
    edit(&served);
    let run = run_entry(
        &served.engine_with(Some(functions)),
        &EntryCall {
            specification: "spec.yaml".into(),
            project: Some("project.yaml".into()),
        },
    )
    .unwrap();
    assert!(run.output().is_none(), "{name}");
    issue_records(&run.issues()).remove(0)
}

/// The engine runs every contract test an environment file declares through
/// the port before it reads data: a port whose `bmi` strays from the
/// package fails the build at the first test of `bmi`.
#[test]
fn the_contract_tests_check_the_port() {
    let issue = first_issue("adam-adsl-bmi", Arc::new(functions::Strayed("bmi")), |_| {});
    assert_eq!(
        issue["condition"], "function_conformance_failed",
        "{issue:?}"
    );
    assert_eq!(issue["rule"], "FUNC-12", "{issue:?}");
    // The first test of the function file, whose root is the definition (D8).
    assert_eq!(
        issue["spec_paths"],
        Json::Array(vec![Json::from("tests[0]")])
    );
}

/// The port verifies the lock as the R host does: a renv.lock that pins
/// another version of the package fails activation with a lock finding.
#[test]
fn the_port_verifies_the_lock() {
    // `yamaa` at the R package's version, so only the benchmarks' package
    // mismatches.
    let lock = br#"{"R": {"Version": "4.6.1"}, "Packages": {"yamaa": {"Package": "yamaa", "Version": "0.6.0"}, "yamaabenchmarks": {"Package": "yamaabenchmarks", "Version": "0.9.0"}}}"#;
    assert_eq!(functions::r_package_version(), "0.6.0");
    let issue = first_issue("adam-adsl-bmi", Arc::new(functions::Benchmarks), |served| {
        served.offer("r/renv.lock", Arc::from(&lock[..]));
    });
    assert_eq!(issue["condition"], "runtime_artifact_mismatch", "{issue:?}");
    assert_eq!(issue["rule"], "FUNC-3", "{issue:?}");
    let context = &issue["context"];
    assert_eq!(context["package"], functions::PACKAGE, "{issue:?}");
    assert_eq!(context["reason"], "version_mismatch", "{issue:?}");
    assert_eq!(context["actual"], functions::PACKAGE_VERSION, "{issue:?}");
    assert_eq!(
        context["expected"],
        Json::Array(vec![Json::from("0.9.0")]),
        "{issue:?}"
    );
}

/// The `selftest` cases (`benchmarks/selftest.yaml`, D105) pass the memory
/// route on an engine without project functions, as the command line runs
/// them: each calls no project function, so it runs with its project through
/// the empty port.
#[test]
fn the_selftest_cases_pass_without_project_functions() {
    let listed = yaml_json(&fs::read(root().join("selftest.yaml")).unwrap());
    let names = listed["cases"]
        .as_array()
        .expect("selftest.yaml lists its cases")
        .iter()
        .map(|name| name.as_str().expect("a case is named by its directory"))
        .collect::<Vec<_>>();
    assert!(!names.is_empty());
    let mut failures = Vec::new();
    for name in names {
        if let CaseOutcome::Findings(findings) = check_case_with(name, &root().join(name), None) {
            failures.extend(findings);
        }
    }
    assert!(
        failures.is_empty(),
        "the memory route without project functions disagrees with the file route:\n{}",
        failures.join("\n")
    );
}

/// Every benchmark passes the memory route; none is exempt.
#[test]
fn memory_route_benchmarks() {
    let cases = benchmark_cases();
    assert!(!cases.is_empty(), "no benchmark cases found");
    let failures = check_all(
        cases
            .iter()
            .map(|(name, case)| (name.as_str(), case.as_path())),
    );
    assert!(
        failures.is_empty(),
        "the memory route disagrees with the file route:\n{}",
        failures.join("\n")
    );
}
