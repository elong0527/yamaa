//! Source-site projection for typed development observations, with no parsing.
use serde_json::{json, Value};
use yamaa_core::numeric_parser::SourceSpan;

pub fn span(text: &str, site: SourceSpan) -> Value {
    json!([
        text[..site.start].chars().count(),
        text[..site.end].chars().count()
    ])
}

pub fn operator(text: &str, start: usize, end: usize) -> Value {
    let offset = text[start..end]
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .expect("typed operator gap")
        .0;
    span(
        text,
        SourceSpan {
            start: start + offset,
            end: start + offset + 1,
        },
    )
}
