//! Materialize an admitted borrowed core context tree as the owned diagnostic record.
use crate::json;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::json::Value as Json;
use yamaa_core::project_diagnostics::{self, Fact};

pub fn fact(value: Fact<'_>) -> Json {
    match value {
        Fact::Text(text) => json!(text),
        Fact::OwnedText(text) => json!(text),
        Fact::Scalar(value) => value.into(),
        Fact::Boolean(value) => json!(value),
        Fact::Integer(value) => json!(value),
        Fact::Unsigned(value) => json!(value),
        Fact::Bound(value) => {
            Json::Number(value.decimal().parse().expect("sealed canonical integer"))
        }
        Fact::Null => Json::Null,
        Fact::Sequence(values) => Json::Array(values.into_iter().map(fact).collect()),
        Fact::Object(values) => Json::Object(
            values
                .into_iter()
                .map(|(name, value)| (name.to_owned(), fact(value)))
                .collect(),
        ),
    }
}
/// The owned record of one borrowed core diagnostic; it records no geometry.
pub fn owned(diagnostic: project_diagnostics::Diagnostic<'_>) -> Diagnostic {
    Diagnostic {
        code: diagnostic.code,
        spec_paths: vec![diagnostic.spec_path],
        context: diagnostic
            .context
            .into_iter()
            .map(|(name, value)| (name.to_owned(), fact(value)))
            .collect(),
        source_span: None,
        operand_route: None,
    }
}
