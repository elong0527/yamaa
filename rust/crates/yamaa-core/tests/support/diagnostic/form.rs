//! The case form both crates' replays read (#3691, #3798), and what a case
//! states beside its issues. `yamaa-core` and `yamaa-application` hold this
//! file byte for byte at `tests/support/diagnostic/form.rs`, since neither
//! crate's tests import the other's (#2158), and
//! `tools.validation.check_test_support` fails while the two differ.
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
//!   then one row per record, each holding one cell per column. Only a case
//!   moved from a test whose assertion named just some columns may write
//!   `"output_subset": true`, as `compares` is for issue rows. Its header
//!   names those columns in the table's order, and it still states every
//!   row in order. Without that marker, the whole table must match,
//!   including its complete header; a new case always states it whole;
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
//!   nothing. A file the operation generated whose whole text no authored
//!   file holds, such as a composed Define-XML document, is instead
//!   `{"select": {path: [text, ...]}}` when the case's source compared parts
//!   of it: each path selects nodes of the document, and its list holds the
//!   text of every node the path selects, each node whole and spelt as the
//!   file spells it, in document order, so `[]` states that the path selects
//!   none. A path of an `.xml` file is an absolute XPath location path of
//!   child steps, such as `/ODM/Study/MetaDataVersion/ItemDef[@OID='IT.DM.AGE']`,
//!   each step a name as the document spells it, prefix included, with at
//!   most one `[@name='value']` predicate, and its last step may be `@name`:
//!   an element's text runs from its start tag through its end tag, and an
//!   attribute's is its value between the quotes. A path of a `.json` file
//!   is a JSON Pointer (RFC 6901) such as `/columns/5/dataType`, matching a
//!   member by its decoded name, and a value's text is its
//!   JSON text, a string's with its quotes. `select` names at least one
//!   path, and a selected node's text is never matched by a substring of it.
//!
//! The application's `input` stages a study directory: its `files`, each its
//! text, a document of the fixture's `documents` (`base`) or an authored
//! file named by its path from the repository root (`file`), as `saved`
//! names one, the last two with `edits`; the files of the benchmark
//! `benchmark` names, each at its path there but its `expected/` directory,
//! `README.md` and `benchmark.yaml`, which the input's `edits` map to their
//! edits; and empty `directories`. An edit replaces text that occurs exactly
//! once, and a file of `files` is never one the benchmark stages. The
//! application's `define` composes a project's submission section and saves it,
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
pub const CASE: [&str; 11] = [
    "case",
    "operation",
    "input",
    "compares",
    "prefix",
    "among",
    "issues",
    "output",
    "output_subset",
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

/// What a case states of one file its `saved` names.
#[derive(Clone, Debug, PartialEq)]
pub enum Expected {
    /// The file's bytes, whole.
    Bytes(Vec<u8>),
    /// Nodes of the document the file holds: each path, and the text of
    /// every node it selects, in document order.
    Select(Vec<Selection>),
    /// The file was removed.
    Removed,
}

/// One path of a `select`, and the text of every node it selects.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    /// The path as the case writes it, for a failure message.
    path: String,
    to: Selector,
    texts: Vec<String>,
}

/// A path read for the format of the file it selects from.
#[derive(Clone, Debug, PartialEq)]
enum Selector {
    /// An XML document's child steps from its document element, and the
    /// attribute the last one names, if it names one.
    Xml(Vec<Step>, Option<String>),
    /// A JSON Pointer's reference tokens, unescaped.
    Json(Vec<String>),
}

/// A child step of an XML path: the element's name, and the attribute and
/// value its predicate requires, if it has one.
#[derive(Clone, Debug, PartialEq)]
struct Step {
    name: String,
    predicate: Option<(String, String)>,
}

impl Expected {
    /// Compare what the case states of the file `name` with its `bytes`
    /// now, or `None` once removed; `label` names the case in a failure.
    pub fn compare(&self, name: &str, bytes: Option<&[u8]>, label: &str) {
        match (self, bytes) {
            (Self::Removed, None) => {}
            (Self::Bytes(stated), Some(bytes)) => assert!(
                stated == bytes,
                "{label}: {name} holds\n{:?}\nnot\n{:?}",
                String::from_utf8_lossy(bytes),
                String::from_utf8_lossy(stated)
            ),
            (Self::Select(selections), Some(bytes)) => {
                let label = format!("{label}: {name}");
                let text = std::str::from_utf8(bytes)
                    .unwrap_or_else(|error| panic!("{label} is not UTF-8 text: {error}"));
                let xml = selections
                    .iter()
                    .any(|selection| matches!(selection.to, Selector::Xml(..)))
                    .then(|| Xml::document(text, &label));
                for selection in selections {
                    let selected = match &selection.to {
                        Selector::Xml(steps, attribute) => {
                            let root = xml.as_ref().expect("an XML document");
                            root.select(steps, attribute.as_deref())
                        }
                        Selector::Json(tokens) => JsonText::pointed(text, tokens, &label)
                            .into_iter()
                            .collect(),
                    };
                    assert!(
                        selected == selection.texts,
                        "{label}: {} selects\n{}\nnot\n{}",
                        selection.path,
                        texts(&selected),
                        texts(&selection.texts)
                    );
                }
            }
            (Self::Removed, Some(bytes)) => panic!(
                "{label}: {name} holds\n{:?}\nand is not removed",
                String::from_utf8_lossy(bytes)
            ),
            (_, None) => panic!("{label}: {name} was removed"),
        }
    }
}

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
    /// The output table's rows, header first, each with the columns the
    /// header names.
    pub output: Option<Vec<Value>>,
    /// Only a migrated partial assertion may opt into selected output columns.
    pub output_subset: bool,
    /// The report rows of each kind the case names, each kind's header first.
    pub report: Option<BTreeMap<String, Vec<Value>>>,
    /// What the operation did to the study directory: each file it created,
    /// changed or removed, and what the case states of it.
    pub saved: Option<BTreeMap<String, Expected>>,
}

impl Stated {
    /// What `case` states beside its issues, each member read as the form
    /// writes it; `label` names the case in a failure.
    pub fn read(case: &Value, label: &str) -> Self {
        let output_subset = match case.get("output_subset") {
            None => false,
            Some(Value::Bool(true)) => {
                assert!(
                    case.get("output").is_some(),
                    "{label}: output_subset requires an output table"
                );
                true
            }
            Some(other) => panic!("{label}: output_subset is written only as true, not {other:?}"),
        };
        Self {
            output: case.get("output").map(|rows| table(rows, "output", label)),
            output_subset,
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
            let projected = self.output_subset.then(|| {
                let columns = positions(output, &stated[0], "output", label);
                output
                    .iter()
                    .map(|row| {
                        let cells = row.as_array().expect("a row");
                        Value::Array(columns.iter().map(|&at| cells[at].clone()).collect())
                    })
                    .collect::<Vec<_>>()
            });
            let output = projected.as_ref().unwrap_or(output);
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
            (Some(stated), Some(saved)) => {
                assert!(
                    stated.keys().eq(saved.keys()),
                    "{label}: {operation} created, changed or removed\n{}\nnot\n{}",
                    listing(saved),
                    stating(stated)
                );
                for (name, expected) in stated {
                    expected.compare(
                        name,
                        saved[name].as_deref(),
                        &format!("{label}: {operation}"),
                    );
                }
            }
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
    let columns = positions(report, header, &format!("report.{kind}"), label);
    let names = report[0].as_array().expect("a header");
    let at = names.iter().position(|column| column == "KIND");
    let at = at.expect("a run report has a KIND column");
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

/// The position in the reported `table` of each column the stated table
/// `member` names in its `header`, which must be columns of that table, named
/// in its order.
fn positions(table: &[Value], header: &Value, member: &str, label: &str) -> Vec<usize> {
    let names = table[0].as_array().expect("a header");
    let stated = header.as_array().expect("a header").iter();
    let stated = stated.filter_map(Value::as_str).collect::<Vec<_>>();
    let columns = stated
        .iter()
        .map(|name| {
            let at = names.iter().position(|column| column == name);
            at.unwrap_or_else(|| panic!("{label}: {member}'s table has no column {name:?}"))
        })
        .collect::<Vec<_>>();
    assert!(
        columns.windows(2).all(|pair| pair[0] < pair[1]),
        "{label}: {member} names its columns in its table's order: {stated:?}"
    );
    columns
}

/// The files `saved` states: each file created or changed, with its text,
/// the bytes of the authored file it names or the nodes it selects, and each
/// file removed, written `null`.
fn written(saved: &Value, label: &str) -> BTreeMap<String, Expected> {
    let files = saved.as_object().unwrap_or_else(|| {
        panic!("{label}: saved maps each file to its bytes or null, not {saved:?}")
    });
    files
        .iter()
        .map(|(name, bytes)| {
            relative(name, label);
            let expected = match bytes {
                Value::Null => Expected::Removed,
                Value::String(text) => Expected::Bytes(text.clone().into_bytes()),
                Value::Object(members) if members.len() == 1 && members.contains_key("file") => {
                    let path = members["file"].as_str().unwrap_or_else(|| {
                        panic!("{label}: {name} names its expected file by its path: {bytes:?}")
                    });
                    Expected::Bytes(authored(path, &format!("{label}: {name}")))
                }
                Value::Object(members) if members.len() == 1 && members.contains_key("select") => {
                    Expected::Select(selections(name, &members["select"], label))
                }
                other => panic!(
                    "{label}: {name} is its text, {{\"file\": path}}, {{\"select\": paths}} \
                     or null, not {other:?}"
                ),
            };
            (name.clone(), expected)
        })
        .collect()
}

/// The paths the `select` of the file `name` writes, each read for the
/// file's format, which its extension names, with the texts it selects.
fn selections(name: &str, select: &Value, label: &str) -> Vec<Selection> {
    let label = format!("{label}: {name}");
    let xml = match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("xml") => true,
        Some("json") => false,
        _ => panic!("{label}: select reads nodes of an .xml or a .json file"),
    };
    let paths = select
        .as_object()
        .filter(|paths| !paths.is_empty())
        .unwrap_or_else(|| {
            panic!(
                "{label}: select maps at least one path to the texts it selects, since {{}} \
                 states nothing: {select:?}"
            )
        });
    paths
        .iter()
        .map(|(path, texts)| {
            let texts = texts
                .as_array()
                .unwrap_or_else(|| panic!("{label}: {path} selects a list of texts: {texts:?}"))
                .iter()
                .map(|text| {
                    let text = text.as_str();
                    text.unwrap_or_else(|| panic!("{label}: {path} selects texts: {texts:?}"))
                        .to_owned()
                })
                .collect();
            let to = if xml {
                xml_path(path, &label)
            } else {
                Selector::Json(json_pointer(path, &label))
            };
            Selection {
                path: path.clone(),
                to,
                texts,
            }
        })
        .collect()
}

/// The XML path `written`: `/`, then child steps separated by `/`, each a
/// name with at most one `[@name='value']` predicate, the last of which may
/// be `@name`.
fn xml_path(written: &str, label: &str) -> Selector {
    let malformed = || -> String {
        format!(
            "{label}: {written:?} is an absolute path of child steps, each a name or \
             name[@name='value'], the last of which may be @name"
        )
    };
    let mut rest = written
        .strip_prefix('/')
        .unwrap_or_else(|| panic!("{}", malformed()));
    let mut steps = Vec::new();
    loop {
        if let Some(attribute) = rest.strip_prefix('@') {
            assert!(!steps.is_empty() && name(attribute), "{}", malformed());
            return Selector::Xml(steps, Some(attribute.to_owned()));
        }
        let end = rest.find(['/', '[']).unwrap_or(rest.len());
        let (step, tail) = rest.split_at(end);
        assert!(name(step), "{}", malformed());
        rest = tail;
        let predicate = match rest.strip_prefix("[@") {
            None => None,
            Some(tail) => {
                let (attribute, tail) = tail
                    .split_once("='")
                    .unwrap_or_else(|| panic!("{}", malformed()));
                let (value, tail) = tail
                    .split_once("']")
                    .unwrap_or_else(|| panic!("{}", malformed()));
                assert!(name(attribute) && !value.contains('\''), "{}", malformed());
                rest = tail;
                Some((attribute.to_owned(), value.to_owned()))
            }
        };
        steps.push(Step {
            name: step.to_owned(),
            predicate,
        });
        if rest.is_empty() {
            return Selector::Xml(steps, None);
        }
        rest = rest
            .strip_prefix('/')
            .unwrap_or_else(|| panic!("{}", malformed()));
    }
}

/// `text` is an XML qualified name, with at most one namespace prefix.
/// XPath operators and node tests are not literal element or attribute names.
fn name(text: &str) -> bool {
    let start = |c| {
        matches!(c, 'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}'
            | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}'
            | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
            | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}'
            | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}'
            | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
    };
    let local = |text: &str| {
        let mut chars = text.chars();
        chars.next().is_some_and(start)
            && chars.all(|c| {
                start(c)
                    || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}'
                        | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
            })
    };
    let mut parts = text.split(':');
    local(parts.next().expect("at least one part"))
        && parts.next().is_none_or(local)
        && parts.next().is_none()
}

/// The reference tokens of the JSON Pointer `written` (RFC 6901), each with
/// `~1` read as `/` and `~0` as `~`.
fn json_pointer(written: &str, label: &str) -> Vec<String> {
    let tokens = written.strip_prefix('/').unwrap_or_else(|| {
        panic!("{label}: {written:?} is a JSON Pointer to a part of the file, so starts with /")
    });
    tokens
        .split('/')
        .map(|token| {
            assert!(
                !token.replace("~0", "").replace("~1", "").contains('~'),
                "{label}: {written:?} writes ~ as ~0 and / as ~1"
            );
            token.replace("~1", "/").replace("~0", "~")
        })
        .collect()
}

/// An element of an XML document: its name and its attributes, each spelt
/// as the document spells it, its whole text from its start tag through its
/// end tag, and its child elements.
struct Element<'a> {
    name: &'a str,
    attributes: Vec<(&'a str, &'a str)>,
    text: &'a str,
    children: Vec<Element<'a>>,
}

impl<'a> Element<'a> {
    /// The text of every node the child `steps` select from this document
    /// element, or of the `attribute` the last one names, in document order.
    fn select(&self, steps: &[Step], attribute: Option<&str>) -> Vec<String> {
        let matches = |step: &Step, element: &Element| {
            element.name == step.name
                && step
                    .predicate
                    .as_ref()
                    .is_none_or(|(name, value)| element.attribute(name) == Some(value.as_str()))
        };
        let (first, steps) = steps.split_first().expect("a step");
        let mut selected = vec![self];
        selected.retain(|element| matches(first, element));
        for step in steps {
            selected = selected
                .into_iter()
                .flat_map(|element| element.children.iter())
                .filter(|child| matches(step, child))
                .collect();
        }
        selected
            .into_iter()
            .filter_map(|element| match attribute {
                None => Some(element.text),
                Some(name) => element.attribute(name),
            })
            .map(str::to_owned)
            .collect()
    }

    /// The value of the attribute `name`, as the document spells it.
    fn attribute(&self, name: &str) -> Option<&'a str> {
        let mut values = self
            .attributes
            .iter()
            .filter(|(written, _)| *written == name);
        values.next().map(|(_, value)| *value)
    }
}

/// A reader of an XML document's elements, which leaves each entity
/// reference as written and skips comments, processing instructions and
/// CDATA sections. A malformed tag, or an end tag that does not close the
/// element it ends, fails the case at the byte it starts.
struct Xml<'a> {
    text: &'a str,
    at: usize,
    label: &'a str,
}

impl<'a> Xml<'a> {
    /// The document element of the XML document `text`.
    fn document(text: &'a str, label: &'a str) -> Element<'a> {
        let mut xml = Self { text, at: 0, label };
        xml.between();
        let root = xml.element();
        xml.between();
        if xml.at != text.len() {
            xml.fail("text after the document element");
        }
        root
    }

    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn fail(&self, expected: &str) -> ! {
        panic!("{}: {expected} at byte {}", self.label, self.at)
    }

    fn space(&mut self) {
        self.at = self.text.len() - self.rest().trim_start().len();
    }

    fn expect(&mut self, token: &str) {
        if !self.rest().starts_with(token) {
            self.fail(&format!("{token:?}"));
        }
        self.at += token.len();
    }

    /// Skip what runs from `open` through `close` when it starts here.
    fn skip(&mut self, open: &str, close: &str) -> bool {
        if !self.rest().starts_with(open) {
            return false;
        }
        let end = self.rest().find(close);
        self.at += end.unwrap_or_else(|| self.fail(&format!("{close:?}"))) + close.len();
        true
    }

    /// Skip the space, comments and processing instructions outside the
    /// document element.
    fn between(&mut self) {
        self.space();
        while self.skip("<?", "?>") || self.skip("<!--", "-->") {
            self.space();
        }
    }

    fn name(&mut self) -> &'a str {
        let rest = self.rest();
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '/' | '>' | '='))
            .unwrap_or(rest.len());
        if end == 0 {
            self.fail("a name");
        }
        self.at += end;
        &rest[..end]
    }

    fn element(&mut self) -> Element<'a> {
        let start = self.at;
        self.expect("<");
        let name = self.name();
        let mut attributes = Vec::new();
        loop {
            self.space();
            if self.rest().starts_with("/>") {
                self.at += 2;
                let text = &self.text[start..self.at];
                let children = Vec::new();
                return Element {
                    name,
                    attributes,
                    text,
                    children,
                };
            }
            if self.rest().starts_with('>') {
                self.at += 1;
                break;
            }
            let attribute = self.name();
            self.space();
            self.expect("=");
            self.space();
            let quote = match self.rest().chars().next() {
                Some(quote @ ('"' | '\'')) => quote,
                _ => self.fail("a quoted attribute value"),
            };
            self.at += 1;
            let end = self.rest().find(quote);
            let end = end.unwrap_or_else(|| self.fail("the closing quote"));
            attributes.push((attribute, &self.rest()[..end]));
            self.at += end + 1;
        }
        let mut children = Vec::new();
        loop {
            let next = self.rest().find('<');
            self.at += next.unwrap_or_else(|| self.fail(&format!("the end tag of {name}")));
            if self.rest().starts_with("</") {
                self.at += 2;
                if self.name() != name {
                    self.fail(&format!("the end tag of {name}"));
                }
                self.space();
                self.expect(">");
                let text = &self.text[start..self.at];
                return Element {
                    name,
                    attributes,
                    text,
                    children,
                };
            }
            let skipped =
                self.skip("<!--", "-->") || self.skip("<![CDATA[", "]]>") || self.skip("<?", "?>");
            if !skipped {
                children.push(self.element());
            }
        }
    }
}

/// A reader of the text of JSON values, which it reads without decoding.
struct JsonText<'a> {
    text: &'a str,
    at: usize,
    label: &'a str,
}

impl<'a> JsonText<'a> {
    /// The text of the value of the JSON document `text` that `tokens`
    /// point to, if there is one.
    fn pointed(text: &'a str, tokens: &[String], label: &'a str) -> Option<String> {
        let mut json = Self { text, at: 0, label };
        json.space();
        let pointed = json.value(Some(tokens));
        json.space();
        if json.at != text.len() {
            json.fail("text after the value");
        }
        pointed.map(str::to_owned)
    }

    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn fail(&self, expected: &str) -> ! {
        panic!("{}: {expected} at byte {}", self.label, self.at)
    }

    fn space(&mut self) {
        self.at = self.text.len() - self.rest().trim_start().len();
    }

    /// Whether `token` follows, read past it when it does.
    fn eat(&mut self, token: char) -> bool {
        let follows = self.rest().starts_with(token);
        if follows {
            self.at += 1;
        }
        follows
    }

    fn expect(&mut self, token: char) {
        if !self.eat(token) {
            self.fail(&format!("{token:?}"));
        }
    }

    /// Read the value that starts here, and return the text of the value
    /// that `pointer` points to within it, if it has one; a value off the
    /// pointer's way is read past.
    fn value(&mut self, pointer: Option<&[String]>) -> Option<&'a str> {
        let start = self.at;
        let next = pointer.and_then(<[String]>::split_first);
        let mut pointed = None;
        match self.rest().chars().next() {
            Some('{') => {
                self.at += 1;
                self.space();
                if !self.eat('}') {
                    loop {
                        self.space();
                        let member = super::json_string(self.string(), self.label);
                        self.space();
                        self.expect(':');
                        self.space();
                        let inner = next.filter(|(token, _)| **token == member);
                        let found = self.value(inner.map(|(_, rest)| rest));
                        pointed = pointed.or(found);
                        self.space();
                        if self.eat('}') {
                            break;
                        }
                        self.expect(',');
                    }
                }
            }
            Some('[') => {
                self.at += 1;
                self.space();
                if !self.eat(']') {
                    for index in 0.. {
                        self.space();
                        let inner = next.filter(|(token, _)| **token == index.to_string());
                        let found = self.value(inner.map(|(_, rest)| rest));
                        pointed = pointed.or(found);
                        self.space();
                        if self.eat(']') {
                            break;
                        }
                        self.expect(',');
                    }
                }
            }
            Some('"') => {
                self.string();
            }
            Some(_) => {
                let rest = self.rest();
                let end = rest
                    .find(|c: char| matches!(c, ',' | '}' | ']') || c.is_whitespace())
                    .unwrap_or(rest.len());
                if end == 0 {
                    self.fail("a value");
                }
                self.at += end;
            }
            None => self.fail("a value"),
        }
        match pointer {
            Some([]) => Some(&self.text[start..self.at]),
            _ => pointed,
        }
    }

    /// Read the string that starts here, and return its text including its
    /// quotes, escapes as written.
    fn string(&mut self) -> &'a str {
        let start = self.at;
        self.expect('"');
        loop {
            match self.text.as_bytes().get(self.at) {
                Some(b'"') => break,
                Some(b'\\') => self.at += 2,
                Some(_) => self.at += 1,
                None => self.fail("the closing quote"),
            }
        }
        self.at += 1;
        &self.text[start..self.at]
    }
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

/// Each file a case states and what it states of it, as `listing` writes
/// them, its selections by path, for a failure message.
fn stating(files: &BTreeMap<String, Expected>) -> String {
    if files.is_empty() {
        return "nothing".to_owned();
    }
    files
        .iter()
        .map(|(name, expected)| match expected {
            Expected::Bytes(bytes) => format!("{name}: {:?}", String::from_utf8_lossy(bytes)),
            Expected::Removed => format!("{name}: null"),
            Expected::Select(selections) => {
                let paths = selections.iter().map(|selection| selection.path.as_str());
                format!("{name}: select {}", paths.collect::<Vec<_>>().join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Each selected text on its own line, quoted, for a failure message;
/// `nothing` when no node was selected.
fn texts(texts: &[String]) -> String {
    if texts.is_empty() {
        return "nothing".to_owned();
    }
    let quoted = texts.iter().map(|text| format!("{text:?}"));
    quoted.collect::<Vec<_>>().join("\n")
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
