//! The replay of the expected diagnostics written as data (#3691).
//!
//! Each file of `tests/fixtures/diagnostics/` holds the cases of one family,
//! and the reader module that owns the file declares it. A member name
//! written twice in one object is refused, as core's reader refuses it. A
//! case names the behaviour it proves (`case`), the operation that reaches it
//! (`operation`, read by [`super::operations`]; a list names several that
//! must each report the same complete rows, each study directory written
//! `{study}/`), that operation's `input`, and the issues it reports, in
//! order. An issue row is the record a run report renders: `phase`,
//! `condition`, `rule` (`null` when the cause cites no card), `spec_paths`
//! and `context`, and `source_span` and `operand_route` only when the issue
//! carries them, so a row without them asserts their absence.
//!
//! A case whose test compared only part of each record says which fields in
//! `compares`, and its rows hold those alone: a field, or one key of the
//! context written `context.<key>`, which every row's `context` then holds,
//! with no other key. A compared field that a row leaves out asserts its
//! absence; a case naming a key of the context does not name the context
//! whole. A case whose test read the first of several issues by position
//! says `prefix`: its rows are the first issues reported, in order, and the
//! issues after them are not compared. A case whose test found its issues by
//! their cause among others says `among`: each row equals a distinct
//! reported issue, in any order, and the issues no row names are not
//! compared. Nothing a test did not compare is filled in. These are the
//! `compares` and [`form::Order`] core's replay reads.
//!
//! An input's files are written to a fresh study directory: a file is its
//! text, a document of the fixture's `documents` taken by name (`base`), or
//! an authored file of the repository taken by its path from the repository
//! root (`file`), the last two with `edits`, each replacing text that occurs
//! exactly once. An input may also stage a benchmark's files (`benchmark`),
//! which [`super::operations::study`] copies with their `edits`. An issue
//! row's text that starts with `{study}/` names a file of that directory as
//! the engine spells its path.
//!
//! A positive case also states the output table's rows, the run report's
//! rows or the files its operation created, changed or removed, as
//! [`super::form`] reads them for both crates' replays, and each is
//! compared with what the operation reports.
//!
//! The cases run in parallel ([`form::in_parallel`]), each operation in a
//! study directory of its own.
//!
//! Every row was authored: in the test it moved from, or from the rule card,
//! registry entry or sibling record its pull request names. No run wrote one.
use super::{
    form::{self, Stated},
    operations::{self, Reported},
};
use std::{collections::BTreeSet, path::Path};
use yamaa_core::json::{Map, Value};

/// The fields of an issue row, in the order a run report writes them.
const FIELDS: [&str; 7] = [
    "phase",
    "condition",
    "rule",
    "spec_paths",
    "context",
    "source_span",
    "operand_route",
];

/// The prefix of a row's text that names a file of the study directory.
const STUDY: &str = "{study}/";

/// One authored case.
pub struct Case {
    /// The fixture file and the case's name, as a failure names it.
    pub label: String,
    pub operations: Vec<String>,
    pub input: Value,
    /// The fields the case compares, when its test compared fewer than all.
    pub compares: Option<Vec<String>>,
    /// How the rows stand to the issues reported.
    pub order: form::Order,
    pub issues: Vec<Value>,
    /// The tables and files the case states beside its issues.
    pub stated: Stated,
}

impl Case {
    /// The case's label, which names it in a failure.
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Every case of every fixture file, by file name and then as written. A
/// case's name is unique across the files, so a reader names it as
/// `<file>::<name>`.
pub fn cases() -> Vec<Case> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/diagnostics");
    let mut files = operations::files().collect::<Vec<_>>();
    files.sort_by_key(|file| file.name);
    let mut written = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    written.sort();
    assert_eq!(
        written,
        files.iter().map(|file| file.name).collect::<Vec<_>>(),
        "each file under {} is one a reader module declares",
        directory.display()
    );
    let mut cases = Vec::new();
    let mut names = BTreeSet::new();
    for file in files {
        let name = file.name;
        let _reading = Naming(format!("while reading {name}"));
        let fixture = yamaa_adapters::json_wire::decode(file.text)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        unique_members(file.text, name);
        closed(&fixture, &form::FIXTURE);
        let documents = fixture.get("documents");
        for case in items(field(&fixture, "cases")) {
            let case_name = text(field(case, "case"));
            assert!(
                names.insert(case_name.to_owned()),
                "{name}: the case {case_name} is named twice"
            );
            let label = format!("{name}::{case_name}");
            let _case = Naming(format!("while reading {label}"));
            closed(case, &form::CASE);
            let operations = match field(case, "operation") {
                Value::Array(items) => items.iter().map(|item| text(item).to_owned()).collect(),
                operation => vec![text(operation).to_owned()],
            };
            let compares = case
                .get("compares")
                .map(|fields| compared_fields(fields, &label));
            let issues = items(field(case, "issues"));
            let order = form::Order::read(case, issues.len(), &label);
            if let Some(fields) = &compares {
                for issue in issues {
                    held_alone(issue, fields, &label);
                }
            }
            cases.push(Case {
                input: resolved(documents, field(case, "input"), &label),
                stated: Stated::read(case, &label),
                label,
                operations,
                compares,
                order,
                issues: issues.to_vec(),
            });
        }
    }
    cases
}

/// Replay every case: the issues each of its operations reports must equal
/// its rows, begin with them for a `prefix`, or, for a case found `among`
/// them, include each of its rows, the tables and files the operation
/// reports must be the ones the case states, and the operations of a case
/// that names several must report the same complete rows.
pub fn replay() {
    form::in_parallel(&cases(), Case::label, |case| {
        let mut reported = Vec::new();
        for operation in &case.operations {
            let label = match case.operations.len() {
                1 => case.label.clone(),
                _ => format!("{} ({operation})", case.label),
            };
            let _replaying = Naming(format!("while replaying {label}"));
            let Reported {
                issues,
                study,
                outcome,
            } = operations::run(operation, &case.input);
            compare_issues(case, &issues, study.as_ref(), &label);
            case.stated.compare(&outcome, operation, &label);
            let actual = crate::support::rows(&issues);
            let complete = match &study {
                Some(study) => actual
                    .iter()
                    .map(|row| unexpanded(row, &study.identity("")))
                    .collect(),
                None => actual,
            };
            reported.push((operation.as_str(), complete));
        }
        form::same_issues(&reported, &case.label);
    });
}

/// Compare the issues of a call with the case's authored rows. API seams
/// reuse this projection and path expansion when comparing a first issue.
pub fn compare_issues(
    case: &Case,
    issues: &[yamaa_core::diagnostic::Diagnostic],
    study: Option<&crate::support::Scratch>,
    label: &str,
) {
    let actual = crate::support::rows(issues);
    let rendered = actual
        .iter()
        .map(yamaa_adapters::json_wire::encode)
        .collect::<Vec<_>>()
        .join("\n");
    let projected = actual
        .iter()
        .map(|row| compared(row, case.compares.as_deref()))
        .collect::<Vec<_>>();
    let expected = case
        .issues
        .iter()
        .map(|row| expanded(row, study, label))
        .collect::<Vec<_>>();
    if case.order == form::Order::Among {
        let mut unmatched = projected;
        for row in &expected {
            let Some(at) = unmatched.iter().position(|reported| reported == row) else {
                panic!(
                    "{label}: no reported issue is {}; the issues are\n{rendered}",
                    yamaa_adapters::json_wire::encode(row)
                )
            };
            unmatched.remove(at);
        }
    } else {
        let compared = case.order.compared(&projected, expected.len());
        assert_eq!(
            Value::Array(compared.to_vec()),
            Value::Array(expected),
            "{label}: the issues are\n{rendered}"
        );
    }
}

/// The entries of a `compares`: each a field, or a key of the context, once,
/// and not every field whole.
fn compared_fields(fields: &Value, label: &str) -> Vec<String> {
    let fields = items(fields)
        .iter()
        .map(|field| text(field).to_owned())
        .collect::<Vec<_>>();
    for field in &fields {
        let known = FIELDS.contains(&field.as_str())
            || field
                .strip_prefix("context.")
                .is_some_and(|key| !key.is_empty() && !key.contains('.'));
        assert!(
            known,
            "{label}: compares names a field of {FIELDS:?} or one key of the context, not {field:?}"
        );
    }
    assert_eq!(
        fields.iter().collect::<BTreeSet<_>>().len(),
        fields.len(),
        "{label}: compares names each field once: {fields:?}"
    );
    assert!(
        !(fields.iter().any(|field| field == "context")
            && fields.iter().any(|field| field.starts_with("context."))),
        "{label}: compares names the context whole or by key, not both: {fields:?}"
    );
    assert!(
        !FIELDS
            .iter()
            .all(|field| fields.iter().any(|named| named == field)),
        "{label}: a case that compares every field writes no compares"
    );
    fields
}

/// `issue` holds only the fields `fields` names, and its `context` exactly
/// the keys they name.
fn held_alone(issue: &Value, fields: &[String], label: &str) {
    let keys = fields
        .iter()
        .filter_map(|field| field.strip_prefix("context."))
        .collect::<BTreeSet<_>>();
    for field in object(issue).keys() {
        assert!(
            fields.iter().any(|named| named == field) || (field == "context" && !keys.is_empty()),
            "{label}: a row holds only the fields it compares, not {field:?}"
        );
    }
    if !keys.is_empty() {
        let written: BTreeSet<&str> = issue
            .get("context")
            .map(|context| object(context).keys().map(String::as_str).collect())
            .unwrap_or_default();
        assert_eq!(
            written, keys,
            "{label}: a row's context holds exactly the keys it compares"
        );
    }
}

/// `row` with only the fields and context keys `compares` names, or whole.
fn compared(row: &Value, compares: Option<&[String]>) -> Value {
    let Some(compares) = compares else {
        return row.clone();
    };
    let row = object(row);
    let mut projected = Map::new();
    let mut context = Map::new();
    for field in compares {
        match field.strip_prefix("context.") {
            Some(key) => {
                if let Some(value) = row.get("context").and_then(|context| context.get(key)) {
                    context.insert(key.to_owned(), value.clone());
                }
            }
            None => {
                if let Some(value) = row.get(field.as_str()) {
                    projected.insert(field.clone(), value.clone());
                }
            }
        }
    }
    if compares.iter().any(|field| field.starts_with("context.")) {
        projected.insert("context".into(), Value::Object(context));
    }
    Value::Object(projected)
}

/// `row` with each text that names a file of the study directory spelt as
/// the engine spells that file's path.
fn expanded(row: &Value, study: Option<&crate::support::Scratch>, label: &str) -> Value {
    match row {
        Value::String(written) => match written.strip_prefix(STUDY) {
            Some(name) => {
                let study =
                    study.unwrap_or_else(|| panic!("{label}: {written} names no study directory"));
                Value::String(study.identity(name))
            }
            None => row.clone(),
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| expanded(item, study, label))
                .collect(),
        ),
        Value::Object(members) => Value::Object(
            members
                .iter()
                .map(|(key, value)| (key.clone(), expanded(value, study, label)))
                .collect(),
        ),
        _ => row.clone(),
    }
}

/// `row` with the study `directory`, wherever a text names it, written
/// `{study}/` as an authored row writes it, so that the rows of two
/// operations, each run in a study directory of its own, compare.
fn unexpanded(row: &Value, directory: &str) -> Value {
    match row {
        Value::String(text) => Value::String(text.replace(directory, STUDY)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| unexpanded(item, directory))
                .collect(),
        ),
        Value::Object(members) => Value::Object(
            members
                .iter()
                .map(|(key, value)| (key.clone(), unexpanded(value, directory)))
                .collect(),
        ),
        _ => row.clone(),
    }
}

/// `input` with each of its `files` that is a document of the fixture or an
/// authored file of the repository written as its text.
fn resolved(documents: Option<&Value>, input: &Value, label: &str) -> Value {
    let Some(files) = input.get("files") else {
        return input.clone();
    };
    let files = object(files)
        .iter()
        .map(|(name, file)| {
            let file = match file {
                Value::String(_) => file.clone(),
                source => Value::String(edited(documents, source, &format!("{label}: {name}"))),
            };
            (name.clone(), file)
        })
        .collect::<Map<_, _>>();
    let mut input = object(input).clone();
    input.insert("files".into(), Value::Object(files));
    Value::Object(input)
}

/// The text of the document `base` names, or of the authored file `file`
/// names by its path from the repository root, with each of its `edits`
/// made.
fn edited(documents: Option<&Value>, source: &Value, label: &str) -> String {
    let members = closed(source, &["base", "file", "edits"]);
    let text = match (members.get("base"), members.get("file")) {
        (Some(base), None) => {
            let base = text(base);
            documents
                .and_then(|documents| documents.get(base))
                .map(|document| self::text(document).to_owned())
                .unwrap_or_else(|| panic!("{label}: no document {base:?}"))
        }
        (None, Some(path)) => String::from_utf8(form::authored(text(path), label))
            .unwrap_or_else(|_| panic!("{label}: {path:?} is not text")),
        _ => panic!("{label}: a file is its text, a document (base) or an authored file (file)"),
    };
    edit(text, source.get("edits").map_or(&[][..], items), label)
}

/// `text` with each of `edits` made in order, each replacing its `from`,
/// which occurs exactly once, with its `to`.
pub fn edit(mut text: String, edits: &[Value], label: &str) -> String {
    for edit in edits {
        closed(edit, &["from", "to"]);
        let (from, to) = (
            self::text(field(edit, "from")),
            self::text(field(edit, "to")),
        );
        assert_eq!(
            text.matches(from).count(),
            1,
            "{label}: an edit replaces text that occurs once: {from:?}"
        );
        text = text.replacen(from, to, 1);
    }
    text
}

/// Refuse a member name written twice in one object of the fixture `name`:
/// `json_wire` keeps the last value, so a repeated `issues` would replay as
/// whichever came last. A member name is a string followed by `:`; `json`
/// is JSON that `json_wire` has admitted.
fn unique_members(json: &str, name: &str) {
    let bytes = json.as_bytes();
    // One entry per open container: the names of an object, or none for an
    // array.
    let mut open: Vec<Option<BTreeSet<String>>> = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'{' => open.push(Some(BTreeSet::new())),
            b'[' => open.push(None),
            b'}' | b']' => {
                open.pop();
            }
            b'"' => {
                let start = at;
                at += 1;
                while bytes[at] != b'"' {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                let after = json[at + 1..].trim_start();
                if let (true, Some(Some(names))) = (after.starts_with(':'), open.last_mut()) {
                    let member = yamaa_adapters::json_wire::decode(&json[start..=at]).unwrap();
                    let member = text(&member);
                    assert!(
                        names.insert(member.to_owned()),
                        "{name}: the member {member:?} is written twice in one object, at byte {start}"
                    );
                }
            }
            _ => {}
        }
        at += 1;
    }
}

/// Names the file being read or the case being replayed when a reader, a
/// comparison or an operation panics, since their own messages do not know
/// it.
struct Naming(String);

impl Drop for Naming {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("{}", self.0);
        }
    }
}

/// This object, whose members must all be among `members`, so a misspelt or
/// unknown member fails rather than being ignored.
#[track_caller]
pub fn closed<'a>(value: &'a Value, members: &[&str]) -> &'a Map<String, Value> {
    let object = object(value);
    for name in object.keys() {
        assert!(
            members.contains(&name.as_str()),
            "unknown member {name:?}; the members are {members:?}"
        );
    }
    object
}

/// The member `key` of an object, which it must have.
#[track_caller]
pub fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    object(value)
        .get(key)
        .unwrap_or_else(|| panic!("no {key:?} member in {value:?}"))
}

#[track_caller]
pub fn object(value: &Value) -> &Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("expected an object, not {value:?}"))
}

#[track_caller]
pub fn items(value: &Value) -> &[Value] {
    value
        .as_array()
        .unwrap_or_else(|| panic!("expected an array, not {value:?}"))
}

#[track_caller]
pub fn text(value: &Value) -> &str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("expected a string, not {value:?}"))
}
