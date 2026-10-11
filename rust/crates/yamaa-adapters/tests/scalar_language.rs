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
fn golden(fixture: &Fixture) -> serde_json::Value {
    let expected = yamaa_adapters::yaml_decode::decode_yaml(
        &fs::read(fixture.0.join("expected/error.yaml")).unwrap(),
        Default::default(),
    )
    .unwrap()
    .document;
    fn json(d: &yamaa_core::schema::Document, id: usize) -> serde_json::Value {
        use yamaa_core::schema::DocumentNode as N;
        match &d.nodes()[id] {
            N::Null => serde_json::Value::Null,
            N::Text(value) => serde_json::json!(value),
            N::Integer(value) => value.parse::<i64>().unwrap().into(),
            N::Float(value) => serde_json::json!(value),
            N::Boolean(value) => (*value).into(),
            N::Sequence(items) => items.iter().map(|id| json(d, *id)).collect(),
            N::Mapping(items) => items
                .iter()
                .map(|(key, value)| {
                    let N::Text(key) = &d.nodes()[*key] else {
                        panic!("key")
                    };
                    (key.clone(), json(d, *value))
                })
                .collect(),
        }
    }
    json(&expected, expected.root())
}

fn issue_json(issue: &yamaa_adapters::issue_rows::Issue) -> serde_json::Value {
    serde_json::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()})
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
#[test]
fn unchanged_scalar_benchmarks_and_negative_truth() {
    for name in [
        "negative-greatest-mixed",
        "negative-least-mixed",
        "negative-str-contains-bool-result",
        "negative-str-contains-invalid-regex",
        "negative-usubjid-length",
    ] {
        let fixture = Fixture::new(name);
        let expected = golden(&fixture);
        if name.ends_with("mixed") {
            fs::remove_dir_all(fixture.0.join("input")).unwrap();
        }
        let issues = match attempt(&fixture) {
            Ok(result) => result.issues().to_vec(),
            Err(issues) => issues,
        };
        assert_eq!(
            issues.iter().map(issue_json).collect::<Vec<_>>(),
            vec![expected],
            "{name}"
        );
    }
    let fixture = Fixture::new("schema-str-contains-missing");
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    let observed = result.observations();
    assert_eq!(observed["handler_counts"][0]["count"], serde_json::json!(2));
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("adae.csv")).unwrap(),
        fs::read(fixture.0.join("expected/adae.csv")).unwrap()
    );
}
fn spec(derivation: &str, result_type: &str) -> String {
    format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv, types: {{I: int, F: float, D: date}}}}}}\noutput: {{path: output.csv, columns: [ID, RESULT]}}\ncolumns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n  - {{name: RESULT, type: {result_type}, derivation: {derivation}}}\n")
}
#[test]
fn extremes_skip_missing_and_keep_wide_integers_and_temporal_values() {
    for (operation, result_type, input, expected) in [
        (
            "{greatest: {sources: [SRC.I, SRC.F]}}",
            "str",
            "ID,I,F,D\na,9007199254740993,9007199254740992,2025-01-01\nb,,,2025-01-01\n",
            "ID,RESULT\na,9007199254740993\nb,\n",
        ),
        (
            "{least: {sources: [SRC.I, SRC.F]}}",
            "str",
            "ID,I,F,D\na,-9007199254740993,-9007199254740992,2025-01-01\nb,,,2025-01-01\n",
            "ID,RESULT\na,-9007199254740993\nb,\n",
        ),
        (
            "{greatest: {sources: [SRC.D, SRC.D]}}",
            "date",
            "ID,I,F,D\na,1,1,2025-01-01\nb,1,1,2025-01-02\n",
            "ID,RESULT\na,2025-01-01\nb,2025-01-02\n",
        ),
    ] {
        let fixture = bespoke(&spec(operation, result_type), input.as_bytes());
        let mut result = fixture.run();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read(fixture.0.join("output.csv")).unwrap(),
            expected.as_bytes()
        );
    }
}
#[test]
fn nested_concat_keeps_order_and_handler_counts_and_stops_at_missing() {
    let derivation="{str_concat: {sources: [{literal: '['}, {str_case: {source: SRC.TEXT, to: upper, missing: absent}}, {str_concat: {sources: [{literal: ']'}, {source: SRC.PEER}], missing: fallback}}]}}";
    let fixture = bespoke(
        &spec(derivation, "str"),
        b"ID,I,F,D,TEXT,PEER\na,1,1,2025-01-01,abc,z\nb,1,1,2025-01-01,,q\nc,1,1,2025-01-01,xyz,\n",
    );
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("output.csv")).unwrap(),
        b"ID,RESULT\na,[ABC]z\nb,[absent]q\nc,[XYZfallback\n"
    );
    let observed = result.observations();
    let counts = observed["handler_counts"].as_array().unwrap();
    assert!(counts.iter().any(|row| row["spec_path"]
        == "columns.RESULT.derivation.str_concat.sources[1].str_case.missing"
        && row["count"] == 1));
    assert!(counts.iter().any(|row| row["spec_path"]
        == "columns.RESULT.derivation.str_concat.sources[2].str_concat.missing"
        && row["count"] == 1));
    let fixture = bespoke(
        &spec(
            "{str_concat: {sources: [{source: SRC.TEXT}, {literal: 42}], missing: fallback}}",
            "str",
        ),
        b"ID,I,F,D,TEXT\na,1,1,2025-01-01,\n",
    );
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("output.csv")).unwrap(),
        b"ID,RESULT\na,fallback\n"
    );
}
#[test]
fn concat_input_diagnostic_preserves_operand_path_and_blocks_publication() {
    let fixture = bespoke(
        &spec(
            "{str_concat: {sources: [{literal: x}, {literal: 42}]}}",
            "str",
        ),
        b"ID,I,F,D\na,1,1,2025-01-01\n",
    );
    let result = fixture.run();
    assert_eq!(result.issues().len(), 1);
    assert_eq!(
        issue_json(&result.issues()[0]),
        serde_json::json!({"phase":"validation","condition":"incompatible_input_type","requirement":"REQ-0308","spec_paths":["columns.RESULT.derivation.str_concat.sources[1]"],"context":{"source":"sources[1]","expected":"str","actual":"int"}})
    );
    assert!(!fixture.0.join("output.csv").exists());
}
#[test]
fn contains_returns_false_and_true_with_single_result_conversion() {
    for (pattern, subject, expected) in [
        ("z", "abc", false),
        ("()", "abc", true),
        ("a|z", "abc", true),
    ] {
        let fixture = bespoke(
            &spec(
                &format!("{{str_contains: {{source: SRC.TEXT, pattern: '{pattern}'}}}}"),
                "str",
            ),
            format!("ID,I,F,D,TEXT\na,1,1,2025-01-01,{subject}\n").as_bytes(),
        );
        let result = fixture.run();
        let issue = &result.issues()[0];
        assert_eq!(
            (
                issue.phase.as_str(),
                issue.condition.as_str(),
                issue.requirement.as_deref()
            ),
            ("convert", "conversion_failed", Some("REQ-0013"))
        );
        assert_eq!(
            issue_json(issue)["context"]["value"],
            serde_json::json!(expected)
        );
    }
}
#[test]
fn row_scalar_derivations_share_the_same_raw_contract() {
    let source="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {I: int, F: float}}}\noutput: {path: output.csv, columns: [ID, RESULT, MAXIMUM]}\ncolumns:\n  - {name: ID, type: str}\n  - {name: RESULT, type: str}\n  - {name: MAXIMUM, type: int}\nrows:\n  - id: values\n    derivations:\n      ID: SRC.ID\n      RESULT: {str_concat: {sources: [{literal: '['}, {source: SRC.TEXT}, {literal: ']'}]}}\n      MAXIMUM: {greatest: {sources: [SRC.I, SRC.F]}}\n";
    let fixture = bespoke(source, b"ID,I,F,TEXT\na,3,2,abc\n");
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("output.csv")).unwrap(),
        b"ID,RESULT,MAXIMUM\na,[abc],3\n"
    );
}
