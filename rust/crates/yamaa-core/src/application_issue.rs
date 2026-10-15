//! Application admission failures are distinct from normative language findings.
use crate::diagnostic::{ConditionCode, Diagnostic};
use crate::json::Value as JsonValue;
use std::vec;

/// Format classified installed-package disagreement. Packaging tools supply the
/// facts; this owns the stable normative issue identity without comparing versions.
pub fn lock_mismatch(context: crate::diagnostic::Context) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::ProjectLockMismatch,
        spec_paths: vec!["lock".into()],
        context,
        source_span: None,
        operand_route: None,
    }
}

pub fn unsupported(operation: &str, path: Option<&str>) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::ApplicationUnsupported,
        spec_paths: path.map(String::from).into_iter().collect(),
        context: [(
            String::from("operation"),
            JsonValue::String(operation.into()),
        )]
        .into(),
        source_span: None,
        operand_route: None,
    }
}

/// Stage and code are adapter-owned classifications, never inferred from an
/// arbitrary exception message or panic payload. No requirement is fabricated.
pub fn rejected(stage: &str, code: &str) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::ApplicationRejected,
        spec_paths: Vec::new(),
        context: vec![
            (String::from("stage"), JsonValue::String(stage.into())),
            (String::from("code"), JsonValue::String(code.into())),
        ]
        .into_iter()
        .collect(),
        source_span: None,
        operand_route: None,
    }
}

/// A policy refusal names its exhausted capacity without inventing a language
/// requirement. The caller supplies only admitted, typed resource facts.
pub fn resource_rejected(
    stage: &str,
    code: &str,
    resource: &str,
    limit: usize,
    actual: Option<usize>,
    spec_paths: Vec<String>,
) -> Diagnostic {
    let mut finding = rejected(stage, code);
    finding.spec_paths = spec_paths;
    finding
        .context
        .insert("resource".into(), JsonValue::String(resource.into()));
    finding
        .context
        .insert("limit".into(), JsonValue::from(limit));
    if let Some(actual) = actual {
        finding
            .context
            .insert("actual".into(), JsonValue::from(actual));
    }
    finding
}

/// A classified failure during the actual atomic publication request. Target
/// admission refusals and opaque interrupts do not establish this cause.
pub fn publication_failed(target: &str) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::OutputPublicationFailed,
        spec_paths: vec!["output.path".into()],
        context: [(String::from("path"), JsonValue::String(target.into()))].into(),
        source_span: None,
        operand_route: None,
    }
}
