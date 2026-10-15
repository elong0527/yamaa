//! Project only core-admitted findings from the retained authored arenas.
use serde_json::{json, Map, Number, Value};
use yamaa_adapters::{
    specification_source::{CapturedSchema, Limits, Source},
    yaml_decode::decode_yaml,
};
use yamaa_core::schema::{
    admit_schema_diagnostics, SchemaDiagnosticBudget, SchemaDiagnosticValue as V, ValidationBudget,
};

fn value(value: V<'_>) -> Value {
    match value {
        V::Text(text) => json!(text),
        V::Count(count) => json!(count),
        V::Null => Value::Null,
        V::Boolean(value) => json!(value),
        V::Float(value) => json!(value),
        V::Integer(text) => Value::Number(text.parse::<Number>().expect("admitted exact integer")),
        V::TextList(values) => json!(values),
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
pub enum SchemaInput {
    Text(String),
    /// The entry module is first; all include identities retain authored spelling.
    Modules(Vec<(String, String)>),
}

#[cfg(test)]
pub fn observe(schema: &str, document: &str) -> Value {
    observe_input(SchemaInput::Text(schema.into()), document, None)
}

pub fn observe_input(input: SchemaInput, document: &str, root: Option<&str>) -> Value {
    let sources = match input {
        SchemaInput::Text(text) => vec![("authored-schema.yaml".into(), text)],
        SchemaInput::Modules(modules) => modules,
    }
    .into_iter()
    .map(|(identity, text)| Source {
        identity,
        bytes: text.into_bytes(),
    })
    .collect();
    let captured = match root {
        Some(root) => CapturedSchema::admit_root(sources, 0, root, Limits::default()),
        None => CapturedSchema::admit(sources, 0, Limits::default()),
    }
    .expect("admitted authored schema closure");
    let decoded =
        decode_yaml(document.as_bytes(), Default::default()).expect("decoded authored document");
    let schema = captured.structure();
    let mut budget = ValidationBudget::new(Default::default());
    let types = schema
        .parse_query_types(&[schema.root_class().name.clone()], &mut budget)
        .expect("captured root class");
    // This private control validates the declared class itself. Whole installed
    // preparation retains its separate shipped-schema/version admission gate.
    let findings = schema
        .validate_types(
            &types,
            &decoded.document,
            decoded.document.root(),
            "",
            false,
            &mut budget,
        )
        .expect("bounded custom-schema validation");
    let report = admit_schema_diagnostics(
        schema,
        Some(&decoded.document),
        &findings,
        &[],
        &mut SchemaDiagnosticBudget::new(),
    )
    .expect("retained schema/document arenas admit every diagnostic");
    let issues = report
        .iter()
        .map(|finding| {
            let context = finding
                .context
                .map(|(name, field)| (name.into(), value(field)))
                .collect::<Map<_, _>>();
            json!({"phase": finding.phase, "condition": finding.condition,
                "requirement": finding.requirement, "spec_paths": [finding.path],
                "context": context})
        })
        .collect::<Vec<_>>();
    json!({"issues": issues})
}
