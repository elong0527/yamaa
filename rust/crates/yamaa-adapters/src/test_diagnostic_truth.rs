//! Frozen pre-migration wire expectations, independent of registry construction.
pub(crate) fn value(name: &str) -> yamaa_core::json::Value {
    let truth: yamaa_core::json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/diagnostic_literal_truth.json"
    ))
    .unwrap();
    truth.get(name).expect("known diagnostic fixture").clone()
}
#[cfg(feature = "yaml")]
pub(crate) fn text(name: &str, field: &str) -> String {
    value(name)[field]
        .as_str()
        .expect("text diagnostic fixture")
        .to_owned()
}
