//! Python report metadata and issue-row conversion.
pub(super) type IssueRow = (String, String, Option<String>, Vec<String>, String);
pub(super) fn issue_rows(diagnostics: &[yamaa::Diagnostic]) -> Vec<IssueRow> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.phase.clone(),
                diagnostic.code.clone(),
                diagnostic.requirement.clone(),
                diagnostic.spec_path.clone(),
                diagnostic.context.to_json().to_owned(),
            )
        })
        .collect()
}
