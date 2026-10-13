//! Independently authored full case reports and unchanged original CSV truth.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-case-predicates-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(
        &self,
        body: &str,
    ) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
        let path = self.0.join("spec.yaml");
        fs::write(&path, body).unwrap();
        let specification = path.to_str().unwrap();
        file_application::domain(
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "independent-case-predicates",
                specification,
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
fn findings(result: &file_application::Domain) -> Vec<serde_json::Value> {
    result
        .issues()
        .iter()
        .map(|issue| {
            let mut value = issue.as_value();
            assert!(value["context"].is_string());
            value["context"] = serde_json::from_str(&issue.context).unwrap();
            value
        })
        .collect()
}
const SOURCE: &str = r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: input.csv, types: {V: int}}}
output: {path: result.csv, columns: [ID, VALUE]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - name: VALUE
    type: int
    derivation:
      case:
        - when: "SRC.V > 0"
          then: {literal: 7}
        - when: 'FALSE'
          then: SRC.BAD
        - otherwise: {literal: 8}
"#;
#[test]
fn implicit_case_predicates_reduce_feeding_values_and_only_true_selects() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("input.csv"),
        "ID,V,BAD\na,1,hidden\na,,hidden\nb,,hidden\nc,-1,hidden\n",
    )
    .unwrap();
    let mut result = fixture.run(SOURCE).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([])
    );
    assert_eq!(
        result.observations()["verifications"],
        serde_json::json!([])
    );
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("result.csv")).unwrap(),
        b"ID,VALUE\na,7\nb,8\nc,8\n"
    );
}
#[test]
fn case_predicate_multiplicity_retains_its_complete_report_and_blocks_publication() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("input.csv"),
        "ID,V,BAD\na,1,hidden\na,2,hidden\nb,,hidden\n",
    )
    .unwrap();
    fs::write(fixture.0.join("result.csv"), "sentinel").unwrap();
    let mut result = fixture.run(SOURCE).unwrap();
    let issues = findings(&result);
    assert_eq!(
        issues,
        vec![
            serde_json::json!({"phase":"derivation","condition":"multiple_values_per_key","requirement":"REQ-0075","spec_paths":["columns.VALUE.derivation.case[0].when"],"context":{"identifier":"SRC.V","value_count":2,"keys":[{"ID":"a"}]}})
        ]
    );
    assert!(result.save().is_err());
    assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), b"sentinel");
}
#[test]
fn filtered_case_source_is_shared_by_columns_and_record_rows() {
    let body = SOURCE
        .replace("        - when: 'FALSE'\n          then: SRC.BAD\n", "")
        .replace("when: \"SRC.V > 0\"", "when: 'TRUE'")
        .replace(
            "then: {literal: 7}",
            "then: {source: {variable: SRC.ID, filter: 'SRC.V > 0'}}",
        )
        .replace("    type: int", "    type: str");
    for (rows, expected) in [
        ("", b"ID,VALUE\na,a\nb,\n".as_slice()),
        (
            "rows:\n  - id: records\n    dataset: SRC\n    derivations: {}\n",
            b"ID,VALUE\na,a\nb,\n".as_slice(),
        ),
    ] {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("input.csv"),
            "ID,V,BAD\na,1,hidden\nb,,hidden\n",
        )
        .unwrap();
        let mut result = fixture.run(&format!("{body}{rows}")).unwrap();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
    }
}

#[test]
fn qualified_case_predicate_and_full_pattern_capture_execute_in_each_expression_position() {
    let expression = "{case: [{when: 'SRC.V > 0', then: {str_extract: {source: SRC.TEXT, pattern: '(?<=prefix:)([A-Z]+)(?=$)', group: 1}}}, {otherwise: {literal: none}}]}";
    let prefix = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {V: int}}}\noutput: {path: result.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n";
    for (position, tail) in [
        ("column-case", format!("  - {{name: VALUE, type: str, derivation: {expression}}}\n")),
        ("record-default-case", format!("  - {{name: VALUE, type: str, derivation: {expression}}}\nrows: [{{id: records, dataset: SRC, derivations: {{}}}}]\n")),
        ("record-local-case", format!("  - {{name: VALUE, type: str}}\nrows: [{{id: records, dataset: SRC, derivations: {{VALUE: {expression}}}}}]\n")),
        ("grouped-local-case", format!("  - {{name: VALUE, type: str}}\nrows: [{{id: groups, dataset: SRC, group_by: [SRC.ID, SRC.V, SRC.TEXT], derivations: {{VALUE: {expression}}}}}]\n")),
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("input.csv"), "ID,V,TEXT\na,1,prefix:ONE\nb,0,prefix:TWO\n").unwrap();
        let mut result = fixture.run(&format!("{prefix}{tail}")).unwrap();
        assert!(result.issues().is_empty(), "{position}: {:?}", result.issues());
        assert!(result.save().unwrap(), "{position}");
        assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), b"ID,VALUE\na,ONE\nb,none\n", "{position}");
    }
}

#[test]
fn verification_predicate_and_literals_check_completed_column_case_and_row_outputs() {
    let prefix = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {V: int}}}\noutput: {path: result.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - name: VALUE\n    type: int\n    verifications:\n      - allowed_values: {values: [0, 1.0]}\n      - range: {min: 0.0, max: 1.5}\n";
    let expression = "{case: [{when: 'SRC.V > 0', then: {literal: 1}}, {otherwise: {literal: 0}}]}";
    for tail in [
        format!("    derivation: {expression}\n"),
        format!("rows: [{{id: records, dataset: SRC, derivations: {{VALUE: {expression}}}}}]\n"),
        format!("rows: [{{id: groups, dataset: SRC, group_by: [SRC.ID, SRC.V], derivations: {{VALUE: {expression}}}}}]\n"),
    ] {
        for predicate in ["VALUE >= 0", "VALUE =", "7"] {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("input.csv"), "ID,V\na,1\nb,0\n").unwrap();
            let declaration = if predicate == "7" { "7".into() } else { format!("'{predicate}'") };
            let built = fixture.run(&format!("{prefix}{tail}verifications: [{{assert: {{require: {declaration}}}}}]\n"));
            if predicate == "7" {
                let Err(issues) = built else { panic!("non-text public predicate was admitted") };
                assert_eq!(issues[0].condition, "invalid_field_type");
                assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0287"));
                assert_eq!(issues[0].spec_paths, ["verifications[0].assert.require"]);
                assert!(!fixture.0.join("result.csv").exists());
                continue;
            }
            let mut result = built.unwrap();
            if predicate == "VALUE >= 0" {
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), b"ID,VALUE\na,1\nb,0\n");
            } else {
                let issue = &result.issues()[0];
                assert_eq!(issue.condition, "invalid_predicate");
                assert_eq!(issue.spec_paths, ["verifications[0].assert.require"]);
                assert!(result.output().is_none());
                assert!(result.save().is_err());
                assert!(!fixture.0.join("result.csv").exists());
            }
        }
    }
}
#[test]
fn unchanged_original_case_and_filtered_source_truth_passes_in_full() {
    let benchmarks = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
    fn copy(source: &Path, target: &Path) {
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let path = target.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                fs::create_dir(&path).unwrap();
                copy(&entry.path(), &path);
            } else {
                fs::copy(entry.path(), path).unwrap();
            }
        }
    }
    let mut remaining = Vec::new();
    for name in [
        "adam-adae-protocol-review",
        "schema-expressions",
        "schema-predicates",
        "sdtm-ae-seriousness",
        "sdtm-mh-ongoing-conditions",
        "sdtm-tr-tumor-measurements",
        "sdtm-tu-tumor-identification",
        "sdtm-ds-reasons",
    ] {
        let fixture = Fixture::new();
        let source = benchmarks.join(name);
        copy(&source, &fixture.0);
        let authored = fs::read(source.join("spec.yaml")).unwrap();
        let mut result = match fixture.run(std::str::from_utf8(&authored).unwrap()) {
            Ok(result) => result,
            Err(issues) => {
                remaining.push(format!("{name}: {issues:?}"));
                continue;
            }
        };
        assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
        assert!(result.save().unwrap(), "{name}");
        for expected in fs::read_dir(source.join("expected")).unwrap() {
            let expected = expected.unwrap();
            if expected
                .path()
                .extension()
                .is_some_and(|extension| extension == "csv")
            {
                assert_eq!(
                    fs::read(fixture.0.join(expected.file_name())).unwrap(),
                    fs::read(expected.path()).unwrap(),
                    "{name}/{}",
                    expected.file_name().to_string_lossy()
                );
            }
        }
        assert_eq!(fs::read(source.join("spec.yaml")).unwrap(), authored);
    }
    assert!(
        remaining.is_empty(),
        "remaining original blockers: {}",
        remaining.join("\n")
    );
}

#[test]
fn independently_authored_installed_host_truth_has_the_same_complete_native_observations() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/case_predicates.json")).unwrap();
    let dispositions: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/case_predicate_dispositions.json")).unwrap();
    for name in dispositions["cases"].as_object().unwrap().keys() {
        assert!(
            truth["cases"].get(name).is_some(),
            "unknown disposition {name}"
        );
    }
    for (name, case) in truth["cases"].as_object().unwrap() {
        if let Some(disposition) = dispositions["cases"].get(name) {
            assert_eq!(
                disposition["classification"],
                "historical_private_reference_defect"
            );
            assert_eq!(case["csv"], disposition["historical_csv"]);
            assert_eq!(disposition["requirement"], "REQ-0148");
            continue;
        }
        let fixture = Fixture::new();
        for (name, contents) in case["inputs"].as_object().unwrap() {
            fs::write(fixture.0.join(name), contents.as_str().unwrap()).unwrap();
        }
        let mut result = fixture
            .run(case["specification"].as_str().unwrap())
            .unwrap();
        let findings = findings(&result);
        assert_eq!(
            serde_json::json!(findings),
            case.get("findings")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "{name}"
        );
        if let Some(csv) = case.get("csv") {
            assert!(result.save().unwrap(), "{name}");
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                csv.as_str().unwrap().as_bytes(),
                "{name}"
            );
        } else {
            assert!(result.output().is_none(), "{name}");
            assert!(result.save().is_err(), "{name}");
            assert!(!fixture.0.join("result.csv").exists(), "{name}");
        }
    }
}

fn lookup_fixture(name: &str) -> (Fixture, String) {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/case_predicates.json")).unwrap();
    let case = &truth["cases"][name];
    let fixture = Fixture::new();
    for (filename, contents) in case["inputs"].as_object().unwrap() {
        fs::write(fixture.0.join(filename), contents.as_str().unwrap()).unwrap();
    }
    (fixture, case["specification"].as_str().unwrap().into())
}

#[test]
fn assertion_lookup_reuses_current_selection_without_adding_derivation_handlers() {
    let (fixture, body) = lookup_fixture("assert_lookup_selected");
    let result = fixture.run(&body).unwrap();
    assert!(result.issues().is_empty());
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.VALUE.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.VALUE.derivation.source.no_match","handler":"no_match","count":1}
        ])
    );
}

#[test]
fn false_when_still_resolves_require_lookup_and_refuses_an_unhandled_absence() {
    let (fixture, body) = lookup_fixture("assert_lookup_only");
    let body = body
        .replace("    no_match: null\n", "")
        .replace("when: 'P.X IS NOT NULL'", "when: 'FALSE'");
    let mut result = fixture.run(&body).unwrap();
    assert!(
        !result.issues().is_empty(),
        "require must be evaluated on exempt rows"
    );
    let definition = yamaa_core::diagnostic::ConditionCode::DatasetUnmatchedKey.definition();
    assert_eq!(result.issues().len(), 1);
    assert_eq!(result.issues()[0].phase, definition.phase);
    assert_eq!(result.issues()[0].condition, definition.condition);
    assert_eq!(
        result.issues()[0].requirement.as_deref(),
        definition.requirement
    );
    assert!(result.output().is_none());
    assert!(result.save().is_err());
    assert!(!fixture.0.join("result.csv").exists());
}

#[test]
fn empty_output_validates_lookup_operand_types_without_selecting_a_donor() {
    let (fixture, body) = lookup_fixture("assert_lookup_empty");
    let body = body
        .replace("when: 'P.X IS NOT NULL'", "when: 'FALSE'")
        .replace(
            "require: 'VALUE = P.X AND P.X >= 10'",
            "require: \"P.X LIKE 'text%'\"",
        );
    let mut result = fixture.run(&body).unwrap();
    assert!(
        !result.issues().is_empty(),
        "empty output still admits declaration types"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([])
    );
    assert!(result.output().is_none());
    assert!(result.save().is_err());
}
