//! Original verification witnesses from python/tests/verification/test_checks.py
//! at c7b88a4f, before the reference evaluator retired in #2110.
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-original-verification-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn build(&self, columns: Value, checks: Value, input: &str) -> file_application::Domain {
        let output = columns
            .as_array()
            .unwrap()
            .iter()
            .map(|column| column["name"].clone())
            .collect::<Vec<_>>();
        let input_source = if output.iter().any(|name| name == "VAL") {
            json!({"path":"input.csv","types":{"VAL":"int"}})
        } else {
            json!({"path":"input.csv"})
        };
        let document = json!({
            "schema_version":"1.0", "domain":"OUT", "base":"SRC",
            "keys":["STUDYID","USUBJID"],
            "input":{"SRC":input_source},
            "columns":columns, "verifications":checks,
            "output":{"path":"out.csv","columns":output}
        });
        let entry = self.0.join("spec.yaml");
        fs::write(&entry, serde_json::to_vec(&document).unwrap()).unwrap();
        fs::write(self.0.join("input.csv"), input).unwrap();
        let entry = entry.to_str().unwrap();
        file_application::domain(
            Request {
                specification: entry,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "original-verification",
                specification: entry,
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
fn columns() -> Value {
    json!([
        {"name":"STUDYID","type":"str","derivation":"SRC.STUDYID"},
        {"name":"USUBJID","type":"str","derivation":"SRC.USUBJID"}
    ])
}

#[test]
fn fractional_row_count_on_an_empty_artifact_is_zero() {
    for (bound, passes) in [
        (json!({"max_fraction":0}), true),
        (json!({"min_fraction":0.1}), false),
    ] {
        let study = Study::new();
        let mut result = study.build(columns(), json!([{"row_count":bound}]), "STUDYID,USUBJID\n");
        if passes {
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.output().is_some());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(study.0.join("out.csv")).unwrap(),
                "STUDYID,USUBJID\n"
            );
        } else {
            assert!(result.output().is_none());
            let issues = result.issues();
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].condition, "row_count_failed");
            assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0385"));
            assert_eq!(issues[0].spec_paths, ["verifications[0].row_count"]);
            let context: Value = serde_json::from_str(&issues[0].context).unwrap();
            assert_eq!(context["denominator"], 0);
            assert_eq!(context["count"], 0);
            assert_eq!(context["keys"], json!([{}]));
            assert!(result.save().is_err());
            assert!(!study.0.join("out.csv").exists());
        }
    }
}

#[test]
fn assertion_when_exempts_false_and_unknown_rows_but_require_must_be_true() {
    let study = Study::new();
    let mut columns = columns();
    columns.as_array_mut().unwrap().extend([
        json!({"name":"GATE","type":"str","derivation":"SRC.GATE"}),
        json!({"name":"VAL","type":"int","derivation":"SRC.VAL"}),
    ]);
    let mut result = study.build(
        columns,
        json!([{"assert":{"when":"GATE = 'Y'","require":"VAL > 0"}}]),
        "STUDYID,USUBJID,GATE,VAL\nS,S-01,Y,1\nS,S-02,Y,0\nS,S-03,Y,\nS,S-04,N,0\nS,S-05,,0\n",
    );
    let issues = result.issues();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition, "assert_failed");
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0384"));
    assert_eq!(issues[0].spec_paths, ["verifications[0].assert"]);
    let context: Value = serde_json::from_str(&issues[0].context).unwrap();
    assert_eq!(
        context["keys"],
        json!([
            {"STUDYID":"S","USUBJID":"S-02"},
            {"STUDYID":"S","USUBJID":"S-03"}
        ])
    );
    assert!(result.output().is_none());
    assert!(result.save().is_err());
    assert!(!study.0.join("out.csv").exists());
}
