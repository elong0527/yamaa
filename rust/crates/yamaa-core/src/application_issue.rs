//! Application admission failures are distinct from normative language findings.
use crate::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    value::Value,
};
use alloc::{string::String, vec, vec::Vec};

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
