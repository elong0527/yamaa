#[path = "../examples/support/schema_declaration.rs"]
mod fixture;
use serde_json::json;

const SCHEMA: &str = "version: '1.0'\nroot_class:\n  - needed: {type: str, required: true}\n  - count: {type: int}\n  - items: {type: 'list[int]'}\n";

#[test]
fn custom_versionless_class_preserves_ordered_required_type_and_unknown_findings() {
    assert_eq!(
        fixture::observe(SCHEMA, "count: true\nextra: null\n"),
        json!({"issues": [
            {"phase":"validation", "condition":"missing_required_field", "requirement":"REQ-0285",
                "spec_paths":["needed"], "context":{"field":"needed", "class":"root_class"}},
            {"phase":"validation", "condition":"invalid_field_type", "requirement":"REQ-0287",
                "spec_paths":["count"], "context":{"expected":"int", "actual":"bool"}},
            {"phase":"validation", "condition":"unknown_field", "requirement":"REQ-0285",
                "spec_paths":["extra"], "context":{"field":"extra", "class":"root_class"}}
        ]})
    );
}

#[test]
fn custom_list_member_path_and_integer_identity_are_native_owned() {
    assert_eq!(
        fixture::observe(
            SCHEMA,
            "needed: kept\nitems: [1, false, 9223372036854775808]\n"
        ),
        json!({"issues": [{"phase":"validation", "condition":"invalid_field_type",
            "requirement":"REQ-0287", "spec_paths":["items[1]"],
            "context":{"expected":"int", "actual":"bool"}}]})
    );
    assert_eq!(
        fixture::observe(
            SCHEMA,
            "needed: kept\ncount: 9223372036854775808\nitems: [1, 2]\n"
        ),
        json!({"issues": []})
    );
}

#[test]
fn declared_document_version_is_a_custom_field_without_a_shipped_version_gate() {
    let schema = "version: '1.0'\nroot_class:\n  - schema_version: {type: str, required: true}\n  - name: {type: str, required: true}\n";
    assert_eq!(
        fixture::observe(schema, "schema_version: custom\nname: kept\n"),
        json!({"issues": []})
    );
}
