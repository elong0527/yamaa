//! Encode core-owned static check findings without executing or entering a port.
use crate::{specification_diagnostics::portable_diagnostic, specification_run_view::RunView};
use std::io::Write;

pub const MAX_ISSUE_BYTES: usize = 16_777_216;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidContext,
    Limit,
    Encode,
}
impl Error {
    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidContext => "invalid check issue context",
            Self::Limit => "check issue byte limit",
            Self::Encode => "check issue encoding failed",
        }
    }
}

/// Both hosts receive context as JSON text, so neither narrows its scalar values.
pub fn issues(run: &dyn RunView) -> Result<String, Error> {
    encode(run, MAX_ISSUE_BYTES)
}

pub fn issue_rows(run: &dyn RunView) -> Result<Vec<crate::issue_rows::Issue>, Error> {
    let rows = run
        .check_diagnostics()
        .into_iter()
        .map(|finding| {
            crate::issue_rows::Issue::from_diagnostic(
                portable_diagnostic(finding).ok_or(Error::InvalidContext)?,
            )
            .map_err(|_| Error::InvalidContext)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !crate::issue_rows::within_limit(&rows, MAX_ISSUE_BYTES) {
        return Err(Error::Limit);
    }
    Ok(rows)
}

fn encode(run: &dyn RunView, maximum: usize) -> Result<String, Error> {
    let mut rows = Vec::new();
    for finding in issue_rows(run)? {
        rows.push(finding.as_value());
    }
    let mut output = Buffer {
        bytes: Vec::new(),
        maximum,
        refused: false,
    };
    serde_json::to_writer(&mut output, &rows).map_err(|_| {
        if output.refused {
            Error::Limit
        } else {
            Error::Encode
        }
    })?;
    String::from_utf8(output.bytes).map_err(|_| Error::Encode)
}

struct Buffer {
    bytes: Vec<u8>,
    maximum: usize,
    refused: bool,
}
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|length| length > self.maximum)
        {
            self.refused = true;
            return Err(std::io::Error::other("check issue byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::specification_run::PreparedRun;
    #[test]
    fn complete_utf8_issue_payload_obeys_the_exact_byte_ceiling() {
        let schema = crate::shipped_schema::capture().unwrap();
        let raw = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: unread.csv}\noutput: {path: result.csv, columns: [ID, V]}\ncolumns:\n  - {name: ID, type: str, label: ID, derivation: SRC.ID}\n  - name: V\n    type: str\n    label: V\n    derivation: SRC.V\n    verifications: [{matches: {pattern: \"\\u00e9(\"}}]\n";
        let document = schema
            .prepare_standalone(crate::specification_source::Source {
                identity: "spec.yaml".into(),
                bytes: raw.as_bytes().to_vec(),
            })
            .unwrap();
        let run = PreparedRun::prepare(document).unwrap();
        let expected = r#"[{"condition":"invalid_regex","context":"{\"pattern\":\"é(\"}","phase":"validation","requirement":"REQ-0827","spec_paths":["columns.V.verifications[0].matches.pattern"]}]"#;
        assert_eq!(issues(&run).unwrap(), expected);
        assert_eq!(encode(&run, expected.len()).unwrap(), expected);
        assert_eq!(encode(&run, expected.len() - 1), Err(Error::Limit));
        assert_eq!(issues(&run).unwrap(), expected);
    }
}
