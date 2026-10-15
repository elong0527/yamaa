//! Authored YAML findings and complete retained report encoding.
use crate::yaml_decode::DecodeFailure;
use yamaa_core::json::{json, Value};

pub(crate) fn encode_outcome(protocol: &str, outcome: Value) -> String {
    json!({"protocol":protocol,"outcome":outcome}).to_string()
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
