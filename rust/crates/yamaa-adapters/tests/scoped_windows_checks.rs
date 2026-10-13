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
