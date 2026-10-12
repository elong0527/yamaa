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
    fn new(name: &str) -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks")
            .join(name);
        let path = std::env::temp_dir().join(format!(
            "yamaa-scalar-{}-{}",
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
    fn run(&self) -> file_application::Domain {
        let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "test",
            specification: &specification,
            base_directory: ".",
        };
        file_application::domain(
            Request {
                specification: &specification,
                environment: None,
            },
            identity,
        )
        .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn bespoke(source: &str, input: &[u8]) -> Fixture {
    let fixture = Fixture::new("adam-adae-text-cleanup");
    fs::write(fixture.0.join("spec.yaml"), source).unwrap();
    fs::write(fixture.0.join("input.csv"), input).unwrap();
    fixture
}
fn attempt(
    fixture: &Fixture,
) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
    let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
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

fn specification(columns: &str, rows: &str, output: &str) -> String {
    format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID, SECTION]\ninput: {{SRC: {{path: input.csv, types: {{I: int, D: date, R: date}}}}}}\noutput: {{path: output.csv, columns: {output}}}\ncolumns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n  - {{name: SECTION, type: str}}\n{columns}rows:\n{rows}")
}
fn saved(fixture: &Fixture) -> Vec<u8> {
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    fs::read(fixture.0.join("output.csv")).unwrap()
}
#[test]
fn row_cases_numeric_and_temporal_share_dependency_lowering() {
    let columns="  - {name: VALUE, type: int}\n  - {name: PICK, type: str}\n  - {name: DAY, type: int}\n  - {name: TEXT, type: str}\n";
    // Authored consumer-before-producer order proves that row-local reads use
    // the dependency graph, and that nested branches see converted values.
    let rows="  - id: one\n    derivations:\n      SECTION: {literal: one}\n      TEXT: {str_concat: {sources: [{case: [{when: 'VALUE = 9', then: {literal: nine}}, {otherwise: {literal: other}}]}, {source: PICK}]}}\n      DAY: {study_day: {date: SRC.D, reference: SRC.R}}\n      PICK: {first_available: {sources: [SRC.TEXT, ID], missing: none}}\n      VALUE: {case: [{when: 'SRC.I > 0', then: {compute: {expr: 'POWER(SRC.I, 2)'}}}, {otherwise: {literal: 0}}]}\n";
    let f = bespoke(
        &specification(columns, rows, "[ID, SECTION, VALUE, PICK, DAY, TEXT]"),
        b"ID,I,D,R,TEXT\na,3,2025-01-02,2025-01-01,x\nb,-2,2024-12-31,2025-01-01,\n",
    );
    assert_eq!(
        saved(&f),
        b"ID,SECTION,VALUE,PICK,DAY,TEXT\na,one,9,x,2,ninex\nb,one,0,b,-1,otherb\n"
    );
}
#[test]
fn each_row_template_has_its_own_filtered_windows_and_chained_passes() {
    let columns="  - {name: VALUE, type: int, derivation: SRC.I}\n  - {name: SEQ, type: int}\n  - {name: NEXT, type: int}\n  - {name: DOUBLE, type: int}\n";
    let derivations="      VALUE: SRC.I\n      SEQ: {row_number: {window: {order_by: [VALUE]}}}\n      DOUBLE: {compute: {expr: 'SEQ * 2'}}\n      NEXT: {row_value: {source: DOUBLE, offset: 1, window: {order_by: [SEQ]}}}\n";
    let rows=format!("  - id: one\n    filter: VALUE > 0\n    derivations:\n      SECTION: {{literal: one}}\n{derivations}  - id: two\n    filter: VALUE > 1\n    derivations:\n      SECTION: {{literal: two}}\n{derivations}");
    let f = bespoke(
        &specification(columns, &rows, "[ID, SECTION, VALUE, SEQ, DOUBLE, NEXT]"),
        b"ID,I,D,R,TEXT\na,3,,,\nb,0,,,\nc,1,,,\nd,2,,,\n",
    );
    assert_eq!(saved(&f),b"ID,SECTION,VALUE,SEQ,DOUBLE,NEXT\na,one,3,3,6,\nc,one,1,1,2,4\nd,one,2,2,4,6\na,two,3,2,4,\nd,two,2,1,2,4\n");
}
#[test]
fn grouped_row_case_reads_group_keys_and_converted_reductions() {
    let columns = "  - {name: TOTAL, type: int}\n  - {name: TEXT, type: str}\n";
    let rows="  - id: grouped\n    group_by: [SRC.ID]\n    derivations:\n      SECTION: {literal: grouped}\n      TEXT: {case: [{when: 'TOTAL > 5', then: {source: {variable: SRC.ID}}}, {otherwise: {literal: low}}]}\n      TOTAL: {aggregate: {expr: 'SUM(SRC.I)'}}\n";
    let f = bespoke(
        &specification(columns, rows, "[ID, SECTION, TOTAL, TEXT]"),
        b"ID,I,D,R,TEXT\na,3,,,\nb,1,,,\na,4,,,\n",
    );
    assert_eq!(
        saved(&f),
        b"ID,SECTION,TOTAL,TEXT\na,grouped,7,a\nb,grouped,1,low\n"
    );
}
#[test]
fn invalid_grouped_case_source_and_window_filter_refuse_publication() {
    let columns = "  - {name: VALUE, type: int}\n";
    let rows="  - id: grouped\n    group_by: [SRC.ID]\n    derivations:\n      SECTION: {literal: grouped}\n      VALUE: {case: [{when: 'SRC.I > 0', then: {literal: 1}}, {otherwise: {literal: 0}}]}\n";
    let f = bespoke(
        &specification(columns, rows, "[ID, SECTION, VALUE]"),
        b"ID,I,D,R,TEXT\na,3,,,\n",
    );
    // Non-grouped driver fields are statically bound after ingestion, never read by a predicate.
    let issues = attempt(&f).map_or_else(|issues| issues, |result| result.issues().to_vec());
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "ungrouped_driver_field");
    assert!(!f.0.join("output.csv").exists());
    let columns="  - {name: VALUE, type: int, derivation: SRC.I}\n  - {name: SEQ, type: int}\n  - {name: AFTER, type: int}\n";
    let rows="  - id: one\n    filter: AFTER > 0\n    derivations:\n      SECTION: {literal: one}\n      SEQ: {row_number: {window: {order_by: [VALUE]}}}\n      AFTER: {compute: {expr: 'SEQ + 1'}}\n";
    let f = bespoke(
        &specification(columns, rows, "[ID, SECTION, SEQ]"),
        b"ID,I,D,R,TEXT\na,3,,,\n",
    );
    let issues = attempt(&f).map_or_else(|issues| issues, |result| result.issues().to_vec());
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "phase_boundary");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap()["available_phase"],
        "window_derivation"
    );
    assert!(!f.0.join("output.csv").exists());
}
#[test]
fn intermediate_identity_keeps_column_key_windows_over_all_constructed_templates() {
    let body = "schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID, SEQ]\ninput:\n  SRC: {path: input.csv, types: {VALUE: int}}\n  D: donor.csv\nintermediates: [{id: P, dataset: D, key: ID}]\noutput: {path: output.csv, columns: [ID, SEQ, VALUE, PICK]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: SRC.VALUE}\n  - {name: SEQ, type: int, derivation: {row_number: {window: {group_by: [ID], order_by: [VALUE]}}}}\n  - {name: PICK, type: str, derivation: P.PICK}\nrows:\n  - {id: first, dataset: SRC, filter: \"SRC.KIND = 'A'\", derivations: {}}\n  - {id: second, dataset: SRC, filter: \"SRC.KIND = 'B'\", derivations: {}}\n";
    let f = bespoke(body, b"ID,KIND,VALUE\nx,A,4\nx,B,1\n");
    fs::write(f.0.join("donor.csv"), b"ID,PICK\nx,kept\n").unwrap();
    assert_eq!(saved(&f), b"ID,SEQ,VALUE,PICK\nx,2,4,kept\nx,1,1,kept\n");
}
#[test]
fn grouped_row_column_defaults_use_the_shared_secondary_source_key_binding() {
    let body = "schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\ninput:\n  SRC: {path: input.csv, types: {VALUE: int}}\n  D: {path: donor.csv, types: {REFERENCE: int}}\noutput: {path: output.csv, columns: [ID, TOTAL, REFERENCE]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: TOTAL, type: int}\n  - {name: REFERENCE, type: int, derivation: D.REFERENCE}\nrows:\n  - id: grouped\n    dataset: SRC\n    group_by: [SRC.ID]\n    derivations: {TOTAL: {aggregate: {expr: 'SUM(SRC.VALUE)'}}}\n";
    let f = bespoke(body, b"ID,VALUE\na,4\na,1\nb,2\n");
    fs::write(f.0.join("donor.csv"), b"ID,REFERENCE\na,10\nb,20\n").unwrap();
    assert_eq!(saved(&f), b"ID,TOTAL,REFERENCE\na,5,10\nb,2,20\n");
    fs::write(f.0.join("donor.csv"), b"ID,REFERENCE\na,10\na,10\nb,20\n").unwrap();
    let mut result = f.run();
    assert!(result.output().is_none());
    assert!(result.save().is_err());
    assert_eq!(result.issues()[0].condition, "multiple_matches");
    assert_eq!(result.issues()[0].requirement.as_deref(), Some("REQ-0127"));
}
#[test]
fn unchanged_vs_units_benchmark_runs_row_cases_and_numeric_branches() {
    let f = Fixture::new("sdtm-vs-units");
    let mut result = f.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(f.0.join("vs.csv")).unwrap(),
        fs::read(f.0.join("expected/vs.csv")).unwrap()
    );
}

#[test]
fn column_and_case_scopes_use_the_shared_dispatch_with_distinct_nested_values() {
    let source="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv}}\noutput: {path: output.csv, columns: [ID, VALUE, TEXT]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {compute: {expr: 'EXP(0) + LN(1) + POWER(3, 2)'}}}\n  - name: TEXT\n    type: str\n    derivation:\n      case:\n        - when: VALUE = 10\n          then: {str_concat: {sources: [{literal: 'prefix:'}, {first_available: {sources: [ID], missing: fallback}}]}}\n        - otherwise: {literal: wrong}\n";
    let f = bespoke(source, b"ID\na\n");
    assert_eq!(saved(&f), b"ID,VALUE,TEXT\na,10,prefix:a\n");
}

#[test]
fn distinct_stored_drivers_group_in_their_own_schemas_and_complete_global_windows() {
    let source = r#"schema_version: '1.0'
domain: TEST
keys: [ID, SECTION]
input:
  ALPHA: {path: input.csv, types: {ID: int, S: int}}
  BETA: {path: beta.csv, types: {OTHER: int, AMOUNT: float}}
output: {path: output.csv, columns: [ID, SECTION, SEQ, GLOBAL, TOTAL]}
columns:
- {name: ID, type: int}
- {name: SECTION, type: str}
- {name: SEQ, type: int}
- {name: GLOBAL, type: int, derivation: {row_number: {window: {order_by: [ID, SECTION]}}}}
- {name: TOTAL, type: float}
rows:
- id: records
  dataset: ALPHA
  filter: 'ID > 1'
  derivations:
    ID: ALPHA.ID
    SECTION: {literal: A}
    SEQ: {row_number: {window: {order_by: [{variable: ID, direction: desc}]}}}
    TOTAL: {literal: 0}
- id: groups
  dataset: BETA
  group_by: [BETA.OTHER]
  derivations:
    ID: BETA.OTHER
    SECTION: {literal: B}
    SEQ: {row_number: {window: {order_by: [{variable: ID, direction: desc}]}}}
    TOTAL: {aggregate: {expr: 'SUM(BETA.AMOUNT)'}}
"#;
    let fixture = bespoke(source, b"ID,S\n3,7\n1,8\n2,9\n");
    fs::write(
        fixture.0.join("beta.csv"),
        b"PAD,OTHER,AMOUNT\nx,10,3\ny,7,5\nz,10,4\n",
    )
    .unwrap();
    assert_eq!(
        saved(&fixture),
        b"ID,SECTION,SEQ,GLOBAL,TOTAL\n3,A,1,2,0\n2,A,2,1,0\n10,B,1,4,7\n7,B,2,3,5\n"
    );
}

#[test]
fn record_filter_promotes_numeric_defaults_and_their_complete_dependencies() {
    let fixture = bespoke(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID, SECTION]
input: {SRC: {path: input.csv, types: {I: int}}}
output: {path: output.csv, columns: [ID, SECTION, VALUE]}
columns:
- {name: ID, type: int}
- {name: SECTION, type: str, derivation: {literal: S}}
- {name: VALUE, type: float, derivation: {compute: {expr: 'POWER(ID, 2)'}}}
rows:
- id: selected
  filter: 'VALUE > 4'
  derivations: {ID: SRC.I}
"#,
        b"I\n1\n3\n2\n4\n",
    );
    assert_eq!(saved(&fixture), b"ID,SECTION,VALUE\n3,S,9\n4,S,16\n");
}

#[test]
fn grouped_filters_reject_window_reads_before_candidate_windows() {
    let fixture = bespoke(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: input.csv, types: {I: int, V: float}}}
output: {path: output.csv, columns: [ID, TOTAL, SEQ]}
columns:
- {name: ID, type: int}
- {name: TOTAL, type: float}
- {name: SEQ, type: int}
rows:
- id: selected
  group_by: [SRC.I]
  filter: 'SEQ = 2 AND TOTAL = 7'
  derivations:
    ID: SRC.I
    TOTAL: {aggregate: {expr: 'SUM(SRC.V)'}}
    SEQ: {row_number: {window: {order_by: [{variable: ID, direction: desc}]}}}
"#,
        b"I,V\n3,8\n2,3\n1,5\n2,4\n",
    );
    // This rejection control is independently authored here; candidate filters
    // under REQ-0058 run before window derivations can supply SEQ.
    let issues = attempt(&fixture).map_or_else(|issues| issues, |result| result.issues().to_vec());
    assert_eq!(
        issues,
        vec![yamaa_adapters::issue_rows::Issue {
            phase: "validation".into(),
            condition: "phase_boundary".into(),
            requirement: Some("REQ-0068".into()),
            spec_paths: vec!["rows[0].filter".into()],
            context: serde_json::json!({
                "available_phase": "window_derivation",
                "identifier": "SEQ",
                "required_phase": "row_filter",
                "row": "selected",
            })
            .to_string(),
        }],
    );
    assert!(!fixture.0.join("output.csv").exists());
}
