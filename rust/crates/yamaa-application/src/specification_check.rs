//! Core-owned static check findings, read without executing or entering a port.
//! Unsupported vocabulary and policy/resource failures remain typed boundaries.
use crate::specification_run::{PreparedRun, RejectedRun};
use yamaa_core::{diagnostic::Diagnostic, specification::PrepareError};

/// The static check findings of a compiled specification, in report order.
pub fn issues(run: &PreparedRun) -> Vec<Diagnostic> {
    run.check_diagnostics()
}

#[derive(Debug)]
pub enum Error<'a> {
    Compile(&'a PrepareError),
    /// A rejected compilation that names no finding.
    Projection,
}
/// The findings of a compilation the compiler refused.
pub fn rejected(run: &RejectedRun) -> Result<Vec<Diagnostic>, Error<'_>> {
    let PrepareError::Invalid(findings) = &run.rejection.error else {
        return Err(Error::Compile(&run.rejection.error));
    };
    if findings.is_empty() {
        return Err(Error::Projection);
    }
    Ok(findings
        .iter()
        .map(|finding| finding.diagnostic())
        .collect())
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;
    use crate::specification_run::PreparedRun;
    #[test]
    fn complete_utf8_issue_payload_preserves_authored_fields() {
        let raw = "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: unread.csv}\noutput: {path: result.csv, columns: [ID, V]}\ncolumns:\n  - {name: ID, type: str, label: ID, derivation: SRC.ID}\n  - name: V\n    type: str\n    label: V\n    derivation: SRC.V\n    verifications: [{matches: {pattern: \"\\u00e9(\"}}]\n";
        let document = crate::shipped_schema::prepare(
            crate::specification_source::Source {
                identity: "spec.yaml".into(),
                bytes: raw.as_bytes().to_vec(),
            },
            "spec.yaml".into(),
            &mut crate::specification_source::EntryOnly,
        )
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
