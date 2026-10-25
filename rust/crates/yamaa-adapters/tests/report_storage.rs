//! Report metadata admission and diagnostic replacement through the storage API.
use yamaa_adapters::{json, json_wire, report_storage::StoredReport};
use yamaa_core::diagnostic::{ConditionCode, Diagnostic};
use yamaa_core::json::{Map, Value};

#[test]
fn metadata_is_admitted_before_diagnostics_are_recorded() {
    let cases = json_wire::decode(include_str!("fixtures/report_metadata.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let result = StoredReport::with_tables(case["metadata"].clone(), Vec::new());
        let actual = match result {
            Ok(mut report) => {
                report.set_diagnostics(vec![Diagnostic {
                    code: ConditionCode::SourceUnknownReference,
                    spec_paths: vec!["input.SRC".into()],
                    context: Map::new(),
                    source_span: None,
                    operand_route: None,
                }]);
                json!({
                    "report": report.observations_value(false),
                    "diagnostics": Value::Array(crate::support::rows(report.diagnostics())),
                })
            }
            Err(error) => json!({"error": format!("{error:?}")}),
        };
        assert_eq!(
            actual,
            case["expected"],
            "{}",
            case["name"].as_str().unwrap()
        );
    }
}
