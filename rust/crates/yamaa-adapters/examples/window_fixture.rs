//! Development-only named-window truth through the shipped typed schema.
use serde_json::{json, Value};
use std::io::{self, BufRead};
use yamaa_adapters::{
    shipped_schema,
    yaml_decode::{decode_yaml, DecodeLimits},
};
use yamaa_core::schema::{
    admit_schema_diagnostics, Document, DocumentNode as N, NormalizationBudget, NormalizationError,
    SchemaDiagnosticBudget, SchemaDiagnosticValue as V,
};

fn node(document: &Document, id: usize) -> Value {
    match &document.nodes()[id] {
        N::Null => Value::Null,
        N::Boolean(value) => json!(value),
        N::Integer(value) => Value::Number(value.parse().expect("fixture integer codec")),
        N::Float(value) => json!(value),
        N::Text(value) => json!(value),
        N::Sequence(values) => Value::Array(values.iter().map(|&id| node(document, id)).collect()),
        N::Mapping(values) => Value::Object(
            values
                .iter()
                .map(|&(key, value)| {
                    let N::Text(name) = &document.nodes()[key] else {
                        panic!("fixture mapping key")
                    };
                    (name.clone(), node(document, value))
                })
                .collect(),
        ),
    }
}

fn context(value: V<'_>) -> Value {
    match value {
        V::Text(value) | V::Integer(value) => json!(value),
        V::Count(value) => json!(value),
        V::Null => Value::Null,
        V::Boolean(value) => json!(value),
        V::Float(value) => json!(value),
        V::TextList(value) => json!(value),
    }
}

fn main() {
    let strict = !std::env::args().any(|argument| argument == "--non-strict");
    let schema = shipped_schema::capture().expect("shipped fixture schema");
    for line in io::stdin().lock().lines() {
        let text: String =
            serde_json::from_str(&line.expect("stdin")).expect("authored fixture source");
        let input = decode_yaml(text.as_bytes(), DecodeLimits::default()).expect("fixture YAML");
        let output = match schema.structure().expand_named_windows(
            &input.document,
            strict,
            &mut NormalizationBudget::new(Default::default()),
        ) {
            Ok(expanded) => {
                json!({"status":"expanded", "document":node(&expanded.document, expanded.document.root()),
                "references":expanded.references.iter().map(|reference|(&reference.path,&reference.definition)).collect::<Vec<_>>()})
            }
            Err(NormalizationError::Invalid(findings)) => {
                let report = admit_schema_diagnostics(
                    schema.structure(),
                    Some(&input.document),
                    &findings,
                    &[],
                    &mut SchemaDiagnosticBudget::new(),
                )
                .expect("typed fixture findings");
                json!({"status":"invalid", "diagnostics":report.iter().map(|finding| json!({
                    "condition":finding.condition, "requirement":finding.requirement, "path":finding.path,
                    "context":finding.context.map(|(name,value)|(name.to_owned(),context(value))).collect::<serde_json::Map<_,_>>()
                })).collect::<Vec<_>>()})
            }
            Err(error) => json!({"status":"refused","reason":format!("{error:?}")}),
        };
        println!("{output}");
    }
}
