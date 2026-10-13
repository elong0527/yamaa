//! Private typed string-template observations for independently authored fixtures.
use serde_json::{json, Value};
use yamaa_core::{
    diagnostic::ContextValue,
    text_template::{AdmissionError, Part, StringTemplate},
    value::Value as Scalar,
};

pub fn observe(text: &str) -> Value {
    let outcome = match StringTemplate::new(text.into(), "fixture.template".into(), None) {
        Ok(parsed) => {
            if let Some(finding) = parsed.declaration_diagnostic() {
                let span = parsed.syntax_failure_span().expect("syntax refusal span");
                let definition = finding.definition();
                let mut context = serde_json::Map::new();
                for (name, value) in &finding.context {
                    if let ContextValue::Scalar(Scalar::Str(text)) = value {
                        context.insert(name.clone(), json!(text));
                    } else {
                        unreachable!("template syntax context contains only strings");
                    }
                }
                json!({"status":"invalid", "condition":definition.condition,
                    "requirement":definition.requirement, "context":context,
                    "position":{"byte":span.start,"character":text[..span.start].chars().count()},
                    "span":{"byte_start":span.start,"byte_end":span.end,
                        "character_start":text[..span.start].chars().count(),
                        "character_end":text[..span.end].chars().count()}})
            } else {
                json!({"status":"parsed", "parts":parts(&parsed, text),
                    "identifiers":parsed.identifiers()})
            }
        }
        Err(error) => json!({"status":match error {
            AdmissionError::Shape => "invalid_shape",
            AdmissionError::Limit => "resource_limit",
        }}),
    };
    json!({"outcome":outcome})
}

/// Coalesce typed literal occurrences into author-facing text parts.
/// Cursor arithmetic projects retained occurrence widths; it makes no syntax decisions.
fn parts(parsed: &StringTemplate, text: &str) -> Vec<Value> {
    let mut output = Vec::new();
    let mut literal = String::new();
    let mut literal_start = 0;
    let mut at = 0;
    for part in parsed.parts() {
        match part {
            Part::Placeholder(index) => {
                if !literal.is_empty() {
                    output.push(json!({"kind":"text", "value":literal,
                        "span":[text[..literal_start].chars().count(),text[..at].chars().count()]}));
                    literal.clear();
                }
                let name = &parsed.identifiers()[*index];
                let start = at + 1;
                let end = start + name.len();
                output.push(json!({"kind":"placeholder", "name":name,
                    "span":[text[..start].chars().count(),text[..end].chars().count()]}));
                at = end + 1;
            }
            Part::Text { .. } | Part::Brace(_) => {
                if literal.is_empty() {
                    literal_start = at;
                }
                literal.push_str(parsed.literal(part).expect("typed literal"));
                at = match part {
                    Part::Text { end, .. } => *end,
                    Part::Brace(_) => at + 2,
                    Part::Placeholder(_) => unreachable!(),
                };
            }
        }
    }
    if !literal.is_empty() {
        output.push(json!({"kind":"text", "value":literal,
            "span":[text[..literal_start].chars().count(),text[..at].chars().count()]}));
    }
    output
}
