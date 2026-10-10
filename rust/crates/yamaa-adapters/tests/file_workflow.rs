#![cfg(any(unix, windows))]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{
    file_resources::Resources,
    file_workflow::{self, Error},
    producer_check,
    producer_graph::Limits,
    shipped_schema,
};
use yamaa_core::project_function::Language;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-workflow-check-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let study = Self(fs::canonicalize(path).unwrap());
        study.write("environment.yaml", "schema_version: '1.0'\n");
        study
    }
    fn path(&self, written: &str) -> String {
        let path = self.0.join(written).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path
            .strip_prefix(r"\\?\")
            .unwrap_or(&path)
            .replace('\\', "/");
        path
    }
    fn write(&self, written: &str, text: &str) {
        let path = self.0.join(written);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn prepare(&self, limits: Limits) -> Result<yamaa_adapters::file_graph::FileGraph, Error> {
        file_workflow::prepare_with_resources(
            Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
            "root.yaml",
            "environment.yaml",
            Language::Python,
            shipped_schema::capture().unwrap(),
            shipped_schema::capture_environment().unwrap(),
            limits,
        )
    }
    fn rejected(&self, limits: Limits) -> Error {
        match self.prepare(limits) {
            Err(error) => error,
            Ok(_) => panic!("expected rejection"),
        }
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn yaml(input: &str, base: &str, output: &str, invalid: bool) -> String {
    let checks = if invalid {
        "\n    verifications: [{matches: {pattern: \"\\u00e9(\"}}]"
    } else {
        ""
    };
    format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - name: VALUE\n    type: str\n    label: Reported value\n    derivation: {base}.VALUE{checks}\noutput: {{path: {output}, columns: [ID, VALUE]}}\n")
}

#[test]
fn complete_original_diamond_checks_each_canonical_node_once_without_data() {
    let study = Study::new();
    study.write(
        "leaf.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", true),
    );
    study.write(
        "left.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: './leaf.yaml'}}",
            "P",
            "left.csv",
            true,
        ),
    );
    study.write(
        "right.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: leaf.yaml}}",
            "P",
            "right.csv",
            false,
        ),
    );
    study.write("root.yaml", &yaml("{L: {path: left.csv, schema: left.yaml}, R: {path: right.csv, schema: right.yaml}, Q: {path: right.csv, schema: './right.yaml'}}", "L", "root.csv", true));
    let graph = study.prepare(Default::default()).unwrap();
    assert_eq!(graph.capture_reads(), 5); // one environment + four canonical nodes
    assert_eq!(graph.graph().checked().metadata().order(), [3, 1, 2, 0]);
    assert!(graph.graph().checked().execution_capability().is_err());
    let expected = ["root.yaml", "left.yaml", "leaf.yaml"].map(|name| yamaa_adapters::issue_rows::Issue {
        phase: "validation".into(), condition: "invalid_regex".into(), requirement: Some("REQ-0827".into()),
        spec_paths: vec!["columns.VALUE.verifications[0].matches.pattern".into()],
        context: serde_json::json!({"pattern":"é(", "source":study.path(name), "entry":study.path("root.yaml")}).to_string(),
    });
    assert_eq!(producer_check::prepared(&graph).unwrap(), expected);
    assert!(matches!(
        producer_check::prepared_with_limit(&graph, 4096),
        Err(producer_check::Error::Projection(_))
    ));
    assert_eq!(producer_check::prepared(&graph).unwrap(), expected);
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(producer_check::prepared(&graph).unwrap(), expected);
    assert_eq!(
        graph.environment().root().source().bytes,
        b"schema_version: '1.0'\n"
    );
}

#[test]
fn ordinary_check_rows_remain_identical_and_public_environment_paths_work() {
    let study = Study::new();
    study.write("yamaa-project.yaml", "version: '1.0'\ndata_roots: []\n");
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "never-written.csv", true),
    );
    let ordinary = yamaa_adapters::project_application::prepare(
        &study.path("root.yaml"),
        &study.path("environment.yaml"),
        Language::Python,
    )
    .unwrap();
    let expected = yamaa_adapters::project_check::prepared(ordinary.run()).unwrap();
    assert_eq!(expected.len(), 1);
    assert_eq!(
        yamaa_adapters::project_application::check(
            &study.path("root.yaml"),
            &study.path("environment.yaml"),
            Language::Python
        ),
        expected
    );
    assert!(!study.0.join("never-read.csv").exists());
    assert!(!study.0.join("never-written.csv").exists());
}

#[test]
fn producer_typed_rejections_keep_complete_authored_rows_and_original_owners() {
    for (input, requirement, condition, path) in [
        (
            "{P: {path: wrong.csv, schema: leaf.yaml}}",
            "REQ-0534",
            "producer_output_path_mismatch",
            "input.P.path",
        ),
        (
            "{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}",
            "REQ-0523",
            "redundant_field_type",
            "input.P.types.ID",
        ),
    ] {
        let study = Study::new();
        study.write(
            "leaf.yaml",
            &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
        );
        study.write("root.yaml", &yaml(input, "P", "root.csv", false));
        let error = study.rejected(Default::default());
        let rows = producer_check::rejected(&error, Language::Python).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].condition, condition);
        assert_eq!(rows[0].requirement.as_deref(), Some(requirement));
        assert_eq!(
            rows[0].spec_paths,
            if requirement == "REQ-0534" {
                vec![path, "input.P.schema"]
            } else {
                vec![path]
            }
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rows[0].context).unwrap()["source"],
            study.path("root.yaml")
        );
        assert!(matches!(
            producer_check::rejected_with_limit(&error, Language::Python, 1024),
            Err(producer_check::Error::Projection(_))
        ));
        fs::remove_dir_all(&study.0).unwrap();
        assert_eq!(
            producer_check::rejected(&error, Language::Python).unwrap(),
            rows
        );
        let Error::Graph(graph) = &error else {
            panic!("retained graph");
        };
        assert_eq!(graph.capture_reads(), 3);
        assert_eq!(graph.documents().len(), 2);
        assert_eq!(graph.captured_sources().count(), 3);
    }
}

#[test]
fn actual_compilation_failure_and_cycle_are_projected_without_recompilation() {
    for cycle in [false, true] {
        let study = Study::new();
        study.write(
            "root.yaml",
            &yaml(
                "{P: {path: leaf.csv, schema: leaf.yaml}}",
                "P",
                "root.csv",
                false,
            ),
        );
        let leaf = if cycle {
            yaml(
                "{R: {path: root.csv, schema: './root.yaml'}}",
                "R",
                "leaf.csv",
                false,
            )
        } else {
            yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                .replace("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}")
        };
        study.write("leaf.yaml", &leaf);
        let error = study.rejected(Default::default());
        let rows = producer_check::rejected(&error, Language::Python).unwrap();
        assert!(!rows.is_empty());
        assert_eq!(
            rows[0].condition,
            if cycle {
                "producer_workflow_cycle"
            } else {
                "unknown_project_function"
            }
        );
        assert_eq!(
            rows[0].requirement.as_deref(),
            Some(if cycle { "REQ-0534" } else { "REQ-0698" })
        );
        let Error::Graph(graph) = &error else {
            panic!("graph");
        };
        assert_eq!(graph.capture_reads(), 3);
        assert_eq!(graph.documents().len(), 2);
        fs::remove_dir_all(&study.0).unwrap();
        assert_eq!(
            producer_check::rejected(&error, Language::Python).unwrap(),
            rows
        );
    }
}

#[test]
fn environment_budget_and_raw_rejections_precede_the_first_graph_read() {
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
    );
    for (bytes, reads, captures) in [(0, 0, 0), (22, 1, 1), (44, 1, 1)] {
        let error = study.rejected(Limits {
            captured_bytes: bytes,
            ..Default::default()
        });
        assert!(
            matches!(producer_check::rejected(&error, Language::Python), Err(producer_check::Error::Original(original)) if std::ptr::eq(original, &error))
        );
        match &error {
            Error::Environment(environment) => {
                assert_eq!(environment.capture_reads(), reads);
                assert_eq!(environment.captured_sources().count(), captures);
            }
            Error::Graph(graph) => {
                assert_eq!(graph.capture_reads(), reads);
                assert_eq!(graph.captured_sources().count(), captures);
            }
            _ => panic!("owned rejection"),
        }
    }
    let raw = "schema_version: '1.0'\nfunctions: [\n";
    study.write("environment.yaml", raw);
    let error = study.rejected(Default::default());
    let Error::Environment(environment) = &error else {
        panic!("early environment");
    };
    assert_eq!(environment.capture_reads(), 1);
    assert_eq!(
        environment.captured_sources().next().unwrap().1,
        raw.as_bytes()
    );
    assert!(matches!(
        producer_check::rejected(&error, Language::Python),
        Err(producer_check::Error::Original(_))
    ));
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(
        environment.captured_sources().next().unwrap().1,
        raw.as_bytes()
    );
}

#[test]
fn supported_environment_and_original_inherited_schema_causes_are_complete() {
    let study = Study::new();
    study.write("environment.yaml", "schema_version: '1.0'\nlanguage: r\n");
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
    );
    let error = study.rejected(Default::default());
    let rows = producer_check::rejected(&error, Language::Python).unwrap();
    assert!(rows
        .iter()
        .any(|r| r.requirement.as_deref() == Some("REQ-0696")));
    let Error::Environment(environment) = &error else {
        panic!("environment");
    };
    assert_eq!(environment.capture_reads(), 1);
    assert!(matches!(
        producer_check::rejected_with_limit(&error, Language::Python, 16),
        Err(producer_check::Error::Projection(_))
    ));
    study.write("environment.yaml", "schema_version: '1.0'\n");
    study.write(
        "parent/base.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false)
            .replace("type: str", "type: invalid"),
    );
    study.write(
        "root.yaml",
        "schema_version: '1.0'\nparents: [parent/base.yaml]\ndomain: CHILD\n",
    );
    let error = study.rejected(Default::default());
    let rows = producer_check::rejected(&error, Language::Python).unwrap();
    assert!(!rows.is_empty());
    assert!(rows.iter().any(
        |r| serde_json::from_str::<serde_json::Value>(&r.context).unwrap()["source"]
            == study.path("parent/base.yaml")
    ));
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(
        producer_check::rejected(&error, Language::Python).unwrap(),
        rows
    );
}

#[test]
fn inherited_graph_findings_retain_the_actual_declaring_layer_after_owner_files_drop() {
    let study = Study::new();
    study.write(
        "producer/base.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "../leaf.csv", true),
    );
    study.write(
        "leaf.yaml",
        "schema_version: '1.0'\nparents: [producer/base.yaml]\ndomain: LEAF\n",
    );
    let parent = yaml("{P: {path: ../leaf.csv, schema: ../leaf.yaml}, Q: {path: ../leaf.csv, schema: '../leaf.yaml'}}", "P", "../root.csv", false)
        .replace("columns:\n", "intermediates: [{id: Q_REFERENCE, dataset: Q}]\ncolumns:\n")
        .replace("output:", "  - {name: OTHER, type: str, label: Other, derivation: Q_REFERENCE.VALUE}\noutput:")
        .replace("columns: [ID, VALUE]", "columns: [ID, VALUE, OTHER]");
    study.write("parents/base.yaml", &parent);
    study.write(
        "root.yaml",
        "schema_version: '1.0'\nparents: [parents/base.yaml]\ndomain: ROOT\n",
    );
    let graph = study.prepare(Default::default()).unwrap();
    assert_eq!(graph.capture_reads(), 5);
    let nodes = graph.graph().nodes();
    assert!(std::sync::Arc::ptr_eq(
        &nodes[0].producers[0].document,
        &nodes[0].producers[1].document
    ));
    let expected = serde_json::json!({"declaring_sources":[study.path("producer/base.yaml")],
        "source":study.path("leaf.yaml"), "entry":study.path("root.yaml"), "pattern":"é("})
    .to_string();
    let rows = producer_check::prepared(&graph).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].context, expected);
    assert_eq!(
        graph.graph().checked().metadata().nodes()[0].producers()[0]
            .schema_origin()
            .declaring_source(),
        study.path("parents/base.yaml")
    );
    let retained = graph.graph().nodes()[1].document.clone();
    drop(graph);
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(
        retained.declaring_source("columns.VALUE.verifications[0].matches.pattern"),
        Some(study.path("producer/base.yaml").as_str())
    );
}

#[test]
fn unsupported_native_capture_keeps_exact_refusal_and_declaring_request() {
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml(
            "{P: {path: absent.csv, schema: missing.yaml}}",
            "P",
            "root.csv",
            false,
        ),
    );
    let error = study.rejected(Default::default());
    let Error::Graph(graph) = &error else {
        panic!("graph");
    };
    assert!(matches!(
        graph.error(),
        yamaa_adapters::file_graph::Error::Resource(yamaa_adapters::file_resources::Error::Missing)
    ));
    assert_eq!(graph.capture_reads(), 2);
    assert_eq!(graph.origin().unwrap().dataset, "P");
    assert!(
        matches!(producer_check::rejected(&error, Language::Python), Err(producer_check::Error::Original(original)) if std::ptr::eq(original, &error))
    );
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(graph.documents().len(), 1);
    assert_eq!(graph.captured_sources().count(), 2);
}

#[test]
fn invalid_producer_contract_and_environment_child_failures_keep_original_metadata() {
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: leaf.yaml}}",
            "P",
            "root.csv",
            false,
        ),
    );
    let original = yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
        .replace("    label: Reported value\n", "");
    study.write("leaf.yaml", &original);
    let error = study.rejected(Default::default());
    let rows = producer_check::rejected(&error, Language::Python).unwrap();
    assert!(rows
        .iter()
        .any(|row| row.requirement.as_deref() == Some("REQ-0534")
            && row.condition == "invalid_producer_contract"));
    assert!(matches!(
        producer_check::rejected_with_limit(&error, Language::Python, 64),
        Err(producer_check::Error::Projection(_))
    ));
    study.write(
        "environment.yaml",
        "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {id: broken.yaml}\n",
    );
    study.write("uv.lock", "version = 1\npackage = []\n");
    let malformed = "function: [\n";
    study.write("broken.yaml", malformed);
    let failure = study.rejected(Default::default());
    let Error::Environment(environment) = &failure else {
        panic!("original environment");
    };
    assert_eq!(environment.capture_reads(), 3);
    assert_eq!(
        environment
            .captured_sources()
            .find(|(path, _)| path == &study.path("broken.yaml"))
            .unwrap()
            .1,
        malformed.as_bytes()
    );
    assert!(
        matches!(producer_check::rejected(&failure, Language::Python), Err(producer_check::Error::Original(original)) if std::ptr::eq(original, &failure))
    );
    fs::remove_dir_all(&study.0).unwrap();
    let Error::Graph(graph) = &error else {
        panic!("original contract");
    };
    assert_eq!(graph.documents()[1].source().bytes, original.as_bytes());
    assert_eq!(
        producer_check::rejected(&error, Language::Python).unwrap(),
        rows
    );
    assert_eq!(
        environment
            .captured_sources()
            .find(|(path, _)| path == &study.path("broken.yaml"))
            .unwrap()
            .1,
        malformed.as_bytes()
    );
}

#[cfg(unix)]
#[test]
fn shared_native_workflow_keeps_literal_backslash_in_canonical_roots() {
    let outer = Study::new();
    let root = outer.0.join("named\\root");
    fs::create_dir(&root).unwrap();
    let study = Study(root);
    study.write("environment.yaml", "schema_version: '1.0'\n");
    study.write(
        "leaf.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
    );
    study.write(
        "root.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: './leaf.yaml'}}",
            "P",
            "root.csv",
            false,
        ),
    );
    let graph = study.prepare(Default::default()).unwrap();
    assert!(producer_check::prepared(&graph).unwrap().is_empty());
    assert_eq!(
        graph.graph().nodes()[0].producers[0].schema_identity,
        study.path("leaf.yaml")
    );
    assert_eq!(graph.capture_reads(), 3);
}
