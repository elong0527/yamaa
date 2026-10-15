//! Authored YAML findings and bounded retained report encoding.
use crate::yaml_decode::DecodeFailure;
use serde_json::{json, Value};
use std::{fmt, io};

/// Host transport failures; authored YAML failures are returned as data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    ResponseLimit,
    Internal,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ResponseLimit => "YAML response exceeds byte limit",
            Self::Internal => "internal YAML decoder failure",
        })
    }
}
impl std::error::Error for Error {}

struct Response {
    bytes: Vec<u8>,
    limit: usize,
}
impl io::Write for Response {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            return Err(io::Error::other("response limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Bound the complete encoded envelope without publishing a partial prefix.
pub(crate) fn encode_outcome(
    protocol: &str,
    outcome: Value,
    limit: usize,
) -> Result<String, Error> {
    let mut response = Response {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(
        &mut response,
        &json!({"protocol":protocol,"outcome":outcome}),
    )
    .map_err(|error| {
        if error.is_io() {
            Error::ResponseLimit
        } else {
            Error::Internal
        }
    })?;
    String::from_utf8(response.bytes).map_err(|_| Error::Internal)
}

/// Semantic source findings are shared by byte probes and document preparation.
/// Resource refusals and internal defects are never fabricated language findings.
fn source_diagnostics<'a>(
    error: &'a DecodeFailure,
    identity: Option<&'a str>,
) -> Option<Vec<yamaa_core::project_diagnostics::Diagnostic<'a>>> {
    use yamaa_core::yaml_diagnostics::Failure;
    let failure = match error {
        DecodeFailure::NonAscii(position) => Failure::NonAscii {
            line: position.line,
            column: position.column,
        },
        DecodeFailure::InvalidYaml { position, reason } => Failure::InvalidYaml {
            line: position.line,
            column: position.column,
            reason,
        },
        DecodeFailure::InvalidText(issues) => Failure::InvalidText(issues),
        DecodeFailure::Limit(_) | DecodeFailure::Internal => return None,
    };
    Some(yamaa_core::yaml_diagnostics::diagnostics(failure, identity))
}
pub(crate) fn capture_diagnostics(error: &DecodeFailure, identity: &str) -> Option<Vec<Value>> {
    Some(
        source_diagnostics(error, Some(identity))?
            .into_iter()
            .map(crate::project_diagnostic_projection::value)
            .collect(),
    )
}
pub(crate) fn semantic_diagnostics(error: &DecodeFailure) -> Option<Vec<Value>> {
    Some(
        source_diagnostics(error, None)?
            .into_iter()
            .map(crate::project_diagnostic_projection::value)
            .collect(),
    )
}
