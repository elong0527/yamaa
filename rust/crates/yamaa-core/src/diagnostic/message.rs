//! Messages rendered from the condition registry (P-MSG-1).
//!
//! `yaml/conditions.yaml` holds one message template per condition, and the
//! table generated from it sits beside this registry. A template is text with
//! placeholders: `{spec_path}` is the record's first spec path, any other
//! `{key}` is the context value of that key, and `{{` and `}}` are literal
//! braces. Text renders as written, a number as its exact token, a Boolean as
//! `true` or `false`, null as `null`, a list as its items joined with `, ` (an
//! item that is itself a list or an object as canonical JSON), and an object
//! as canonical JSON. Every cause of a condition records each key its
//! template names, which the tests that reach every cause check; at run time
//! a key a record lacks renders as `null`, the way a missing value does.

use super::{ConditionCode, Diagnostic};
use crate::json::Value;

/// A placeholder of a cause's message that its record does not fill.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the {condition} message names {key}, which the record does not hold")]
pub struct UnresolvedPlaceholder {
    pub condition: &'static str,
    pub key: String,
}

/// The registered message template of a condition, or `None` for a name the
/// registry does not hold.
pub fn template(condition: &str) -> Option<&'static str> {
    let table = super::message_templates::TEMPLATES;
    table
        .binary_search_by(|(name, _)| (*name).cmp(condition))
        .ok()
        .map(|index| table[index].1)
}

impl ConditionCode {
    /// The message template of the cause's condition. Every condition the
    /// core registry names is registered with a message, which the tests
    /// check; the condition's own name stands in for a missing one.
    pub fn template(self) -> &'static str {
        let condition = self.definition().condition;
        template(condition).unwrap_or(condition)
    }
}

impl Diagnostic {
    /// The record's message: its condition's template with each placeholder
    /// replaced by the record's value.
    pub fn message(&self) -> String {
        render(self, false).unwrap_or_default()
    }

    /// The record's message, or the first placeholder it does not fill.
    pub fn checked_message(&self) -> Result<String, UnresolvedPlaceholder> {
        render(self, true)
    }
}

fn render(diagnostic: &Diagnostic, strict: bool) -> Result<String, UnresolvedPlaceholder> {
    let template = super::message_templates::VARIANTS
        .iter()
        .find(|(condition, key, _)| {
            *condition == diagnostic.definition().condition && diagnostic.context.contains_key(*key)
        })
        .map_or_else(|| diagnostic.code.template(), |(_, _, template)| *template);
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(at) = rest.find(['{', '}']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some(after) = tail.strip_prefix("{{") {
            out.push('{');
            rest = after;
        } else if let Some(after) = tail.strip_prefix("}}") {
            out.push('}');
            rest = after;
        } else if let Some(close) = tail.strip_prefix('{').and_then(|body| body.find('}')) {
            let key = &tail[1..=close];
            let filled = if key == "spec_path" {
                diagnostic.spec_paths.first().map(|path| out.push_str(path))
            } else {
                diagnostic
                    .context
                    .get(key)
                    .map(|value| write_value(value, &mut out))
            };
            if filled.is_none() {
                if strict {
                    return Err(UnresolvedPlaceholder {
                        condition: diagnostic.definition().condition,
                        key: key.into(),
                    });
                }
                out.push_str("null");
            }
            rest = &tail[close + 2..];
        } else {
            // The registry admits no stray brace; one renders as written.
            out.push_str(&tail[..1]);
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// A context value as message text.
fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::String(text) => out.push_str(text),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                match item {
                    Value::Array(_) | Value::Object(_) => write_json(item, out),
                    scalar => write_value(scalar, out),
                }
            }
        }
        Value::Object(_) => write_json(value, out),
        scalar => write_json(scalar, out),
    }
}

/// Canonical JSON text, as the issues table's `context` column writes it:
/// compact, members in key order, numbers as their exact tokens, and strings
/// escaping only quotes, backslashes and control characters.
fn write_json(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Value::Number(number) => out.push_str(number.as_str()),
        Value::String(text) => write_json_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json(item, out);
            }
            out.push(']');
        }
        Value::Object(members) => {
            out.push('{');
            for (index, (key, member)) in members.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json_string(key, out);
                out.push(':');
                write_json(member, out);
            }
            out.push('}');
        }
    }
}

fn write_json_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            control if control < ' ' => {
                out.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{diagnostic::Severity, json::Number};

    fn record(code: ConditionCode, spec_paths: &[&str], context: &[(&str, Value)]) -> Diagnostic {
        Diagnostic {
            code,
            spec_paths: spec_paths.iter().map(|path| (*path).into()).collect(),
            context: context
                .iter()
                .map(|(key, value)| ((*key).into(), value.clone()))
                .collect(),
            source_span: None,
            operand_route: None,
        }
    }
    fn text(value: &str) -> Value {
        Value::String(value.into())
    }
    fn number(token: &str) -> Value {
        Value::Number(token.parse::<Number>().unwrap())
    }

    /// The table lists every registered condition once, in byte order, so
    /// the binary search finds each one.
    #[test]
    fn the_table_is_sorted_and_unique() {
        let table = super::super::message_templates::TEMPLATES;
        assert!(table.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (condition, message) in table {
            assert_eq!(template(condition), Some(*message));
        }
        assert_eq!(template("not_a_condition"), None);
    }

    /// Every cause renders from a template of its own condition, and every
    /// template is well formed: one clause in lower case or opening with a
    /// placeholder, ASCII, no final period, `{key}` names and doubled braces.
    #[test]
    fn every_cause_has_a_well_formed_template() {
        for code in super::super::CONDITIONS {
            let template = code.template();
            assert!(!template.is_empty(), "{code:?} has no message");
            assert!(template.bytes().all(|byte| (b' '..=b'~').contains(&byte)));
            let first = template.as_bytes()[0];
            assert!(first.is_ascii_lowercase() || first == b'{' && !template.starts_with("{{"));
            assert!(!template.ends_with('.'));
            let mut rest = template;
            while let Some(at) = rest.find(['{', '}']) {
                let tail = &rest[at..];
                if tail.starts_with("{{") || tail.starts_with("}}") {
                    rest = &tail[2..];
                    continue;
                }
                let close = tail.find('}').expect("a placeholder closes");
                let key = &tail[1..close];
                assert!(
                    tail.starts_with('{')
                        && key.starts_with(|c: char| c.is_ascii_lowercase())
                        && key
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                    "{code:?} template {template:?}"
                );
                rest = &tail[close + 1..];
            }
            assert_eq!(code.severity(), Severity::Error);
        }
    }

    #[test]
    fn placeholders_take_the_record_values() {
        let forward = record(
            ConditionCode::ColumnForwardReference,
            &["columns.ASTDY.derivation"],
            &[("column", text("ASTDY")), ("dependency", text("TRTSDT"))],
        );
        assert_eq!(
            forward.checked_message().unwrap(),
            "column ASTDY reads TRTSDT, which is declared after it"
        );
        assert_eq!(forward.message(), forward.checked_message().unwrap());
        assert_eq!(forward.severity().as_str(), "error");
        // `{spec_path}` is the first spec path.
        let unknown = record(
            ConditionCode::SchemaUnknownField,
            &["columns.X.extra", "columns.Y"],
            &[
                ("field", text("extra")),
                ("class", text("column_class")),
                ("identifier", text("extra")),
            ],
        );
        assert_eq!(
            unknown.message(),
            "extra is not a known field or reference at columns.X.extra"
        );
    }

    #[test]
    fn values_render_by_kind() {
        // A list joins its items; numbers keep their exact tokens.
        let cycle = record(
            ConditionCode::ColumnDependencyCycle,
            &["columns.A.derivation"],
            &[("cycle", Value::Array(vec![text("A"), text("B"), text("A")]))],
        );
        assert_eq!(cycle.message(), "A, B, A form a dependency cycle");
        let overflow = record(
            ConditionCode::ImputationMonthRange,
            &["impute.month"],
            &[("month", number("99999999999999999999999"))],
        );
        assert_eq!(
            overflow.message(),
            "month 99999999999999999999999 is outside 1 to 12"
        );
        let mut out = String::new();
        for value in [
            Value::Null,
            Value::Bool(true),
            Value::Bool(false),
            number("-0.0"),
            number("1.5e300"),
        ] {
            write_value(&value, &mut out);
            out.push(' ');
        }
        assert_eq!(out, "null true false -0.0 1.5e300 ");
    }

    /// Objects, and lists inside lists, render as the canonical JSON text of
    /// the issues table's `context` column.
    #[test]
    fn objects_render_as_canonical_json() {
        let object = Value::Object(
            [
                (
                    "b".to_string(),
                    Value::Array(vec![Value::Null, number("2")]),
                ),
                ("a".to_string(), text("q\"\\\n\r\t\u{8}\u{c}\u{1}\u{7f}é🙂")),
                ("c".to_string(), Value::Object(Default::default())),
            ]
            .into_iter()
            .collect(),
        );
        let mut out = String::new();
        write_value(&object, &mut out);
        assert_eq!(
            out,
            "{\"a\":\"q\\\"\\\\\\n\\r\\t\\b\\f\\u0001\u{7f}é🙂\",\"b\":[null,2],\"c\":{}}"
        );
        let mut out = String::new();
        write_value(
            &Value::Array(vec![
                text("x"),
                Value::Array(vec![text("y"), number("1")]),
                object,
            ]),
            &mut out,
        );
        assert!(out.starts_with("x, [\"y\",1], {\"a\""), "{out}");
    }

    /// A record that lacks a key its template names fails the checked
    /// rendering, and renders the key as `null` otherwise.
    #[test]
    fn an_absent_key_is_reported_and_renders_as_null() {
        let gap = record(
            ConditionCode::ColumnForwardReference,
            &["columns.A.derivation"],
            &[("column", text("A"))],
        );
        assert_eq!(
            gap.checked_message(),
            Err(UnresolvedPlaceholder {
                condition: ConditionCode::ColumnForwardReference.definition().condition,
                key: "dependency".into(),
            })
        );
        assert_eq!(
            gap.message(),
            "column A reads null, which is declared after it"
        );
        let pathless = record(
            ConditionCode::SchemaUnknownField,
            &[],
            &[("identifier", text("extra"))],
        );
        assert_eq!(pathless.checked_message().unwrap_err().key, "spec_path");
    }
}
