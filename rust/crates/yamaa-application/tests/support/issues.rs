//! One assertion for a list of issues: each issue's complete record, compared
//! with one authored literal, so a test states what it expects once and a
//! mismatch shows the whole list (AGENTS.md, "Assert the whole record, once,
//! with a label").
use yamaa_core::{diagnostic::Diagnostic, json::Value};

/// Compare `issues` with `expected`, one authored row per issue, in order.
/// A row is the complete record, as the run report renders it: `phase`,
/// `condition`, `rule` (`null` when the cause cites no card), `spec_paths`,
/// `context`, and `source_span` and `operand_route` when the issue carries
/// them. A row written without those two keys asserts their absence, so a
/// diagnostic that gains or loses geometry fails the test. A mismatch prints
/// every actual row under `label`, so the author sees the list as a whole.
#[track_caller]
pub fn assert_issues(issues: &[Diagnostic], expected: &[Value], label: &str) {
    let actual = super::rows(issues);
    assert_eq!(
        Value::Array(actual.clone()),
        Value::Array(expected.to_vec()),
        "{label}: the issues are\n{}",
        rendered(&actual)
    );
}

/// `issues` is empty; a mismatch prints every row under `label`.
#[track_caller]
pub fn assert_no_issues(issues: &[Diagnostic], label: &str) {
    assert_issues(issues, &[], label);
}

/// One row per line, in canonical JSON, for a failure message.
fn rendered(rows: &[Value]) -> String {
    rows.iter()
        .map(yamaa_adapters::json_wire::encode)
        .collect::<Vec<_>>()
        .join("\n")
}
