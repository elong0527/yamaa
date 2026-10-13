//! Implicit joins bind completed keys, preserve laziness and keep source scopes.
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
            "yamaa-secondary-scalars-{}-{}",
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
    fn saved(&self) -> Vec<u8> {
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
#[test]
fn row_filters_join_other_inputs_after_keys_and_preserve_eager_predicate_failures() {
    for (driver, filter, expected) in [
        ("SRC", "ID = 3 OR B.N = 40", "ID,VALUE\n2,0\n3,0\n"),
        ("B", "SRC.N >= 20", "ID,VALUE\n2,0\n3,0\n"),
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("a.csv"), "ID,N\n1,10\n2,20\n3,30\n").unwrap();
        fs::write(fixture.0.join("b.csv"), "ID,N\n2,40\n1,50\n3,60\n").unwrap();
        fs::write(fixture.0.join("spec.yaml"), format!("schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\ninput:\n  SRC: {{path: a.csv, types: {{ID: int, N: int}}}}\n  B: {{path: b.csv, types: {{ID: int, N: int}}}}\ncolumns:\n  - {{name: ID, type: int, derivation: {{source: {{variable: {driver}.ID}}}}}}\n  - {{name: VALUE, type: int}}\nrows:\n  - id: selected\n    dataset: {driver}\n    filter: '{filter}'\n    derivations:\n      VALUE: {{literal: 0}}\noutput: {{path: result.csv, columns: [ID, VALUE]}}\n")).unwrap();
        assert_eq!(fixture.saved(), expected.as_bytes());
    }
    let fixture = Fixture::new();
    fs::write(fixture.0.join("a.csv"), "ID\n3\n").unwrap();
    fs::write(fixture.0.join("b.csv"), "ID,N\n3,60\n3,70\n").unwrap();
    fs::write(fixture.0.join("spec.yaml"), "schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\ninput:\n  SRC: {path: a.csv, types: {ID: int}}\n  B: {path: b.csv, types: {ID: int, N: int}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\nrows:\n  - id: selected\n    dataset: SRC\n    filter: 'ID = 3 OR B.N = 40'\n    derivations: {}\noutput: {path: result.csv, columns: [ID]}\n").unwrap();
    let mut result = fixture.run().unwrap();
    assert!(result.output().is_none());
    assert_eq!(result.issues().len(), 1);
    assert_eq!(result.issues()[0].condition, "multiple_matches");
    assert_eq!(result.issues()[0].requirement.as_deref(), Some("REQ-0127"));
    assert_eq!(result.issues()[0].spec_paths, ["rows[0].filter"]);
    let context: serde_json::Value = serde_json::from_str(&result.issues()[0].context).unwrap();
    assert_eq!(context["match_count"], 2);
    assert!(result.save().is_err());
    assert!(!fixture.0.join("result.csv").exists());
}
#[test]
fn nested_row_text_and_date_operands_match_completed_keys_and_skip_unselected_donors() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("a.csv"),
        b"ID,D,USE\na,2020-01-03,Y\nb,2020-01-04,N\nc,2020-01-05,Y\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("b.csv"),
        b"ID,D,TEXT\nb,2020-01-01,poison1\nb,2020-01-01,poison2\na,2020-01-01,Lower\n",
    )
    .unwrap();
    fs::write(fixture.0.join("spec.yaml"),"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {A: {path: a.csv, types: {D: date}}, B: {path: b.csv, types: {D: date}}}\nbase: A\noutput: {path: result.csv, columns: [ID, TEXT, DAYS]}\ncolumns:\n  - {name: ID, type: str, derivation: A.ID}\n  - {name: TEXT, type: str}\n  - {name: DAYS, type: int}\nrows:\n  - id: records\n    dataset: A\n    derivations:\n      TEXT: {case: [{when: \"A.USE = 'Y'\", then: {str_case: {source: B.TEXT, to: upper, missing: null}}}, {otherwise: {literal: SKIP}}]}\n      DAYS: {case: [{when: \"A.USE = 'Y'\", then: {date_diff: {start: B.D, end: A.D, unit: day}}}, {otherwise: {literal: 0}}]}\n").unwrap();
    assert_eq!(fixture.saved(), b"ID,TEXT,DAYS\na,LOWER,2\nb,SKIP,0\nc,,\n");
    let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
    fs::write(
        fixture.0.join("spec.yaml"),
        original.replace(
            "{str_case: {source: B.TEXT, to: upper, missing: null}}",
            "{first_available: {sources: [B.TEXT], missing: NONE}}",
        ),
    )
    .unwrap();
    assert_eq!(
        fixture.saved(),
        b"ID,TEXT,DAYS\na,Lower,2\nb,SKIP,0\nc,NONE,\n"
    );
}
#[test]
fn original_secondary_source_fixtures_preserve_frozen_csv_bytes() {
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
        ("adam-advs-bmi", "advs.csv"),
        ("schema-rows-override", "advs.csv"),
        ("sdtm-se-subject-elements", "se.csv"),
    ] {
        let fixture = Fixture::new();
        copy(&root.join(name), &fixture.0);
        let mut domain = fixture
            .run()
            .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
        assert!(domain.issues().is_empty(), "{name}: {:?}", domain.issues());
        assert!(domain.save().unwrap());
        assert_eq!(
            fs::read(fixture.0.join(result)).unwrap(),
            fs::read(root.join(name).join("expected").join(result)).unwrap(),
            "{name}"
        );
    }
}
#[test]
fn foreign_driver_scalar_operands_join_primary_records_and_column_predicates_share_keys() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("a.csv"),
        b"ID,TEXT,USE\na,lower,Y\nb,mixed,N\n",
    )
    .unwrap();
    fs::write(fixture.0.join("b.csv"), b"ID,N\nb,2\na,1\nc,3\n").unwrap();
    fs::write(fixture.0.join("spec.yaml"),"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {A: {path: a.csv}, B: {path: b.csv, types: {N: int}}}\nbase: A\noutput: {path: result.csv, columns: [ID, TEXT, FLAG]}\ncolumns:\n  - {name: ID, type: str}\n  - {name: TEXT, type: str}\n  - {name: FLAG, type: str, derivation: {flag: \"A.USE = 'Y'\"}}\nrows:\n  - id: beta\n    dataset: B\n    derivations:\n      ID: B.ID\n      TEXT: {str_case: {source: A.TEXT, to: upper, missing: null}}\n").unwrap();
    assert_eq!(fixture.saved(), b"ID,TEXT,FLAG\nb,MIXED,\na,LOWER,Y\nc,,\n");
    let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
    fs::write(fixture.0.join("spec.yaml"),original.replace("TEXT: {str_case: {source: A.TEXT, to: upper, missing: null}}",
        "TEXT: {source: {variable: A.TEXT, filter: \"A.USE = 'Y'\", order_by: [A.ID], keep: first}}" )).unwrap();
    assert_eq!(fixture.saved(), b"ID,TEXT,FLAG\nb,,\na,lower,Y\nc,,\n");
}
#[test]
fn forbidden_foreign_numeric_identifier_is_classified_before_donor_evaluation() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("a.csv"), b"ID,TEXT\na,valid\n").unwrap();
    fs::write(fixture.0.join("b.csv"), b"ID,N\na,1\n").unwrap();
    fs::write(fixture.0.join("spec.yaml"),"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {A: {path: a.csv}, B: {path: b.csv, types: {N: int}}}\nbase: A\noutput: {path: result.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, derivation: A.ID}\n  - {name: VALUE, type: int}\nrows:\n  - id: alpha\n    dataset: A\n    derivations:\n      VALUE: {compute: {expr: 'B.N + 1'}}\n").unwrap();
    let issues = match fixture.run() {
        Ok(mut domain) => {
            assert!(domain.output().is_none());
            assert!(domain.save().is_err());
            domain.issues()
        }
        Err(issues) => issues,
    };
    assert!(!issues.is_empty());
    assert!(
        issues.iter().all(|issue| issue.phase == "validation"),
        "{issues:?}"
    );
    assert!(
        issues.iter().any(|issue| issue.condition == "unknown_field"
            && issue.requirement.as_deref() == Some("REQ-0410")
            && issue.spec_paths == ["rows[0].derivations.VALUE.compute.expr"]),
        "{issues:?}"
    );
    assert!(!fixture.0.join("result.csv").exists());
    let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
    for (operation, path) in [
        (
            "case: [{when: 'B.N = 1', then: {literal: 1}}, {otherwise: {literal: 0}}]",
            "rows[0].derivations.VALUE.case[0].when",
        ),
        (
            "flag: {condition: 'B.N = 1', true_value: 1, false_value: 0}",
            "rows[0].derivations.VALUE.flag.condition",
        ),
    ] {
        fs::write(
            fixture.0.join("spec.yaml"),
            original.replace("compute: {expr: 'B.N + 1'}", operation),
        )
        .unwrap();
        let issues = match fixture.run() {
            Ok(domain) => domain.issues().to_vec(),
            Err(issues) => issues,
        };
        assert!(
            issues.iter().any(|issue| issue.condition == "unknown_field"
                && issue.requirement.as_deref() == Some("REQ-0410")
                && issue.spec_paths == [path]),
            "{issues:?}"
        );
    }
}

#[test]
fn nested_structured_sources_keep_filters_stable_choices_absence_and_lazy_handlers() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("a.csv"),
        b"ID,USE,FALLBACK\na,Y,fallback\nb,N,fallback\nc,Y,fallback\n",
    )
    .unwrap();
    fs::write(fixture.0.join("b.csv"),b"ID,SEQ,USE,TEXT,NUM\na,10,Y,late,2\na,1,Y,early,1\na,1,Y,same-order,1\na,50,N,excluded,9\nb,1,Y,hidden,1\nb,2,Y,hidden2,2\n").unwrap();
    fs::write(
        fixture.0.join("spec.yaml"),
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {A: {path: a.csv}, B: {path: b.csv, types: {SEQ: int}}}
base: A
output: {path: result.csv, columns: [ID, TEXT, FIRST, MAP, ABSENT]}
columns:
  - {name: ID, type: str, derivation: A.ID}
  - {name: TEXT, type: str}
  - {name: FIRST, type: str}
  - {name: MAP, type: int}
  - {name: ABSENT, type: str}
rows:
  - id: records
    dataset: A
    derivations:
      TEXT:
        case:
          - when: "A.USE = 'Y'"
            then: {source: {variable: B.TEXT, filter: "B.USE = 'Y'", order_by: [B.SEQ], keep: last}}
          - otherwise: {literal: SKIP}
      FIRST:
        case:
          - when: "A.USE = 'Y'"
            then:
              first_available:
                sources:
                  - {variable: B.TEXT, filter: "B.USE = 'Y' AND B.SEQ = 10"}
                  - {variable: A.FALLBACK, filter: "A.USE = 'Y'"}
                missing: NONE
          - otherwise: {literal: SKIP}
      MAP:
        case:
          - when: "A.USE = 'Y'"
            then:
              mapping:
                source: {variable: B.NUM, filter: "B.USE = 'Y' AND B.SEQ = 10"}
                dict: {'1': 10, '2': 20}
                missing: 0
          - otherwise: {literal: 0}
      ABSENT:
        case:
          - when: "A.USE = 'Y'"
            then: {source: {variable: B.NOTTHERE, absent: EDGE}}
          - otherwise: {literal: SKIP}
"#,
    )
    .unwrap();
    let mut domain = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
    assert!(domain.issues().is_empty(), "{:?}", domain.issues());
    assert!(domain.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("result.csv")).unwrap(),
        b"ID,TEXT,FIRST,MAP,ABSENT\na,late,late,20,EDGE\nb,SKIP,SKIP,0,SKIP\nc,,fallback,0,EDGE\n"
    );
    let counts = domain.observations()["handler_counts"]
        .as_array()
        .unwrap()
        .clone();
    for (path, handler, count) in [
        (
            "rows[0].derivations.TEXT.case[0].then.source.multiple_matches",
            "multiple_matches",
            1,
        ),
        (
            "rows[0].derivations.MAP.case[0].then.mapping.missing",
            "missing",
            1,
        ),
        (
            "rows[0].derivations.ABSENT.case[0].then.source.absent",
            "absent",
            2,
        ),
    ] {
        assert!(
            counts.contains(&serde_json::json!({"spec_path":path,"handler":handler,"count":count})),
            "{counts:?}"
        );
    }
    let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
    fs::write(fixture.0.join("spec.yaml"),original.replace("then: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: last}}",
        "then: {str_concat: {sources: [{source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: first}}], missing: null}}" )).unwrap();
    assert_eq!(
        fixture.saved(),
        b"ID,TEXT,FIRST,MAP,ABSENT\na,early,late,20,EDGE\nb,SKIP,SKIP,0,SKIP\nc,,fallback,0,EDGE\n"
    );
    let case = "TEXT:\n        case:\n          - when: \"A.USE = 'Y'\"\n            then: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: last}}\n          - otherwise: {literal: SKIP}";
    fs::write(fixture.0.join("spec.yaml"), original.replace(case, "TEXT: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: last}}" )).unwrap();
    assert_eq!(fixture.saved(),b"ID,TEXT,FIRST,MAP,ABSENT\na,late,late,20,EDGE\nb,hidden2,SKIP,0,SKIP\nc,,fallback,0,EDGE\n");
}

#[test]
fn global_columns_join_captured_ports_after_mixed_driver_rows_and_windows() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("a.csv"), b"ID,TEXT\na,primary\n").unwrap();
    fs::write(
        fixture.0.join("b.csv"),
        b"ID,EXTRA,TEXT\nb,ignored,beta\nc,ignored,gamma\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("spec.yaml"),
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
base: A
input: {A: {path: a.csv}, B: {path: b.csv}}
output: {path: result.csv, columns: [ID, RN, PRIMARY, SECONDARY]}
columns:
  - {name: ID, type: str}
  - {name: ORD, type: int}
  - {name: RN, type: int, derivation: {row_number: {window: {order_by: [ORD, ID]}}}}
  - name: PRIMARY
    type: str
    derivation:
      case:
        - when: 'RN > 0'
          then: A.TEXT
        - otherwise: {literal: SKIP}
  - name: SECONDARY
    type: str
    derivation:
      case:
        - when: 'RN > 0'
          then: B.TEXT
        - otherwise: {literal: SKIP}
rows:
  - {id: alpha, dataset: A, derivations: {ID: A.ID, ORD: {literal: 1}}}
  - {id: beta, dataset: B, derivations: {ID: B.ID, ORD: {literal: 2}}}
"#,
    )
    .unwrap();
    assert_eq!(
        fixture.saved(),
        b"ID,RN,PRIMARY,SECONDARY\na,1,primary,\nb,2,,beta\nc,3,,gamma\n"
    );
    let body = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
    fs::write(fixture.0.join("spec.yaml"),body.replace("then: A.TEXT","then: {source: {variable: A.TEXT, filter: \"A.ID = 'a'\", order_by: [A.ID], keep: first}}" )).unwrap();
    assert_eq!(
        fixture.saved(),
        b"ID,RN,PRIMARY,SECONDARY\na,1,primary,\nb,2,,beta\nc,3,,gamma\n"
    );
}
