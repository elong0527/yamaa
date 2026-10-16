//! Encode core-owned static check findings without executing or entering a port.
use crate::{specification_diagnostics::portable_diagnostic, specification_run_view::RunView};

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidContext,
    Encode,
}
impl Error {
    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidContext => "invalid check issue context",
            Self::Encode => "check issue encoding failed",
        }
    }
}

/// Both hosts receive context as JSON text, so neither narrows its scalar values.
pub fn issues(run: &dyn RunView) -> Result<String, Error> {
    Ok(crate::issue_rows::encode(&crate::issue_rows::collect(
        issue_rows(run)?,
    )))
}

pub fn issue_rows(run: &dyn RunView) -> Result<Vec<crate::issue_rows::Issue>, Error> {
    let rows = run
        .check_diagnostics()
        .into_iter()
        .map(|finding| {
            crate::issue_rows::Issue::from_diagnostic(portable_diagnostic(finding))
                .map_err(|_| Error::InvalidContext)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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
        let expected: yamaa_core::json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/check_invalid_regex_truth.json"
        ))
        .unwrap();
        let expected = serde_json::to_string(&expected).unwrap();
        assert_eq!(issues(&run).unwrap(), expected);
        assert_eq!(issues(&run).unwrap(), expected);
    }
}
