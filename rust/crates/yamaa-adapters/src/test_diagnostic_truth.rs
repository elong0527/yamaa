//! Frozen pre-migration wire expectations, independent of registry construction.
pub(crate) fn value(name: &str) -> yamaa_core::json::Value {
    let truth: yamaa_core::json::Value = crate::json_wire::decode(include_str!(
        "../tests/fixtures/diagnostic_literal_truth.json"
    ))
    .unwrap();
    truth.get(name).expect("known diagnostic fixture").clone()
}
