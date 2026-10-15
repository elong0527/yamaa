use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{
    file_resources::Resources,
    file_workflow::{self, Error},
    producer_check, shipped_schema,
};
use yamaa_core::project_function::Language;

static NEXT: AtomicUsize = AtomicUsize::new(0);
#[test]
fn unavailable_environment_precedes_bad_entry_and_valid_environment_is_retained_before_missing_entry(
) {
    let study = Study::new();
    let entry = study.path("root.yaml");
    let environment = study.path("missing-environment.yaml");
    for entry_bytes in [None, Some("[invalid entry that must not be read")] {
        if let Some(bytes) = entry_bytes {
            study.write("root.yaml", bytes);
        }
        let file_workflow::SelectionError::Preparation(Error::Environment(owner)) =
            file_workflow::select(&entry, &environment, Language::Python)
                .err()
                .unwrap()
        else {
            panic!("original unavailable environment owner")
        };
        assert!(matches!(
            owner.error(),
            yamaa_adapters::file_project::Error::Resource(
                yamaa_adapters::file_resources::Error::Missing
            )
        ));
        assert_eq!(owner.capture_reads(), 0);
        assert_eq!(owner.captured_sources().count(), 0);
        let rejection = Error::Environment(owner);
        let rows = producer_check::rejected(&rejection, Language::Python).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].condition, "project_environment_missing");
        assert_eq!(rows[0].requirement.as_deref(), Some("REQ-0694"));
        assert_eq!(rows[0].spec_paths, ["environment"]);
        assert_eq!(
            rows[0].context,
            serde_json::json!({"cause":"missing", "path":environment}).to_string()
        );
        let Error::Environment(owner) = &rejection else {
            unreachable!()
        };
        assert_eq!(owner.capture_reads(), 0);
        assert_eq!(owner.captured_sources().count(), 0);
    }
    fs::remove_file(study.0.join("root.yaml")).unwrap();
    let environment = study.path("environment.yaml");
    study.write(
        "environment.yaml",
        "schema_version: '1.0'\nlanguage: python\n",
    );
    let file_workflow::SelectionError::Preparation(Error::Graph(owner)) =
        file_workflow::select(&entry, &environment, Language::Python)
            .err()
            .unwrap()
    else {
        panic!("valid environment precedes missing entry")
    };
    assert!(matches!(
        owner.error(),
        yamaa_adapters::file_graph::Error::Document(
            yamaa_adapters::file_preparation::Error::Resource(
                yamaa_adapters::file_resources::Error::Missing
            )
        )
    ));
    assert_eq!(owner.capture_reads(), 1);
    let sources = owner.captured_sources().collect::<Vec<_>>();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].0, environment);
    assert_eq!(sources[0].1, b"schema_version: '1.0'\nlanguage: python\n");
    assert!(!study.0.join("input.csv").exists());
}
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
    fn prepare(&self) -> Result<yamaa_adapters::file_graph::FileGraph, Error> {
        file_workflow::prepare_with_resources(
            Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
            "root.yaml",
            "environment.yaml",
            Language::Python,
            shipped_schema::capture().unwrap(),
            shipped_schema::capture_environment().unwrap(),
        )
    }
    fn rejected(&self) -> Error {
        match self.prepare() {
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
fn seventeen_column_verifications_keep_their_identities_without_reading_data() {
    let study = Study::new();
    let mut verifications = String::from("    verifications:\n");
    for index in 0..17 {
        verifications.push_str(&format!("      - not_missing: {{id: CHECK{index}}}\n"));
    }
    let source = yaml("{RAW: never-read.csv}", "RAW", "result.csv", false).replace(
        "    derivation: RAW.VALUE\n",
        &format!("    derivation: RAW.VALUE\n{verifications}"),
    );
    study.write("root.yaml", &source);
    let graph = study.prepare().unwrap();
    assert_eq!(graph.capture_reads(), 2);
    assert!(producer_check::prepared(&graph).unwrap().is_empty());
    let compiled = yamaa_core::specification::PreparedSpecification::prepare(
        graph.graph().nodes()[0].document.model(),
    )
    .unwrap();
    for index in 0..17 {
        let path = format!("columns.VALUE.verifications[{index}].not_missing");
        assert_eq!(
            compiled.verification_identity(&path),
            Some(format!("CHECK{index}").as_str())
        );
        assert!(matches!(
            compiled.column_check(&path),
            Some(yamaa_core::dataset::Check::NotMissing)
        ));
    }
    assert!(!study.0.join("never-read.csv").exists());
    assert!(!study.0.join("result.csv").exists());
}

#[test]
fn raw_schema_checks_keep_all_findings_and_public_checks_take_the_first_ten_thousand() {
    use yamaa_adapters::issue_rows::Issue;

    let study = Study::new();
    let mut source = yaml("{RAW: never-read.csv}", "RAW", "result.csv", false);
    // Independent unknown root fields are all observed by schema validation.
    // Payload compilers keep their separate first-invalid-prefix semantics.
    for index in 0..10_001 {
        source.push_str(&format!("extra_{index:05}: true\n"));
    }
    assert!(source.len() < 8 * 1024 * 1024);
    study.write("root.yaml", &source);

    let owner = study.rejected();
    let Error::Graph(graph) = &owner else {
        panic!("schema rejection owner")
    };
    assert_eq!(graph.capture_reads(), 2); // environment and specification only
    let raw = producer_check::rejected(&owner, Language::Python).unwrap();
    assert_eq!(raw.len(), 10_001);
    for (index, issue) in raw.iter().enumerate() {
        let name = format!("extra_{index:05}");
        assert_eq!(
            issue,
            &Issue {
                phase: "validation".into(),
                condition: "unknown_field".into(),
                requirement: Some("REQ-0285".into()),
                spec_paths: vec![name.clone()],
                context: serde_json::json!({"class":"root_class", "field":name}).to_string(),
            },
            "root field {index} keeps its complete finding in authored order"
        );
    }

    let public = yamaa_adapters::project_application::check(
        &study.path("root.yaml"),
        &study.path("environment.yaml"),
        Language::Python,
    );
    assert_eq!(public.len(), 10_000);
    assert_eq!(public, raw[..10_000]);
    assert!(!study.0.join("never-read.csv").exists());
    assert!(!study.0.join("result.csv").exists());
}

#[test]
fn original_decode_diagnostics_remain_bounded_and_owned() {
    use yamaa_adapters::issue_rows::Issue;
    let study = Study::new();
    study.write("root.yaml", "\u{e9}\n");
    let owner = study.rejected();
    let expected = vec![Issue {
        phase: "validation".into(),
        condition: "non_ascii_source".into(),
        requirement: Some("REQ-0705".into()),
        spec_paths: vec!["$".into()],
        context: serde_json::json!({"line":1, "column":1, "path":study.path("root.yaml")})
            .to_string(),
    }];
    assert_eq!(
        producer_check::rejected(&owner, Language::Python).unwrap(),
        expected
    );
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
    let graph = study.prepare().unwrap();
    assert_eq!(graph.capture_reads(), 5); // one environment + four canonical nodes
    assert_eq!(graph.graph().metadata().order(), [3, 1, 2, 0]);
    let expected = ["root.yaml", "left.yaml", "leaf.yaml"].map(|name| yamaa_adapters::issue_rows::Issue {
        phase: "validation".into(), condition: "invalid_regex".into(), requirement: Some("REQ-0827".into()),
        spec_paths: vec!["columns.VALUE.verifications[0].matches.pattern".into()],
        context: serde_json::json!({"pattern":"é(", "source":study.path(name), "entry":study.path("root.yaml")}).to_string(),
    });
    assert_eq!(producer_check::prepared(&graph).unwrap(), expected);
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
        let error = study.rejected();
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
        let error = study.rejected();
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
fn raw_environment_rejection_precedes_the_first_graph_read() {
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
    );
    let raw = "schema_version: '1.0'\nfunctions: [\n";
    study.write("environment.yaml", raw);
    let error = study.rejected();
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
    let error = study.rejected();
    let rows = producer_check::rejected(&error, Language::Python).unwrap();
    assert!(rows
        .iter()
        .any(|r| r.requirement.as_deref() == Some("REQ-0696")));
    let Error::Environment(environment) = &error else {
        panic!("environment");
    };
    assert_eq!(environment.capture_reads(), 1);
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
    let error = study.rejected();
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
    let graph = study.prepare().unwrap();
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
        graph.graph().metadata().nodes()[0].producers()[0]
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
    let error = study.rejected();
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
    let error = study.rejected();
    let rows = producer_check::rejected(&error, Language::Python).unwrap();
    assert!(rows
        .iter()
        .any(|row| row.requirement.as_deref() == Some("REQ-0534")
            && row.condition == "invalid_producer_contract"));
    study.write(
        "environment.yaml",
        "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {id: broken.yaml}\n",
    );
    study.write("uv.lock", "version = 1\npackage = []\n");
    let malformed = "function: [\n";
    study.write("broken.yaml", malformed);
    let failure = study.rejected();
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
    let graph = study.prepare().unwrap();
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
    let graph = study.prepare().unwrap();
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
    let graph = study.prepare().unwrap();
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
    let rejection = study.rejected();
    let Error::Graph(graph) = &rejection else {
        panic!("the original root rejection must remain owned");
    };
    assert_eq!(graph.capture_reads(), 2);
    assert!(graph.documents().is_empty());
    drop(study);
    let retained = graph.retained_sources();
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
    assert_eq!(graph.capture_reads(), 2);
}

impl Study {
    fn select(&self) -> Result<file_workflow::Selected, file_workflow::SelectionError> {
        file_workflow::select_with_resources(
            Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
            "root.yaml",
            "environment.yaml",
            Language::Python,
            shipped_schema::capture().unwrap(),
            shipped_schema::capture_environment().unwrap(),
        )
    }
    fn selection_rejection(&self) -> file_workflow::SelectionError {
        match self.select() {
            Err(error) => error,
            Ok(_) => panic!("expected selected preparation rejection"),
        }
    }
}
struct NoActivation;
impl yamaa_core::project_activation::ActivationPort for NoActivation {
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
        _: &[yamaa_core::function_invocation::Argument<'_>],
    ) -> Result<yamaa_core::value::Value, yamaa_core::function_invocation::HostError<()>> {
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
    use yamaa_core::specification_output::ArtifactPort;
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
    );
    let Selected::Ordinary(mut selected) = study.select().unwrap() else {
        panic!("ordinary capability");
    };
    assert_eq!(selected.capture_reads(), 2);
    assert!(!study.0.join("never-read.csv").exists());
    assert!(!study.0.join("root.csv").exists());
    // An independent study source is first created and read after preparation.
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
    let Selected::Producer(graph) = study.select().unwrap() else {
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
    assert_eq!(graph.graph().metadata().order(), [3, 1, 2, 0]);
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
    let SelectionError::Ordinary(rejected) = study.selection_rejection() else {
        panic!("original ordinary compiler rejection");
    };
    assert_eq!(rejected.capture_reads(), 2);
    assert_eq!(rejected.entry(), "root.yaml");
    let rows = yamaa_adapters::project_check::rejected(rejected.run()).unwrap();
    assert_eq!(rows[0].condition, "unknown_project_function");
    assert_eq!(rows[0].requirement.as_deref(), Some("REQ-0698"));
    fs::remove_dir_all(&study.0).unwrap();
    let held = rejected.retained_sources();
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
        let SelectionError::Preparation(error @ Error::Graph(_)) = study.selection_rejection()
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
fn selected_raw_environment_and_root_failures_keep_exact_capture_prefixes() {
    use file_workflow::SelectionError;
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
    );
    study.write("environment.yaml", "schema_version: '1.0'\nfunctions: [\n");
    let SelectionError::Preparation(Error::Environment(environment)) = study.selection_rejection()
    else {
        panic!("raw environment rejection");
    };
    assert_eq!(environment.capture_reads(), 1);
    study.write("environment.yaml", "schema_version: '1.0'\n");
    study.write("root.yaml", "[");
    let SelectionError::Preparation(root @ Error::Graph(_)) = study.selection_rejection() else {
        panic!("raw root rejection");
    };
    let rows = producer_check::rejected(&root, Language::Python).unwrap();
    assert_eq!(rows[0].condition, "invalid_yaml");
    fs::remove_dir_all(&study.0).unwrap();
    assert_eq!(environment.retained_sources().len(), 1);
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
        let SelectionError::Preparation(error @ Error::Graph(_)) = study.selection_rejection()
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
    assert_eq!(owner.retained_sources().len(), 0);
    assert_eq!(
        project.run().captured_environment().root().source().bytes,
        b"schema_version: '1.0'\n"
    );
}

#[test]
fn absent_environment_selects_producer_without_capturing_or_inventing_metadata() {
    let study = Study::new();
    // A nearby environment is deliberately unreadable as YAML. Absence must
    // neither discover it nor convert it into an empty captured document.
    study.write("environment.yaml", "[");
    study.write(
        "leaf.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
    );
    study.write(
        "root.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: leaf.yaml}}",
            "P",
            "root.csv",
            false,
        ),
    );
    let selected = file_workflow::select_request(
        yamaa_core::build::Request {
            specification: &study.path("root.yaml"),
            environment: None,
        },
        Language::Python,
    )
    .unwrap_or_else(|error| {
        panic!(
            "original producer admission: {:?}",
            yamaa_adapters::project_application::selection_failure(&error, Language::Python)
        )
    });
    let file_workflow::Selected::Producer(graph) = selected else {
        panic!("schema link selects producer");
    };
    assert_eq!(graph.capture_reads(), 2);
    assert!(graph.environment().root_optional().is_none());
    assert!(graph.environment().lock().is_none());
    assert!(graph.environment().captures().is_empty());
    assert!(graph
        .graph()
        .metadata()
        .environment()
        .functions()
        .is_empty());
    assert_eq!(graph.captured_sources().count(), 2);
    assert!(!study.0.join("never-read.csv").exists());
    assert!(!study.0.join("leaf.csv").exists());
    assert!(!study.0.join("root.csv").exists());
    fs::remove_dir_all(&study.0).unwrap();
    assert!(producer_check::prepared(&graph).unwrap().is_empty());
}

#[test]
fn plain_workflow_profile_inspection_keeps_physical_kind_before_study_capture() {
    use serde_json::Value as JsonValue;
    use yamaa_core::diagnostic::ConditionCode as C;
    use yamaa_core::specification::PrepareError;

    let study = Study::new();
    study.write("environment.yaml", "[");
    study.write("parent/base.yaml", "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input/unknown}\noutput: {path: output.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n");
    study.write(
        "entry/root.yaml",
        "schema_version: '1.0'\nparents: [../parent/base.yaml]\n",
    );
    fs::create_dir_all(study.0.join("parent/input")).unwrap();
    for (kind, expected) in [
        (0, C::ResourceNotRegularFile),
        (1, C::SourceUnknownProfile),
        (2, C::SourceUnknownProfile),
        (3, C::ResourceSymlink),
    ] {
        let input = study.0.join("parent/input/unknown");
        match kind {
            0 => fs::create_dir(&input).unwrap(),
            1 => fs::write(&input, b"ID\n1\n").unwrap(),
            2 => (),
            _ => {
                fs::write(study.0.join("parent/input/target"), b"ID\n1\n").unwrap();
                #[cfg(unix)]
                std::os::unix::fs::symlink("target", &input).unwrap();
                #[cfg(windows)]
                std::os::windows::fs::symlink_file("target", &input).unwrap();
            }
        }
        let Err(file_workflow::SelectionError::Plain(owner)) = file_workflow::select_request(
            yamaa_core::build::Request {
                specification: &study.path("entry/root.yaml"),
                environment: None,
            },
            Language::Python,
        ) else {
            panic!("plain route must retain its preparation rejection");
        };
        assert_eq!(
            owner.capture_reads(),
            2,
            "only root and parent metadata were captured"
        );
        let yamaa_adapters::file_preparation::Error::Compile(
            yamaa_adapters::specification_run::Error::Prepare(PrepareError::Invalid(findings)),
        ) = owner.error()
        else {
            panic!("original compiler finding remains owned");
        };
        let issue = findings[0].diagnostic();
        assert_eq!(issue.code, expected);
        assert_eq!(issue.spec_paths, ["input.SRC.path"]);
        if matches!(kind, 0 | 3) {
            assert_eq!(
                issue.context["path"],
                JsonValue::String("input/unknown".into())
            );
            assert_eq!(issue.context["dataset"], JsonValue::String("SRC".into()));
        }
        let sources = owner.retained_sources();
        assert_eq!(sources.len(), 2);
        assert!(sources
            .iter()
            .all(|(identity, _)| !identity.ends_with("environment.yaml")
                && !identity.ends_with("input/unknown")));
        match kind {
            0 => fs::remove_dir(&input).unwrap(),
            2 => (),
            _ => fs::remove_file(&input).unwrap(),
        }
    }
}
