//! The installation self-test (#3030 D28, D105, D108): the selftest cases,
//! run through the installed engine and compared with their expected files.
//!
//! The cases are the existing benchmarks that `benchmarks/selftest.yaml`
//! names, so their expected bytes stay independently authored there. The
//! facade embeds each case's files when it builds (`cases`), and [`selftest`]
//! serves them as held bytes through a [`MemoryResolver`], as the hosts serve
//! a case through the memory route: it writes no temporary file, reads no
//! package data directory and uses no network, so every host and the command
//! line run the same bytes through the shipped runtime.
//!
//! A case runs as the benchmark runners run it. Each entry is built with its
//! project by [`SpecRequest::run`] and saved, and what it saved is served to
//! the later entries and to the case's declared submission
//! ([`ProjectRequest::submission`], with the case's creation time), which is
//! saved in turn. The engine has no project functions, as the command line's
//! has none: the cases call none (D110).
//!
//! [`SpecRequest::run`]: crate::SpecRequest::run
//! [`ProjectRequest::submission`]: crate::ProjectRequest::submission
//!
//! - A case passes when every file it saved, read back from the resolver,
//!   equals the expected file of that name, and every expected file was
//!   saved. A Parquet file is compared as the public conformance runner
//!   compares it, since its bytes depend on the writer that authored it:
//!   each column's stored schema (its physical type, logical and converted
//!   type annotations, and definition and repetition levels), then the
//!   values, floats by their bits. The engine's reader reads NaN and the
//!   infinities as missing (VALUE-4), so here a stored NaN equals a missing
//!   value, as for the memory-route runner but not the public one.
//! - A negative case passes when its first issue matches its
//!   `expected/error.yaml` as the conformance runners match it: the phase,
//!   condition and spec paths exactly, the rule when the file names one, and
//!   the file's context as a subset of the issue's. The issue is its
//!   submission's when it declares one, and its entries' otherwise.
//! - A case that needs a feature this build does not compile is not covered:
//!   it is `skipped`, never passed (D105, D108).
//!
//! The result keeps the shape of `docs/design/interface-1.0/api.md` at tag
//! `yamaa-v0.6.2`: a status, one row per case and the engine [`Info`]. Its
//! printed text names the adapters that no case covered in this build: those
//! `benchmarks/selftest.yaml` lists under `uncovered`, and those that only a
//! skipped case exercises.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::api::{Status, SubmissionOptions};
use crate::cancel::CancellationToken;
use crate::diagnostic::{ContextValue, Diagnostic};
use crate::engine::{Engine, EngineOptions};
use crate::host::functions::HostLanguage;
use crate::host::resolver::{MemoryResolver, ResourceKind, ResourceResolver};
use crate::info::{info, Host, Info};
use crate::table::Table;
use crate::text::{self, Summary};

mod cases;

/// The selftest table's columns, in order.
pub const SELFTEST_COLUMNS: [&str; 3] = ["case", "status", "message"];

/// A negative case's expected issue, among its expected files.
const ERROR: &str = "error.yaml";

/// One selftest case: a benchmark that `benchmarks/selftest.yaml` names, as
/// the generated table describes it.
struct Case {
    /// The benchmark's directory name.
    name: &'static str,
    /// The optional features it needs, as [`Info::features`] names them.
    features: &'static [&'static str],
    /// The adapters it exercises.
    adapters: &'static [&'static str],
    /// Its entries, in run order.
    entries: &'static [Entry],
    /// Its declared submission, if any.
    define: Option<Define>,
    /// Every file the runners serve, by its path in the case, with its bytes.
    files: &'static [(&'static str, &'static [u8])],
    /// Its expected files, by name, with their bytes.
    expected: &'static [(&'static str, &'static [u8])],
}

/// One entry of a case: a specification, run with its project when it has
/// one.
struct Entry {
    specification: &'static str,
    project: Option<&'static str>,
}

/// A case's declared submission: its project, section and creation time.
struct Define {
    project: &'static str,
    section: &'static str,
    created: &'static str,
}

/// What [`selftest`] found: one row per case, the adapters that no case
/// covered, and the engine information. The hosts return its
/// [`status`](SelfTest::status), [`table`](SelfTest::table) and
/// [`info`](SelfTest::info), and every host prints its
/// [`summary`](SelfTest::summary).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfTest {
    cases: Vec<SelfTestCase>,
    not_covered: Vec<&'static str>,
    info: Info,
}

/// One case's outcome: one row of the selftest table.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfTestCase {
    /// The case: the benchmark it runs, as `benchmarks/selftest.yaml` names
    /// it.
    pub case: &'static str,
    /// `succeeded` when the case reproduced its expected files or issue,
    /// `failed` when it did not, and `skipped` when it is not covered: this
    /// build does not compile a feature it needs.
    pub status: Status,
    /// Why the case failed or is not covered; `None` when it passed. Not
    /// frozen.
    pub message: Option<String>,
}

/// Run every selftest case through this engine, as `host` asks, with
/// `host_version` the Python or R version it runs on, or `None` for the
/// command line.
///
/// # Example
///
/// ```
/// let selftest = yamaa::selftest(yamaa::Host::Cli, None);
/// assert_eq!(selftest.info().host.as_str(), "cli");
/// if selftest.info().features.len() == 4 {
///     assert_eq!(selftest.status(), yamaa::Status::Succeeded);
/// }
/// ```
pub fn selftest(host: Host, host_version: Option<&str>) -> SelfTest {
    let info = info(host, host_version);
    let results = cases::CASES
        .iter()
        .map(|case| run(case, &info))
        .collect::<Vec<_>>();
    SelfTest::new(cases::CASES, results, cases::UNCOVERED, info)
}

impl SelfTest {
    fn new(
        cases: &[Case],
        results: Vec<SelfTestCase>,
        uncovered: &[&'static str],
        info: Info,
    ) -> Self {
        let covered = cases
            .iter()
            .zip(&results)
            .filter(|(_, result)| result.status != Status::Skipped)
            .flat_map(|(case, _)| case.adapters.iter().copied())
            .collect::<BTreeSet<_>>();
        let mut not_covered = uncovered.iter().copied().collect::<BTreeSet<_>>();
        for (case, _) in cases
            .iter()
            .zip(&results)
            .filter(|(_, result)| result.status == Status::Skipped)
        {
            not_covered.extend(
                case.adapters
                    .iter()
                    .copied()
                    .filter(|adapter| !covered.contains(adapter)),
            );
        }
        Self {
            cases: results,
            not_covered: not_covered.into_iter().collect(),
            info,
        }
    }

    /// `failed` when a case failed, and `succeeded` otherwise; a case that is
    /// not covered fails nothing.
    pub fn status(&self) -> Status {
        if self.cases.iter().any(|case| case.status == Status::Failed) {
            Status::Failed
        } else {
            Status::Succeeded
        }
    }

    /// Each case's outcome, in the order `benchmarks/selftest.yaml` lists the
    /// cases.
    pub fn cases(&self) -> &[SelfTestCase] {
        &self.cases
    }

    /// The adapters that no case covered in this build, sorted: those
    /// `benchmarks/selftest.yaml` lists under `uncovered`, and those that
    /// only a case this build skips exercises.
    pub fn not_covered(&self) -> &[&'static str] {
        &self.not_covered
    }

    /// The engine information (D41) of the host that ran the cases.
    pub fn info(&self) -> &Info {
        &self.info
    }

    /// The selftest table with the [`SELFTEST_COLUMNS`], one row per case:
    /// the case, its status and its message, missing when it passed.
    pub fn table(&self) -> Table {
        use yamaa_adapters::public_table::{PublicColumn, PublicTable};

        let text = |cell: fn(&SelfTestCase) -> Option<&str>| {
            PublicColumn::Str(self.cases.iter().map(cell).collect())
        };
        let columns = [
            text(|item| Some(item.case)),
            text(|item| Some(item.status.as_str())),
            text(|item| item.message.as_deref()),
        ];
        let columns = SELFTEST_COLUMNS
            .into_iter()
            .map(str::to_owned)
            .zip(columns)
            .collect();
        Table::from_public(PublicTable::new(columns).expect("the selftest columns form one table"))
    }

    /// The printed text (D37, P-PRINT-1): `yamaa selftest: <status>, <p> of
    /// <c> cases passed`; then `first failure: <case>: <message>` when a case
    /// failed; then `not covered: <adapters>` when an adapter was not
    /// covered; then the engine, language, host and platform, such as
    /// `engine 1.0.0, language 1.0, python 3.12.4, linux-x86_64`.
    pub fn summary(&self) -> Summary {
        let passed = self
            .cases
            .iter()
            .filter(|case| case.status == Status::Succeeded)
            .count();
        let mut lines = Vec::new();
        if let Some(failed) = self.cases.iter().find(|case| case.status == Status::Failed) {
            lines.push(text::ascii(&format!(
                "first failure: {}: {}",
                failed.case,
                failed.message.as_deref().unwrap_or_default()
            )));
        }
        if !self.not_covered.is_empty() {
            lines.push(format!("not covered: {}", self.not_covered.join(", ")));
        }
        let info = &self.info;
        let host = match &info.host_version {
            Some(version) => format!("{} {version}", info.host.as_str()),
            None => info.host.as_str().to_owned(),
        };
        lines.push(text::ascii(&format!(
            "engine {}, language {}, {host}, {}",
            info.version,
            info.language_versions.join(", "),
            info.platform
        )));
        Summary {
            line: format!(
                "yamaa selftest: {}, {passed} of {} passed",
                self.status().as_str(),
                text::count(self.cases.len(), "case", "cases")
            ),
            members: lines,
            first_issue: None,
        }
    }
}

/// One case's outcome on this build.
fn run(case: &Case, info: &Info) -> SelfTestCase {
    let (status, message) = match case
        .features
        .iter()
        .find(|feature| !info.features.contains(feature))
    {
        Some(feature) => (
            Status::Skipped,
            Some(format!(
                "not covered: this build does not compile {feature}"
            )),
        ),
        None => contained(|| check(case, info)),
    };
    SelfTestCase {
        case: case.name,
        status,
        message,
    }
}

/// The status and message of one case's `check`, which a panic fails as it
/// fails the case, so that the cases after it still run.
fn contained(check: impl FnOnce() -> Result<(), String>) -> (Status, Option<String>) {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)) {
        Ok(Ok(())) => (Status::Succeeded, None),
        Ok(Err(message)) => (Status::Failed, Some(message)),
        Err(payload) => {
            let reason = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("a panic");
            (
                Status::Failed,
                Some(format!("the engine stopped: {reason}")),
            )
        }
    }
}

/// Run one case and compare it with its expected files, or say why it does
/// not match them.
fn check(case: &Case, info: &Info) -> Result<(), String> {
    let served = serve(case);
    // Each host claims its own language, and the command line none (#3030
    // section 2.10); the cases call no project function, so the claim
    // decides nothing they read.
    let (name, version) = (
        info.host.as_str().to_owned(),
        info.host_version.clone().unwrap_or_default(),
    );
    let options = match info.host {
        Host::Python => EngineOptions::new(HostLanguage::Python, name, version),
        Host::R => EngineOptions::new(HostLanguage::R, name, version),
        Host::Cli => EngineOptions::neutral(name, version),
    };
    let engine = Engine::new(
        options.with_resolver(Arc::clone(&served.resolver) as Arc<dyn ResourceResolver>),
    );
    let token = CancellationToken::new();
    let expected_issue = case
        .expected
        .iter()
        .find(|(name, _)| *name == ERROR)
        .map(|(_, bytes)| *bytes);
    // A negative case without a submission expects its entries' first issue,
    // and they save nothing.
    let negative_entries = expected_issue.is_some() && case.define.is_none();
    let mut saved = Vec::new();
    let mut issues = Vec::new();
    for entry in case.entries {
        let request = engine.spec(entry.specification).cancel_with(&token);
        let request = match entry.project {
            Some(project) => request.project(project),
            None => request,
        };
        let mut run = request.run().map_err(|failure| failure.message)?;
        let diagnostics = run.diagnostics();
        if negative_entries {
            if run.output().is_some() {
                return Err(format!(
                    "the run of {} built an output, but the case expects an issue",
                    entry.specification
                ));
            }
            issues.extend(diagnostics);
            continue;
        }
        if let Some(first) = diagnostics.first() {
            return Err(format!(
                "the run of {} reported an issue: {}",
                entry.specification,
                first.text()
            ));
        }
        let paths = run.save(&token).map_err(|failure| failure.message)?;
        offer_saved(&served, &paths);
        saved.extend(paths);
    }
    if let Some(define) = &case.define {
        let mut submission = engine
            .project(define.project)
            .section(define.section)
            .cancel_with(&token)
            .submission(&SubmissionOptions::new(crate::VERSION).with_created(define.created))
            .map_err(|failure| failure.message)?;
        let diagnostics = submission.diagnostics();
        match expected_issue {
            Some(expected) => expect_issue(expected, diagnostics.first())?,
            None => {
                if let Some(first) = diagnostics.first() {
                    return Err(format!(
                        "the submission reported an issue: {}",
                        first.text()
                    ));
                }
                saved.extend(submission.save(&token).map_err(|failure| failure.message)?);
            }
        }
    } else if let Some(expected) = expected_issue {
        expect_issue(expected, issues.first())?;
    }
    compare(case, &served.resolver, &saved)
}

/// Whether every saved file equals the expected file of its name, and every
/// expected file but the expected issue was saved.
fn compare(case: &Case, resolver: &MemoryResolver, saved: &[String]) -> Result<(), String> {
    let mut written = BTreeSet::new();
    for path in saved {
        let name = path.rsplit('/').next().unwrap_or(path);
        let Some((_, expected)) = case.expected.iter().find(|(file, _)| *file == name) else {
            return Err(format!(
                "{name} was written, but the case expects no {name}"
            ));
        };
        let Some(published) = resolver.published(path) else {
            return Err(format!("{name} was saved but not published"));
        };
        if name.ends_with(".parquet") {
            compare_parquet(name, &published, expected)?;
        } else if *published != **expected {
            return Err(format!(
                "the output bytes of {name} differ from the expected bytes"
            ));
        }
        written.insert(name);
    }
    match case
        .expected
        .iter()
        .find(|(name, _)| *name != ERROR && !written.contains(name))
    {
        Some((name, _)) => Err(format!("{name} was not written")),
        None => Ok(()),
    }
}

/// Whether the saved Parquet file `name` holds what its expected file holds,
/// as the public conformance runner compares them, since the bytes depend on
/// the writer. First every column must be stored alike: the physical type,
/// logical type and definition and repetition levels that the runner's
/// `parquet_signature` compares, with each annotation as stored, so a legacy
/// converted type does not stand in for its logical type as it does for that
/// runner's reader. Then the values must be the same in the same rows, floats
/// by their bits. Neither file reads in a build without the `parquet`
/// feature.
fn compare_parquet(name: &str, saved: &[u8], expected: &[u8]) -> Result<(), String> {
    use yamaa_adapters::parquet_source::{parse, signature};

    let (Ok(saved_columns), Ok(expected_columns), Ok(saved), Ok(expected)) = (
        signature(saved),
        signature(expected),
        parse(saved),
        parse(expected),
    ) else {
        return Err(format!("{name} does not read as a Parquet file"));
    };
    if saved_columns != expected_columns {
        return Err(format!(
            "the stored columns of {name} differ from the expected columns"
        ));
    }
    if !same_values(&saved, &expected) {
        return Err(format!(
            "the output table of {name} differs from the expected table"
        ));
    }
    Ok(())
}

/// Whether two tables the engine's Parquet reader read hold the same columns
/// and the same values in the same rows, a float by its bits, so that `-0.0`
/// differs from `0.0` as it does for the conformance runners.
fn same_values(
    saved: &yamaa_adapters::column_table::ColumnTable,
    expected: &yamaa_adapters::column_table::ColumnTable,
) -> bool {
    use yamaa_core::table::TableAccess;

    let columns = |table: &yamaa_adapters::column_table::ColumnTable| {
        table
            .schema()
            .columns()
            .iter()
            .map(|column| (column.name.clone(), column.kind))
            .collect::<Vec<_>>()
    };
    let width = columns(saved).len();
    columns(saved) == columns(expected)
        && saved.row_count() == expected.row_count()
        && (0..saved.row_count()).all(|row| {
            (0..width).all(
                |column| match (saved.cell(row, column), expected.cell(row, column)) {
                    (Ok(left), Ok(right)) => same_cell(left, right),
                    _ => false,
                },
            )
        })
}

/// Whether two cells hold the same value: a float by its bits, any other
/// value by its equality.
fn same_cell(
    left: yamaa_core::table::ValueRef<'_>,
    right: yamaa_core::table::ValueRef<'_>,
) -> bool {
    use yamaa_core::table::ValueRef;

    match (left, right) {
        (ValueRef::Float(left), ValueRef::Float(right)) => {
            left.get().to_bits() == right.get().to_bits()
        }
        (left, right) => left == right,
    }
}

/// Whether the first issue matches the expected issue `expected`, the
/// `error.yaml` of a negative case, as the conformance runners match it.
fn expect_issue(expected: &[u8], first: Option<&Diagnostic>) -> Result<(), String> {
    let decoded = yamaa_adapters::yaml_decode::decode_yaml(expected)
        .map_err(|_| format!("the case's {ERROR} cannot be read"))?;
    let ContextValue::Map(want) = document_value(&decoded.document, decoded.document.root()) else {
        return Err(format!("the case's {ERROR} is not a mapping"));
    };
    let Some(first) = first else {
        let condition = match want.get("condition") {
            Some(ContextValue::Text(condition)) => condition.as_str(),
            _ => "an issue",
        };
        return Err(format!(
            "no issue was reported, but the case expects {condition}"
        ));
    };
    if issue_matches(&want, first) {
        Ok(())
    } else {
        Err(format!(
            "the first issue differs from the expected issue: {}",
            first.text()
        ))
    }
}

/// The runners' match: `phase`, `condition` and `spec_paths` exactly, `rule`
/// when the expectation names one, and the expected `context` a subset of
/// the issue's.
fn issue_matches(want: &BTreeMap<String, ContextValue>, got: &Diagnostic) -> bool {
    let text = |value: &str| ContextValue::Text(value.to_owned());
    let field = |key: &str| want.get(key).unwrap_or(&ContextValue::Null);
    let paths = ContextValue::List(got.spec_path.iter().map(|path| text(path)).collect());
    let rule = got.rule.as_deref().map_or(ContextValue::Null, text);
    let context = ContextValue::Map(
        got.context
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    );
    let empty = ContextValue::Map(BTreeMap::new());
    let expected_context = match want.get("context") {
        None | Some(ContextValue::Null) => &empty,
        Some(context) => context,
    };
    same(field("phase"), &text(got.phase.as_str()))
        && same(field("condition"), &text(got.code.as_str()))
        && same(field("spec_paths"), &paths)
        && want
            .get("rule")
            .is_none_or(|expected| same(expected, &rule))
        && subset(expected_context, &context)
}

/// The expected value `want` within `got`: every member of an expected
/// mapping present in the reported one and a subset of it, and any other
/// value the same.
fn subset(want: &ContextValue, got: &ContextValue) -> bool {
    match (want, got) {
        (ContextValue::Map(want), ContextValue::Map(got)) => want
            .iter()
            .all(|(key, value)| got.get(key).is_some_and(|other| subset(value, other))),
        (ContextValue::Map(_), _) => false,
        _ => same(want, got),
    }
}

/// Value equality as the runners compare decoded JSON: numbers by value,
/// whether written as integers or fractions, and everything else exactly.
fn same(left: &ContextValue, right: &ContextValue) -> bool {
    fn number(value: &ContextValue) -> Option<f64> {
        match value {
            ContextValue::Int(value) => Some(*value as f64),
            // An integer above `i64::MAX` that `u64` holds compares by value;
            // a wider number compares only by its spelling.
            ContextValue::Wide(text) => text.parse::<u64>().ok().map(|value| value as f64),
            ContextValue::Float(value) => Some(*value),
            _ => None,
        }
    }
    match (left, right) {
        (ContextValue::Float(_), _) | (_, ContextValue::Float(_)) => {
            number(left).is_some() && number(left) == number(right)
        }
        (ContextValue::List(left), ContextValue::List(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(l, r)| same(l, r))
        }
        (ContextValue::Map(left), ContextValue::Map(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, value)| right.get(key).is_some_and(|other| same(value, other)))
        }
        _ => left == right,
    }
}

/// A decoded YAML node as a context value; mapping keys are text.
fn document_value(document: &yamaa_core::schema::Document, node: usize) -> ContextValue {
    use yamaa_core::schema::DocumentNode;

    match &document.nodes()[node] {
        DocumentNode::Null => ContextValue::Null,
        DocumentNode::Boolean(value) => ContextValue::Bool(*value),
        DocumentNode::Integer(text) => text
            .parse()
            .map_or_else(|_| ContextValue::Wide(text.clone()), ContextValue::Int),
        DocumentNode::Float(value) => ContextValue::Float(*value),
        DocumentNode::Text(text) => ContextValue::Text(text.clone()),
        DocumentNode::Sequence(items) => ContextValue::List(
            items
                .iter()
                .map(|&item| document_value(document, item))
                .collect(),
        ),
        DocumentNode::Mapping(items) => ContextValue::Map(
            items
                .iter()
                .filter_map(|&(key, value)| match &document.nodes()[key] {
                    DocumentNode::Text(key) => Some((key.clone(), document_value(document, value))),
                    _ => None,
                })
                .collect(),
        ),
    }
}

/// One served case: a memory resolver that holds every file the runners
/// serve, and the declaring identities it serves them under (#3249).
struct Served {
    resolver: Arc<MemoryResolver>,
    identities: Vec<String>,
}

/// Serve each file of `case` as held bytes under a per-file reference, and
/// each directory that holds one as a kind fact, as the file route reports a
/// directory. The declaring identities are the case root and every YAML file
/// of the case.
fn serve(case: &Case) -> Served {
    let served = Served {
        resolver: Arc::new(MemoryResolver::new()),
        identities: std::iter::once("/".to_owned())
            .chain(
                case.files
                    .iter()
                    .filter(|(path, _)| path.ends_with(".yaml"))
                    .map(|(path, _)| format!("/{path}")),
            )
            .collect(),
    };
    let mut directories = BTreeSet::new();
    for (path, bytes) in case.files {
        offer(&served, path, Arc::from(*bytes));
        let mut parts = path.rsplit_once('/').map(|(directory, _)| directory);
        while let Some(directory) = parts {
            directories.insert(directory);
            parts = directory.rsplit_once('/').map(|(parent, _)| parent);
        }
    }
    for directory in directories {
        for declaring in &served.identities {
            for spelling in spellings(declaring, directory) {
                served
                    .resolver
                    .insert_kind(declaring, &spelling, ResourceKind::Directory);
            }
        }
    }
    served
}

/// Serve `bytes` as the file at the case-relative path `written`, under
/// every declaring identity and each spelling it may use.
fn offer(served: &Served, written: &str, bytes: Arc<[u8]>) {
    let reference = format!("object:{written}");
    for declaring in &served.identities {
        for spelling in spellings(declaring, written) {
            served
                .resolver
                .insert(declaring, &spelling, &reference, Arc::clone(&bytes));
        }
    }
}

/// Serve what a save published, as the runners read an entry's saved files
/// in later calls.
fn offer_saved(served: &Served, paths: &[String]) {
    for path in paths {
        if let (Some(written), Some(bytes)) = (
            path.strip_prefix("memory:/"),
            served.resolver.published(path),
        ) {
            offer(served, written, bytes);
        }
    }
}

/// The spellings under which the file `identity` names `target`, a
/// case-relative path: the case-relative path itself, and the path relative
/// to the declaring file's own directory.
fn spellings(identity: &str, target: &str) -> Vec<String> {
    let directory = identity
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let from = directory
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let to = target
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec![".."; from.len() - common];
    parts.extend(&to[common..]);
    let relative = parts.join("/");
    if relative.is_empty() || relative == target {
        vec![target.to_owned()]
    } else {
        vec![target.to_owned(), relative]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A case's files, editable.
    type Files = Vec<(&'static str, Vec<u8>)>;

    /// The listed case `name`.
    fn listed(name: &str) -> &'static Case {
        cases::CASES
            .iter()
            .find(|case| case.name == name)
            .expect("the case is listed")
    }

    /// The bytes of the file `name` among `files`.
    fn file(files: &'static [(&'static str, &'static [u8])], name: &str) -> &'static [u8] {
        files
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, bytes)| *bytes)
            .expect("the case has the file")
    }

    /// A copy of the listed case `name` with `edit` applied to the files it
    /// serves and to its expected files, for a test that needs a case to
    /// fail. The fixture lives here, never under benchmarks/.
    fn edited(name: &str, edit: impl FnOnce(&mut Files, &mut Files)) -> Case {
        let case = listed(name);
        let copy = |files: &[(&'static str, &'static [u8])]| {
            files
                .iter()
                .map(|(name, bytes)| (*name, bytes.to_vec()))
                .collect::<Files>()
        };
        let (mut files, mut expected) = (copy(case.files), copy(case.expected));
        edit(&mut files, &mut expected);
        let leak = |files: Files| {
            &*Vec::leak(
                files
                    .into_iter()
                    .map(|(name, bytes)| (name, &*Vec::leak(bytes)))
                    .collect::<Vec<_>>(),
            )
        };
        Case {
            name: case.name,
            features: case.features,
            adapters: case.adapters,
            entries: case.entries,
            define: case.define.as_ref().map(|define| Define {
                project: define.project,
                section: define.section,
                created: define.created,
            }),
            files: leak(files),
            expected: leak(expected),
        }
    }

    /// Replace `from`, which the text file `name` of `files` holds, with
    /// `to`.
    fn replace(files: &mut Files, name: &str, from: &str, to: &str) {
        let (_, bytes) = files
            .iter_mut()
            .find(|(file, _)| *file == name)
            .expect("the case has the file");
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains(from), "{name} holds {from}");
        *bytes = text.replace(from, to).into_bytes();
    }

    fn engine_info() -> Info {
        info(Host::Cli, None)
    }

    /// Whether this build compiles every feature `case` needs.
    fn compiled(case: &Case) -> bool {
        case.features
            .iter()
            .all(|feature| engine_info().features.contains(feature))
    }

    /// Every listed case passes on this engine, which has no project
    /// functions (D110), in every host; a case whose feature this build does
    /// not compile is not covered.
    #[test]
    // Clippy 1.82, the minimum toolchain's, reads the generated table as a
    // constant and finds the first assertion always false; the pinned Clippy
    // does not. The assertion guards against an empty generated table.
    #[allow(clippy::const_is_empty)]
    fn the_listed_cases_pass_without_project_functions() {
        assert!(!cases::CASES.is_empty());
        for (host, version) in [
            (Host::Cli, None),
            (Host::Python, Some("3.12.4")),
            (Host::R, Some("4.6.1")),
        ] {
            let result = selftest(host, version);
            assert_eq!(result.status(), Status::Succeeded, "{result:?}");
            for (case, outcome) in cases::CASES.iter().zip(result.cases()) {
                assert_eq!(outcome.case, case.name);
                if compiled(case) {
                    assert_eq!(outcome.status, Status::Succeeded, "{outcome:?}");
                    assert_eq!(outcome.message, None);
                } else {
                    assert_eq!(outcome.status, Status::Skipped, "{outcome:?}");
                }
            }
        }
    }

    /// One changed byte of an expected file fails its case, naming the file,
    /// and the selftest with it.
    #[test]
    fn a_changed_expected_byte_fails_its_case() {
        let case = edited("adam-adsl-xpt", |_, expected| {
            let (_, bytes) = expected
                .iter_mut()
                .find(|(name, _)| *name == "adsl.xpt")
                .unwrap();
            bytes[100] ^= 1;
        });
        if !compiled(&case) {
            return;
        }
        let outcome = run(&case, &engine_info());
        assert_eq!(outcome.status, Status::Failed);
        assert_eq!(
            outcome.message.as_deref(),
            Some("the output bytes of adsl.xpt differ from the expected bytes")
        );
        let result = SelfTest::new(
            std::slice::from_ref(&case),
            vec![outcome],
            &[],
            engine_info(),
        );
        assert_eq!(result.status(), Status::Failed);
        let printed = result.summary().to_string();
        let lines = printed.lines().collect::<Vec<_>>();
        assert_eq!(lines[0], "yamaa selftest: failed, 0 of 1 case passed");
        assert_eq!(
            lines[1],
            "first failure: adam-adsl-xpt: the output bytes of adsl.xpt differ from the expected bytes"
        );
    }

    /// The SAS transport case compares what the SAS reader decoded: a stored
    /// number that decodes otherwise, AGE 24 stored as 25, fails its case.
    #[test]
    fn a_changed_transport_number_fails_its_case() {
        let case = edited("schema-source-xpt", |files, _| {
            let (_, bytes) = files
                .iter_mut()
                .find(|(name, _)| *name == "input/adsl_raw.xpt")
                .unwrap();
            // The first observation follows 22 records: eight header records,
            // the seven NAMESTRs in 13 and the observation header. AGE, at
            // byte 40, holds the IBM double 24: exponent 0x42, fraction 0x18.
            let age = 22 * 80 + 40;
            assert_eq!(bytes[age..age + 8], [0x42, 0x18, 0, 0, 0, 0, 0, 0]);
            bytes[age + 1] = 0x19;
        });
        if !compiled(&case) {
            return;
        }
        let outcome = run(&case, &engine_info());
        assert_eq!(outcome.status, Status::Failed);
        assert_eq!(
            outcome.message.as_deref(),
            Some("the output bytes of adsl.csv differ from the expected bytes")
        );
    }

    /// The ODM case compares what the ODM reader read: a collected age that
    /// the export holds otherwise, 67 exported as 68, fails its case.
    #[test]
    fn a_changed_odm_value_fails_its_case() {
        let case = edited("sdtm-dm-odm-snapshot", |files, _| {
            replace(
                files,
                "input/odm.xml",
                r#"<ItemData ItemOID="IT.DM.AGE" Value="67"/>"#,
                r#"<ItemData ItemOID="IT.DM.AGE" Value="68"/>"#,
            );
        });
        if !compiled(&case) {
            return;
        }
        let outcome = run(&case, &engine_info());
        assert_eq!(outcome.status, Status::Failed);
        assert_eq!(
            outcome.message.as_deref(),
            Some("the output bytes of dm.csv differ from the expected bytes")
        );
    }

    /// A Parquet file is compared by how it stores each column and by what it
    /// holds, never by its bytes, which the listed case's saved file does not
    /// share with the authored one: a file whose columns are stored otherwise
    /// fails on them, and a file that does not read fails as such.
    #[test]
    fn a_parquet_file_compares_by_its_stored_columns_then_its_values() {
        let case = listed("negative-transport-empty-string");
        if !compiled(case) {
            return;
        }
        let expected = file(case.expected, "dm.parquet");
        assert_eq!(compare_parquet("dm.parquet", expected, expected), Ok(()));
        let input = file(case.files, "input/dm_raw.parquet");
        assert_eq!(
            compare_parquet("dm.parquet", input, expected),
            Err("the stored columns of dm.parquet differ from the expected columns".to_owned())
        );
        assert_eq!(
            compare_parquet("dm.parquet", &expected[..expected.len() - 8], expected),
            Err("dm.parquet does not read as a Parquet file".to_owned())
        );
    }

    /// A saved Parquet file whose columns are stored as the expected file's
    /// but whose values differ fails on its values: here the specification
    /// writes another literal into DOMAIN, a change no other check sees.
    #[test]
    fn a_changed_parquet_value_fails_its_case() {
        let case = edited("negative-transport-empty-string", |files, _| {
            replace(
                files,
                "spec.yaml",
                "derivation: \"'DM'\"",
                "derivation: \"'XX'\"",
            );
        });
        if !compiled(&case) {
            return;
        }
        let outcome = run(&case, &engine_info());
        assert_eq!(outcome.status, Status::Failed);
        assert_eq!(
            outcome.message.as_deref(),
            Some("the output table of dm.parquet differs from the expected table")
        );
    }

    /// A float compares by its bits, as both conformance runners compare it:
    /// `-0.0` is not `0.0`.
    #[test]
    fn a_float_compares_by_its_bits() {
        use yamaa_core::table::ValueRef;
        use yamaa_core::value::FiniteFloat;

        let float = |value| ValueRef::Float(FiniteFloat::new(value).unwrap());
        assert!(same_cell(float(1.5), float(1.5)));
        assert!(!same_cell(float(-0.0), float(0.0)));
        assert!(same_cell(ValueRef::Int(3), ValueRef::Int(3)));
        assert!(!same_cell(ValueRef::Str("a"), ValueRef::Str("b")));
        assert!(!same_cell(ValueRef::Missing, ValueRef::Str("")));
    }

    /// Every saved file needs an expected file of its name, and every
    /// expected file but the expected issue must be saved.
    #[test]
    fn saved_and_expected_files_must_pair() {
        let without = edited("adam-adsl-xpt", |_, expected| {
            expected.retain(|(name, _)| *name != "adsl.csv");
        });
        if !compiled(&without) {
            return;
        }
        let outcome = run(&without, &engine_info());
        assert_eq!(outcome.status, Status::Failed);
        assert_eq!(
            outcome.message.as_deref(),
            Some("adsl.csv was written, but the case expects no adsl.csv")
        );
        let extra = edited("adam-adsl-xpt", |_, expected| {
            expected.push(("adsl.json", b"{}".to_vec()));
        });
        let outcome = run(&extra, &engine_info());
        assert_eq!(outcome.status, Status::Failed);
        assert_eq!(
            outcome.message.as_deref(),
            Some("adsl.json was not written")
        );
    }

    /// A panic inside a case fails that case with what the panic said, and
    /// returns, so that the cases after it still run.
    #[test]
    fn a_panic_fails_its_case() {
        assert_eq!(contained(|| Ok(())), (Status::Succeeded, None));
        assert_eq!(
            contained(|| Err("why".to_owned())),
            (Status::Failed, Some("why".to_owned()))
        );
        assert_eq!(
            contained(|| panic!("the decoder broke")),
            (
                Status::Failed,
                Some("the engine stopped: the decoder broke".to_owned())
            )
        );
        let reason = String::from("an owned reason");
        assert_eq!(
            contained(move || std::panic::panic_any(reason)),
            (
                Status::Failed,
                Some("the engine stopped: an owned reason".to_owned())
            )
        );
        assert_eq!(
            contained(|| std::panic::panic_any(7_u8)),
            (
                Status::Failed,
                Some("the engine stopped: a panic".to_owned())
            )
        );
    }

    /// A negative case passes only on its expected issue: another phase,
    /// condition, rule, spec path or context value fails it, and the message
    /// gives the reported issue.
    #[test]
    fn a_negative_case_matches_its_expected_issue() {
        for (from, to) in [
            ("phase: output", "phase: validation"),
            (
                "condition: transport_text_invalid",
                "condition: transport_length_exceeded",
            ),
            ("rule: XPT-7", "rule: XPT-8"),
            ("datasets.DM.columns.INVNAM", "datasets.DM.columns.SITEID"),
            ("row: 1", "row: 2"),
            ("cause: empty_string", "cause: non_ascii"),
        ] {
            let case = edited("negative-transport-empty-string", |_, expected| {
                replace(expected, ERROR, from, to);
            });
            if !compiled(&case) {
                return;
            }
            let outcome = run(&case, &engine_info());
            assert_eq!(outcome.status, Status::Failed, "{to}");
            let message = outcome.message.unwrap();
            assert!(
                message.starts_with("the first issue differs from the expected issue: ")
                    && message.ends_with("[transport_text_invalid, XPT-7]"),
                "{message}"
            );
        }
    }

    /// An expected issue that names no rule matches on everything else, as the
    /// runners match it.
    #[test]
    fn an_expected_issue_without_a_rule_matches() {
        let case = edited("negative-transport-empty-string", |_, expected| {
            replace(expected, ERROR, "rule: XPT-7\n", "");
        });
        if !compiled(&case) {
            return;
        }
        let outcome = run(&case, &engine_info());
        assert_eq!(outcome.status, Status::Succeeded, "{outcome:?}");
    }

    /// The expected context is a subset: a value the issue reports but the
    /// expectation leaves out matches, and integers match their fractions.
    #[test]
    fn the_expected_context_is_a_subset() {
        let map = |pairs: &[(&str, ContextValue)]| {
            ContextValue::Map(
                pairs
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), value.clone()))
                    .collect(),
            )
        };
        let text = |value: &str| ContextValue::Text(value.to_owned());
        let got = map(&[
            ("row", ContextValue::Int(1)),
            ("column", text("INVNAM")),
            ("keys", ContextValue::List(vec![text("A"), text("B")])),
        ]);
        assert!(subset(&map(&[("row", ContextValue::Float(1.0))]), &got));
        assert!(subset(&map(&[]), &got));
        assert!(!subset(&map(&[("row", ContextValue::Int(2))]), &got));
        assert!(!subset(&map(&[("missing", ContextValue::Null)]), &got));
        // A list is compared whole.
        assert!(!subset(
            &map(&[("keys", ContextValue::List(vec![text("A")]))]),
            &got
        ));
        assert!(!subset(&text("1"), &ContextValue::Int(1)));
    }

    /// A case that needs a feature this build does not compile is not
    /// covered: it is skipped, its adapters join those the list names, the
    /// selftest still succeeds, and its text says what was not covered.
    #[test]
    fn a_case_without_its_feature_is_not_covered() {
        let case = Case {
            name: "case-without-feature",
            features: &["unbuilt", "yaml"],
            adapters: &["csv", "unbuilt-adapter"],
            entries: &[],
            define: None,
            files: &[],
            expected: &[],
        };
        let info = info(Host::Python, Some("3.12.4"));
        let outcome = run(&case, &info);
        assert_eq!(
            outcome,
            SelfTestCase {
                case: "case-without-feature",
                status: Status::Skipped,
                message: Some("not covered: this build does not compile unbuilt".to_owned()),
            }
        );
        let listed = cases::CASES
            .iter()
            .find(|case| case.name == "adam-adsl-xpt")
            .unwrap();
        let passed = SelfTestCase {
            case: listed.name,
            status: Status::Succeeded,
            message: None,
        };
        let result = SelfTest::new(
            &[case, edited("adam-adsl-xpt", |_, _| {})],
            vec![outcome.clone(), passed.clone()],
            &["odm"],
            info.clone(),
        );
        assert_eq!(result.status(), Status::Succeeded);
        // csv is covered by the case that ran; the skipped case's other
        // adapter is not.
        assert_eq!(result.not_covered(), ["odm", "unbuilt-adapter"]);
        assert_eq!(result.cases(), [outcome, passed]);
        let languages = info.language_versions.join(", ");
        assert_eq!(
            result.summary().to_string(),
            format!(
                "yamaa selftest: succeeded, 1 of 2 cases passed\n\
                 not covered: odm, unbuilt-adapter\n\
                 engine {}, language {languages}, python 3.12.4, {}",
                crate::VERSION,
                info.platform
            )
        );
    }

    /// The printed text of the listed cases: in a build with every feature,
    /// as every package ships, every case passes and a listed case covers
    /// each adapter, so no `not covered:` line is printed; the command line
    /// has no host version.
    #[test]
    fn a_full_build_covers_every_adapter() {
        let result = selftest(Host::Cli, None);
        let printed = result.summary();
        let lines = printed.lines().collect::<Vec<_>>();
        let info = result.info();
        assert_eq!(
            lines.last().copied(),
            Some(
                format!(
                    "engine {}, language {}, cli, {}",
                    crate::VERSION,
                    info.language_versions.join(", "),
                    info.platform
                )
                .as_str()
            )
        );
        for adapter in cases::UNCOVERED {
            assert!(result.not_covered().contains(adapter), "{adapter}");
        }
        if info.features.len() == 4 {
            let not_covered = result.not_covered();
            assert!(not_covered.is_empty(), "{not_covered:?}");
            assert_eq!(lines.len(), 2, "{lines:?}");
            assert_eq!(
                lines[0],
                format!(
                    "yamaa selftest: succeeded, {0} of {0} cases passed",
                    cases::CASES.len()
                )
            );
        }
    }

    /// The table has the selftest columns, one row per case, with a missing
    /// message for a case that passed.
    #[test]
    #[cfg(feature = "arrow")]
    fn the_table_has_one_row_per_case() {
        use arrow_array::{Array, StringArray};

        let result = SelfTest {
            cases: vec![
                SelfTestCase {
                    case: "one",
                    status: Status::Succeeded,
                    message: None,
                },
                SelfTestCase {
                    case: "two",
                    status: Status::Failed,
                    message: Some("why".to_owned()),
                },
            ],
            not_covered: Vec::new(),
            info: engine_info(),
        };
        let table = result.table();
        let names = table
            .columns()
            .iter()
            .map(|field| field.name().as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, SELFTEST_COLUMNS);
        assert_eq!(table.rows(), 2);
        let batch = &table.batches()[0];
        let column = |index: usize| {
            batch
                .column(index)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .clone()
        };
        assert_eq!(column(0).value(1), "two");
        assert_eq!(column(1).value(0), "succeeded");
        assert_eq!(column(1).value(1), "failed");
        assert!(column(2).is_null(0));
        assert_eq!(column(2).value(1), "why");
    }

    #[test]
    fn spellings_add_the_path_relative_to_the_declaring_file() {
        assert_eq!(spellings("/", "input/ae.csv"), ["input/ae.csv"]);
        assert_eq!(spellings("/spec.yaml", "input/ae.csv"), ["input/ae.csv"]);
        assert_eq!(
            spellings("/input/schema.yaml", "input/ae.csv"),
            ["input/ae.csv", "ae.csv"]
        );
        assert_eq!(
            spellings("/adam/adsl.yaml", "input/ae.csv"),
            ["input/ae.csv", "../input/ae.csv"]
        );
    }
}
