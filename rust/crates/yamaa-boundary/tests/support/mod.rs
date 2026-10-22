//! The shared host truth that the families decode,
//! `rust/tests/host-fixtures/cases.json`, read once for the whole binary.
use std::sync::LazyLock;

use serde_json::Value;

static TRUTH: LazyLock<Value> = LazyLock::new(|| {
    let truth: Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    assert_eq!(truth["version"], "1", "the installed-host fixture version");
    truth
});

/// One family of the shared truth.
pub fn family(name: &str) -> &'static Value {
    let family = &TRUTH["families"][name];
    assert!(family.is_object(), "cases.json has no family {name}");
    family
}

/// The text items of a family's list under `key`, in order.
pub fn texts(family: &Value, key: &str) -> Vec<String> {
    family[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}
