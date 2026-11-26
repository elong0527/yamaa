//! Host input overrides name aliases from the composed specification.
use super::{ConditionCode, Context, Diagnostic};
use crate::json::Value;

/// A supplied alias must name a declared input. Available aliases have a
/// stable sorted order in the refusal, independent of declaration order.
pub fn input_override(alias: &str, allowed: &[&str]) -> Option<Diagnostic> {
    if allowed.contains(&alias) {
        return None;
    }
    Some(Diagnostic::new(
        ConditionCode::InputOverrideAlias,
        vec![format!("inputs.{alias}")],
        Context::from([
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
    ))
}

/// What the shared Arrow admission refused in a host stream: the stream
/// as a whole, named by `value`, or one of its columns.
#[derive(Clone, Copy, Debug)]
pub enum FrameCause<'a> {
    /// The stream as a whole, such as a batch whose schema is not the
    /// stream's.
    Stream { value: &'a str },
    /// A column, by its name and its Arrow type as Arrow C++ spells it: a
    /// type no frame admits, or, with `record`, a value in that one-based
    /// record of the stream: text that is not UTF-8, or a temporal value
    /// that is no whole second within the years 1 to 9999.
    Column {
        column: &'a str,
        arrow_type: &'a str,
        record: Option<usize>,
    },
}

/// A host stream that the shared Arrow admission refused, with the stable
/// table `code` of its cause.
pub fn input_frame(alias: &str, code: &str, cause: FrameCause<'_>) -> Diagnostic {
    let mut context = Context::from([("code".into(), Value::String(code.into()))]);
    match cause {
        FrameCause::Stream { value } => {
            context.insert("value".into(), Value::String(value.into()));
        }
        FrameCause::Column {
            column,
            arrow_type,
            record,
        } => {
            context.insert("column".into(), Value::String(column.into()));
            context.insert("type".into(), Value::String(arrow_type.into()));
            if let Some(record) = record {
                context.insert("record".into(), Value::from(record));
            }
        }
    }
    Diagnostic::new(
        ConditionCode::InputFrameValue,
        vec![format!("inputs.{alias}")],
        context,
    )
}

/// A host frame's column whose type is not the one the input's `types`
/// declare for it (INPUT-5): the frame stands in for the declared dataset,
/// so its declaration still types it.
pub fn input_frame_type(
    alias: &str,
    column: &str,
    declared: crate::value::ColumnType,
    found: crate::value::ColumnType,
) -> Diagnostic {
    Diagnostic::new(
        ConditionCode::InputFrameType,
        vec![format!("inputs.{alias}")],
        Context::from([
            ("column".into(), Value::String(column.into())),
            ("declared".into(), Value::String(declared.name().into())),
            ("found".into(), Value::String(found.name().into())),
        ]),
    )
}
