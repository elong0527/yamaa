//! Encode an already admitted borrowed core context tree into the issue envelope.
use crate::issue_rows::{InvalidIssue, Issue};
use serde_json::{json, Map, Value as Json};
use yamaa_core::{
    project_diagnostics::{Diagnostic, Fact},
    value::Value,
};

pub(crate) fn fact(value: Fact<'_>) -> Json {
    match value {
        Fact::Text(text) => json!(text),
        Fact::OwnedText(text) => json!(text),
        Fact::Scalar(value) => match value {
            Value::Missing => Json::Null,
            Value::Str(value) => json!(value),
            Value::Int(value) => json!(value),
            Value::Float(value) => json!(value.get()),
            Value::Bool(value) => json!(value),
            Value::Date(value) => json!(value.to_string()),
            Value::DateTime(value) => json!(value.to_string()),
        },
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
pub(crate) fn value(diagnostic: Diagnostic<'_>) -> Json {
    let mapping = diagnostic.code.definition();
    let context: Map<_, _> = diagnostic
        .context
        .into_iter()
        .map(|(name, value)| (name.to_owned(), fact(value)))
        .collect();
    crate::issue_rows::report(
        mapping.phase,
        mapping.condition,
        mapping.requirement,
        vec![diagnostic.spec_path.to_owned()],
        context.into(),
    )
}
pub(crate) fn issue(diagnostic: Diagnostic<'_>) -> Result<Issue, InvalidIssue> {
    Issue::from_diagnostic(value(diagnostic))
}
pub(crate) fn finish<'a, E>(
    diagnostics: Vec<Diagnostic<'_>>,
    maximum: usize,
) -> Result<Vec<Issue>, yamaa_core::project_diagnostics::Error<'a, E>> {
    use yamaa_core::project_diagnostics::Error;
    let issues = diagnostics
        .into_iter()
        .map(issue)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::InvalidContext)?;
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Limit);
    }
    Ok(issues)
}
