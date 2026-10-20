//! Authored YAML findings as diagnostic records.
use crate::yaml_decode::DecodeFailure;
use yamaa_core::diagnostic::Diagnostic;

/// Semantic source findings are shared by byte probes and document preparation.
/// Resource refusals and internal defects are never fabricated language findings.
fn source_diagnostics(error: &DecodeFailure, identity: Option<&str>) -> Option<Vec<Diagnostic>> {
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
        DecodeFailure::Unavailable | DecodeFailure::Limit(_) | DecodeFailure::Internal => {
            return None
        }
    };
    Some(
        yamaa_core::yaml_diagnostics::diagnostics(failure, identity)
            .into_iter()
            .map(crate::project_diagnostic_projection::owned)
            .collect(),
    )
}
pub(crate) fn capture_diagnostics(
    error: &DecodeFailure,
    identity: &str,
) -> Option<Vec<Diagnostic>> {
    source_diagnostics(error, Some(identity))
}
pub(crate) fn semantic_diagnostics(error: &DecodeFailure) -> Option<Vec<Diagnostic>> {
    source_diagnostics(error, None)
}
