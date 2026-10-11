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
fn original_decode_diagnostics_remain_bounded_and_owned() {
    use yamaa_adapters::issue_rows::Issue;
    let study = Study::new();
    study.write("root.yaml", "\u{e9}\n");
    let owner = study.rejected(Limits::default());
    let expected = vec![Issue {
        phase: "validation".into(),
        condition: "non_ascii_source".into(),
        requirement: None,
        spec_paths: vec!["$".into()],
        context: serde_json::json!({"line":1, "column":1, "path":study.path("root.yaml")})
            .to_string(),
    }];
    assert_eq!(
        producer_check::rejected(&owner, Language::Python).unwrap(),
        expected
    );
    assert!(matches!(
        producer_check::rejected_with_limit(&owner, Language::Python, 32),
        Err(producer_check::Error::Projection(_))
    ));
    assert_eq!(
        yamaa_adapters::project_application::check(
            &study.path("root.yaml"),
            &study.path("environment.yaml"),
            Language::Python
        ),
        expected
    );
    study.write("root.yaml", "[");
    let malformed = yamaa_adapters::project_application::check(
        &study.path("root.yaml"),
        &study.path("environment.yaml"),
        Language::Python,
    );
    assert_eq!(malformed.len(), 1);
    assert_eq!(
        (
            &*malformed[0].phase,
            &*malformed[0].condition,
            &malformed[0].spec_paths
        ),
        ("validation", "invalid_yaml", &vec!["$".to_owned()])
    );
    study.write("environment.yaml", "[");
    let environment = yamaa_adapters::project_application::check(
        &study.path("root.yaml"),
        &study.path("environment.yaml"),
        Language::Python,
    );
    assert_eq!(
        environment,
        yamaa_adapters::file_application::rejected("environment", "environment_boundary")
    );
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(
        producer_check::rejected(&owner, Language::Python).unwrap(),
        expected
    );
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

#[test]
fn ordinary_original_input_facts_survive_files_without_study_authority() {
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "never-written.csv", false),
    );
    let graph = study.prepare(Default::default()).unwrap();
    assert_eq!(graph.capture_reads(), 2);
    assert_eq!(graph.graph().nodes().len(), 1);
    assert!(graph.graph().nodes()[0].producers.is_empty());
    let document = &graph.graph().nodes()[0].document;
    let raw = document.normalization_input();
    let input = raw.field(raw.root(), "input").unwrap();
    let declaration = raw.field(input, "RAW").unwrap();
    assert!(!yamaa_core::producer_admission::is_producer_input(
        raw,
        declaration
    ));
    let original = fs::read(study.0.join("root.yaml")).unwrap();
    drop(study);
    assert_eq!(document.source().bytes, original);
    assert!(producer_check::prepared(&graph).unwrap().is_empty());
}

#[test]
fn inherited_original_producer_facts_use_the_contributing_input_layer() {
    let study = Study::new();
    study.write(
        "leaf.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
    );
    study.write(
        "base.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: leaf.yaml}}",
            "P",
            "consumer.csv",
            false,
        ),
    );
    study.write(
        "root.yaml",
        "schema_version: '1.0'\nparents: [base.yaml]\ndomain: TEST\n",
    );
    let graph = study.prepare(Default::default()).unwrap();
    assert_eq!(graph.capture_reads(), 4);
    assert_eq!(graph.graph().nodes().len(), 2);
    assert_eq!(graph.graph().nodes()[0].producers.len(), 1);
    let document = &graph.graph().nodes()[0].document;
    assert!(document
        .raw()
        .document
        .field(document.raw().document.root(), "input")
        .is_none());
    let raw = document.normalization_input();
    let input = raw.field(raw.root(), "input").unwrap();
    let declaration = raw.field(input, "P").unwrap();
    assert!(yamaa_core::producer_admission::is_producer_input(
        raw,
        declaration
    ));
    assert_eq!(
        document
            .written_input_origin("P", "schema")
            .unwrap()
            .declaring_source,
        study.path("base.yaml")
    );
    drop(study);
    assert!(producer_check::prepared(&graph).unwrap().is_empty());
}

#[test]
fn rejected_null_schema_keeps_the_original_shipped_schema_cause_and_raw_entry() {
    let study = Study::new();
    let original = yaml(
        "{RAW: {path: never-read.csv, schema: null}}",
        "RAW",
        "never-written.csv",
        false,
    );
    study.write("root.yaml", &original);
    let rejection = study.rejected(Default::default());
    let Error::Graph(graph) = &rejection else {
        panic!("the original root rejection must remain owned");
    };
    assert_eq!(graph.capture_reads(), 2);
    assert!(graph.documents().is_empty());
    drop(study);
    let retained = graph.retained_sources(16, 65536).unwrap();
    assert_eq!(retained.len(), 2);
    assert!(
        retained
            .iter()
            .any(|(name, bytes)| name.ends_with("/root.yaml")
                && bytes.as_ref() == original.as_bytes())
    );
    let findings = producer_check::rejected(&rejection, Language::Python).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].condition, "invalid_field_type");
    assert_eq!(findings[0].requirement.as_deref(), Some("REQ-0287"));
    assert_eq!(findings[0].spec_paths, ["input.RAW.schema"]);
    assert!(producer_check::rejected_with_limit(&rejection, Language::Python, 1).is_err());
    assert_eq!(graph.capture_reads(), 2);
}

impl Study {
    fn select(
        &self,
        limits: Limits,
    ) -> Result<file_workflow::Selected, file_workflow::SelectionError> {
        file_workflow::select_with_resources(
            Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
            "root.yaml",
            "environment.yaml",
            Language::Python,
            shipped_schema::capture().unwrap(),
            shipped_schema::capture_environment().unwrap(),
            limits,
        )
    }
    fn selection_rejection(&self, limits: Limits) -> file_workflow::SelectionError {
        match self.select(limits) {
            Err(error) => error,
            Ok(_) => panic!("expected selected preparation rejection"),
        }
    }
}
struct NoActivation;
impl yamaa_engine::project_activation::ActivationPort for NoActivation {
    type Handle = ();
    type Error = ();
    fn verify_lock(
        &mut self,
        _: Language,
        _: &yamaa_core::project_environment::LockReference,
        _: &[yamaa_core::function_signature::ProjectFunctionIdentity],
    ) -> Result<(), ()> {
        panic!("functionless workflow must not activate")
    }
    fn bind(
        &mut self,
        _: &yamaa_core::function_signature::ProjectFunctionIdentity,
        _: &yamaa_core::function_signature::LogicalSignature,
    ) -> Result<(), ()> {
        panic!("functionless workflow must not bind")
    }
    fn invoke(
        &mut self,
        _: &(),
        _: &[yamaa_engine::function_invocation::Argument<'_>],
    ) -> Result<yamaa_core::value::Value, yamaa_engine::function_invocation::HostError<()>> {
        panic!("functionless workflow must not call")
    }
}
fn ordinary_report_id() -> yamaa_adapters::specification_report::Identity<'static> {
    yamaa_adapters::specification_report::Identity {
        runtime: "test",
        runtime_version: "1",
        engine_version: "0.1.0",
        example: "selected-workflow",
        specification: "root.yaml",
        base_directory: ".",
    }
}

#[test]
fn selected_ordinary_keeps_complete_report_and_explicit_held_byte_save() {
    use file_workflow::Selected;
    use yamaa_engine::specification_output::ArtifactPort;
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
    );
    let Selected::Ordinary(mut selected) = study
        .select(Limits {
            captured_bytes: 2
                * (b"schema_version: '1.0'\n".len()
                    + yaml("{RAW: never-read.csv}", "RAW", "root.csv", false).len()),
            snapshots: 2,
            ..Default::default()
        })
        .unwrap()
    else {
        panic!("ordinary capability");
    };
    assert_eq!(selected.capture_reads(), 2);
    assert!(!study.0.join("never-read.csv").exists());
    assert!(!study.0.join("root.csv").exists());
    // Metadata scope ends only after complete preparation. An independent study
    // source is first created and read afterwards under ordinary build admission.
    study.write("never-read.csv", "ID,VALUE\n1,alpha\n2,beta\n");
    let mut ordinary = yamaa_adapters::file_project::FileProject::prepare(
        Resources::new(&study.path(""), &study.path(""), &[]).unwrap(),
        "root.yaml",
        "environment.yaml",
        Language::Python,
        shipped_schema::capture().unwrap(),
        shipped_schema::capture_environment().unwrap(),
    )
    .unwrap();
    let selected_attempt = selected.build(&mut NoActivation);
    let ordinary_attempt = ordinary.build(&mut NoActivation);
    assert_eq!(selected.capture_reads(), 3);
    let selected_result = yamaa_adapters::project_report::build_result(
        selected.run(),
        &selected_attempt,
        ordinary_report_id(),
        &[],
    )
    .unwrap();
    let ordinary_result = yamaa_adapters::project_report::build_result(
        ordinary.run(),
        &ordinary_attempt,
        ordinary_report_id(),
        &[],
    )
    .unwrap();
    assert_eq!(selected_result.output(), ordinary_result.output());
    assert_eq!(
        selected_attempt
            .dataset
            .result
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .dataset
            .rows(),
        &[
            vec![
                yamaa_core::value::Value::Int(1),
                yamaa_core::value::Value::Str("alpha".into())
            ],
            vec![
                yamaa_core::value::Value::Int(2),
                yamaa_core::value::Value::Str("beta".into())
            ]
        ]
    );
    assert_eq!(
        selected_result.observations(),
        ordinary_result.observations()
    );
    assert_eq!(selected_result.issues(), ordinary_result.issues());
    assert!(!study.0.join("root.csv").exists());
    struct Save(Vec<(String, Vec<u8>)>);
    impl ArtifactPort for Save {
        type Error = ();
        fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), ()> {
            self.0.push((path.into(), bytes.into()));
            Ok(())
        }
    }
    let retained = selected.retained_run();
    drop(selected);
    drop(ordinary);
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(
        retained.document().source().bytes,
        yaml("{RAW: never-read.csv}", "RAW", "root.csv", false).as_bytes()
    );
    let mut publisher = Save(vec![]);
    assert_eq!(
        selected_result.save(&mut publisher).unwrap()["artifacts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        publisher.0,
        [("root.csv".into(), b"ID,VALUE\n1,alpha\n2,beta\n".to_vec())]
    );
}

#[test]
fn selected_inherited_producer_seeds_original_closure_with_shared_aliases_and_diamond() {
    use file_workflow::Selected;
    let study = Study::new();
    study.write(
        "leaf.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
    );
    study.write(
        "left.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: leaf.yaml}}",
            "P",
            "left.csv",
            false,
        ),
    );
    study.write(
        "right.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: './leaf.yaml'}}",
            "P",
            "right.csv",
            false,
        ),
    );
    let parent = yaml("{L: {path: ../left.csv, schema: ../left.yaml}, R: {path: ../right.csv, schema: ../right.yaml}, Q: {path: ../right.csv, schema: '../right.yaml'}}", "L", "../root.csv", true)
        .replace("columns:\n", "intermediates: [{id: RIGHT, dataset: R}, {id: ALIAS, dataset: Q}]\ncolumns:\n")
        .replace("output:", "  - {name: RIGHT_VALUE, type: str, label: Right value, derivation: RIGHT.VALUE}\n  - {name: ALIAS_VALUE, type: str, label: Alias value, derivation: ALIAS.VALUE}\noutput:")
        .replace("columns: [ID, VALUE]", "columns: [ID, VALUE, RIGHT_VALUE, ALIAS_VALUE]");
    study.write("parents/base.yaml", &parent);
    let entry = "schema_version: '1.0'\nparents: [parents/base.yaml]\ndomain: ROOT\n";
    study.write("root.yaml", entry);
    let Selected::Producer(graph) = study.select(Default::default()).unwrap() else {
        panic!("inherited producer capability");
    };
    assert_eq!(graph.capture_reads(), 6); // environment, root, parent, left/right/leaf
    let nodes = graph.graph().nodes();
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0].document.source().bytes, entry.as_bytes());
    assert!(nodes[0]
        .document
        .raw()
        .document
        .field(nodes[0].document.raw().document.root(), "input")
        .is_none());
    assert!(std::sync::Arc::ptr_eq(
        &nodes[0].producers[1].document,
        &nodes[0].producers[2].document
    ));
    assert!(std::sync::Arc::ptr_eq(
        &nodes[1].producers[0].document,
        &nodes[2].producers[0].document
    ));
    assert_eq!(graph.graph().checked().metadata().order(), [3, 1, 2, 0]);
    assert!(graph.graph().checked().execution_capability().is_err());
    let rows = producer_check::prepared(&graph).unwrap();
    assert_eq!(rows.len(), 1);
    let context: serde_json::Value = serde_json::from_str(&rows[0].context).unwrap();
    assert_eq!(
        context["declaring_sources"],
        serde_json::json!([study.path("parents/base.yaml")])
    );
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(producer_check::prepared(&graph).unwrap(), rows);
    assert_eq!(graph.capture_reads(), 6);
}

#[test]
fn selected_ordinary_compile_failure_keeps_original_run_environment_and_raw_resources() {
    use file_workflow::SelectionError;
    let study = Study::new();
    let raw = yaml("{RAW: never-read.csv}", "RAW", "root.csv", false)
        .replace("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}");
    study.write("root.yaml", &raw);
    let SelectionError::Ordinary(rejected) = study.selection_rejection(Default::default()) else {
        panic!("original ordinary compiler rejection");
    };
    assert_eq!(rejected.capture_reads(), 2);
    assert_eq!(rejected.entry(), "root.yaml");
    let rows = yamaa_adapters::project_check::rejected(rejected.run()).unwrap();
    assert_eq!(rows[0].condition, "unknown_project_function");
    assert_eq!(rows[0].requirement.as_deref(), Some("REQ-0698"));
    assert!(rejected.retained_sources(1, 65536).is_err());
    fs::remove_dir_all(&study.0).unwrap();
    let held = rejected.retained_sources(16, 65536).unwrap();
    assert_eq!(held.len(), 2);
    assert!(held.iter().any(
        |(identity, bytes)| identity.ends_with("root.yaml") && bytes.as_ref() == raw.as_bytes()
    ));
    assert_eq!(
        yamaa_adapters::project_check::rejected(rejected.run()).unwrap(),
        rows
    );
    assert_eq!(
        rejected.run().captured.root().source().bytes,
        b"schema_version: '1.0'\n"
    );
}

#[test]
fn selected_invalid_original_declarations_never_fall_back_to_ordinary_compilation() {
    use file_workflow::SelectionError;
    for (input, expected) in [
        ("{RAW: {path: never-read.csv, schema: null}}", "REQ-0287"),
        ("{P: {path: wrong.csv, schema: leaf.yaml}}", "REQ-0534"),
        (
            "{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}",
            "REQ-0523",
        ),
    ] {
        let study = Study::new();
        study.write("root.yaml", &yaml(input, "RAW", "root.csv", false));
        study.write(
            "leaf.yaml",
            &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
        );
        let SelectionError::Preparation(error @ Error::Graph(_)) =
            study.selection_rejection(Default::default())
        else {
            panic!("owned original metadata/graph rejection");
        };
        let rows = producer_check::rejected(&error, Language::Python).unwrap();
        assert!(
            rows.iter()
                .any(|r| r.requirement.as_deref() == Some(expected)),
            "expected {expected}; actual {rows:?}"
        );
        let Error::Graph(owner) = &error else {
            unreachable!()
        };
        let reads = owner.capture_reads();
        assert_eq!(reads, if expected == "REQ-0287" { 2 } else { 3 });
        fs::remove_dir_all(&study.0).unwrap();
        assert_eq!(
            producer_check::rejected(&error, Language::Python).unwrap(),
            rows
        );
        assert_eq!(owner.capture_reads(), reads);
    }
}

#[test]
fn selected_early_environment_root_and_metadata_quota_failures_keep_exact_prefixes() {
    use file_workflow::SelectionError;
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
    );
    for (limits, reads) in [
        (
            Limits {
                captured_bytes: 0,
                ..Default::default()
            },
            0,
        ),
        (
            Limits {
                captured_bytes: 44,
                ..Default::default()
            },
            1,
        ),
        (
            Limits {
                snapshots: 1,
                ..Default::default()
            },
            1,
        ),
    ] {
        let SelectionError::Preparation(error) = study.selection_rejection(limits) else {
            panic!("preparation quota");
        };
        match &error {
            Error::Environment(owner) => {
                assert_eq!(owner.capture_reads(), reads);
                assert!(owner.retained_sources(16, 65536).unwrap().len() <= 1);
            }
            Error::Graph(owner) => {
                assert_eq!(owner.capture_reads(), reads);
                assert_eq!(owner.retained_sources(16, 65536).unwrap().len(), 1);
            }
            _ => panic!("owned bounded prefix"),
        }
    }
    study.write("environment.yaml", "schema_version: '1.0'\nfunctions: [\n");
    let SelectionError::Preparation(Error::Environment(environment)) =
        study.selection_rejection(Default::default())
    else {
        panic!("raw environment rejection");
    };
    assert_eq!(environment.capture_reads(), 1);
    study.write("environment.yaml", "schema_version: '1.0'\n");
    study.write("root.yaml", "[");
    let SelectionError::Preparation(root @ Error::Graph(_)) =
        study.selection_rejection(Default::default())
    else {
        panic!("raw root rejection");
    };
    let rows = producer_check::rejected(&root, Language::Python).unwrap();
    assert_eq!(rows[0].condition, "invalid_yaml");
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(environment.retained_sources(16, 65536).unwrap().len(), 1);
    assert_eq!(
        producer_check::rejected(&root, Language::Python).unwrap(),
        rows
    );
}

#[test]
fn selected_producer_cycle_missing_labels_and_compiler_failures_keep_typed_whole_owners() {
    use file_workflow::SelectionError;
    for mode in 0..3 {
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
        let leaf = match mode {
            0 => yaml(
                "{P: {path: root.csv, schema: root.yaml}}",
                "P",
                "leaf.csv",
                false,
            ),
            1 => yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                .replace("label: Identifier, ", "")
                .replace("    label: Reported value\n", ""),
            _ => yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                .replace("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}"),
        };
        study.write("leaf.yaml", &leaf);
        let SelectionError::Preparation(error @ Error::Graph(_)) =
            study.selection_rejection(Default::default())
        else {
            panic!("whole producer rejection");
        };
        let rows = producer_check::rejected(&error, Language::Python).unwrap();
        let requirement = ["REQ-0534", "REQ-0534", "REQ-0698"][mode];
        assert!(
            rows.iter()
                .any(|r| r.requirement.as_deref() == Some(requirement)),
            "{rows:?}"
        );
        let Error::Graph(owner) = &error else {
            unreachable!()
        };
        assert_eq!(owner.capture_reads(), 3);
        assert_eq!(owner.documents().len(), 2);
        fs::remove_dir_all(&study.0).unwrap();
        assert_eq!(
            producer_check::rejected(&error, Language::Python).unwrap(),
            rows
        );
    }
}

#[test]
fn selected_recursive_graph_quotas_continue_environment_and_entry_scope_before_growth() {
    use file_workflow::SelectionError;
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
    study.write(
        "leaf.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
    );
    for limits in [
        Limits {
            snapshots: 2,
            ..Default::default()
        },
        Limits {
            graph: yamaa_core::producer_graph::Limits {
                edges: 0,
                ..Default::default()
            },
            ..Default::default()
        },
        Limits {
            graph: yamaa_core::producer_graph::Limits {
                nodes: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    ] {
        let SelectionError::Preparation(Error::Graph(owner)) = study.selection_rejection(limits)
        else {
            panic!("cumulative producer quota");
        };
        assert_eq!(owner.capture_reads(), 2);
        assert_eq!(owner.documents().len(), 1);
        assert!(matches!(
            owner.error(),
            yamaa_adapters::file_graph::Error::Resource(
                yamaa_adapters::file_resources::Error::Limit
            ) | yamaa_adapters::file_graph::Error::Document(
                yamaa_adapters::file_preparation::Error::Resource(
                    yamaa_adapters::file_resources::Error::Limit
                )
            ) | yamaa_adapters::file_graph::Error::Limit(_)
        ));
        assert_eq!(owner.retained_sources(16, 65536).unwrap().len(), 2);
        assert_eq!(owner.origin().unwrap().dataset, "P");
    }
    assert!(!study.0.join("never-read.csv").exists());
    assert!(!study.0.join("leaf.csv").exists());
    assert!(!study.0.join("root.csv").exists());
}

#[test]
fn selected_entry_paths_keep_absolute_environment_policy_and_early_owned_refusal() {
    use file_workflow::{Selected, SelectionError};
    let study = Study::new();
    study.write("yamaa-project.yaml", "version: '1.0'\ndata_roots: []\n");
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
    );
    let Selected::Ordinary(project) = file_workflow::select(
        &study.path("root.yaml"),
        &study.path("environment.yaml"),
        Language::Python,
    )
    .unwrap() else {
        panic!("configured ordinary capability");
    };
    assert_eq!(project.capture_reads(), 2); // environment + root; no configuration
    let Err(SelectionError::Preparation(Error::Environment(owner))) =
        file_workflow::select(&study.path("root.yaml"), "", Language::Python)
    else {
        panic!("original lexical environment refusal");
    };
    assert_eq!(owner.capture_reads(), 0);
    assert!(matches!(
        owner.error(),
        yamaa_adapters::file_project::Error::Resource(
            yamaa_adapters::file_resources::Error::InvalidPath
        )
    ));
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(owner.retained_sources(16, 65536).unwrap().len(), 0);
    assert_eq!(
        project.run().captured_environment().root().source().bytes,
        b"schema_version: '1.0'\n"
    );
}

#[test]
fn known_entry_limits_refuse_before_the_first_environment_read_in_both_routes() {
    use file_workflow::SelectionError;
    use std::sync::Arc;
    use yamaa_adapters::file_graph::Error as G;
    let study = Study::new();
    // A malformed environment would produce a raw decode refusal if entered.
    study.write("environment.yaml", "[");
    let schema = shipped_schema::capture().unwrap();
    let environment_schema = shipped_schema::capture_environment().unwrap();
    let oversized = "x".repeat(65_537);
    for (entry, limits, expected) in [
        (
            "root.yaml",
            Limits {
                graph: yamaa_core::producer_graph::Limits {
                    nodes: 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            "producer_graph_nodes",
        ),
        (
            "root.yaml",
            Limits {
                schema_modules: 0,
                ..Default::default()
            },
            "producer_graph_schema_modules",
        ),
        (
            "root.yaml",
            Limits {
                schema_bytes: 0,
                ..Default::default()
            },
            "producer_graph_schema_bytes",
        ),
        (
            oversized.as_str(),
            Limits::default(),
            "producer_graph_identity_bytes",
        ),
        // First request spellings fit exactly; the sealed entry spelling does
        // not. This refusal must precede even the environment root inspection.
        (
            "root.yaml",
            Limits {
                graph: yamaa_core::producer_graph::Limits {
                    text_bytes: "root.yaml".len() + "environment.yaml".len(),
                    ..Default::default()
                },
                ..Default::default()
            },
            "resource_limit",
        ),
        (
            "root.yaml",
            Limits {
                graph: yamaa_core::producer_graph::Limits {
                    work: "root.yaml".len() + "environment.yaml".len(),
                    ..Default::default()
                },
                ..Default::default()
            },
            "resource_limit",
        ),
    ] {
        for selection in [false, true] {
            let resources = Resources::new(&study.path(""), &study.path(""), &[]).unwrap();
            let error = if selection {
                match file_workflow::select_with_resources(
                    resources,
                    entry,
                    "environment.yaml",
                    Language::Python,
                    Arc::clone(&schema),
                    Arc::clone(&environment_schema),
                    limits,
                ) {
                    Err(SelectionError::Preparation(error)) => error,
                    _ => panic!("selection must refuse the known entry limit"),
                }
            } else {
                match file_workflow::prepare_with_resources(
                    resources,
                    entry,
                    "environment.yaml",
                    Language::Python,
                    Arc::clone(&schema),
                    Arc::clone(&environment_schema),
                    limits,
                ) {
                    Err(error) => error,
                    Ok(_) => panic!("static preparation must refuse the known entry limit"),
                }
            };
            let Error::Admission(owner) = &error else {
                panic!("original pre-capture admission owner: {error:?}")
            };
            match owner.error() {
                G::Limit(name) => assert_eq!(*name, expected),
                G::Resource(yamaa_adapters::file_resources::Error::Limit) => {
                    assert_eq!(expected, "resource_limit")
                }
                other => panic!("original typed cause: {other:?}"),
            }
            assert_eq!(owner.capture_reads(), 0);
            assert_eq!(owner.entry(), Some(entry));
            assert_eq!(owner.environment(), Some("environment.yaml"));
            assert!(Arc::ptr_eq(owner.specification_schema(), &schema));
            assert!(Arc::ptr_eq(owner.environment_schema(), &environment_schema));
            assert!(owner.retained_sources(16, 131072).unwrap().is_empty());
            assert!(
                matches!(producer_check::rejected(&error, Language::Python), Err(producer_check::Error::Original(original)) if std::ptr::eq(original, &error))
            );
        }
    }
}

#[test]
fn known_entry_refusal_keeps_preexisting_raw_captures_and_both_schemas_after_files_drop() {
    use file_workflow::SelectionError;
    use std::sync::Arc;
    let study = Study::new();
    let original = b"version: '1.0'\ndata_roots: []\n";
    study.write(
        "captured-config.yaml",
        std::str::from_utf8(original).unwrap(),
    );
    let mut resources = Resources::new(&study.path(""), &study.path(""), &[]).unwrap();
    let (held, _) = resources.capture("captured-config.yaml", 65536).unwrap();
    let schema = shipped_schema::capture().unwrap();
    let environment_schema = shipped_schema::capture_environment().unwrap();
    let rejection = file_workflow::select_with_resources(
        resources,
        "root.yaml",
        "environment.yaml",
        Language::Python,
        Arc::clone(&schema),
        Arc::clone(&environment_schema),
        Limits {
            graph: yamaa_core::producer_graph::Limits {
                nodes: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let Err(SelectionError::Preparation(error @ Error::Admission(_))) = rejection else {
        panic!("owned known entry refusal")
    };
    let Error::Admission(owner) = &error else {
        unreachable!()
    };
    assert_eq!(owner.capture_reads(), 1); // existing config only, no new read
    assert!(Arc::ptr_eq(owner.specification_schema(), &schema));
    assert!(Arc::ptr_eq(owner.environment_schema(), &environment_schema));
    assert!(owner.retained_sources(0, 0).is_err());
    let path = study.path("captured-config.yaml");
    drop(schema);
    drop(environment_schema);
    drop(study);
    let sources = owner.retained_sources(16, 65536).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].0, path);
    assert_eq!(sources[0].1.as_ref(), original);
    assert!(Arc::ptr_eq(&sources[0].1, &held));
    assert!(matches!(
        owner.error(),
        yamaa_adapters::file_graph::Error::Limit("producer_graph_nodes")
    ));
    assert!(
        matches!(producer_check::rejected(&error, Language::Python), Err(producer_check::Error::Original(original)) if std::ptr::eq(original, &error))
    );
    assert_eq!(owner.capture_reads(), 1);
}
