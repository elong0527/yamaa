//! Application admission failures are distinct from normative language findings.
use crate::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    value::Value,
};
use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

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
            ContextValue::Scalar(Value::Str(operation.into())),
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
            (
                String::from("stage"),
                ContextValue::Scalar(Value::Str(stage.into())),
            ),
            (
                String::from("code"),
                ContextValue::Scalar(Value::Str(code.into())),
            ),
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
    // This is a completion boundary, not an inventory of findings or a claim
    // that no earlier check ran. `build` may include report projection after
    // successful execution, so only narrower classified causes narrow it.
    let incomplete_from = match (stage, code) {
        ("capture" | "prepare" | "check" | "inheritance", _) => "specification",
        ("ingest" | "bind", _) | ("build", "source_boundary") => "source_binding",
        ("execute", _) | ("build", "execution_boundary") => "data",
        ("output" | "report", _) => "output",
        _ => stage,
    };
    finding.context.insert(
        "incomplete_checks_from".into(),
        ContextValue::Scalar(Value::Str(incomplete_from.into())),
    );
    finding.spec_paths = spec_paths;
    finding.context.insert(
        "resource".into(),
        ContextValue::Scalar(Value::Str(resource.into())),
    );
    finding
        .context
        .insert("limit".into(), ContextValue::Integer(limit.to_string()));
    if let Some(actual) = actual {
        finding
            .context
            .insert("actual".into(), ContextValue::Integer(actual.to_string()));
    }
    finding
}

/// A classified failure during the actual atomic publication request. Target
/// admission refusals and opaque interrupts do not establish this cause.
pub fn publication_failed(target: &str) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::OutputPublicationFailed,
        spec_paths: vec!["output.path".into()],
        context: [(
            String::from("path"),
            ContextValue::Scalar(Value::Str(target.into())),
        )]
        .into(),
        source_span: None,
        operand_route: None,
    }
}
