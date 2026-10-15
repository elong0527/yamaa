//! Independent YAML, donor bytes and exact saved truth for per-record aggregates.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-aggregate-records-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
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
    fn save(&self) -> Vec<u8> {
        let mut domain = self.run().unwrap_or_else(|issues| panic!("{issues:?}"));
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        fs::read(self.0.join("result.csv")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn specification(operation: &str) -> String {
    format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput:\n  A: {{path: a.csv, types: {{CUT: int}}}}\n  B: {{path: b.csv, types: {{N: int}}}}\nbase: A\noutput: {{path: result.csv, columns: [ID, CUT, TOTAL]}}\ncolumns:\n  - {{name: ID, type: str, derivation: A.ID}}\n  - {{name: CUT, type: int, derivation: A.CUT}}\n  - name: TOTAL\n    type: float\n    derivation:\n      aggregate:\n{operation}")
}
#[test]
fn ordered_converted_bindings_reduce_only_matched_filtered_inclusive_records() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("a.csv"), b"ID,CUT\na,3\nb,2\nc,\nd,1\n").unwrap();
    fs::write(fixture.0.join("b.csv"),b"ID,N,V,USE\na,2,1.5,Y\na,3,2,Y\na,4,invalid,Y\na,1,invalid,N\nb,2,bad,Y\nc,1,99,Y\nd,2,99,Y\n").unwrap();
    fs::write(fixture.0.join("spec.yaml"),specification("        filter: B.USE = 'Y'\n        between: {value: CUT, lower: N}\n        derive:\n          - name: VALUE\n            type: float\n            derivation: {value: {source: B.V}, unconvertible: 0}\n          - name: TWICE\n            type: float\n            derivation: {compute: {expr: 'VALUE * 2'}}\n        expr: SUM(TWICE)\n")).unwrap();
    assert_eq!(fixture.save(), b"ID,CUT,TOTAL\na,3,7\nb,2,0\nc,,\nd,1,\n");
}
#[test]
fn aggregate_bindings_admit_extractions_and_templates_before_binding() {
    for (derivation, kind, expression) in [
        (
            "{str_extract: {source: B.V, pattern: '([0-9]+)$', group: 1}}",
            "int",
            "MAX(V)",
        ),
        (
            "{case: [{when: \"B.USE = 'Y'\", then: {str_extract: {source: B.V, pattern: '([0-9]+)$', group: 1}}}, {otherwise: {literal: 0}}]}",
            "int",
            "MAX(V)",
        ),
        ("{str_template: '{B.V}!'}", "str", "COUNT(V)"),
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("a.csv"), b"ID,CUT\na,3\nb,2\n").unwrap();
        fs::write(
            fixture.0.join("b.csv"),
            b"ID,N,V,USE\na,1,PERIOD 1,Y\na,2,PERIOD 2,Y\nb,1,PERIOD 1,Y\n",
        )
        .unwrap();
        fs::write(
            fixture.0.join("spec.yaml"),
            specification(&format!(
                "        derive:\n          - {{name: V, type: {kind}, derivation: {derivation}}}\n        expr: {expression}\n"
            )),
        )
        .unwrap();
        let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        let issues = file_application::check(
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
        );
        assert!(issues.is_empty(), "{derivation}: {issues:?}");
        assert_eq!(fixture.save(), b"ID,CUT,TOTAL\na,3,2\nb,2,1\n", "{derivation}");
    }
}
#[test]
fn original_public_aggregate_fixtures_preserve_frozen_csv_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
    fn copy(from: &Path, to: &Path) {
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == "expected" {
                continue;
            }
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                fs::create_dir(&target).unwrap();
                copy(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    for (name, result) in [
        ("schema-aggregation", "adsl.csv"),
        ("schema-derive-date-epoch", "adex.csv"),
        ("schema-derive-text-score", "adqs.csv"),
        ("schema-aggregate-derive-intermediate", "adex.csv"),
        ("adam-adsl-new-therapy", "adsl.csv"),
        ("adam-adtr-nadir", "adtr.csv"),
        ("adam-adtte-pfs", "adtte.csv"),
        ("sdtm-rs-timepoint-response", "rs.csv"),
    ] {
        let fixture = Fixture::new();
        copy(&root.join(name), &fixture.0);
        let mut domain = fixture
            .run()
            .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
        assert!(domain.issues().is_empty(), "{name}: {:?}", domain.issues());
        assert!(domain.save().unwrap(), "{name}");
        assert_eq!(
            fs::read(fixture.0.join(result)).unwrap(),
            fs::read(root.join(name).join("expected").join(result)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn derive_and_range_faults_fail_before_any_output_with_registered_causes() {
    for (operation,condition,requirement) in [
        ("        derive: []\n        expr: SUM(V)\n","invalid_derive_step","REQ-1189"),
        ("        derive:\n          - {name: V, type: float, derivation: W}\n        expr: SUM(V)\n","unknown_derive_variable","REQ-1189"),
        ("        derive:\n          - {name: V, type: float, derivation: {literal: 1}}\n        expr: SUM(V)\n","invalid_derive_step","REQ-1191"),
        ("        between: {value: CUT, lower: USE}\n        expr: SUM(B.N)\n","incomparable_range_types","REQ-0121"),
        ("        between: {value: CUT, lower: ABSENT}\n        expr: SUM(B.N)\n","unknown_field","REQ-0121"),
    ] {
        let fixture=Fixture::new();
        fs::write(fixture.0.join("a.csv"),b"ID,CUT\na,2\n").unwrap();
        fs::write(fixture.0.join("b.csv"),b"ID,N,USE\na,2,Y\n").unwrap();
        fs::write(fixture.0.join("spec.yaml"),specification(operation)).unwrap();
        let issues=match fixture.run(){Ok(mut domain)=>{assert!(domain.output().is_none());assert!(domain.save().is_err());domain.issues()},Err(issues)=>issues};
        assert!(issues.iter().any(|issue|issue.condition==condition && issue.requirement.as_deref()==Some(requirement)),"{issues:?}");
        assert!(!fixture.0.join("result.csv").exists());
    }
}
#[test]
fn named_derive_read_requires_keep_and_uses_original_current_record() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("a.csv"), b"ID,CUT\na,2\nb,3\n").unwrap();
    fs::write(fixture.0.join("b.csv"), b"ID,N\nb,10\na,1\na,4\n").unwrap();
    let operation="        derive:\n          - {name: N, type: int, derivation: B.N}\n          - {name: C, type: int, derivation: CAP.CUT}\n          - {name: VALUE, type: int, derivation: {least: {sources: [N, C]}}}\n        expr: SUM(VALUE)\n";
    let spec=specification(operation).replace("base: A\n","base: A\nintermediates:\n  - {id: CAP, dataset: A, key: {ID: A.ID}, order_by: [A.CUT], keep: first}\n");
    fs::write(fixture.0.join("spec.yaml"), &spec).unwrap();
    assert_eq!(fixture.save(), b"ID,CUT,TOTAL\na,2,3\nb,3,3\n");
    fs::write(
        fixture.0.join("spec.yaml"),
        spec.replace(", order_by: [A.CUT], keep: first", ""),
    )
    .unwrap();
    let issues = match fixture.run() {
        Ok(domain) => domain.issues(),
        Err(issues) => issues,
    };
    assert!(
        issues
            .iter()
            .any(|issue| issue.condition == "invalid_derive_step"
                && issue.requirement.as_deref() == Some("REQ-1242")),
        "{issues:?}"
    );
}

fn original_fieldless_count(expression: &str, key: &str, filter: &str) -> String {
    format!(
        "schema_version: '1.0'\ndomain: OUT\ninput:\n  SRC: {{path: src.csv, types: {{N: int}}}}\n  OTHER: {{path: other.csv, types: {{N: int}}}}\nbase: SRC\nkeys: [K]\noutput: {{path: out.csv, columns: [K, V]}}\ncolumns:\n  - name: V\n    type: int\n    derivation:\n      aggregate:\n        expr: '{expression}'\n        key: {key}\n{filter}  - {{name: K, type: int, derivation: SRC.N}}\n"
    )
}

fn fieldless_count_fixture(source: &str) -> Fixture {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("src.csv"), "N\n1\n").unwrap();
    fs::write(fixture.0.join("other.csv"), "N\n1\n1\n2\n").unwrap();
    fs::write(fixture.0.join("spec.yaml"), source).unwrap();
    fixture
}

fn saved_count(fixture: &Fixture, mut result: file_application::Domain) -> Vec<u8> {
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(!fixture.0.join("out.csv").exists());
    assert!(result.save().unwrap());
    fs::read(fixture.0.join("out.csv")).unwrap()
}

// Original planning/test_aggregate_relation_reads.py inputs: COUNT still owns
// a relation even though it reads no payload field. Its key producer is written
// after the consuming column, and only the two matching donors are eligible.
#[test]
fn original_fieldless_reductions_schedule_real_key_dependencies_once() {
    for (expression, count) in [
        ("COUNT(OTHER.*)", 2),
        ("SUM(OTHER.N)", 2),
        ("COUNT(OTHER.*) + COUNT(OTHER.*)", 4),
        ("COUNT(OTHER.*) + SUM(OTHER.N)", 4),
    ] {
        for key in [
            "{N: K}",
            "{N: {source: K}}",
            "{N: {literal: 1}}",
            "{N: {greatest: {sources: [K, K]}}}",
        ] {
            let source = original_fieldless_count(expression, key, "");
            let fixture = fieldless_count_fixture(&source);
            let result = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
            assert!(
                result.issues().is_empty(),
                "{expression}, {key}: {:?}",
                result.issues()
            );
            assert_eq!(
                saved_count(&fixture, result),
                format!("K,V\n1,{count}\n").as_bytes()
            );
        }
    }
    let source = original_fieldless_count("COUNT(OTHER.*)", "{N: K}", "")
        .replace("keys: [K]", "keys: [N]")
        .replace("columns: [K, V]", "columns: [N, V]")
        .replace("        key: {N: K}\n", "")
        .replace("name: K", "name: N");
    let fixture = fieldless_count_fixture(&source);
    let result = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert_eq!(saved_count(&fixture, result), b"N,V\n1,2\n");
}

fn complete_aggregate_issues(fixture: &Fixture) -> Vec<serde_json::Value> {
    let issues = match fixture.run() {
        Ok(mut result) => {
            assert!(result.output().is_none());
            assert!(result.save().is_err());
            result.issues()
        }
        Err(issues) => issues,
    };
    assert!(!fixture.0.join("out.csv").exists());
    issues
        .iter()
        .map(|issue| {
            serde_json::json!({
                "phase": issue.phase, "condition": issue.condition,
                "requirement": issue.requirement, "spec_paths": issue.spec_paths,
                "context": serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()
            })
        })
        .collect()
}

#[test]
fn original_fieldless_reduction_type_findings_keep_one_authored_owner() {
    for expression in [
        "COUNT(OTHER.*)",
        "SUM(OTHER.N)",
        "COUNT(OTHER.*) + COUNT(OTHER.*)",
        "COUNT(OTHER.*) + SUM(OTHER.N)",
    ] {
        for (literal, kind) in [("true", "bool"), ("x", "str")] {
            let fixture = fieldless_count_fixture(&original_fieldless_count(
                expression,
                &format!("{{N: {{literal: {literal}}}}}"),
                "",
            ));
            assert_eq!(
                complete_aggregate_issues(&fixture),
                vec![serde_json::json!({
                    "phase": "validation", "condition": "incompatible_input_type",
                    "requirement": "REQ-0004", "spec_paths": ["columns.V.derivation.aggregate.expr"],
                    "context": {"source": "key[N]", "expected": kind, "actual": "int"}
                })],
                "{expression}, {literal}"
            );
        }
    }
    for (expression, path) in [
        ("COUNT(OTHER.*)", "columns.V.derivation.aggregate.filter"),
        ("SUM(OTHER.N)", "columns.V.derivation.aggregate.expr"),
        (
            "COUNT(OTHER.*) + SUM(OTHER.N)",
            "columns.V.derivation.aggregate.expr",
        ),
    ] {
        let fixture = fieldless_count_fixture(&original_fieldless_count(
            expression,
            "{N: {literal: true}}",
            "        filter: OTHER.N > 0\n",
        ));
        assert_eq!(
            complete_aggregate_issues(&fixture),
            vec![serde_json::json!({
                "phase": "validation", "condition": "incompatible_input_type",
                "requirement": "REQ-0004", "spec_paths": [path],
                "context": {"source": "key[N]", "expected": "bool", "actual": "int"}
            })],
            "{expression}"
        );
    }
}

#[test]
fn original_fieldless_reduction_unknown_names_do_not_invent_join_findings() {
    for (expression, key, identifier, requirement) in [
        ("COUNT(OTHER.*)", "{ABSENT: K}", "OTHER.ABSENT", "REQ-0141"),
        ("COUNT(OTHER.*)", "{N: ABSENT}", "ABSENT", "REQ-0141"),
        ("COUNT(ABSENT.*)", "{N: K}", "ABSENT.*", "REQ-0103"),
    ] {
        let fixture = fieldless_count_fixture(&original_fieldless_count(expression, key, ""));
        assert_eq!(
            complete_aggregate_issues(&fixture),
            vec![serde_json::json!({
                "phase": "validation", "condition": "unknown_field",
                "requirement": requirement, "spec_paths": ["columns.V.derivation.aggregate.expr"],
                "context": {"identifier": identifier}
            })],
            "{expression}, {key}"
        );
    }
    for (expression, identifier) in [
        ("SUM(ABSENT.N)", "ABSENT.N"),
        ("COUNT(ABSENT.*)", "ABSENT.*"),
    ] {
        let fixture = fieldless_count_fixture(&original_fieldless_count(
            expression,
            "{N: K}",
            "        filter: ABSENT.OTHER > 0\n",
        ));
        assert_eq!(
            complete_aggregate_issues(&fixture),
            vec![serde_json::json!({
                "phase": "validation", "condition": "unknown_field",
                "requirement": "REQ-0103", "spec_paths": ["columns.V.derivation.aggregate.expr"],
                "context": {"identifier": identifier}
            })],
            "{expression}"
        );
    }
}

#[test]
fn fieldless_literal_bindings_keep_the_filter_as_the_only_donor_reference() {
    for expression in ["COUNT(OTHER.*)", "SUM(H)"] {
        for key in ["{N: {literal: true}}", "{N: K}"] {
            let source = original_fieldless_count(expression, key, "        filter: OTHER.N > 0\n")
                .replace(
                    "        filter: OTHER.N > 0\n",
                    "        derive:\n          - {name: H, type: int, derivation: {literal: 1}}\n        filter: OTHER.N > 0\n",
                );
            let fixture = fieldless_count_fixture(&source);
            if key == "{N: K}" {
                let result = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
                assert_eq!(saved_count(&fixture, result), b"K,V\n1,2\n");
            } else {
                assert_eq!(
                    complete_aggregate_issues(&fixture),
                    vec![serde_json::json!({
                        "phase": "validation", "condition": "incompatible_input_type",
                        "requirement": "REQ-0004", "spec_paths": ["columns.V.derivation.aggregate.filter"],
                        "context": {"source": "key[N]", "expected": "bool", "actual": "int"}
                    })],
                    "{expression}"
                );
            }
        }
    }
}

const ORIGINAL_DERIVE_SPEC: &str = r#"schema_version: "1.0"
domain: ADQS
keys: [STUDYID, USUBJID]
input:
  QS: {path: input/qs.csv}
output:
  path: adqs.csv
  columns: [STUDYID, USUBJID, AVAL]
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: QS.STUDYID
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: QS.USUBJID
  - name: AVAL
    type: float
    label: Analysis Value
    derivation:
      aggregate:
        filter: "QS.QSCAT = 'SCALE'"
        key: [STUDYID, USUBJID]
        derive:
DERIVE_BINDINGS
        expr: "REDUCTION_EXPRESSION"
rows:
  - id: subscale
    filter: "QS.QSCAT = 'SCALE' AND QS.QSTESTCD = 'PF01'"
    derivations: {}
"#;
const ORIGINAL_QS: &str = r#"STUDYID,USUBJID,QSTESTCD,QSCAT,QSORRES
S1,001,PF01,SCALE,1
S1,001,PF02,SCALE,2
S1,001,PF03,SCALE,3
"#;
const ORIGINAL_QS_WITH_TEXT: &str = r#"STUDYID,USUBJID,QSTESTCD,QSCAT,QSORRES
S1,001,PF01,SCALE,1
S1,001,PF02,SCALE,N/A
S1,001,PF03,SCALE,3
"#;
const ORIGINAL_ENUM_DERIVE_SPEC: &str = r#"schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  EX: {path: input/ex.csv}
output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, TRTEDT]
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: EX.STUDYID
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: EX.USUBJID
  - name: TRTEDT
    type: date
    label: Date of Last Exposure
    derivation:
      aggregate:
        key: [STUDYID, USUBJID]
        derive:
          - name: H
            type: date
            derivation:
              date_impute:
                source: EX.EXENDTC
                day: last
                minimum_source_precision: month
                missing: null
        expr: "MAX(H)"
rows:
  - id: one
    derivations: {}
"#;
const ORIGINAL_EX: &str = r#"STUDYID,USUBJID,EXENDTC
S1,001,2024-05
S1,002,2024-06
"#;

fn original_derive_fixture(bindings: &str, expression: &str, input: &str) -> Fixture {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("input")).unwrap();
    fs::write(fixture.0.join("input/qs.csv"), input).unwrap();
    let bindings = bindings
        .lines()
        .map(|line| format!("          {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        fixture.0.join("spec.yaml"),
        ORIGINAL_DERIVE_SPEC
            .replace("DERIVE_BINDINGS", &bindings)
            .replace("REDUCTION_EXPRESSION", expression),
    )
    .unwrap();
    fixture
}

fn original_derive_issues(fixture: &Fixture) -> Vec<yamaa_adapters::issue_rows::Issue> {
    match fixture.run() {
        Ok(mut domain) => {
            assert!(domain.output().is_none());
            assert!(domain.save().is_err());
            domain.issues()
        }
        Err(issues) => issues,
    }
}

#[test]
fn original_aggregate_bindings_reject_duplicates_and_forward_references() {
    let binding = "- name: QSNUM\n  type: float\n  derivation: QS.QSORRES";
    let fixture =
        original_derive_fixture(&format!("{binding}\n{binding}"), "SUM(QSNUM)", ORIGINAL_QS);
    let issues = original_derive_issues(&fixture);
    assert_eq!(issues[0].condition, "invalid_derive_step");
    let context: serde_json::Value = serde_json::from_str(&issues[0].context).unwrap();
    assert!(context["reason"].as_str().unwrap().contains("unique"));

    let fixture = original_derive_fixture(
        &format!("- name: DOUBLED\n  type: float\n  derivation: QSNUM\n{binding}"),
        "SUM(DOUBLED)",
        ORIGINAL_QS,
    );
    let issues = original_derive_issues(&fixture);
    assert_eq!(issues[0].condition, "unknown_derive_variable");
    let context: serde_json::Value = serde_json::from_str(&issues[0].context).unwrap();
    assert_eq!(context["variable"], "QSNUM");
}

#[test]
fn original_aggregate_bindings_read_converted_earlier_values_and_recover_once() {
    for (bindings, expression, input, expected) in [
        (
            "- name: QSNUM\n  type: float\n  derivation: QS.QSORRES\n- name: DOUBLED\n  type: float\n  derivation:\n    compute:\n      expr: 'QSNUM * 2'",
            "SUM(DOUBLED)",
            ORIGINAL_QS,
            12.0,
        ),
        (
            "- name: QSNUM\n  type: float\n  derivation:\n    value:\n      source: QS.QSORRES\n    unconvertible: 0",
            "SUM(QSNUM)",
            ORIGINAL_QS_WITH_TEXT,
            4.0,
        ),
    ] {
        let fixture = original_derive_fixture(bindings, expression, input);
        let domain = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        let table = domain.output_table().unwrap();
        assert_eq!(table.rows(), 1);
        let columns = table.columns();
        let yamaa_adapters::public_table::Values::Str(study) = &columns[0].1 else {
            panic!("STUDYID must remain text");
        };
        let yamaa_adapters::public_table::Values::Str(subject) = &columns[1].1 else {
            panic!("USUBJID must remain text");
        };
        let yamaa_adapters::public_table::Values::Float(values) = &columns[2].1 else {
            panic!("AVAL must retain its declared float type");
        };
        assert_eq!(study, &[Some("S1".into())]);
        assert_eq!(subject, &[Some("001".into())]);
        assert_eq!(values, &[Some(expected)]);
    }
    let fixture = original_derive_fixture(
        "- name: QSNUM\n  type: float\n  derivation: QS.QSORRES",
        "SUM(QSNUM)",
        ORIGINAL_QS_WITH_TEXT,
    );
    assert_eq!(
        original_derive_issues(&fixture)[0].condition,
        "conversion_failed"
    );
}

#[test]
fn original_aggregate_derive_enum_arguments_are_not_variable_references() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("input")).unwrap();
    fs::write(fixture.0.join("input/ex.csv"), ORIGINAL_EX).unwrap();
    fs::write(fixture.0.join("spec.yaml"), ORIGINAL_ENUM_DERIVE_SPEC).unwrap();
    let mut domain = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
    assert!(domain.issues().is_empty(), "{:?}", domain.issues());
    assert!(domain.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("adsl.csv")).unwrap(),
        b"STUDYID,USUBJID,TRTEDT\nS1,001,2024-05-31\nS1,002,2024-06-30\n"
    );
}

#[test]
fn original_aggregate_binding_relation_and_field_findings_keep_their_context() {
    let fixture = original_derive_fixture(
        "- name: A\n  type: float\n  derivation: QS.QSORRES\n- name: B\n  type: float\n  derivation: EX.EXSEQ",
        "SUM(A)",
        ORIGINAL_QS,
    );
    let issues = original_derive_issues(&fixture);
    assert!(issues
        .iter()
        .any(|issue| issue.condition == "invalid_derive_step"));

    let fixture = original_derive_fixture(
        "- name: A\n  type: float\n  derivation: QS.QSORRES\n- name: B\n  type: float\n  derivation: XX.YY\n- name: CAPNUM\n  type: float\n  derivation: CAP.DSNUM",
        "SUM(A)",
        ORIGINAL_QS,
    );
    add_original_cap_intermediate(&fixture, true);
    let issues = original_derive_issues(&fixture);
    let issue = issues
        .iter()
        .find(|issue| issue.condition == "invalid_derive_step")
        .unwrap();
    assert_eq!(issue.requirement.as_deref(), Some("REQ-1191"));
    let context: serde_json::Value = serde_json::from_str(&issue.context).unwrap();
    assert_eq!(context["relations"], serde_json::json!(["QS", "XX"]));

    for (source, requirement) in [("QS.NOSUCHFIELD", "REQ-0103"), ("CAP.NOSUCH", "REQ-0125")] {
        let fixture = original_derive_fixture(
            &format!("- name: A\n  type: float\n  derivation: {source}"),
            "SUM(A)",
            ORIGINAL_QS,
        );
        add_original_cap_intermediate(&fixture, true);
        let issues = original_derive_issues(&fixture);
        let unknown = issues
            .iter()
            .filter(|issue| issue.condition == "unknown_field")
            .collect::<Vec<_>>();
        assert_eq!(unknown.len(), 1, "{issues:?}");
        assert_eq!(unknown[0].requirement.as_deref(), Some(requirement));
        let context: serde_json::Value = serde_json::from_str(&unknown[0].context).unwrap();
        assert_eq!(context["identifier"], source);
    }

    let fixture = original_derive_fixture(
        "- name: QSNUM\n  type: float\n  derivation: QS.QSORRES\n- name: CAPNUM\n  type: float\n  derivation: CAP.DSNUM\n- name: CAPPED\n  type: float\n  derivation: {least: {sources: [QSNUM, CAPNUM]}}",
        "SUM(CAPPED)",
        ORIGINAL_QS,
    );
    add_original_cap_intermediate(&fixture, false);
    let issues = original_derive_issues(&fixture);
    let invalid = issues
        .iter()
        .filter(|issue| issue.condition == "invalid_derive_step")
        .collect::<Vec<_>>();
    assert_eq!(invalid.len(), 1, "{issues:?}");
    assert_eq!(invalid[0].requirement.as_deref(), Some("REQ-1242"));
    let context: serde_json::Value = serde_json::from_str(&invalid[0].context).unwrap();
    assert_eq!(context["intermediate"], "CAP");
}

fn add_original_cap_intermediate(fixture: &Fixture, keep: bool) {
    fs::write(
        fixture.0.join("input/ds.csv"),
        b"STUDYID,USUBJID,DSNUM\nS1,001,9\nS1,001,2\nS2,002,10\n",
    )
    .unwrap();
    let path = fixture.0.join("spec.yaml");
    let spec = fs::read_to_string(&path).unwrap()
        .replace("  QS: {path: input/qs.csv}", "  QS: {path: input/qs.csv}\n  DS: {path: input/ds.csv}")
        .replace("output:\n", &format!("intermediates:\n  - id: CAP\n    dataset: DS\n    key: [STUDYID, USUBJID]{}\noutput:\n", if keep { "\n    order_by: [DS.DSNUM]\n    keep: first" } else { "" }))
        .replace("  - id: subscale\n", "  - id: subscale\n    dataset: QS\n");
    fs::write(path, spec).unwrap();
}
