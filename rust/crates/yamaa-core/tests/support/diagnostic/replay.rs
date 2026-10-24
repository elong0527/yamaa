//! The replay of the expected diagnostics written as data (#3691).
//!
//! Each file of `tests/fixtures/diagnostics/` holds the cases of one family,
//! and the reader module that owns the file declares it. A case names the
//! behaviour it proves (`case`), the core operation that reaches it
//! (`operation`, read by [`super::operations`]; a list names several that
//! must each report the same issues), that operation's `input`, and the
//! complete list of `issues` it reports, in order. An issue row is the record
//! a run report renders: `phase`, `condition`, `rule` (`null` when the cause
//! cites no card), `spec_paths` and `context`, and `source_span` and
//! `operand_route` only when the issue carries them, so a row without them
//! asserts their absence.
//!
//! A case whose table compared only part of each record says which fields in
//! `compares`, and its rows hold those alone; nothing a table did not compare
//! is filled in. A case whose table found its record by cause among several
//! issues says `"among": true`: its rows are some of the issues reported,
//! each equal to a distinct one after the `compares` projection, in any
//! order, since the issues the table left unstated give its rows no authored
//! position; the replay renders and counts only the issues they match. An
//! input may take its `document` from one of the file's `documents`, by name
//! (`base`), with `edits` that set or remove a member at a path of keys and
//! list indexes.
//!
//! Every row was authored: in the Rust table it moved from, or from the rule
//! card, registry entry or sibling record its pull request names. No run
//! wrote one.
use super::{
    json::{self, Json},
    operations, rendered,
};
use std::{collections::BTreeSet, path::Path};
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    json::Value,
};

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

/// One authored case.
pub struct Case {
    /// The fixture file and the case's name, as a failure names it.
    pub label: String,
    pub operations: Vec<String>,
    pub input: Json,
    /// The fields the case compares, when its table compared fewer than all.
    pub compares: Option<Vec<String>>,
    /// Whether the rows are some of the issues reported, in any order.
    pub among: bool,
    pub issues: Vec<Value>,
}

/// Every case of every fixture file, by file name and then as written. A
/// case's name is unique across the files, so a rule card cites it as
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
        let fixture = json::read(file.text, name);
        let fixture = fixture.closed(&["about", "documents", "cases"]);
        for case in fixture.field("cases").items() {
            let case = case.closed(&["case", "operation", "input", "compares", "among", "issues"]);
            let case_name = case.field("case").text();
            assert!(
                names.insert(case_name.to_owned()),
                "{name}: the case {case_name} is named twice"
            );
            let label = format!("{name}::{case_name}");
            let operations = match case.field("operation") {
                Json::Array(items) => items.iter().map(|item| item.text().into()).collect(),
                operation => vec![operation.text().into()],
            };
            let compares = case.get("compares").map(|fields| {
                let fields = fields
                    .items()
                    .iter()
                    .map(|field| field.text().to_owned())
                    .collect::<Vec<_>>();
                assert!(
                    fields.iter().all(|field| FIELDS.contains(&field.as_str()))
                        && fields.iter().collect::<BTreeSet<_>>().len() == fields.len()
                        && fields.len() < FIELDS.len(),
                    "{label}: compares names some of the fields {FIELDS:?}, each once: {fields:?}"
                );
                fields
            });
            let issues = case.field("issues").items();
            let among = case.get("among").is_some_and(|among| {
                assert!(among.boolean(), "{label}: among is written only as true");
                assert!(
                    !issues.is_empty(),
                    "{label}: an among case names at least one row"
                );
                true
            });
            if let Some(fields) = &compares {
                for issue in issues {
                    for (field, _) in issue.members() {
                        assert!(
                            fields.contains(field),
                            "{label}: a row holds only the fields it compares, not {field:?}"
                        );
                    }
                }
            }
            cases.push(Case {
                input: resolved(fixture, case.field("input"), &label),
                label,
                operations,
                compares,
                among,
                issues: issues.iter().map(Json::value).collect(),
            });
        }
    }
    cases
}

/// `diagnostic` as the issue row a run report renders.
pub fn row(diagnostic: &Diagnostic) -> Value {
    let definition = diagnostic.definition();
    let mut row = yamaa_core::json::Map::from([
        ("phase".into(), Value::String(definition.phase.into())),
        (
            "condition".into(),
            Value::String(definition.condition.into()),
        ),
        (
            "rule".into(),
            definition
                .rule
                .map_or(Value::Null, |rule| Value::String(rule.into())),
        ),
        (
            "spec_paths".into(),
            Value::Array(
                diagnostic
                    .spec_paths
                    .iter()
                    .map(|path| Value::String(path.clone()))
                    .collect(),
            ),
        ),
        ("context".into(), Value::Object(diagnostic.context.clone())),
    ]);
    if let Some(span) = diagnostic.source_span {
        row.insert(
            "source_span".into(),
            Value::Object(
                [
                    ("start".into(), Value::from(span.start)),
                    ("end".into(), Value::from(span.end)),
                ]
                .into(),
            ),
        );
    }
    if let Some(route) = &diagnostic.operand_route {
        row.insert(
            "operand_route".into(),
            Value::Array(route.iter().cloned().map(Value::String).collect()),
        );
    }
    Value::Object(row)
}

/// Replay every case: the issues each of its operations reports must equal
/// its rows, or hold them when the case is `among`, each issue a row compares
/// renders its registered message from its own record (P-MSG-1), and the
/// causes the cases reach are returned.
pub fn replay() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for case in cases() {
        for operation in &case.operations {
            let label = match case.operations.len() {
                1 => case.label.clone(),
                _ => format!("{} ({operation})", case.label),
            };
            let _replaying = Naming(format!("while replaying {label}"));
            let issues = operations::run(operation, &case.input);
            let rows = issues
                .iter()
                .map(|issue| compared(row(issue), case.compares.as_deref()))
                .collect::<Vec<_>>();
            let checked = if case.among {
                match among(&case.issues, &rows) {
                    Ok(matched) => matched.into_iter().map(|index| &issues[index]).collect(),
                    Err(unmatched) => panic!(
                        "{label}: the rows {unmatched:?} are not among the reported rows {rows:?}"
                    ),
                }
            } else {
                assert_eq!(
                    Value::Array(rows),
                    Value::Array(case.issues.clone()),
                    "{label}"
                );
                issues.iter().collect::<Vec<_>>()
            };
            reached.extend(checked.into_iter().map(rendered));
        }
    }
    reached
}

/// Match each `authored` row of an `among` case to a distinct `reported`
/// row equal to it, whatever their order: the indexes of the reported rows
/// matched, in the authored order, or the authored rows that match none.
pub fn among(authored: &[Value], reported: &[Value]) -> Result<Vec<usize>, Vec<Value>> {
    let mut taken = vec![false; reported.len()];
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    for row in authored {
        match (0..reported.len()).find(|&index| !taken[index] && reported[index] == *row) {
            Some(index) => {
                taken[index] = true;
                matched.push(index);
            }
            None => unmatched.push(row.clone()),
        }
    }
    if unmatched.is_empty() {
        Ok(matched)
    } else {
        Err(unmatched)
    }
}

/// `row` with only the fields `compares` names, or whole.
fn compared(row: Value, compares: Option<&[String]>) -> Value {
    match (row, compares) {
        (Value::Object(fields), Some(compares)) => Value::Object(
            fields
                .into_iter()
                .filter(|(field, _)| compares.contains(field))
                .collect(),
        ),
        (row, _) => row,
    }
}

/// Names the file being read or the case being replayed when a reader, a
/// comparison or a rendering panics, since their own messages do not know it.
struct Naming(String);

impl Drop for Naming {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("{}", self.0);
        }
    }
}

/// `input` with a `base` document of the fixture and its `edits` applied,
/// as the `document` its operation reads.
fn resolved(fixture: &Json, input: &Json, label: &str) -> Json {
    let Some(base) = input.get("base") else {
        assert!(
            input.get("edits").is_none(),
            "{label}: edits need a base document"
        );
        return input.clone();
    };
    let mut document = fixture
        .field("documents")
        .get(base.text())
        .unwrap_or_else(|| panic!("{label}: no document {base:?}"))
        .clone();
    for edit in input.get("edits").map_or(&[][..], Json::items) {
        let edit = edit.closed(&["set", "to", "remove"]);
        match (edit.get("set"), edit.get("to"), edit.get("remove")) {
            (Some(path), Some(value), None) => set(&mut document, &segments(path), value.clone()),
            (None, None, Some(path)) => remove(&mut document, &segments(path)),
            _ => panic!("{label}: an edit is {{set, to}} or {{remove}}: {edit:?}"),
        }
    }
    let mut members = vec![("document".to_owned(), document)];
    members.extend(
        input
            .members()
            .iter()
            .filter(|(name, _)| name != "base" && name != "edits")
            .cloned(),
    );
    Json::Object(members)
}

fn segments(path: &Json) -> Vec<&str> {
    path.items().iter().map(Json::text).collect()
}

/// The member or item at `path`.
fn at<'a>(mut json: &'a mut Json, path: &[&str]) -> &'a mut Json {
    for segment in path {
        json = match json {
            Json::Object(members) => {
                &mut members
                    .iter_mut()
                    .find(|(name, _)| name == segment)
                    .unwrap_or_else(|| panic!("no member {segment:?}"))
                    .1
            }
            Json::Array(items) => &mut items[segment.parse::<usize>().expect("an index")],
            other => panic!("{segment:?} reads into the scalar {other:?}"),
        };
    }
    json
}

/// Set the member or item at `path`, appending a member a mapping lacks.
fn set(json: &mut Json, path: &[&str], value: Json) {
    let (last, parent) = path.split_last().expect("a path");
    match at(json, parent) {
        Json::Object(members) => match members.iter_mut().find(|(name, _)| name == last) {
            Some((_, old)) => *old = value,
            None => members.push(((*last).into(), value)),
        },
        Json::Array(items) => items[last.parse::<usize>().expect("an index")] = value,
        other => panic!("{last:?} is set in the scalar {other:?}"),
    }
}

/// Remove the member or item at `path`, which must be there.
fn remove(json: &mut Json, path: &[&str]) {
    let (last, parent) = path.split_last().expect("a path");
    match at(json, parent) {
        Json::Object(members) => {
            let before = members.len();
            members.retain(|(name, _)| name != last);
            assert_ne!(before, members.len(), "no member {last:?} to remove");
        }
        Json::Array(items) => {
            items.remove(last.parse::<usize>().expect("an index"));
        }
        other => panic!("{last:?} is removed from the scalar {other:?}"),
    }
}
