#[path = "../examples/support/odm_declaration.rs"]
mod fixture;
use serde_json::{json, Value};

fn spec(derivation: Value) -> Value {
    json!({"schema_version":"1.0", "domain":"RESULT", "keys":["USUBJID"],
        "input":{"ODM":"never-read.csv"},
        "output":{"path":"never-written.csv","columns":["USUBJID","AGE"]},
        "columns":[{"name":"USUBJID","type":"str","label":"Subject", "derivation":"ODM.SubjectKey"},
            {"name":"AGE","type":"int","label":"Age","derivation":derivation}]})
}
fn header() -> Vec<String> {
    [
        "StudyOID",
        "MetaDataVersionOID",
        "SubjectKey",
        "StudyEventOID",
        "StudyEventRepeatKey",
        "FormOID",
        "FormRepeatKey",
        "ItemGroupOID",
        "ItemGroupRepeatKey",
        "ItemOID",
        "Value",
    ]
    .map(String::from)
    .into()
}
fn observe(spec: Value, headers: &[Vec<String>]) -> Value {
    fixture::observe(&spec.to_string(), headers)
}

#[test]
fn canonical_catalog_excludes_vendor_fields_and_header_case_is_native_owned() {
    let mut names = header()
        .iter()
        .map(|name| name.to_uppercase())
        .collect::<Vec<_>>();
    names.push("SITEID".into());
    let observed = observe(spec(json!({"odm":"ODM.IT.DM.AGE"})), &[names]);
    assert_eq!(observed["status"], "prepared");
    assert_eq!(observed["binding"]["status"], "bound");
    assert_eq!(observed["fields"], json!(header()));
    assert_eq!(
        observed["catalog"]["ODM"],
        json!(header()
            .iter()
            .map(|name| (name, "str"))
            .collect::<Vec<_>>())
    );
}

#[test]
fn overridden_partial_column_preserves_scope_fact_and_unrelated_binding_refusal() {
    let mut source = spec(json!({"odm":"ODM.IT.DM.AGE"}));
    source["input"]["DM"] = json!("never-read-other.csv");
    source["rows"] =
        json!([{"id":"other_source", "dataset":"DM", "derivations":{"AGE":{"literal":0}}}]);
    // serde_json's authored fixture text orders input DM before ODM.
    let observed = observe(source.clone(), &[vec!["USUBJID".into()], header()]);
    assert_eq!(observed["status"], "prepared");
    assert_eq!(
        observed["binding"]["issues"][0]["condition"],
        "no_applicable_keys"
    );
    source["rows"][0]["derivations"] = json!({});
    let observed = observe(source, &[vec!["USUBJID".into()], header()]);
    assert_eq!(observed["issues"][0]["condition"], "invalid_odm_context");
    assert_eq!(
        observed["issues"][0]["context"],
        json!({"dataset":"ODM","location":"column"})
    );
    assert_eq!(
        observed["issues"][0]["spec_paths"],
        json!(["columns.AGE.derivation.odm"])
    );
}

#[test]
fn intrinsic_vendor_filter_and_intermediate_derive_contexts_precede_metadata() {
    let observed = observe(
        spec(json!({"odm":{"item":"ODM.IT.DM.AGE","filter":"ODM.SITEID = '9'"}})),
        &[],
    );
    assert_eq!(observed["issues"][0]["condition"], "unknown_field");
    assert_eq!(
        observed["issues"][0]["spec_paths"],
        json!(["columns.AGE.derivation.odm.filter"])
    );
    let mut source = spec(json!({"literal":1}));
    source["intermediates"] =
        json!([{"id":"AGES","dataset":"ODM","derivations":{"AGE":{"odm":"ODM.IT.DM.AGE"}}}]);
    let observed = observe(source, &[]);
    assert_eq!(observed["issues"][0]["condition"], "invalid_odm_context");
    assert_eq!(
        observed["issues"][0]["context"],
        json!({"dataset":"ODM","location":"intermediate"})
    );
    assert_eq!(
        observed["issues"][0]["spec_paths"],
        json!(["intermediates[0].derivations.AGE.odm"])
    );
    let observed = observe(
        spec(
            json!({"aggregate":{"derive":[{"name":"VALUE","type":"int","derivation":{"odm":"ODM.IT.DM.AGE"}}],"expr":"SUM(VALUE)"}}),
        ),
        &[],
    );
    assert_eq!(observed["issues"][0]["condition"], "invalid_odm_context");
    assert_eq!(
        observed["issues"][0]["context"],
        json!({"dataset":"ODM","location":"derive"})
    );
    assert_eq!(
        observed["issues"][0]["spec_paths"],
        json!(["columns.AGE.derivation.aggregate.derive[0].derivation.odm"])
    );
}

#[test]
fn missing_fixed_fields_and_declared_types_ordinal_preserve_exact_causes() {
    let missing = header()
        .into_iter()
        .filter(|name| name != "FormOID" && name != "FormRepeatKey")
        .collect::<Vec<_>>();
    let observed = observe(spec(json!({"odm":"ODM.IT.DM.AGE"})), &[missing]);
    assert_eq!(
        observed["issues"][0]["condition"],
        "odm_schema_field_missing"
    );
    assert_eq!(
        observed["issues"][0]["context"],
        json!({"dataset":"ODM","fields":["FormOID","FormRepeatKey"]})
    );
    for (field, value) in [
        ("types", json!({"Value":"int"})),
        ("ordinal", json!("SourceOrdinal")),
    ] {
        let mut source = spec(json!({"odm":"ODM.IT.DM.AGE"}));
        source["input"]["ODM"] = json!({"path":"never-read.csv",field:value});
        let observed = observe(source, &[]);
        assert_eq!(observed["issues"][0]["condition"], "odm_schema_field_type");
        assert_eq!(
            observed["issues"][0]["context"],
            json!({"dataset":"ODM","declared":field})
        );
        assert_eq!(
            observed["issues"][0]["spec_paths"],
            json!([format!("input.ODM.{field}")])
        );
    }
}
