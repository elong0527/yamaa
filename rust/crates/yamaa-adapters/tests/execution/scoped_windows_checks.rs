#![cfg(any(unix, windows))]
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-scoped-window-checks-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn benchmark(name: &str) -> (Self, PathBuf) {
        fn copy(source: &Path, destination: &Path) {
            fs::create_dir_all(destination).unwrap();
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &destination.join(entry.file_name()));
                } else {
                    fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
                }
            }
        }
        let study = Self::new();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks")
            .join(name);
        copy(&root.join("input"), &study.0.join("input"));
        fs::copy(root.join("spec.yaml"), study.0.join("spec.yaml")).unwrap();
        (study, root)
    }
    fn build(&self) -> file_application::Domain {
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
                example: "scopes",
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
#[test]
fn original_row_count_fractions_keep_exact_primary_and_governed_log_bytes() {
    let (study, root) = Study::benchmark("schema-row-count-fractions");
    let mut result = study.build();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    for name in ["adsl.csv", "adsl-checks.csv", "adsl-warnings.csv"] {
        assert_eq!(
            fs::read(study.0.join(name)).unwrap(),
            fs::read(root.join("expected").join(name)).unwrap(),
            "{name}"
        );
    }
}
#[test]
fn original_unnamed_grouped_count_reports_two_retained_groups_and_counts() {
    let (study, _) = Study::benchmark("negative-count-unnamed");
    let result = study.build();
    assert!(result.output().is_none());
    let issues = result.issues();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition, "row_count_failed");
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0385"));
    let context: serde_json::Value = serde_json::from_str(&issues[0].context).unwrap();
    assert_eq!(context["failure_count"], 2);
    assert_eq!(context["counts"], serde_json::json!([2, 0]));
    assert_eq!(context["keys"].as_array().unwrap().len(), 2);
}
#[test]
fn original_driver_order_and_ordinal_windows_match_unchanged_saved_outputs() {
    for (name, output) in [
        ("schema-source-ordinal", "records.csv"),
        ("adam-adfa-fever", "adfa.csv"),
        ("sdtm-ec-collected-exposure", "ec.csv"),
        ("sdtm-ex-combination", "ex.csv"),
        ("sdtm-fa-reactogenicity-diary", "fa.csv"),
        ("sdtm-relrec-links", "relrec.csv"),
    ] {
        let (study, root) = Study::benchmark(name);
        let mut result = study.build();
        assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
        assert!(result.save().unwrap(), "{name}");
        assert_eq!(
            fs::read(study.0.join(output)).unwrap(),
            fs::read(root.join("expected").join(output)).unwrap(),
            "{name}"
        );
    }
}
#[test]
fn when_binds_groups_without_changing_the_fraction_denominator() {
    let study = Study::new();
    fs::write(
        study.0.join("input.csv"),
        "I,G,F,B\n1,A,Y,Y\n2,A,N,\n3,B,N,\n4,B,N,\n",
    )
    .unwrap();
    fs::write(study.0.join("spec.yaml"),r#"schema_version: '1.0'
domain: TEST
keys: [I]
input: {SRC: input.csv}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: G, type: str, derivation: SRC.G}
  - {name: F, type: str, derivation: SRC.F}
  - {name: B, type: str, derivation: SRC.B}
output: {path: out.csv, columns: [I, G, F, B], verification_log: checks.csv}
verifications:
  - row_count: {group_by: [G], filter: "F = 'Y'", when: "B = 'Y'", min_fraction: 0.5, max_fraction: 0.5}
"#).unwrap();
    let mut result = study.build();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert!(fs::read_to_string(study.0.join("checks.csv"))
        .unwrap()
        .contains("held,,2,0,{}"));
}

#[test]
fn grouped_warning_retains_all_counts_in_logs_and_samples_only_issue_display() {
    let study = Study::new();
    fs::write(
        study.0.join("input.csv"),
        "I,G,F\n1,A,N\n2,B,N\n3,C,N\n4,D,N\n5,E,N\n6,F,N\n",
    )
    .unwrap();
    fs::write(
        study.0.join("spec.yaml"),
        r#"schema_version: '1.0'
domain: TEST
keys: [I]
input: {SRC: input.csv}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: G, type: str, derivation: SRC.G}
  - {name: F, type: str, derivation: SRC.F}
output: {path: out.csv, columns: [I, G, F], warning_log: warnings.csv}
verifications:
  - row_count: {group_by: [G], filter: "F = 'Y'", min_fraction: 1, severity: warning}
"#,
    )
    .unwrap();
    let mut result = study.build();
    assert!(result.issues().is_empty());
    let check = &result.observations()["verifications"][0];
    assert_eq!(
        check["failure"]["context"]["counts"],
        serde_json::json!([0, 0, 0, 0, 0])
    );
    assert_eq!(
        check["failure"]["log_context"]["counts"],
        serde_json::json!([0, 0, 0, 0, 0, 0])
    );
    assert_eq!(
        check["failure"]["log_context"]["denominators"],
        serde_json::json!([1, 1, 1, 1, 1, 1])
    );
    assert_eq!(
        check["failure"]["offending_keys"].as_array().unwrap().len(),
        6
    );
    assert!(result.save().unwrap());
    assert!(fs::read_to_string(study.0.join("warnings.csv"))
        .unwrap()
        .contains("[0,0,0,0,0,0]"));
}

#[test]
fn original_row_filters_read_named_lookup_state_after_candidate_derivations() {
    for (name, file) in [
        ("adam-adqs-missed-visit-locf", "adqs.csv"),
        ("adam-advs-windows", "advs.csv"),
    ] {
        let (study, root) = Study::benchmark(name);
        let mut result = study.build();
        assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read(study.0.join(file)).unwrap(),
            fs::read(root.join("expected").join(file)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn original_named_intermediate_driver_preserves_filtered_source_order_and_csv_truth() {
    let (study, root) = Study::benchmark("adam-adae-serious-events");
    let mut result = study.build();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(study.0.join("adae.csv")).unwrap(),
        fs::read(root.join("expected/adae.csv")).unwrap()
    );
}

#[test]
fn named_driver_reads_current_record_and_groups_filtered_rows_in_first_occurrence_order() {
    for grouped in [false, true] {
        let study = Study::new();
        fs::write(
            study.0.join("input.csv"),
            "I,G,V,K\n1,B,10,Y\n2,A,20,N\n3,A,30,Y\n4,B,40,Y\n5,C,50,N\n",
        )
        .unwrap();
        let keys = if grouped { "G" } else { "I" };
        let (group, derivations, expected) = if grouped {
            ("    group_by: [DRV.G]\n", "      I: {literal: 0}\n      G: DRV.G\n      V: {aggregate: {expr: 'SUM(DRV.V)'}}\n", "I,G,V\n0,B,50\n0,A,30\n")
        } else {
            ("", "      I: {source: {variable: DRV.I}}\n      G: {str_case: {source: DRV.G, to: lower}}\n      V: {source: {variable: DRV.V}}\n", "I,G,V\n1,b,10\n3,a,30\n4,b,40\n")
        };
        fs::write(study.0.join("spec.yaml"), format!("schema_version: '1.0'\ndomain: TEST\nkeys: [{keys}]\ninput: {{SRC: {{path: input.csv, types: {{I: int, V: int}}}}}}\nintermediates:\n  - {{id: DRV, dataset: SRC, filter: \"SRC.K = 'Y'\"}}\ncolumns:\n  - {{name: I, type: int}}\n  - {{name: G, type: str}}\n  - {{name: V, type: int}}\nrows:\n  - id: selected\n    dataset: DRV\n{group}    derivations:\n{derivations}output: {{path: out.csv, columns: [I, G, V]}}\n")).unwrap();
        let mut result = study.build();
        assert!(
            result.issues().is_empty(),
            "grouped={grouped}: {:?}",
            result.issues()
        );
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read_to_string(study.0.join("out.csv")).unwrap(),
            expected
        );
    }
}

#[test]
fn named_driver_column_projection_rejects_hidden_scalar_and_grouping_fields() {
    for (group, derive, expected_path) in [
        (
            "",
            "I: {source: {variable: DRV.I}}",
            "rows[0].derivations.I.source",
        ),
        (
            "    group_by: [DRV.I]\n",
            "I: {literal: 1}",
            "rows[0].group_by[0]",
        ),
    ] {
        let study = Study::new();
        fs::write(study.0.join("input.csv"), "I,V\n1,10\n").unwrap();
        fs::write(study.0.join("spec.yaml"), format!("schema_version: '1.0'\ndomain: TEST\nkeys: [I]\ninput: {{SRC: {{path: input.csv, types: {{I: int, V: int}}}}}}\nintermediates:\n  - {{id: DRV, dataset: SRC, columns: [V]}}\ncolumns:\n  - {{name: I, type: int}}\nrows:\n  - id: selected\n    dataset: DRV\n{group}    derivations:\n      {derive}\noutput: {{path: out.csv, columns: [I]}}\n")).unwrap();
        let result = study.build();
        assert!(result.output().is_none());
        assert!(
            result.issues().iter().any(|issue| {
                issue.condition == "unknown_field"
                    && issue.spec_paths.iter().any(|path| path == expected_path)
            }),
            "{:?}",
            result.issues()
        );
        assert!(!study.0.join("out.csv").exists());
    }
}

// Preserve the authored inputs and assertions from the runtime tests removed
// with the Python evaluator. These cases exercise the shared public build;
// they do not restore the retired Python planner or its extension hooks.
#[test]
fn original_filtered_missing_targets_keep_their_value_in_every_arrival_order() {
    let source = r#"schema_version: "1.0"
domain: EXAMPLE
keys: [GROUP_ID]
input:
  SOURCE: input.csv
output:
  path: example.csv
  columns: [GROUP_ID, TARGET_VALUE]
  order_by: [GROUP_ID]
columns:
  - name: GROUP_ID
    type: str
    label: Group Identifier
    derivation: SOURCE.GroupID
  - name: TARGET_VALUE
    type: str
    label: Target Value
    derivation:
      source:
        filter: "SOURCE.RecordKind = 'TARGET'"
        variable: SOURCE.Value
"#;
    for records in [
        [
            "G1,UNRELATED,g1-unrelated",
            "G1,TARGET,",
            "G2,UNRELATED,g2-unrelated",
            "G2,TARGET,g2-target",
        ],
        [
            "G1,TARGET,",
            "G1,UNRELATED,g1-unrelated",
            "G2,TARGET,g2-target",
            "G2,UNRELATED,g2-unrelated",
        ],
        [
            "G2,UNRELATED,g2-unrelated",
            "G1,UNRELATED,g1-unrelated",
            "G2,TARGET,g2-target",
            "G1,TARGET,",
        ],
    ] {
        let study = Study::new();
        fs::write(study.0.join("spec.yaml"), source).unwrap();
        fs::write(
            study.0.join("input.csv"),
            format!("GroupID,RecordKind,Value\n{}\n", records.join("\n")),
        )
        .unwrap();
        let mut result = study.build();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(!study.0.join("example.csv").exists());
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read(study.0.join("example.csv")).unwrap(),
            b"GROUP_ID,TARGET_VALUE\nG1,\nG2,g2-target\n"
        );
    }
}

fn original_key_identity(source: &str, columns: &str, types: &str) -> String {
    format!(
        "schema_version: '1.0'\ndomain: OUT\ninput: {{SRC: {{path: input.csv, types: {types}}}}}\nkeys: [B, A]\noutput: {{path: out.csv, columns: {columns}}}\ncolumns:\n{source}"
    )
}

fn save_csv(study: &Study, mut result: file_application::Domain, path: &str) -> Vec<u8> {
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(!study.0.join(path).exists());
    assert!(result.save().unwrap());
    fs::read(study.0.join(path)).unwrap()
}

#[test]
fn original_reordered_key_names_preserve_typed_values_and_dependencies() {
    for (source, types, input, expected) in [
        (
            "  - {name: A, type: str, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n",
            "{X: str, Y: str}",
            "X,Y\nfirst,second\nnext,third\nfirst,second\n",
            "A,B\nfirst,second\nnext,third\n",
        ),
        (
            "  - {name: A, type: str, derivation: SRC.X}\n  - {name: B, type: int, derivation: SRC.Y}\n",
            "{X: str, Y: int}",
            "X,Y\nfirst,2\nnext,3\nfirst,2\n",
            "A,B\nfirst,2\nnext,3\n",
        ),
        (
            "  - {name: A, type: int, derivation: SRC.X}\n  - {name: B, type: int, derivation: {compute: {expr: 'A + 1'}}}\n",
            "{X: int}",
            "X\n4\n2\n4\n",
            "A,B\n4,5\n2,3\n",
        ),
    ] {
        let study = Study::new();
        fs::write(
            study.0.join("spec.yaml"),
            original_key_identity(source, "[A, B]", types),
        )
        .unwrap();
        fs::write(study.0.join("input.csv"), input).unwrap();
        let result = study.build();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert_eq!(save_csv(&study, result, "out.csv"), expected.as_bytes());
    }
}

#[test]
fn original_missing_key_identity_is_distinct_from_user_text_and_record_order() {
    for (source, columns, types, input) in [
        (
            "  - {name: A, type: str, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n",
            "[A, B]",
            "{X: str, Y: str}",
            "X,Y\n,present\n",
        ),
        (
            "  - {name: A, type: int, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n",
            "[A, B]",
            "{X: int, Y: str}",
            "X,Y\n1,__missing_key__\n,present\n",
        ),
        (
            "  - {name: A, type: int, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n  - {name: VALUE, type: str, derivation: SRC.Z}\n",
            "[A, B, VALUE]",
            "{X: int, Y: str, Z: str}",
            "X,Y,Z\n,present,missing\n0,__missing_key__,valid\n",
        ),
        (
            "  - {name: A, type: int, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n  - {name: VALUE, type: str, derivation: SRC.Z}\n",
            "[A, B, VALUE]",
            "{X: int, Y: str, Z: str}",
            "X,Y,Z\n1,__missing_key__,valid\n,present,missing\n",
        ),
    ] {
        let study = Study::new();
        fs::write(
            study.0.join("spec.yaml"),
            original_key_identity(source, columns, types),
        )
        .unwrap();
        fs::write(study.0.join("input.csv"), input).unwrap();
        let mut result = study.build();
        let issues = result.issues();
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].condition, "missing_key");
        assert_eq!(issues[0].spec_paths, ["keys[1]"]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
            serde_json::json!({"column": "A", "missing_count": 1,
                "keys": [{"B": "present", "A": null}]})
        );
        assert!(result.output().is_none());
        assert!(result.save().is_err());
        assert!(!study.0.join("out.csv").exists());
    }
}

fn original_window_spec(extra_columns: &str, extra_derivations: &str, window: &str) -> String {
    format!(
        "schema_version: '1.0'\ndomain: OUT\nbase: SRC\ninput: {{SRC: {{path: input.csv, types: {{G: str, X: float, S: int}}}}}}\nkeys: [GRP, SEQ]\noutput: {{path: out.csv, columns: [GRP, SEQ, VAL, PREV]}}\ncolumns:\n  - {{name: GRP, type: str}}\n  - {{name: SEQ, type: int}}\n  - {{name: VAL, type: float}}\n  - {{name: PREV, type: float}}\n{extra_columns}rows:\n  - id: visits\n    derivations:\n      GRP: SRC.G\n      SEQ: SRC.S\n      VAL: SRC.X\n      PREV: {{row_value: {{source: VAL, offset: -1, window: {window}}}}}\n{extra_derivations}"
    )
}

fn original_visits(study: &Study, source: &str) -> file_application::Domain {
    fs::write(study.0.join("spec.yaml"), source).unwrap();
    fs::write(
        study.0.join("input.csv"),
        "G,X,S\na,1,1\na,2,2\na,3,3\nb,10,1\nb,20,2\n",
    )
    .unwrap();
    let result = study.build();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    result
}

fn float_column(result: &file_application::Domain, name: &str) -> Vec<Option<f64>> {
    let (_, values) = result
        .output_table()
        .unwrap()
        .columns()
        .iter()
        .find(|(column, _)| column == name)
        .unwrap();
    let yamaa_adapters::public_table::Values::Float(values) = values else {
        panic!("{name}: {values:?}");
    };
    values.clone()
}

#[test]
fn original_row_window_lags_keep_partition_ties_and_template_ownership() {
    let expected = vec![None, Some(1.0), Some(2.0), None, Some(10.0)];
    for window in [
        "{group_by: [GRP], order_by: [SEQ]}",
        "{group_by: [GRP], order_by: [GRP]}",
    ] {
        let study = Study::new();
        let result = original_visits(&study, &original_window_spec("", "", window));
        assert_eq!(float_column(&result, "PREV"), expected);
    }
    let study = Study::new();
    let source = original_window_spec("", "", "{order_by: [SEQ]}").replace(
        "  - id: visits\n",
        "  - id: a_only\n    filter: \"SRC.G = 'a'\"\n",
    );
    let second = source
        .split_once("rows:\n")
        .unwrap()
        .1
        .replace("a_only", "b_only")
        .replace("SRC.G = 'a'", "SRC.G = 'b'");
    let result = original_visits(&study, &format!("{source}{second}"));
    assert_eq!(float_column(&result, "PREV"), expected);
}

#[test]
fn original_row_window_chains_complete_scalars_before_later_window_reads() {
    let window = "{group_by: [GRP], order_by: [SEQ]}";
    for (columns, derivations, column, expected) in [
        (
            "  - {name: PREV2, type: float}\n",
            "      PREV2: {row_value: {source: PREV, offset: -1, window: {group_by: [GRP], order_by: [SEQ]}}}\n",
            "PREV2",
            vec![None, None, Some(1.0), None, None],
        ),
        (
            "  - {name: CHG, type: float}\n  - {name: DOUBLED, type: float}\n",
            "      DOUBLED: {compute: {expr: 'CHG * 2'}}\n      CHG: {compute: {expr: 'VAL - PREV'}}\n",
            "DOUBLED",
            vec![None, Some(2.0), Some(2.0), None, Some(20.0)],
        ),
        (
            "  - {name: DOUBLED, type: float}\n  - {name: BAD, type: float}\n",
            "      DOUBLED: {compute: {expr: 'PREV * 2'}}\n      BAD: {row_value: {source: DOUBLED, offset: -1, window: {group_by: [GRP], order_by: [SEQ]}}}\n",
            "BAD",
            vec![None, None, Some(2.0), None, None],
        ),
    ] {
        let study = Study::new();
        let source = original_window_spec(columns, derivations, window)
            .replace("columns: [GRP, SEQ, VAL, PREV]", &format!("columns: [GRP, SEQ, VAL, PREV, {column}]"));
        let result = original_visits(&study, &source);
        assert_eq!(float_column(&result, column), expected);
    }
}

#[test]
fn original_windows_in_one_case_column_keep_distinct_orderings() {
    let study = Study::new();
    let source = original_window_spec("", "", "{group_by: [GRP], order_by: [SEQ]}")
        .replace("  - {name: PREV, type: float}", r#"  - name: PREV
    type: float
    derivation:
      case:
        - when: SEQ = 1
          then: {row_value: {source: VAL, offset: 1, window: {group_by: [GRP], order_by: [SEQ]}}}
        - otherwise: {row_value: {source: VAL, offset: 1, window: {group_by: [GRP], order_by: [{variable: SEQ, direction: desc}]}}}"#)
        .replace("      PREV: {row_value: {source: VAL, offset: -1, window: {group_by: [GRP], order_by: [SEQ]}}}\n", "");
    let result = original_visits(&study, &source);
    assert_eq!(
        float_column(&result, "PREV"),
        [Some(2.0), Some(1.0), Some(2.0), Some(20.0), Some(10.0)]
    );
}

#[test]
fn original_source_ordinals_follow_physical_swaps_without_adding_output_fields() {
    for labels in [["ROW_B", "ROW_A"], ["ROW_A", "ROW_B"]] {
        let (study, _) = Study::benchmark("schema-source-ordinal");
        fs::write(
            study.0.join("input/source.csv"),
            format!(
                "GroupID,RecordLabel,Include\nG1,{},Y\nG1,{},Y\n",
                labels[0], labels[1]
            ),
        )
        .unwrap();
        let result = study.build();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert_eq!(
            save_csv(&study, result, "records.csv"),
            format!(
                "GROUP_ID,ROW_SEQUENCE,RECORD_LABEL,SOURCE_ORDINAL\nG1,1,{},1\nG1,2,{},2\n",
                labels[0], labels[1]
            )
            .as_bytes()
        );
    }
    let (study, _) = Study::benchmark("schema-source-ordinal");
    let source = fs::read_to_string(study.0.join("spec.yaml")).unwrap();
    let (before, after) = source.split_once("  - name: SOURCE_ORDINAL\n").unwrap();
    let rows = after.split_once("rows:\n").unwrap().1;
    let source = format!("{before}rows:\n{rows}").replace(", SOURCE_ORDINAL]", "]");
    fs::write(study.0.join("spec.yaml"), source).unwrap();
    let result = study.build();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert_eq!(
        save_csv(&study, result, "records.csv"),
        b"GROUP_ID,ROW_SEQUENCE,RECORD_LABEL\nG1,1,ROW_B\nG1,2,ROW_A\nG1,3,ROW_A\nG2,1,ROW_B\nG2,2,ROW_A\n"
    );
}

#[test]
fn original_ordinal_windows_keep_input_positions_across_reused_templates() {
    let (study, _) = Study::benchmark("schema-source-ordinal");
    let mut source = fs::read_to_string(study.0.join("spec.yaml")).unwrap();
    source.push_str(
        "  - id: second\n    dataset: SOURCE\n    filter: SOURCE.SourceOrdinal = 1\n    derivations: {}\n",
    );
    fs::write(study.0.join("spec.yaml"), source).unwrap();
    let result = study.build();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert_eq!(
        save_csv(&study, result, "records.csv"),
        b"GROUP_ID,ROW_SEQUENCE,RECORD_LABEL,SOURCE_ORDINAL\nG1,1,ROW_B,2\nG1,2,ROW_A,5\nG1,3,ROW_A,6\nG2,1,ROW_B,1\nG2,2,ROW_B,1\nG2,3,ROW_A,4\n"
    );
}

#[test]
fn original_ordinal_group_reductions_run_before_group_numbering() {
    let (study, _) = Study::benchmark("schema-source-ordinal");
    fs::write(
        study.0.join("spec.yaml"),
        r#"schema_version: "1.0"
domain: RECORDS
keys: [GROUP_ID]
input:
  SOURCE: {path: input/source.csv, ordinal: SourceOrdinal}
output:
  path: records.csv
  columns: [GROUP_ID, SOURCE_ORDINAL, ROW_SEQUENCE]
  order_by: [GROUP_ID, ROW_SEQUENCE]
columns:
  - {name: GROUP_ID, type: str, derivation: SOURCE.GroupID}
  - {name: ROW_SEQUENCE, type: int}
  - {name: SOURCE_ORDINAL, type: int}
rows:
  - id: included
    dataset: SOURCE
    group_by: [SOURCE.GroupID]
    derivations:
      SOURCE_ORDINAL: {aggregate: "MIN(SOURCE.SourceOrdinal)"}
      ROW_SEQUENCE: {row_number: {window: {order_by: [SOURCE_ORDINAL]}}}
"#,
    )
    .unwrap();
    let result = study.build();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert_eq!(
        save_csv(&study, result, "records.csv"),
        b"GROUP_ID,SOURCE_ORDINAL,ROW_SEQUENCE\nG1,2,2\nG2,1,1\n"
    );
}
