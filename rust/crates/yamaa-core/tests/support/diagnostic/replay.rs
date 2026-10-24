//! The replay of the expected diagnostics written as data (#3691).
//!
//! Each file of `tests/fixtures/diagnostics/` holds the cases of one family,
//! and the reader module that owns the file declares it. A case names the
//! behaviour it proves (`case`), the core operation that reaches it
//! (`operation`, read by [`super::operations`]; a list names several that
//! must each report the same complete rows), that operation's `input`, and the
//! complete list of `issues` it reports, in order. An issue row is the record
//! a run report renders: `phase`, `condition`, `rule` (`null` when the cause
//! cites no card), `spec_paths` and `context`, and `source_span` and
//! `operand_route` only when the issue carries them, so a row without them
//! asserts their absence.
//!
//! A case whose table compared only part of each record says which fields in
//! `compares`, and its rows hold those alone; nothing a table did not compare
//! is filled in. A case whose table compared some keys of the context, not
//! the whole of it, names each as `context.<key>` in place of `context`:
//! each row's context holds exactly those keys, and each reported context is
//! compared at them alone. A case whose table read the first of several
//! issues by position says `"prefix": true`: its rows are the first issues
//! reported, in order, after the `compares` projection, and the issues after
//! them are not compared. A case whose table found its record by cause
//! among several issues says `"among": true`: its rows are some of the
//! issues reported, each equal to a distinct one after the `compares`
//! projection, in any order, since the issues the table left unstated give
//! its rows no authored position. For either, the replay renders and counts
//! only the issues the rows match ([`form::Order`]). An input may take its
//! `document` from one of the file's `documents`, by name (`base`), with
//! `edits` that set or remove a member at a path of keys and list indexes.
//!
//! A positive case also states the output table's rows, the run report's
//! rows or the files a save created, changed or removed, as [`super::form`]
//! reads them for both crates' replays. Each core operation reports its
//! issues alone, so a core case that states one of them fails, naming the
//! operation that reports no such thing.
//!
//! Every row was authored: in the Rust table it moved from, or from the rule
//! card, registry entry or sibling record its pull request names. No run
//! wrote one.
use super::{
    form::{self, Outcome, Stated},
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
    /// The fields the case compares, when its table compared fewer than all,
    /// each a field or a `context.<key>`.
    pub compares: Option<Vec<String>>,
    /// How the rows stand to the issues reported.
    pub order: form::Order,
    pub issues: Vec<Value>,
    /// The tables and files the case states beside its issues.
    pub stated: Stated,
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
        let fixture = fixture.closed(&form::FIXTURE);
        for case in fixture.field("cases").items() {
            let case_name = case.field("case").text();
            assert!(
                names.insert(case_name.to_owned()),
                "{name}: the case {case_name} is named twice"
            );
            let label = format!("{name}::{case_name}");
            let _case = Naming(format!("while reading {label}"));
            let case = case.closed(&form::CASE);
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
                let keys = context_keys(&fields);
                let whole = fields
                    .iter()
                    .filter(|field| !field.starts_with("context."))
                    .collect::<Vec<_>>();
                assert!(
                    whole.iter().all(|field| FIELDS.contains(&field.as_str()))
                        && keys.iter().all(|key| !key.is_empty() && !key.contains('.'))
                        && fields.iter().collect::<BTreeSet<_>>().len() == fields.len()
                        && whole.len() < FIELDS.len()
                        && (keys.is_empty() || !whole.iter().any(|field| *field == "context")),
                    "{label}: compares names some of the fields {FIELDS:?}, each once, \
                     or some keys of the context as `context.<key>` in place of \
                     `context`: {fields:?}"
                );
                fields
            });
            let issues = case.field("issues").items();
            let value = case.value();
            let order = form::Order::read(&value, issues.len(), &label);
            if let Some(fields) = &compares {
                let keys = context_keys(fields);
                for issue in issues {
                    for (field, _) in issue.members() {
                        assert!(
                            fields.contains(field) || (field == "context" && !keys.is_empty()),
                            "{label}: a row holds only the fields it compares, not {field:?}"
                        );
                    }
                    if !keys.is_empty() {
                        let written = issue
                            .get("context")
                            .unwrap_or_else(|| {
                                panic!("{label}: a row holds the context keys it compares")
                            })
                            .members()
                            .iter()
                            .map(|(key, _)| key.as_str())
                            .collect::<BTreeSet<_>>();
                        assert_eq!(
                            written,
                            keys.iter().copied().collect::<BTreeSet<_>>(),
                            "{label}: a row's context holds exactly the keys it compares"
                        );
                    }
                }
            }
            cases.push(Case {
                input: resolved(fixture, case.field("input"), &label),
                stated: Stated::read(&value, &label),
                label,
                operations,
                compares,
                order,
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
/// its rows, begin with them for a `prefix`, or hold them when the case is
/// `among`, the operations of a case that names several must report the
/// same complete rows, each issue a row compares renders its registered
/// message from its own record (P-MSG-1), and the causes the cases reach are
/// returned.
pub fn replay() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for case in cases() {
        let mut reported = Vec::new();
        for operation in &case.operations {
            let label = match case.operations.len() {
                1 => case.label.clone(),
                _ => format!("{} ({operation})", case.label),
            };
            let _replaying = Naming(format!("while replaying {label}"));
            let issues = operations::run(operation, &case.input);
            let complete = issues.iter().map(row).collect::<Vec<_>>();
            let rows = complete
                .iter()
                .map(|row| compared(row.clone(), case.compares.as_deref()))
                .collect::<Vec<_>>();
            let checked = if case.order == form::Order::Among {
                match among(&case.issues, &rows) {
                    Ok(matched) => matched.into_iter().map(|index| &issues[index]).collect(),
                    Err(unmatched) => panic!(
                        "{label}: the rows {unmatched:?} are not among the reported rows {rows:?}"
                    ),
                }
            } else {
                let compared = case.order.compared(&rows, case.issues.len());
                assert_eq!(
                    Value::Array(compared.to_vec()),
                    Value::Array(case.issues.clone()),
                    "{label}"
                );
                issues[..compared.len()].iter().collect::<Vec<_>>()
            };
            reached.extend(checked.into_iter().map(rendered));
            // A core operation builds no table and saves no file.
            case.stated.compare(&Outcome::default(), operation, &label);
            reported.push((operation.as_str(), complete));
        }
        form::same_issues(&reported, &case.label);
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

/// `row` with only the fields `compares` names, its context with only the
/// keys a `context.<key>` names, or whole.
pub fn compared(row: Value, compares: Option<&[String]>) -> Value {
    match (row, compares) {
        (Value::Object(fields), Some(compares)) => {
            let keys = context_keys(compares);
            Value::Object(
                fields
                    .into_iter()
                    .filter_map(|(field, value)| match value {
                        Value::Object(context) if field == "context" && !keys.is_empty() => {
                            let context = context
                                .into_iter()
                                .filter(|(key, _)| keys.contains(&key.as_str()))
                                .collect();
                            Some((field, Value::Object(context)))
                        }
                        value => compares.contains(&field).then_some((field, value)),
                    })
                    .collect(),
            )
        }
        (row, _) => row,
    }
}

/// The context keys `compares` names as `context.<key>`.
fn context_keys(compares: &[String]) -> Vec<&str> {
    compares
        .iter()
        .filter_map(|field| field.strip_prefix("context."))
        .collect()
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
