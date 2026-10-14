#![cfg(any(unix, windows))]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-log-witness-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("input")).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn benchmark() -> (Self, String) {
        let study = Self::new();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks/schema-verification-log");
        fs::copy(source.join("input/dm.csv"), study.0.join("input/dm.csv")).unwrap();
        let raw = fs::read_to_string(source.join("spec.yaml")).unwrap();
        (study, raw)
    }
    fn build(&self, raw: &str) -> file_application::Domain {
        fs::write(self.0.join("spec.yaml"), raw).unwrap();
        let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        file_application::domain(
            Request {
                specification: &path,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "logs",
                specification: &path,
                base_directory: ".",
            },
        )
        .unwrap()
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
const VERIFICATION_HEADER: &str = "REPORT_VERSION,ARTIFACT,SPEC_PATH,VERIFICATION_ID,CHECK,TARGET,REQUIREMENT,SEVERITY,OUTCOME,CONDITION,EVALUATED_COUNT,FAILURE_COUNT,DETAILS\n";
const WARNING_HEADER: &str = "LOG_VERSION,ARTIFACT,SEVERITY,CONDITION,REQUIREMENT,SPEC_PATH,VERIFICATION_ID,FAILURE_COUNT,OFFENDING_KEYS,DETAILS\n";
#[test]
fn original_verification_benchmark_matches_three_unchanged_goldens_without_build_writes() {
    let (study, raw) = Study::benchmark();
    let mut result = study.build(&raw);
    assert!(result.issues().is_empty());
    assert!(result.output().is_some());
    assert_eq!(result.log_table(false).unwrap().rows(), 4);
    assert_eq!(result.log_table(true).unwrap().rows(), 1);
    assert_eq!(result.observations()["artifacts"], serde_json::json!([]));
    for name in ["adsl.csv", "adsl-checks.csv", "adsl-warnings.csv"] {
        assert!(!study.0.join(name).exists());
    }
    fs::write(study.0.join("input/dm.csv"), "changed").unwrap();
    fs::remove_file(study.0.join("spec.yaml")).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../benchmarks/schema-verification-log/expected");
    for _ in 0..2 {
        assert!(result.save().unwrap());
        for name in ["adsl.csv", "adsl-checks.csv", "adsl-warnings.csv"] {
            assert_eq!(
                fs::read(study.0.join(name)).unwrap(),
                fs::read(root.join(name)).unwrap(),
                "{name}"
            );
        }
        assert_eq!(
            result.observations()["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["adsl-checks", "adsl-warnings", "adsl"]
        );
        let mut expected: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/specs/schema-verification-log.json"))
                .unwrap();
        expected["runtime"] = serde_json::json!("test");
        expected["runtime_version"] = serde_json::json!("test");
        expected["engine_version"] = serde_json::json!(yamaa_core::VERSION);
        expected["example"] = serde_json::json!("logs");
        for group in ["nodes", "tables", "verifications", "callbacks"] {
            for record in expected[group].as_array_mut().unwrap() {
                record["specification"] =
                    serde_json::json!(study.0.join("spec.yaml").to_str().unwrap());
            }
        }
        assert_eq!(result.observations(), expected);
    }
}
#[test]
fn failed_save_publishes_only_retained_verification_and_preserves_primary_and_warning() {
    let (study, raw) = Study::benchmark();
    let mut result = study.build(&raw.replace("severity: warning", "severity: error"));
    assert!(result.output().is_none());
    assert!(result.warning_log().is_none());
    assert_eq!(result.log_table(false).unwrap().rows(), 2);
    assert_eq!(result.issues().len(), 1);
    assert_eq!(result.issues()[0].condition, "range_failed");
    for name in ["adsl.csv", "adsl-checks.csv", "adsl-warnings.csv"] {
        fs::write(study.0.join(name), "sentinel").unwrap();
    }
    fs::remove_file(study.0.join("input/dm.csv")).unwrap();
    fs::remove_file(study.0.join("spec.yaml")).unwrap();
    for _ in 0..2 {
        assert!(result.save().is_err());
        assert_eq!(fs::read(study.0.join("adsl.csv")).unwrap(), b"sentinel");
        assert_eq!(
            fs::read(study.0.join("adsl-warnings.csv")).unwrap(),
            b"sentinel"
        );
        let expected = format!("{VERIFICATION_HEADER}1.0,adsl.csv,columns.AGE.verifications[0].not_missing,,not_missing,AGE,REQ-0375,error,held,,3,0,{{}}\n1.0,adsl.csv,columns.AGE.verifications[1].range,,range,AGE,REQ-0377,error,violated,range_failed,3,1,\"{{\"\"column\"\":\"\"AGE\"\"}}\"\n");
        assert_eq!(
            fs::read(study.0.join("adsl-checks.csv")).unwrap(),
            expected.as_bytes()
        );
        assert_eq!(
            result.observations()["artifacts"].as_array().unwrap().len(),
            1
        );
        assert_eq!(result.issues().len(), 1);
    }
}
#[test]
fn primary_encoding_limit_retains_completed_verification_without_warning_authority() {
    let study = Study::new();
    fs::write(study.0.join("input/input.csv"), "ID,VALUE\n1,0.125\n").unwrap();
    let mut result = study.build("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input/input.csv, types: {VALUE: float}}}\noutput: {path: out.csv, columns: [ID, VALUE], decimals: 999999999999999999999999999999, warning_log: warnings.csv, verification_log: checks.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: float\n    derivation: SRC.VALUE\n    verifications:\n      - not_missing: {}\n      - range: {max: 0, severity: warning}\n");
    assert!(result.output().is_none());
    assert!(result.warning_log().is_none());
    assert_eq!(result.log_table(false).unwrap().rows(), 2);
    assert_eq!(result.issues().len(), 1);
    assert_eq!(result.issues()[0].condition, "engine_rejected");
    let context: serde_json::Value = serde_json::from_str(&result.issues()[0].context).unwrap();
    assert_eq!(context["stage"], "output");
    assert_eq!(context["code"], "output_limit");
    fs::write(study.0.join("out.csv"), "primary sentinel").unwrap();
    fs::write(study.0.join("warnings.csv"), "warning sentinel").unwrap();
    fs::remove_file(study.0.join("input/input.csv")).unwrap();
    fs::remove_file(study.0.join("spec.yaml")).unwrap();
    for _ in 0..2 {
        assert!(result.save().is_err());
        assert_eq!(
            fs::read(study.0.join("out.csv")).unwrap(),
            b"primary sentinel"
        );
        assert_eq!(
            fs::read(study.0.join("warnings.csv")).unwrap(),
            b"warning sentinel"
        );
        let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
        assert!(checks.starts_with(VERIFICATION_HEADER));
        assert!(checks.contains("not_missing,VALUE,REQ-0375,error,held"));
        assert!(checks.contains("range,VALUE,REQ-0377,warning,violated,range_failed"));
        assert_eq!(
            result.observations()["artifacts"].as_array().unwrap().len(),
            1
        );
        assert_eq!(result.issues()[0].condition, "engine_rejected");
    }
}

#[test]
fn whole_target_preflight_precedes_first_log_write_and_can_retry_held_bytes() {
    let (study, raw) = Study::benchmark();
    let mut result = study.build(&raw.replace("path: adsl.csv", "path: absent/adsl.csv"));
    fs::write(study.0.join("adsl-checks.csv"), "old checks").unwrap();
    fs::write(study.0.join("adsl-warnings.csv"), "old warnings").unwrap();
    assert!(!result.save().unwrap());
    assert_eq!(
        fs::read(study.0.join("adsl-checks.csv")).unwrap(),
        b"old checks"
    );
    assert_eq!(
        fs::read(study.0.join("adsl-warnings.csv")).unwrap(),
        b"old warnings"
    );
    assert_eq!(result.observations()["artifacts"], serde_json::json!([]));
    fs::create_dir(study.0.join("absent")).unwrap();
    assert!(result.save().unwrap());
    assert!(result.issues().is_empty());
    assert_eq!(
        result.observations()["artifacts"].as_array().unwrap().len(),
        3
    );
}
#[test]
fn empty_declared_logs_replace_stale_findings_with_typed_headers() {
    let study = Study::new();
    fs::write(study.0.join("input/input.csv"), "ID\n1\n").unwrap();
    let mut result = study.build("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input/input.csv}\noutput: {path: out.csv, columns: [ID], warning_log: warnings.csv, verification_log: checks.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n");
    assert!(result.issues().is_empty());
    for warning in [false, true] {
        let table = result.log_table(warning).unwrap();
        assert_eq!(table.rows(), 0);
    }
    for name in ["warnings.csv", "checks.csv"] {
        fs::write(study.0.join(name), "stale").unwrap();
    }
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(study.0.join("checks.csv")).unwrap(),
        VERIFICATION_HEADER.as_bytes()
    );
    assert_eq!(
        fs::read(study.0.join("warnings.csv")).unwrap(),
        WARNING_HEADER.as_bytes()
    );
}

#[test]
fn warning_prefix_survives_a_later_derivation_failure_without_warning_authority() {
    let (study, raw) = Study::benchmark();
    let raw = raw.replace("\nverifications:\n", "\n  - name: FAIL\n    type: float\n    derivation: {compute: {expr: '1 / 0'}}\n\nverifications:\n");
    let mut result = study.build(&raw);
    assert!(result.output().is_none());
    assert!(result.warning_log().is_none());
    assert_eq!(result.log_table(false).unwrap().rows(), 2);
    let report = result.observations();
    assert_eq!(report["verifications"][1]["severity"], "warning");
    assert_eq!(
        report["verifications"][1]["failure"]["condition"],
        "range_failed"
    );
    assert_eq!(result.issues().len(), 1);
    assert_eq!(result.issues()[0].condition, "division_by_zero");
    assert!(result.save().is_err());
    assert!(study.0.join("adsl-checks.csv").exists());
    assert!(!study.0.join("adsl-warnings.csv").exists());
    assert!(!study.0.join("adsl.csv").exists());
    assert_eq!(
        result.observations()["artifacts"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn parquet_logs_preserve_governed_logical_rows_and_integer_columns() {
    let (study, raw) = Study::benchmark();
    let mut result = study.build(
        &raw.replace("adsl-checks.csv", "checks.parquet")
            .replace("adsl-warnings.csv", "warnings.parquet"),
    );
    assert!(result.issues().is_empty());
    assert!(result.save().unwrap());
    let report = result.observations();
    for (name, rows, integer_columns) in [("checks", 4, vec![10, 11]), ("warnings", 1, vec![7])] {
        let artifact = report["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == name)
            .unwrap();
        assert_eq!(artifact["profile"], "parquet");
        assert_eq!(artifact["row_count"], rows);
        assert_eq!(artifact["content"], "");
        let table = result.log_table(name == "warnings").unwrap();
        for column in integer_columns {
            assert!(matches!(
                table.columns()[column].1,
                yamaa_adapters::public_table::Values::Int(_)
            ));
        }
        assert_eq!(
            fs::read(study.0.join(format!("{name}.parquet")))
                .unwrap()
                .get(..4),
            Some(b"PAR1".as_slice())
        );
    }
    let warning: serde_json::Value =
        serde_json::from_str(report["artifacts"][1]["records"][1].as_str().unwrap()).unwrap();
    assert_eq!(warning[7], 1);
    assert_eq!(warning[8], "[{\"USUBJID\":\"S-2\"}]");
}
