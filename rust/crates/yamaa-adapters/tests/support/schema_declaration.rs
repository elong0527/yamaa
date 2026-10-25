//! Project only core-admitted findings from the retained authored arenas.
use yamaa_adapters::json;
use yamaa_adapters::{
    specification_source::{CapturedSchema, Source},
    yaml_decode::decode_yaml,
};
use yamaa_core::json::{Map, Number, Value};
use yamaa_core::schema::{admit_schema_diagnostics, SchemaDiagnosticValue as V};

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

/// The issues of validating `document` against the class `root` names, or
/// the entry's root class, of the schema closure `modules`: the entry module
/// first, each include identity spelt as authored.
pub fn observe(modules: Vec<(String, String)>, document: &str, root: Option<&str>) -> Value {
    let sources = modules
        .into_iter()
        .map(|(identity, text)| Source {
            identity,
            bytes: text.into_bytes(),
        })
        .collect();
    let captured = match root {
        Some(root) => CapturedSchema::admit_root(sources, 0, root),
        None => CapturedSchema::admit(sources, 0),
    }
    .expect("admitted authored schema closure");
    let decoded = decode_yaml(document.as_bytes()).expect("decoded authored document");
    let schema = captured.structure();
    let types = schema
        .parse_query_types(&[schema.root_class().name.clone()])
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
        )
        .expect("admitted custom-schema validation");
    let report = admit_schema_diagnostics(schema, Some(&decoded.document), &findings, &[])
        .expect("retained schema/document arenas admit every diagnostic");
    let issues = report
        .iter()
        .map(|finding| {
            let context = finding
                .context
                .map(|(name, field)| (name.into(), value(field)))
                .collect::<Map<String, _>>();
            json!({"phase": finding.phase, "condition": finding.condition,
                "rule": finding.rule, "spec_paths": [finding.path],
                "context": context})
        })
        .collect::<Vec<_>>();
    json!({"issues": issues})
}
