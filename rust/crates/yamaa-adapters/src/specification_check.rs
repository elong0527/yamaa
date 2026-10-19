//! Core-owned static check findings, read without executing or entering a port.
use crate::specification_run_view::RunView;
use yamaa_core::diagnostic::Diagnostic;

/// The static check findings of a compiled specification, in report order.
pub fn issues(run: &dyn RunView) -> Vec<Diagnostic> {
    run.check_diagnostics()
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;
    use crate::specification_run::PreparedRun;
    #[test]
    fn complete_utf8_issue_payload_preserves_authored_fields() {
        let schema = crate::shipped_schema::capture().unwrap();
        let raw = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: unread.csv}\noutput: {path: result.csv, columns: [ID, V]}\ncolumns:\n  - {name: ID, type: str, label: ID, derivation: SRC.ID}\n  - name: V\n    type: str\n    label: V\n    derivation: SRC.V\n    verifications: [{matches: {pattern: \"\\u00e9(\"}}]\n";
        let document = schema
            .prepare_standalone(crate::specification_source::Source {
                identity: "spec.yaml".into(),
                bytes: raw.as_bytes().to_vec(),
            })
            .unwrap();
        let run = PreparedRun::prepare(document).unwrap();
        let expected: yamaa_core::json::Value = crate::json_wire::decode(include_str!(
            "../tests/fixtures/check_invalid_regex_truth.json"
        ))
        .unwrap();
        for _ in 0..2 {
            let rendered = issues(&run)
                .iter()
                .map(crate::report_storage::diagnostic_value)
                .collect::<Vec<_>>();
            assert_eq!(yamaa_core::json::Value::Array(rendered), expected);
        }
    }
}
