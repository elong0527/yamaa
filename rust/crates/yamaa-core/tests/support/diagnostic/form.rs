//! The case form both crates' replays read (#3691, #3798), and what a case
//! states beside its issues. `yamaa-core` and `yamaa-adapters` hold this file
//! byte for byte at `tests/support/diagnostic/form.rs`, since neither crate's
//! tests import the other's (#2158), and `tools.validation.check_test_support`
//! fails while the two differ.
//!
//! A fixture file holds `about`, its `documents` and its `cases`. A case
//! names the behaviour it proves (`case`), the operation that reaches it
//! (`operation`, or a list of several that must each report the same issues,
//! their complete rows compared with each other whatever `compares`,
//! `prefix` and `among` leave out), that operation's `input`, and the issue
//! rows it reports (`issues`, which each replay reads with `compares`, and
//! in the [`Order`] that `prefix` or `among` writes). Its rows are every
//! issue reported, in order, unless it writes `"prefix": true`, for the first
//! issues reported, in order, the issues after them not compared, or
//! `"among": true`, for some of them, each a distinct one, in any order.
//! A positive case also states what its operation built and saved:
//!
//! - `output`, the output table, whole: a header row of its column names,
//!   then one row per record, each holding one cell per column;
//! - `report`, the run report's rows of each kind it names (`run`, `input`,
//!   `rows`, `handler`, `column` or `check`, CHECK-23), in report order, each
//!   kind's written as a table whose header names the report columns it
//!   compares, in the report's order (CHECK-24), `KIND` aside; a kind the
//!   case does not name is not compared, a header with no row under it
//!   states that the report has no row of that kind, and `{}` states nothing
//!   and is refused. A case names every column of each kind it states; a
//!   header that names fewer is for a case whose source compared just
//!   those columns, as `compares` is for issue rows;
//! - `saved`, what the operation did to the study directory the case
//!   staged, whether it was refused or not: each file it created or changed,
//!   by its path there with forward slashes, and its bytes, its text or
//!   `{"file": path}` naming an authored expected file by its path from the
//!   repository root, such as a benchmark's `expected/` file; and each file
//!   it removed, as `null`. `{}` states that it created, changed and removed
//!   nothing.
//!
//! The adapters' `input` stages a study directory: its `files`, each its
//! text, a document of the fixture's `documents` (`base`) or an authored
//! file named by its path from the repository root (`file`), as `saved`
//! names one, the last two with `edits`; the files of the benchmark
//! `benchmark` names, each at its path there but its `expected/` directory,
//! `README.md` and `benchmark.yaml`, which the input's `edits` map to their
//! edits; and empty `directories`. An edit replaces text that occurs exactly
//! once, and a file of `files` is never one the benchmark stages. The
//! adapters' `define` composes a project's submission section and saves it,
//! as `save` builds and saves, so each reports the issues of its save too.
//!
//! A cell is `null` when the value is missing, its text for `str`, its
//! integer for `int`, `true` or `false` for `bool`, and for `float` the
//! shortest number that reads back as the same double, spelt as a
//! diagnostic context spells one (`2.5`, `10.0`, `1e16`). A `date` is its
//! `YYYY-MM-DD` text and a `datetime` its `YYYY-MM-DDTHH:MM:SS` text. A case
//! states a table only when its operation reports one, and `saved` exactly
//! when its operation saves.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use yamaa_core::{
    json::{Number, Value},
    table::ValueRef,
};

/// The members a fixture file may write.
pub const FIXTURE: [&str; 3] = ["about", "documents", "cases"];

/// The members a case may write.
pub const CASE: [&str; 10] = [
    "case",
    "operation",
    "input",
    "compares",
    "prefix",
    "among",
    "issues",
    "output",
    "report",
    "saved",
];

/// The kinds of the run report's rows, in report order (CHECK-23).
const KINDS: [&str; 6] = ["run", "input", "rows", "handler", "column", "check"];

/// The files of a study directory: each one's path there, with forward
/// slashes, and its bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

/// What an operation did to a study directory: each file it created or
/// changed, by its path there, with its bytes, and each file it removed,
/// with none.
pub type Saved = BTreeMap<String, Option<Vec<u8>>>;

/// How a case's issue rows stand to the issues its operation reports, each
/// row compared through the case's `compares`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// The rows are every issue reported, in order.
    Complete,
    /// The rows are the first issues reported, in order (`"prefix": true`):
    /// a test that read the first of several issues by position.
    Prefix,
    /// Each row is a distinct issue reported, in any order (`"among": true`):
    /// a test that found its issues by their cause among others.
    Among,
}

impl Order {
    /// The order `case` writes, whose `issues` hold `rows` rows: `prefix`
    /// or `among`, each written only as `true`, never both, and each naming
    /// at least one row; `label` names the case in a failure.
    pub fn read(case: &Value, rows: usize, label: &str) -> Self {
        let written = |member: &str| match case.get(member) {
            None => false,
            Some(Value::Bool(true)) => true,
            Some(other) => panic!("{label}: {member} is written only as true, not {other:?}"),
        };
        let order = match (written("prefix"), written("among")) {
            (false, false) => return Self::Complete,
            (true, false) => Self::Prefix,
            (false, true) => Self::Among,
            (true, true) => panic!("{label}: a case writes prefix or among, not both"),
        };
        assert!(
            rows > 0,
            "{label}: a case whose rows are some of the issues names at least one"
        );
        order
    }

    /// The reported rows a case's `written` rows are compared with: for a
    /// prefix, the first `written`, or every one when fewer were reported,
    /// so that the comparison fails; otherwise every one, which a complete
    /// case's rows equal in order and among which an `among` case finds each
    /// of its rows.
    pub fn compared<T>(self, reported: &[T], written: usize) -> &[T] {
        match self {
            Self::Prefix => &reported[..written.min(reported.len())],
            Self::Complete | Self::Among => reported,
        }
    }
}

/// What an operation reports beside its issues: the output table and the run
/// report it built, each as its rows, header first, and what an operation
/// that saves did to the study directory.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub output: Option<Vec<Value>>,
    pub report: Option<Vec<Value>>,
    pub saved: Option<Saved>,
}

/// What a case states beside its issues.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stated {
    /// The output table, whole, header first.
    pub output: Option<Vec<Value>>,
    /// The report rows of each kind the case names, each kind's header first.
    pub report: Option<BTreeMap<String, Vec<Value>>>,
    /// What the operation did to the study directory.
    pub saved: Option<Saved>,
}

impl Stated {
    /// What `case` states beside its issues, each member read as the form
    /// writes it; `label` names the case in a failure.
    pub fn read(case: &Value, label: &str) -> Self {
        Self {
            output: case.get("output").map(|rows| table(rows, "output", label)),
            report: case.get("report").map(|kinds| report(kinds, label)),
            saved: case.get("saved").map(|files| written(files, label)),
        }
    }

    /// Compare what the case states with what `operation` reported: the
    /// output table, the report rows of each kind it names and what it did
    /// to the study directory.
    pub fn compare(&self, reported: &Outcome, operation: &str, label: &str) {
        if let Some(stated) = &self.output {
            let output = reported
                .output
                .as_ref()
                .unwrap_or_else(|| panic!("{label}: {operation} reports no output table"));
            assert!(
                stated == output,
                "{label}: the output rows are\n{}\nnot\n{}",
                lines(output),
                lines(stated)
            );
        }
        if let Some(kinds) = &self.report {
            let report = reported
                .report
                .as_ref()
                .unwrap_or_else(|| panic!("{label}: {operation} reports no run report"));
            for (kind, stated) in kinds {
                let rows = of_kind(report, kind, &stated[0], label);
                assert!(
                    *stated == rows,
                    "{label}: the report's {kind} rows are\n{}\nnot\n{}",
                    lines(&rows),
                    lines(stated)
                );
            }
        }
        match (&self.saved, &reported.saved) {
            (None, None) => {}
            (Some(_), None) => panic!("{label}: {operation} saves nothing to state as saved"),
            (None, Some(_)) => panic!("{label}: {operation} saves, so the case states saved"),
            (Some(stated), Some(saved)) => assert!(
                stated == saved,
                "{label}: {operation} created, changed or removed\n{}\nnot\n{}",
                listing(saved),
                listing(stated)
            ),
        }
    }
}

/// What an operation did to a study directory that held `staged` before it
/// and holds `now` after it: each file it created or changed, with its bytes
/// now, and each file it removed.
pub fn saved(staged: &Files, now: &Files) -> Saved {
    let removed = staged
        .keys()
        .filter(|name| !now.contains_key(*name))
        .map(|name| (name.clone(), None));
    let written = now
        .iter()
        .filter(|(name, bytes)| staged.get(*name) != Some(*bytes))
        .map(|(name, bytes)| (name.clone(), Some(bytes.clone())));
    removed.chain(written).collect()
}

/// The operations of the case `label` names each reported the same complete
/// issue rows, which `reported` holds by operation, in order: a case that
/// names several states that each reports the same issues, so the rows are
/// compared whole, whatever its own rows compare. A failure names the first
/// row that differs.
pub fn same_issues(reported: &[(&str, Vec<Value>)], label: &str) {
    let Some(((first, rows), others)) = reported.split_first() else {
        return;
    };
    for (operation, other) in others {
        let row = |rows: &[Value], at: usize| rows.get(at).map_or_else(|| "no row".into(), json);
        if let Some(at) = (0..rows.len().max(other.len())).find(|&at| rows.get(at) != other.get(at))
        {
            panic!(
                "{label}: {first} and {operation} report different issues, first at index {at}:\n\
                 {first}: {}\n{operation}: {}",
                row(rows, at),
                row(other, at)
            );
        }
    }
}

/// A table's rows as an operation reports them: the header of its `names`,
/// then each of its `count` rows, the cell at a row and a column read by
/// `at`.
pub fn rows<'a>(
    names: impl IntoIterator<Item = &'a str>,
    count: usize,
    mut at: impl FnMut(usize, usize) -> ValueRef<'a>,
) -> Vec<Value> {
    let header = names
        .into_iter()
        .map(|name| Value::String(name.into()))
        .collect::<Vec<_>>();
    let width = header.len();
    let mut rows = vec![Value::Array(header)];
    for row in 0..count {
        rows.push(Value::Array(
            (0..width).map(|column| cell(at(row, column))).collect(),
        ));
    }
    rows
}

/// A value as a table's cell writes it.
pub fn cell(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Missing => Value::Null,
        ValueRef::Str(text) => Value::String(text.into()),
        ValueRef::Int(value) => Value::from(value),
        ValueRef::Float(value) => {
            Value::Number(Number::from_f64(value.get()).expect("a finite float"))
        }
        ValueRef::Bool(value) => Value::Bool(value),
        ValueRef::Date(date) => Value::String(date.to_string()),
        ValueRef::DateTime(datetime) => Value::String(datetime.to_string()),
    }
}

/// The rows of the table `member`: a header of distinct column names, then
/// rows that hold one scalar cell per column.
fn table(written: &Value, member: &str, label: &str) -> Vec<Value> {
    let rows = written
        .as_array()
        .unwrap_or_else(|| panic!("{label}: {member} is a list of rows, not {written:?}"));
    let Some((header, records)) = rows.split_first() else {
        panic!("{label}: {member} starts with its header row");
    };
    let names = header
        .as_array()
        .filter(|names| !names.is_empty())
        .unwrap_or_else(|| panic!("{label}: {member}'s header names its columns: {header:?}"));
    let mut distinct = BTreeSet::new();
    for name in names {
        let name = name
            .as_str()
            .unwrap_or_else(|| panic!("{label}: each {member} column is named by text: {name:?}"));
        assert!(
            distinct.insert(name),
            "{label}: {member} names the column {name:?} twice"
        );
    }
    for record in records {
        let cells = record
            .as_array()
            .filter(|cells| cells.len() == names.len())
            .unwrap_or_else(|| {
                panic!("{label}: each {member} row holds one cell per column: {record:?}")
            });
        for cell in cells {
            assert!(
                !matches!(cell, Value::Array(_) | Value::Object(_)),
                "{label}: each {member} cell is a scalar, not {cell:?}"
            );
        }
    }
    rows.clone()
}

/// The report rows `report` states: each kind it names, and that kind's
/// table, whose header leaves out `KIND`.
fn report(kinds: &Value, label: &str) -> BTreeMap<String, Vec<Value>> {
    let kinds = kinds
        .as_object()
        .unwrap_or_else(|| panic!("{label}: report maps each kind to its rows, not {kinds:?}"));
    assert!(
        !kinds.is_empty(),
        "{label}: report names at least one kind, since an empty report states nothing"
    );
    kinds
        .iter()
        .map(|(kind, rows)| {
            assert!(
                KINDS.contains(&kind.as_str()),
                "{label}: report names a kind of {KINDS:?}, not {kind:?}"
            );
            let rows = table(rows, &format!("report.{kind}"), label);
            assert!(
                !rows[0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|name| name == "KIND"),
                "{label}: report.{kind} is keyed by its kind, so its header leaves out KIND"
            );
            (kind.clone(), rows)
        })
        .collect()
}

/// The rows of `kind` in `report`, the reported table, header first, each
/// holding the columns `header` names, which must be report columns in the
/// report's order.
fn of_kind(report: &[Value], kind: &str, header: &Value, label: &str) -> Vec<Value> {
    let names = report[0].as_array().expect("a header");
    let position = |name: &str| {
        names
            .iter()
            .position(|column| column == name)
            .unwrap_or_else(|| panic!("{label}: the run report has no column {name:?}"))
    };
    let stated = header.as_array().expect("a header").iter();
    let stated = stated.filter_map(Value::as_str).collect::<Vec<_>>();
    let columns = stated.iter().map(|name| position(name)).collect::<Vec<_>>();
    assert!(
        columns.windows(2).all(|pair| pair[0] < pair[1]),
        "{label}: report.{kind} names its columns in the report's order: {stated:?}"
    );
    let at = position("KIND");
    let mut rows = vec![header.clone()];
    for row in &report[1..] {
        let cells = row.as_array().expect("a row");
        if cells[at] == kind {
            let cells = columns.iter().map(|&column| cells[column].clone());
            rows.push(Value::Array(cells.collect()));
        }
    }
    rows
}

/// The files `saved` states: each file created or changed, with its text or
/// the bytes of the authored file it names, and each file removed, written
/// `null`.
fn written(saved: &Value, label: &str) -> Saved {
    let files = saved.as_object().unwrap_or_else(|| {
        panic!("{label}: saved maps each file to its bytes or null, not {saved:?}")
    });
    files
        .iter()
        .map(|(name, bytes)| {
            relative(name, label);
            let bytes = match bytes {
                Value::Null => None,
                Value::String(text) => Some(text.clone().into_bytes()),
                Value::Object(members) if members.len() == 1 && members.contains_key("file") => {
                    let path = members["file"].as_str().unwrap_or_else(|| {
                        panic!("{label}: {name} names its expected file by its path: {bytes:?}")
                    });
                    Some(authored(path, &format!("{label}: {name}")))
                }
                other => {
                    panic!("{label}: {name} is its text, {{\"file\": path}} or null, not {other:?}")
                }
            };
            (name.clone(), bytes)
        })
        .collect()
}

/// The bytes of the authored file at `path`, relative to the repository root,
/// such as a benchmark's `expected/` file; `label` names the case and member
/// that name it in a failure.
pub fn authored(path: &str, label: &str) -> Vec<u8> {
    relative(path, label);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    std::fs::read(root.join(path)).unwrap_or_else(|error| panic!("{label}: {path}: {error}"))
}

/// `path` is relative, spelt with forward slashes, and names no `.` or `..`.
fn relative(path: &str, label: &str) {
    assert!(
        !path.starts_with('/')
            && !path.contains('\\')
            && path
                .split('/')
                .all(|segment| !matches!(segment, "" | "." | "..")),
        "{label}: {path:?} is a relative path with forward slashes"
    );
}

/// Each row on its own line, its cells as JSON spells them.
fn lines(rows: &[Value]) -> String {
    let cell = |cell: &Value| match cell {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => format!("{text:?}"),
        other => format!("{other:?}"),
    };
    rows.iter()
        .map(|row| match row {
            Value::Array(cells) => {
                let cells = cells.iter().map(cell).collect::<Vec<_>>();
                format!("[{}]", cells.join(", "))
            }
            other => format!("{other:?}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `value` as JSON text, its strings quoted as Rust quotes them, for a
/// failure message.
fn json(value: &Value) -> String {
    let joined = |items: Vec<String>| items.join(", ");
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => format!("{text:?}"),
        Value::Array(items) => format!("[{}]", joined(items.iter().map(json).collect())),
        Value::Object(members) => format!(
            "{{{}}}",
            joined(
                members
                    .iter()
                    .map(|(key, value)| format!("{key:?}: {}", json(value)))
                    .collect()
            )
        ),
    }
}

/// Each file's path and its text, or `null` for a file removed, for a failure
/// message; `nothing` when there is no file.
fn listing(files: &Saved) -> String {
    if files.is_empty() {
        return "nothing".to_owned();
    }
    files
        .iter()
        .map(|(name, bytes)| match bytes {
            Some(bytes) => format!("{name}: {:?}", String::from_utf8_lossy(bytes)),
            None => format!("{name}: null"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Run `replay` on each of `cases`, on as many threads as the machine runs
/// at once, each taking the next case no thread has taken, so that a replay
/// takes about the time of its slowest cases rather than the sum of them
/// all (#3866); each case writes its own study directory. Return what each
/// case returned, in the cases' order. A case that fails does not stop the
/// others: once every case has run, the replay fails, naming each case that
/// failed by `label`, in the cases' order, with its own failure message,
/// which names the operation and the first row that differs.
pub fn in_parallel<C: Sync, R: Send>(
    cases: &[C],
    label: impl Fn(&C) -> &str + Sync,
    replay: impl Fn(&C) -> R + Sync,
) -> Vec<R> {
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        sync::atomic::{AtomicUsize, Ordering},
        thread,
    };
    let next = AtomicUsize::new(0);
    let threads = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(cases.len());
    let mut ran = thread::scope(|scope| {
        let workers = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut ran = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(case) = cases.get(at) else {
                            return ran;
                        };
                        ran.push((at, catch_unwind(AssertUnwindSafe(|| replay(case)))));
                    }
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("each case's failure is caught"))
            .collect::<Vec<_>>()
    });
    ran.sort_by_key(|(at, _)| *at);
    let mut returned = Vec::with_capacity(cases.len());
    let mut failed = Vec::new();
    for (at, outcome) in ran {
        match outcome {
            Ok(value) => returned.push(value),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("a panic with no message");
                failed.push(format!("{}:\n{message}", label(&cases[at])));
            }
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {} cases failed:\n\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n\n")
    );
    returned
}
