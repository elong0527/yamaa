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
    fn new(input: &str, specification: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-nested-windows-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("input.csv"), input).unwrap();
        fs::write(path.join("spec.yaml"), specification).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn build(&self) -> file_application::Domain {
        self.try_build().unwrap()
    }
    fn try_build(&self) -> Result<file_application::Domain, file_application::Failure> {
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
                example: "nested-windows",
                specification: &path,
                base_directory: ".",
            },
        )
    }
    fn saved(&self) -> String {
        let mut result = self.build();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        fs::read_to_string(self.0.join("out.csv")).unwrap()
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn nested_case_and_concat_windows_use_the_full_completed_context() {
    let study = Study::new(
        "I,G,O,V\n1,A,2,a2\n2,A,1,a1\n3,A,3,a3\n4,B,1,b1\n",
        r#"schema_version: '1.0'
domain: TEST
keys: [I]
input: {SRC: {path: input.csv, types: {I: int, O: int}}}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - name: N
    type: int
    derivation:
      case:
        - when: 'I > 1'
          then: {row_number: {window: {group_by: [SRC.G], order_by: [SRC.O]}}}
        - otherwise: {literal: -1}
  - name: C
    type: str
    derivation:
      str_concat:
        sources:
          - {literal: 'prior='}
          - case:
              - when: 'I = 3'
                then: {row_value: {source: SRC.V, offset: -1, window: {group_by: [SRC.G], order_by: [SRC.O]}}}
              - otherwise: {literal: skip}
output: {path: out.csv, columns: [I, N, C]}
"#,
    );
    assert_eq!(
        study.saved(),
        "I,N,C\n1,-1,prior=skip\n2,1,prior=skip\n3,3,prior=a2\n4,1,prior=skip\n"
    );
}
#[test]
fn nested_windows_remain_template_local_and_observe_pre_window_row_filtering() {
    let study = Study::new(
        "I,O\n1,2\n2,1\n3,3\n",
        r#"schema_version: '1.0'
domain: TEST
keys: [T, I]
input: {SRC: {path: input.csv, types: {I: int, O: int}}}
columns:
  - {name: T, type: str}
  - {name: I, type: int, derivation: SRC.I}
  - {name: N, type: int}
rows:
  - id: first
    dataset: SRC
    derivations:
      T: {literal: A}
      N: {case: [{when: 'I > 0', then: {row_number: {window: {order_by: [SRC.O]}}}}]}
  - id: second
    dataset: SRC
    filter: 'SRC.I > 1'
    derivations:
      T: {literal: B}
      N: {case: [{when: 'I > 0', then: {row_number: {window: {order_by: [SRC.O]}}}}]}
output: {path: out.csv, columns: [T, I, N]}
"#,
    );
    assert_eq!(study.saved(), "T,I,N\nA,1,2\nA,2,1\nA,3,3\nB,2,1\nB,3,2\n");
}
#[test]
fn output_key_template_keeps_transitive_window_defaults_over_all_constructed_rows() {
    let study = Study::new(
        "I\n1\n2\n3\n",
        r#"schema_version: '1.0'
domain: TEST
keys: [T, K]
input: {SRC: {path: input.csv, types: {I: int}}}
columns:
  - {name: T, type: str}
  - {name: I, type: int, derivation: SRC.I}
  - name: N
    type: int
    derivation: {row_number: {window: {order_by: [T, I]}}}
  - {name: NC, type: str, derivation: N}
  - {name: K, type: str, derivation: {str_template: 'R{NC}'}}
rows:
  - id: first
    dataset: SRC
    derivations: {T: {literal: A}}
  - id: second
    dataset: SRC
    filter: 'SRC.I > 1'
    derivations: {T: {literal: B}}
output: {path: out.csv, columns: [T, K, I, N]}
"#,
    );
    assert_eq!(
        study.saved(),
        "T,K,I,N\nA,R1,1,1\nA,R2,2,2\nA,R3,3,3\nB,R4,2,4\nB,R5,3,5\n"
    );
}
#[test]
fn unreachable_window_branch_has_no_runtime_failure_but_reached_branch_retains_it() {
    let specification = r#"schema_version: '1.0'
domain: TEST
keys: [I]
input: {SRC: {path: input.csv, types: {I: int, D: date}}}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: D, type: date, derivation: SRC.D}
  - name: B
    type: str
    derivation:
      case:
        - when: 'I < 0'
          then: {baseline_flag: {date: D, reference_date: D}}
        - otherwise: {literal: skipped}
output: {path: out.csv, columns: [I, B]}
"#;
    let input = "I,D\n1,2024-01-01\n2,2024-01-01\n";
    let skipped = Study::new(input, specification);
    assert_eq!(skipped.saved(), "I,B\n1,skipped\n2,skipped\n");
    let reached = Study::new(input, &specification.replace("I < 0", "I = 2"));
    let result = reached.build();
    assert!(result.output().is_none());
    assert_eq!(result.issues().len(), 1, "{:?}", result.issues());
    assert_eq!(result.issues()[0].condition, "ambiguous_baseline");
    assert!(result.issues()[0]
        .spec_paths
        .iter()
        .any(|path| path.contains("case[0].then.baseline_flag")));
}
#[test]
fn arbitrary_width_window_offsets_select_absent_neighbors_without_float_conversion() {
    let study = Study::new(
        "I,V\n1,a\n2,b\n",
        r#"schema_version: '1.0'
domain: TEST
keys: [I]
input: {SRC: input.csv}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: V, type: str, derivation: SRC.V}
  - name: BEFORE
    type: str
    derivation: {row_value: {source: V, offset: -99999999999999999999999999999999999999, window: {order_by: [I]}}}
  - name: AFTER
    type: str
    derivation: {row_value: {source: V, offset: 99999999999999999999999999999999999999, window: {order_by: [I]}}}
output: {path: out.csv, columns: [I, BEFORE, AFTER]}
"#,
    );
    assert_eq!(study.saved(), "I,BEFORE,AFTER\n1,,\n2,,\n");
}

#[test]
fn selected_window_partition_does_not_evaluate_unselected_partition_failures() {
    let study = Study::new(
        "I,G,D\n1,A,2024-01-01\n2,B,2024-01-01\n3,B,2024-01-01\n",
        r#"schema_version: '1.0'
domain: TEST
keys: [I]
input: {SRC: {path: input.csv, types: {I: int, D: date}}}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: G, type: str, derivation: SRC.G}
  - {name: D, type: date, derivation: SRC.D}
  - name: B
    type: str
    derivation:
      case:
        - when: "G = 'A'"
          then: {baseline_flag: {date: D, reference_date: D, window: {group_by: [G]}}}
        - otherwise: {literal: skipped}
output: {path: out.csv, columns: [I, B]}
"#,
    );
    assert_eq!(study.saved(), "I,B\n1,Y\n2,skipped\n3,skipped\n");
}

#[test]
fn missing_environment_keeps_unknown_nested_function_static_before_input_capture() {
    let study = Study::new(
        "I\n1\n",
        r#"schema_version: '1.0'
domain: TEST
keys: [I]
input: {SRC: input.csv}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - name: V
    type: int
    derivation:
      case:
        - when: 'I > 0'
          then: {function: {name: unknown, args: {x: I}}}
output: {path: out.csv, columns: [I, V]}
"#,
    );
    fs::remove_file(study.0.join("input.csv")).unwrap();
    let Err(issues) = study.try_build() else {
        panic!("static function failure before input capture")
    };
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition, "unknown_project_function");
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0698"));
    assert_eq!(
        issues[0].spec_paths,
        ["columns.V.derivation.case[0].then.function.name"]
    );
}
