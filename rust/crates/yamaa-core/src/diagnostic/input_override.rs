//! Host input overrides name aliases from the composed specification.
use super::{ConditionCode, Context, Diagnostic};
use crate::json::Value;

/// A supplied alias must name a declared input. Available aliases have a
/// stable sorted order in the refusal, independent of declaration order.
pub fn input_override(alias: &str, allowed: &[&str]) -> Option<Diagnostic> {
    if allowed.contains(&alias) {
        return None;
    }
    Some(Diagnostic {
        code: ConditionCode::InputOverrideAlias,
        spec_paths: vec![format!("inputs.{alias}")],
        context: Context::from([
            ("value".into(), Value::String(alias.into())),
            (
                "allowed".into(),
                Value::Array(
                    allowed
                        .iter()
                        .copied()
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .map(|name| Value::String(name.into()))
                        .collect(),
                ),
            ),
        ]),
        source_span: None,
        operand_route: None,
    })
}

/// A host stream that the shared Arrow admission refused.
pub fn input_frame(alias: &str, code: &str, value: &str) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::InputFrameValue,
        spec_paths: vec![format!("inputs.{alias}")],
        context: Context::from([
            ("code".into(), Value::String(code.into())),
            ("value".into(), Value::String(value.into())),
        ]),
        source_span: None,
        operand_route: None,
    }
}
