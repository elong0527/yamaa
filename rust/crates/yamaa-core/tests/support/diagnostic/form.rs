//! The case form both crates' replays read (#3691, #3798), and what a case
//! states beside its issues. `yamaa-core` and `yamaa-adapters` hold this file
//! byte for byte at `tests/support/diagnostic/form.rs`, since neither crate's
//! tests import the other's (#2158), and `tools.validation.check_test_support`
//! fails while the two differ.
//!
//! A fixture file holds `about`, its `documents` and its `cases`. A case
//! names the behaviour it proves (`case`), the operation that reaches it
//! (`operation`, or a list of several that must each report the same), that
//! operation's `input`, and the issue rows it reports (`issues`, which each
//! replay reads with `compares` and `among`). A positive case also states
//! what its operation built and saved:
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
pub const CASE: [&str; 9] = [
    "case",
    "operation",
    "input",
    "compares",
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
                    relative(path, label);
                    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
                    Some(
                        std::fs::read(root.join(path))
                            .unwrap_or_else(|error| panic!("{label}: {name}: {path}: {error}")),
                    )
                }
                other => {
                    panic!("{label}: {name} is its text, {{\"file\": path}} or null, not {other:?}")
                }
            };
            (name.clone(), bytes)
        })
        .collect()
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
