use std::sync::Arc;
use yamaa_adapters::specification_source::{CapturedSchema, Source};

pub fn environment_schema() -> Arc<CapturedSchema> {
    CapturedSchema::admit_root(
        vec![
            Source {
                identity: "schema_environment.yaml".into(),
                bytes: include_bytes!("../fixtures/environment-candidate/schema_environment.yaml")
                    .to_vec(),
            },
            Source {
                identity: "schema_shared.yaml".into(),
                bytes: include_bytes!("../fixtures/environment-candidate/schema_shared.yaml")
                    .to_vec(),
            },
        ],
        0,
        "environment_class",
        Default::default(),
    )
    .unwrap()
}

pub fn domain_schema() -> Arc<CapturedSchema> {
    // Exercise the pending versionless format in a held candidate closure. This
    // does not qualify or change the authoritative installed domain schema.
    let shipped = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut modules = shipped.sources().to_vec();
    let function_module = modules
        .iter_mut()
        .find(|module| module.identity == "schema_function.yaml")
        .unwrap();
    let text = std::str::from_utf8(&function_module.bytes).unwrap();
    let old = "        - contract_version:\n            type: function_contract_version\n            required: true\n            description: See REQ-1085.\n";
    assert!(text.contains(old));
    function_module.bytes = text.replacen(old, "", 1).into_bytes();
    CapturedSchema::admit(modules, 0, Default::default()).unwrap()
}
