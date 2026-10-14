//! `check` reports the output-declaration findings that `domain` reaches at
//! its output stage (#2295): the same first issue, in both host routes.
//! Committed negative benchmarks and `expected/error.yaml` remain the oracle.
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
        let p = std::env::temp_dir().join(format!(
            "yamaa-check-output-{}-{}-{}",
            name,
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn copy(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for item in fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_dir() {
            copy(&item.path(), &destination.join(item.file_name()));
        } else {
            fs::copy(item.path(), destination.join(item.file_name())).unwrap();
        }
    }
}

fn issue(issue: &yamaa_adapters::issue_rows::Issue) -> serde_json::Value {
    serde_json::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()})
}

fn expected(path: &Path) -> serde_json::Value {
    let text = fs::read_to_string(path).unwrap();
    let d = yamaa_adapters::yaml_decode::decode_yaml(text.as_bytes(), Default::default())
        .unwrap()
        .document;
    fn value(d: &yamaa_core::schema::Document, id: usize) -> serde_json::Value {
        use yamaa_core::schema::DocumentNode as N;
        match &d.nodes()[id] {
            N::Null => serde_json::Value::Null,
            N::Boolean(v) => (*v).into(),
            N::Integer(v) => serde_json::from_str(v).unwrap(),
            N::Float(v) => serde_json::json!(v),
            N::Text(v) => v.clone().into(),
            N::Sequence(ids) => ids.iter().map(|&id| value(d, id)).collect(),
            N::Mapping(pairs) => pairs
                .iter()
                .map(|&(key, item)| (value(d, key).as_str().unwrap().to_owned(), value(d, item)))
                .collect(),
        }
    }
    value(&d, d.root())
}

fn identity<'a>(runtime: &'a str, example: &'a str, specification: &'a str) -> Identity<'a> {
    Identity {
        runtime,
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example,
        specification,
        base_directory: ".",
    }
}

/// Run `check` and `domain` over one staged specification and return the
/// first issue each route reports.
fn first_issues(
    specification: &str,
    example: &str,
    runtime: &str,
) -> (serde_json::Value, serde_json::Value) {
    let request = || Request {
        specification,
        environment: None,
    };
    let checked = file_application::check(request(), identity(runtime, example, specification));
    assert!(!checked.is_empty(), "{example}: check reported no issues");
    let built = file_application::domain(request(), identity(runtime, example, specification));
    let built = match built {
        Ok(result) => {
            let issues = result.issues().to_vec();
            assert!(!issues.is_empty(), "{example}: domain reported no issues");
            issues
        }
        Err(failure) => {
            assert!(!failure.is_empty(), "{example}: domain reported no issues");
            failure
        }
    };
    (issue(&checked[0]), issue(&built[0]))
}

fn benchmarks_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks")
}

/// Every committed negative benchmark whose first issue comes from `output.*`
/// is an output-declaration validation case. The denominator is pinned so a
/// benchmark that silently stops being selected fails this test.
fn output_validation_benchmarks() -> Vec<String> {
    let root = benchmarks_root();
    let mut selected = Vec::new();
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("negative-") {
            continue;
        }
        let oracle = entry.path().join("expected/error.yaml");
        if !oracle.exists() {
            continue;
        }
        let expected = expected(&oracle);
        let from_output = expected["spec_paths"]
            .as_array()
            .and_then(|paths| paths.first())
            .and_then(|path| path.as_str())
            .is_some_and(|path| path.starts_with("output."));
        if from_output {
            selected.push(name);
        }
    }
    selected.sort();
    selected
}

#[test]
fn check_matches_domain_on_output_validation_benchmarks() {
    let selected = output_validation_benchmarks();
    assert_eq!(
        selected,
        [
            "negative-order-repeated",
            "negative-order-unknown-column",
            "negative-output-transport",
        ],
        "output-validation benchmark denominator changed"
    );
    let root = benchmarks_root();
    for name in &selected {
        let original = root.join(name);
        let fixture = Fixture::new(name);
        copy(&original.join("input"), &fixture.0.join("input"));
        fs::copy(original.join("spec.yaml"), fixture.0.join("spec.yaml")).unwrap();
        let spec = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        let oracle = expected(&original.join("expected/error.yaml"));
        for runtime in ["python", "r"] {
            let (checked, built) = first_issues(&spec, name, runtime);
            assert_eq!(
                checked, built,
                "{name} ({runtime}): check/domain first issue"
            );
            assert_eq!(
                checked, oracle,
                "{name} ({runtime}): first issue vs expected/error.yaml"
            );
        }
    }
}

/// The three rows of #2295's table, with the issue's own minimal study: an
/// undeclared output column, `decimals` on a parquet output, and a negative
/// `decimals`. None is covered by a committed negative benchmark.
#[test]
fn check_matches_domain_on_the_issue_rows() {
    let rows: [(&str, &str, serde_json::Value); 3] = [
        (
            "undeclared",
            "{path: out.csv, columns: [USUBJID, N, ZZZ]}",
            serde_json::json!({"phase":"validation","condition":"undeclared_column","requirement":"REQ-0234","spec_paths":["output.columns[2]"],"context":{"column":"ZZZ"}}),
        ),
        (
            "decimals-parquet",
            "{path: out.parquet, columns: [USUBJID, N], decimals: 2}",
            serde_json::json!({"phase":"validation","condition":"decimals_not_applicable","requirement":"REQ-0762","spec_paths":["output.decimals"],"context":{"path":"out.parquet","profile":"parquet"}}),
        ),
        (
            "decimals-negative",
            "{path: out.csv, columns: [USUBJID, N], decimals: -1}",
            serde_json::json!({"phase":"validation","condition":"invalid_field_type","requirement":"REQ-0744","spec_paths":["output.decimals"],"context":{"expected":"a non-negative integer","actual":-1}}),
        ),
    ];
    for (name, output, oracle) in rows {
        let fixture = Fixture::new(name);
        fs::create_dir(fixture.0.join("input")).unwrap();
        fs::write(fixture.0.join("input/dm.csv"), "USUBJID,AGE\nS-001,34\n").unwrap();
        let spec = format!(
            "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput: {{DM: input/dm.csv}}\noutput: {output}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n  - {{name: N, type: float, derivation: DM.AGE}}\n"
        );
        fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
        let spec_path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        for runtime in ["python", "r"] {
            let (checked, built) = first_issues(&spec_path, name, runtime);
            assert_eq!(
                checked, built,
                "{name} ({runtime}): check/domain first issue"
            );
            assert_eq!(checked, oracle, "{name} ({runtime}): first issue");
        }
    }
}
