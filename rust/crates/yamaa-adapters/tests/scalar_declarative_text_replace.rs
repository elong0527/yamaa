#![cfg(feature = "yaml")]
//! `str_replace` through file application: the authored benchmarks, then
//! bespoke specifications for each binding, handler and failure route.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::json;
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::domain_entry::Request;
use yamaa_core::json::Value as Json;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks")
            .join(name);
        let path = std::env::temp_dir().join(format!(
            "yamaa-str-replace-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fn copy(source: &Path, target: &Path) {
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let dst = target.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    fs::create_dir(&dst).unwrap();
                    copy(&entry.path(), &dst);
                } else {
                    fs::copy(entry.path(), dst).unwrap();
                }
            }
        }
        copy(&source, &path);
        Self(path)
    }
    fn bespoke(specification: &Json, input: &str) -> Self {
        let fixture = Self::new("schema-str-replace");
        fs::write(
            fixture.0.join("spec.yaml"),
            yamaa_adapters::json_wire::encode(specification),
        )
        .unwrap();
        fs::write(fixture.0.join("input.csv"), input).unwrap();
        fixture
    }
    fn attempt(&self) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
        let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        file_application::domain(
            Request {
                specification: &specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "test",
                specification: &specification,
                base_directory: ".",
            },
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn issue_json(issue: &yamaa_adapters::issue_rows::Issue) -> Json {
    json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":issue.context.clone()})
}
fn issues(fixture: &Fixture) -> Vec<Json> {
    match fixture.attempt() {
        Ok(result) => result.issues().iter().map(issue_json).collect(),
        Err(issues) => issues.iter().map(issue_json).collect(),
    }
}
/// One `TEXT` input column and one `RESULT` output column derived from it.
fn specification(text_type: &str, result: Json) -> Json {
    json!({
        "schema_version": "1.0",
        "domain": "TEST",
        "keys": ["ID"],
        "input": {"SRC": "input.csv"},
        "output": {"path": "output.csv", "columns": ["ID", "TEXT", "RESULT"]},
        "columns": [
            {"name": "ID", "type": "str", "derivation": "SRC.ID"},
            {"name": "TEXT", "type": text_type, "derivation": "SRC.TEXT"},
            {"name": "RESULT", "type": "str", "derivation": result}
        ]
    })
}
fn saved(fixture: &Fixture) -> (String, Json) {
    let mut result = fixture
        .attempt()
        .unwrap_or_else(|issues| panic!("preparation {issues:?}"));
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    let handlers = result.observations()["handler_counts"].clone();
    assert!(result.save().unwrap());
    (
        fs::read_to_string(fixture.0.join("output.csv")).unwrap(),
        handlers,
    )
}
#[test]
fn authored_positive_benchmark_reproduces_its_expected_bytes() {
    let fixture = Fixture::new("schema-str-replace");
    let mut result = fixture
        .attempt()
        .unwrap_or_else(|issues| panic!("preparation {issues:?}"));
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert_eq!(
        result.observations()["handler_counts"],
        json!([{"spec_path":"columns.AESPDIG.derivation.str_replace.missing","handler":"missing","count":1}])
    );
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("adae.csv")).unwrap(),
        fs::read(fixture.0.join("expected/adae.csv")).unwrap()
    );
}
#[test]
fn authored_negative_benchmark_is_refused_before_any_input_read() {
    let fixture = Fixture::new("negative-str-replace-invalid-regex");
    fs::remove_dir_all(fixture.0.join("input")).unwrap();
    let found = issues(&fixture);
    assert_eq!(found.len(), 1, "{found:?}");
    let issue = &found[0];
    assert_eq!(issue["phase"], "validation");
    assert_eq!(issue["condition"], "invalid_regex");
    assert_eq!(issue["requirement"], "REQ-0827");
    assert_eq!(
        issue["spec_paths"],
        json!(["columns.AESPDIG.derivation.str_replace.pattern"])
    );
    assert_eq!(issue["context"]["pattern"], "[^0-9");
}
#[test]
fn values_without_a_match_are_kept_and_empty_results_stay_text() {
    let fixture = Fixture::bespoke(
        &specification(
            "str",
            json!({"str_replace": {"source": "TEXT", "pattern": "[^0-9]", "replacement": "", "missing": "none"}}),
        ),
        "ID,TEXT\nrow1,AE-0-12\nrow2,12\nrow3,NONE\nrow4,\n",
    );
    let (csv, handlers) = saved(&fixture);
    assert_eq!(
        csv,
        "ID,TEXT,RESULT\nrow1,AE-0-12,012\nrow2,12,12\nrow3,NONE,\"\"\nrow4,,none\n"
    );
    assert_eq!(
        handlers,
        json!([{"spec_path":"columns.RESULT.derivation.str_replace.missing","handler":"missing","count":1}])
    );
}
#[test]
fn empty_matches_and_literal_replacement_survive_file_application() {
    let fixture = Fixture::bespoke(
        &specification(
            "str",
            json!({"str_replace": {"source": "TEXT", "pattern": "a*", "replacement": "$&"}}),
        ),
        "ID,TEXT\nrow1,baa\nrow2,x\n",
    );
    let (csv, handlers) = saved(&fixture);
    assert_eq!(csv, "ID,TEXT,RESULT\nrow1,baa,$&b$&$&\nrow2,x,$&x$&\n");
    assert_eq!(handlers, json!([]));
}
#[test]
fn missing_source_without_handler_stops_with_the_row_keys() {
    let fixture = Fixture::bespoke(
        &specification(
            "str",
            json!({"str_replace": {"source": "TEXT", "pattern": "a", "replacement": "b"}}),
        ),
        "ID,TEXT\nrow1,\n",
    );
    assert_eq!(
        issues(&fixture),
        vec![json!({
            "phase": "mapping",
            "condition": "missing_input",
            "requirement": "REQ-0334",
            "spec_paths": ["columns.RESULT.derivation.str_replace"],
            "context": {"variable": "TEXT", "keys": [{"ID": "row1"}]}
        })]
    );
}
#[test]
fn non_string_and_unknown_sources_fail_validation_at_the_source() {
    let fixture = Fixture::bespoke(
        &specification(
            "int",
            json!({"str_replace": {"source": "TEXT", "pattern": "1", "replacement": "2"}}),
        ),
        "ID,TEXT\nrow1,12\n",
    );
    assert_eq!(
        issues(&fixture),
        vec![json!({
            "phase": "validation",
            "condition": "incompatible_input_type",
            "requirement": "REQ-0308",
            "spec_paths": ["columns.RESULT.derivation.str_replace.source"],
            "context": {"source": "TEXT", "expected": "str", "actual": "int"}
        })]
    );
    let fixture = Fixture::bespoke(
        &specification(
            "str",
            json!({"str_replace": {"source": "NOPE", "pattern": "1", "replacement": "2"}}),
        ),
        "ID,TEXT\nrow1,12\n",
    );
    let found = issues(&fixture);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["condition"], "unknown_field");
    assert_eq!(
        found[0]["spec_paths"],
        json!(["columns.RESULT.derivation.str_replace.source"])
    );
}
#[test]
fn nested_case_branches_and_row_templates_bind_the_same_operation() {
    let mut nested = specification(
        "str",
        json!({"case": [
            {"when": "ID = 'row1'", "then": {"str_replace": {"source": "TEXT", "pattern": "\\s+", "replacement": " "}}},
            {"otherwise": {"str_replace": {"source": "TEXT", "pattern": "^\\s+|\\s+$", "replacement": "", "missing": {"literal": null}}}}
        ]}),
    );
    let fixture = Fixture::bespoke(&nested, "ID,TEXT\nrow1,\"a  b\"\nrow2,\" c \"\nrow3,\n");
    let (csv, handlers) = saved(&fixture);
    assert_eq!(csv, "ID,TEXT,RESULT\nrow1,a  b,a b\nrow2, c ,c\nrow3,,\n");
    assert_eq!(
        handlers,
        json!([{"spec_path":"columns.RESULT.derivation.case[1].otherwise.str_replace.missing","handler":"missing","count":1}])
    );
    nested["columns"] = json!([
        {"name": "ID", "type": "str"},
        {"name": "TEXT", "type": "str"},
        {"name": "RESULT", "type": "str", "derivation": {"str_replace": {"source": "TEXT", "pattern": "-", "replacement": "/"}}}
    ]);
    nested["rows"] = json!([{
        "id": "records",
        "dataset": "SRC",
        "derivations": {
            "ID": "SRC.ID",
            "TEXT": {"str_replace": {"source": "SRC.TEXT", "pattern": "[a-z]", "replacement": "-", "missing": "x-y"}}
        }
    }]);
    let fixture = Fixture::bespoke(&nested, "ID,TEXT\nrow1,a1b\nrow2,\n");
    let (csv, handlers) = saved(&fixture);
    assert_eq!(csv, "ID,TEXT,RESULT\nrow1,-1-,/1/\nrow2,x-y,x/y\n");
    assert_eq!(
        handlers,
        json!([{"spec_path":"rows[0].derivations.TEXT.str_replace.missing","handler":"missing","count":1}])
    );
}
