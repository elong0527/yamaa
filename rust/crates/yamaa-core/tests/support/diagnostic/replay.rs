//! The replay of the expected diagnostics written as data (#3691).
//!
//! Each file of `tests/fixtures/diagnostics/` holds the cases of one family.
//! A case names the behaviour it proves (`case`), the core operation that
//! reaches it (`operation`, read by [`super::operations`]), that operation's
//! `input`, and the complete list of `issues` it reports, in order. An issue
//! row is the record a run report renders: `phase`, `condition`, `rule`
//! (`null` when the cause cites no card), `spec_paths` and `context`, and
//! `source_span` and `operand_route` only when the issue carries them, so a
//! row without them asserts their absence. Every row was authored: in the
//! Rust table it moved from, or from the rule card, registry entry or
//! sibling record its pull request names. No run wrote one.
use super::{
    json::{self, Json},
    operations, rendered,
};
use std::{collections::BTreeSet, path::Path};
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    json::Value,
};

/// One authored case.
pub struct Case {
    /// The fixture file and the case's name, as a failure names it.
    pub label: String,
    pub operation: String,
    pub input: Json,
    pub issues: Vec<Value>,
}

/// Every case of every fixture file, by file name and then as written. A
/// case's name is unique across the files, so a rule card cites it as
/// `<file>::<name>`.
pub fn cases() -> Vec<Case> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/diagnostics");
    let mut files = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    files.sort();
    let mut cases = Vec::new();
    let mut names = BTreeSet::new();
    for path in files {
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        let text = std::fs::read_to_string(&path).unwrap();
        let file = json::read(&text, &name);
        for (key, _) in file.members() {
            assert!(
                ["about", "cases"].contains(&key.as_str()),
                "{name}: unknown member {key:?}"
            );
        }
        for case in file.field("cases").items() {
            for (key, _) in case.members() {
                assert!(
                    ["case", "operation", "input", "issues"].contains(&key.as_str()),
                    "{name}: unknown case member {key:?}"
                );
            }
            let case_name = case.field("case").text();
            assert!(
                names.insert(case_name.to_owned()),
                "{name}: the case {case_name} is named twice"
            );
            cases.push(Case {
                label: format!("{name}::{case_name}"),
                operation: case.field("operation").text().into(),
                input: case.field("input").clone(),
                issues: case
                    .field("issues")
                    .items()
                    .iter()
                    .map(Json::value)
                    .collect(),
            });
        }
    }
    assert!(!cases.is_empty(), "no case under {}", directory.display());
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

/// Replay every case: the issues its operation reports must equal its rows,
/// each issue renders its registered message from its own record (P-MSG-1),
/// and the causes the cases reach are returned.
pub fn replay() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for case in cases() {
        let issues = operations::run(&case.operation, &case.input);
        assert_eq!(
            Value::Array(issues.iter().map(row).collect()),
            Value::Array(case.issues),
            "{}",
            case.label
        );
        reached.extend(issues.iter().map(rendered));
    }
    reached
}
