//! Authored public-file observations and immutable original benchmark artifacts.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-output-order-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fn copy(from: &Path, to: &Path) {
            for entry in fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    fs::create_dir(&target).unwrap();
                    copy(&entry.path(), &target)
                } else {
                    fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks")
                .join(name),
            &root,
        );
        Self(root)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<Issue>> {
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
fn authored(spec: &str, input: &[u8]) -> Fixture {
    let f = Fixture::new("schema-ordering");
    fs::write(f.0.join("spec.yaml"), spec).unwrap();
    fs::write(f.0.join("source.csv"), input).unwrap();
    f
}
#[test]
fn original_ordering_artifact_matches_immutable_csv_truth() {
    let fixture = Fixture::new("schema-ordering");
    let mut result = fixture.run().unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("adae.csv")).unwrap(),
        fs::read(fixture.0.join("expected/adae.csv")).unwrap()
    );
}
#[test]
fn internal_working_columns_order_the_projection_and_omission_keeps_construction_order() {
    let spec = r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: source.csv, types: {SORT: int}}}
output: {path: result.csv, columns: [ID], order_by: [SORT]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: SORT, type: int, derivation: SRC.SORT}
"#;
    for (source, expected) in [
        (spec.to_owned(), b"ID\nb\nc\na\n".as_slice()),
        (
            spec.replace(", order_by: [SORT]", ""),
            b"ID\na\nb\nc\n".as_slice(),
        ),
    ] {
        let fixture = authored(&source, b"ID,SORT\na,2\nb,1\nc,1\n");
        let mut result = fixture.run().unwrap();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
    }
}
#[test]
fn ordering_and_unknown_profile_negatives_keep_registered_first_findings() {
    for (name, condition, path, requirement, context) in [
        (
            "negative-order-repeated",
            "duplicate_order_term",
            "output.order_by[2]",
            "REQ-0224",
            serde_json::json!({"column":"USUBJID"}),
        ),
        (
            "negative-order-unknown-column",
            "undeclared_column",
            "output.order_by[1]",
            "REQ-0224",
            serde_json::json!({"column":"VISITNUM"}),
        ),
        (
            "negative-source-unknown-format",
            "source_profile_unknown",
            "input.DM.path",
            "REQ-0852",
            serde_json::json!({"dataset":"DM", "path":"input/dm.txt"}),
        ),
    ] {
        let fixture = Fixture::new(name);
        let issues = match fixture.run() {
            Ok(result) => {
                assert!(result.output().is_none());
                result.issues().to_vec()
            }
            Err(issues) => issues,
        };
        assert_eq!(issues.len(), 1, "{name}: {issues:?}");
        let issue = &issues[0];
        assert_eq!(
            (
                issue.phase.as_str(),
                issue.condition.as_str(),
                issue.requirement.as_deref()
            ),
            ("validation", condition, Some(requirement)),
            "{name}"
        );
        assert_eq!(issue.spec_paths, [path], "{name}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&issue.context).unwrap(),
            context,
            "{name}"
        );
    }
}
#[test]
fn csv_present_empty_is_rejected_before_study_bytes_are_requested() {
    let fixture = authored(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: absent.csv, empty_string: present}}
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: SRC.ID}]
"#,
        b"unused\n",
    );
    let issues = fixture.run().err().expect("static declaration");
    assert_eq!(issues.len(), 1);
    assert_eq!(
        (
            issues[0].phase.as_str(),
            issues[0].condition.as_str(),
            issues[0].requirement.as_deref()
        ),
        (
            "validation",
            "empty_string_present_unsupported",
            Some("REQ-1161")
        )
    );
    assert_eq!(issues[0].spec_paths, ["input.SRC.empty_string"]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
        serde_json::json!({"dataset":"SRC"})
    );
    assert!(!fixture.0.join("result.csv").exists());
}

/// Root predicates gate raw base records before key conversion and keep first key appearance.
#[test]
fn root_filter_reads_raw_driver_values_before_key_derivation() {
    let fixture = authored(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: source.csv}
filter: "SRC.KEEP = 'Y'"
output: {path: result.csv, columns: [ID, V]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: V, type: str, derivation: SRC.V}
"#,
        b"ID,V,KEEP\nbad,excluded,N\n02,seven,Y\n2,conflict,\n1,eight,Y\n2,seven,Y\n",
    );
    let mut result = fixture.run().unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("result.csv")).unwrap(),
        b"ID,V\n2,seven\n1,eight\n"
    );
    // A raw string comparison would fail if the filter read the completed integer key.
    let raw = authored(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: source.csv}
filter: "SRC.ID = '02'"
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: int, derivation: SRC.ID}]
"#,
        b"ID\n02\n3\n",
    );
    let mut result = raw.run().unwrap();
    assert!(result.issues().is_empty());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(raw.0.join("result.csv")).unwrap(), b"ID\n2\n");
}
#[test]
fn root_filter_uses_explicit_base_independently_of_input_declaration_order() {
    let fixture = authored(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {OTHER: other.csv, SRC: source.csv}
base: SRC
filter: "SRC.KEEP = 'Y'"
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: SRC.ID}]
"#,
        b"ID,KEEP\na,N\nb,Y\n",
    );
    fs::write(fixture.0.join("other.csv"), b"ID,KEEP\nwrong,Y\n").unwrap();
    let mut result = fixture.run().unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), b"ID\nb\n");
}
#[test]
fn root_filter_forbids_constructed_columns_and_conflicting_row_templates() {
    let fixture = authored(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: source.csv}
filter: "ID = 'a'"
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: SRC.ID}]
"#,
        b"ID\na\n",
    );
    let result = fixture.run().unwrap();
    assert!(result.output().is_none());
    assert_eq!(result.issues().len(), 1);
    let issue = &result.issues()[0];
    assert_eq!(
        (
            issue.phase.as_str(),
            issue.condition.as_str(),
            issue.requirement.as_deref()
        ),
        ("validation", "phase_boundary", Some("REQ-1170"))
    );
    assert_eq!(issue.spec_paths, ["filter"]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&issue.context).unwrap(),
        serde_json::json!({"identifier":"ID", "available_phase":"column_derivation", "required_phase":"row_filter"})
    );
    let mut spec = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
    spec.push_str("rows: [{id: records, dataset: SRC, derivations: {}}]\n");
    fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
    fs::remove_file(fixture.0.join("source.csv")).unwrap();
    let issues = fixture
        .run()
        .err()
        .expect("static conflicting row construction");
    assert_eq!(issues.len(), 1);
    assert_eq!(
        (
            issues[0].condition.as_str(),
            issues[0].requirement.as_deref()
        ),
        ("conflicting_row_construction", Some("REQ-1171"))
    );
}

#[test]
fn source_ordinals_precede_filtering_and_count_multiline_records_once() {
    let spec = r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: source.csv, ordinal: REC}}
filter: "SRC.KEEP = 'Y'"
output: {path: result.csv, columns: [ID, POSITION]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
"#;
    for (input, expected) in [
        (
            b"ID,KEEP,NOTE\na,N,first\nb,Y,\"two\nlines\"\nc,Y,last\n".as_slice(),
            b"ID,POSITION\nb,2\nc,3\n".as_slice(),
        ),
        (b"ID,KEEP,NOTE\n", b"ID,POSITION\n"),
    ] {
        let fixture = authored(spec, input);
        let mut result = fixture.run().unwrap();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
    }
}

#[test]
fn ordinal_collision_and_fixed_odm_schema_have_complete_public_diagnostics() {
    let fixture = Fixture::new("negative-source-ordinal-collision");
    let result = fixture.run().unwrap();
    assert!(result.output().is_none());
    let issues = result.issues();
    assert_eq!(issues.len(), 1);
    assert_eq!(
        (
            issues[0].phase.as_str(),
            issues[0].condition.as_str(),
            issues[0].requirement.as_deref()
        ),
        ("validation", "ordinal_field_collision", Some("REQ-1294"))
    );
    assert_eq!(issues[0].spec_paths, ["input.SOURCE.ordinal"]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
        serde_json::json!({"dataset":"SOURCE", "field":"SourceOrdinal"})
    );

    let fixture = authored(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: absent.parquet, ordinal: REC}}
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: {odm: {item: SRC.ITEM}}}]
"#,
        b"unused\n",
    );
    let issues = fixture
        .run()
        .err()
        .expect("fixed schema rejects ordinal before capture");
    assert_eq!(issues.len(), 1);
    assert_eq!(
        (
            issues[0].phase.as_str(),
            issues[0].condition.as_str(),
            issues[0].requirement.as_deref()
        ),
        ("validation", "odm_schema_field_type", Some("REQ-1295"))
    );
    assert_eq!(issues[0].spec_paths, ["input.SRC.ordinal"]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
        serde_json::json!({"dataset":"SRC", "declared":"ordinal"})
    );
}

#[test]
fn metadata_odm_spelling_does_not_turn_an_ordinal_source_into_odm() {
    let fixture = authored(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: source.csv, ordinal: REC}}
output: {path: result.csv, columns: [ID, POSITION]}
columns:
  - {name: ID, type: int, metadata: {odm: SRC.ITEM}, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
"#,
        b"ID\n3\n1\n",
    );
    let mut result = fixture.run().unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("result.csv")).unwrap(),
        b"ID,POSITION\n3,1\n1,2\n"
    );
}

#[test]
fn nested_column_and_row_odm_derivations_still_reject_ordinals_before_capture() {
    for declarations in [
        "columns:\n  - name: ID\n    type: str\n    derivation:\n      case:\n        - when: \"1 = 1\"\n          then: {odm: {item: SRC.ITEM}}\n        - otherwise: {literal: x}\n",
        "columns: [{name: ID, type: str, derivation: SRC.ID}]\nrows:\n  - id: records\n    derivations:\n      ID: {value: {odm: {item: SRC.ITEM}}}\n",
    ] {
        let fixture = authored(
            &format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: absent.parquet, ordinal: REC}}}}\noutput: {{path: result.csv, columns: [ID]}}\n{declarations}"),
            b"unused\n",
        );
        let issues = fixture.run().err().expect("ODM ordinal admission before capture");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].condition, "odm_schema_field_type");
        assert_eq!(issues[0].spec_paths, ["input.SRC.ordinal"]);
    }
}

#[test]
fn parquet_records_expose_identical_ordinal_policy_without_changing_stored_fields() {
    use yamaa_adapters::{arrow_table::TableLimits, parquet_artifact, typed_csv};
    let fixture = authored(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: source.parquet, ordinal: REC}}
output: {path: result.csv, columns: [ID, POSITION]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
"#,
        b"unused\n",
    );
    for (input, expected) in [
        (
            b"ID\nb\na\n".as_slice(),
            b"ID,POSITION\nb,1\na,2\n".as_slice(),
        ),
        (b"ID\n", b"ID,POSITION\n"),
    ] {
        // Known physical input; the expected generated positions above are authored independently.
        let table = typed_csv::parse(
            input,
            &[],
            Default::default(),
            TableLimits {
                max_rows: 2,
                max_columns: 1,
                max_batches: 1,
                max_cells: 2,
            },
        )
        .unwrap();
        let bytes = parquet_artifact::render(
            &table,
            &[0],
            parquet_artifact::Limits {
                output_bytes: 4096,
                staged_bytes: 4096,
                cells: 2,
                columns: 1,
            },
        )
        .unwrap();
        fs::write(fixture.0.join("source.parquet"), bytes).unwrap();
        let mut result = fixture.run().unwrap();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
    }
}
