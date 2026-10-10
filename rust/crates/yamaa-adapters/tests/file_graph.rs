#![cfg(any(unix, windows))]
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use yamaa_adapters::{
    file_graph::{Error, FileGraph, RejectedGraph},
    file_resources::{Error as ResourceError, Resources},
    producer_graph::{Error as GraphError, Limits},
    project_source::{self, CapturePort, OwnedEnvironment, Reply, Request},
    shipped_schema,
    specification_source::Source,
};
use yamaa_core::{
    producer_graph::Error as CoreError, project_function::Language, specification::PrepareError,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-native-graph-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
    fn path(&self, written: &str) -> String {
        self.0.join(written).to_str().unwrap().into()
    }
    fn write(&self, written: &str, text: &str) {
        let path = self.0.join(written);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn resources(&self) -> Resources {
        Resources::new(&self.path(""), &self.path(""), &[]).unwrap()
    }
    fn graph(&self, limits: Limits) -> Result<FileGraph, Box<RejectedGraph>> {
        FileGraph::prepare(
            self.resources(),
            "root.yaml",
            environment(),
            shipped_schema::capture().unwrap(),
            limits,
        )
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
struct NoMetadata;
impl CapturePort for NoMetadata {
    type Error = ();
    fn capture(&mut self, _: Request<'_>) -> Result<Reply, ()> {
        panic!("no environment dependency declared");
    }
}
fn environment() -> OwnedEnvironment {
    project_source::prepare(
        shipped_schema::capture_environment().unwrap(),
        Source {
            identity: "held-environment.yaml".into(),
            bytes: b"schema_version: '1.0'\n".to_vec(),
        },
        Language::Python,
        &mut NoMetadata,
        Default::default(),
    )
    .unwrap()
    .into_owned()
}
fn yaml(input: &str, base: &str, output: &str) -> String {
    format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - {{name: VALUE, type: float, label: Reported value, derivation: {base}.VALUE}}\noutput: {{path: {output}, columns: [ID, VALUE], decimals: 2}}\n")
}
fn leaf(study: &Study) {
    study.write(
        "leaf.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv"),
    );
}
fn rejected(result: Result<FileGraph, Box<RejectedGraph>>) -> Box<RejectedGraph> {
    match result {
        Ok(_) => panic!("expected rejection"),
        Err(error) => error,
    }
}

#[test]
fn native_diamond_aliases_own_one_canonical_document_and_no_artifact_authority() {
    let study = Study::new();
    leaf(&study);
    study.write(
        "left.yaml",
        &yaml(
            "{LEAF: {path: leaf.csv, schema: './leaf.yaml'}}",
            "LEAF",
            "left.csv",
        ),
    );
    study.write(
        "right.yaml",
        &yaml(
            "{LEAF: {path: leaf.csv, schema: leaf.yaml}}",
            "LEAF",
            "right.csv",
        ),
    );
    let root = yaml("{RIGHT: {path: right.csv, schema: right.yaml}, LEFT: {path: left.csv, schema: left.yaml}, AGAIN: {path: left.csv, schema: './left.yaml'}}", "RIGHT", "root.csv");
    study.write("root.yaml", &root);
    let graph = study.graph(Limits::default()).unwrap();
    assert_eq!(graph.capture_reads(), 4);
    let nodes = graph.graph().nodes();
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0].document.source().bytes, root.as_bytes());
    assert_eq!(graph.graph().checked().metadata().order(), [3, 1, 2, 0]);
    assert!(Arc::ptr_eq(
        &nodes[0].producers[1].document,
        &nodes[0].producers[2].document
    ));
    assert!(Arc::ptr_eq(
        &nodes[1].producers[0].document,
        &nodes[2].producers[0].document
    ));
    assert_eq!(
        graph.graph().checked().metadata().nodes()[0].dependencies(),
        [1, 2, 2]
    );
    assert!(matches!(
        graph.graph().checked().execution_capability(),
        Err(PrepareError::Unsupported(_))
    ));
    let paths = graph.captured_sources().map(|(p, _)| p).collect::<Vec<_>>();
    assert!(paths.iter().all(|p| p.ends_with(".yaml")));
    for path in [
        "never-read.csv",
        "leaf.csv",
        "left.csv",
        "right.csv",
        "root.csv",
    ] {
        assert!(!study.0.join(path).exists());
    }
}

#[test]
fn inherited_schema_input_and_output_origins_resolve_independently() {
    let study = Study::new();
    study.write(
        "origins/schema.yaml",
        &yaml(
            "{P: {path: ../data/p.csv, schema: '../producer/p.yaml'}}",
            "P",
            "unused.csv",
        ),
    );
    study.write(
        "origins/path.yaml",
        &yaml("{P: {path: ../data/p.csv}}", "P", "root.csv"),
    );
    study.write(
        "producer/base.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "../data/p.csv"),
    );
    study.write(
        "producer/p.yaml",
        "schema_version: '1.0'\nparents: [base.yaml]\ndomain: PRODUCER\n",
    );
    study.write(
        "root.yaml",
        "schema_version: '1.0'\nparents: [origins/schema.yaml, origins/path.yaml]\ndomain: ROOT\n",
    );
    let graph = study.graph(Limits::default()).unwrap();
    let p = &graph.graph().nodes()[0].producers[0];
    let declaration = &graph.graph().checked().metadata().nodes()[0].producers()[0];
    assert_eq!(p.schema_identity, study.path("producer/p.yaml"));
    assert_eq!(p.source_identity, study.path("data/p.csv"));
    assert_eq!(p.output_identity, study.path("data/p.csv"));
    assert_eq!(
        declaration.schema_origin().declaring_source(),
        study.path("origins/schema.yaml")
    );
    assert_eq!(
        declaration.input_origin().declaring_source(),
        study.path("origins/path.yaml")
    );
    assert_eq!(
        declaration.output_origin().declaring_source(),
        study.path("producer/base.yaml")
    );
    assert_eq!(declaration.schema_origin().written(), "../producer/p.yaml");
    assert_eq!(graph.capture_reads(), 5);
    assert!(!study.0.join("data").exists());
}

#[test]
fn mismatch_preserves_portable_req_0534_and_original_metadata_after_files_disappear() {
    let study = Study::new();
    leaf(&study);
    let root = yaml(
        "{P: {path: different.csv, schema: leaf.yaml}}",
        "P",
        "root.csv",
    );
    study.write("root.yaml", &root);
    let failure = rejected(study.graph(Limits::default()));
    match failure.error() {
        Error::Graph(GraphError::Graph(CoreError::Admission {
            node: 0,
            error: yamaa_core::producer_admission::Error::Invalid(diagnostics),
        })) => {
            assert_eq!(
                diagnostics[0].code,
                yamaa_core::diagnostic::ConditionCode::ProducerOutputPathMismatch
            );
            assert!(diagnostics[0].spec_paths.contains(&"input.P.path".into()));
        }
        other => panic!("unexpected error: {other:?}"),
    }
    let prefix = failure
        .captured_sources()
        .map(|(p, b)| (p, b.to_vec()))
        .collect::<BTreeMap<_, _>>();
    fs::remove_file(study.0.join("root.yaml")).unwrap();
    fs::remove_file(study.0.join("leaf.yaml")).unwrap();
    assert_eq!(prefix[&study.path("root.yaml")], root.as_bytes());
    assert_eq!(
        failure
            .captured_sources()
            .map(|(p, b)| (p, b.to_vec()))
            .collect::<BTreeMap<_, _>>(),
        prefix
    );
    assert_eq!(failure.documents().len(), 2);
    assert_eq!(
        failure.environment().root().source().bytes,
        b"schema_version: '1.0'\n"
    );
}

#[test]
fn cycle_captures_each_node_once_and_retains_full_authored_route() {
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml(
            "{NEXT: {path: next.csv, schema: next.yaml}}",
            "NEXT",
            "root.csv",
        ),
    );
    study.write(
        "next.yaml",
        &yaml(
            "{ROOT: {path: root.csv, schema: './root.yaml'}}",
            "ROOT",
            "next.csv",
        ),
    );
    let failure = rejected(study.graph(Limits::default()));
    match failure.error() {
        Error::Graph(GraphError::Graph(CoreError::Cycle {
            node: 1,
            diagnostic,
        })) => {
            assert_eq!(
                diagnostic.code,
                yamaa_core::diagnostic::ConditionCode::ProducerWorkflowCycle
            );
            assert_eq!(diagnostic.spec_paths, ["input.ROOT.schema"]);
            let rendered = format!("{:?}", diagnostic.context);
            assert!(rendered.contains(&study.path("root.yaml")));
            assert!(rendered.contains(&study.path("next.yaml")));
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(failure.capture_reads(), 2);
    assert_eq!(failure.documents().len(), 2);
}

#[test]
fn missing_producer_keeps_captured_consumer_and_original_declaration() {
    let study = Study::new();
    study.write(
        "root.yaml",
        &yaml(
            "{P: {path: absent.csv, schema: missing.yaml}}",
            "P",
            "root.csv",
        ),
    );
    let failure = rejected(study.graph(Limits::default()));
    assert!(matches!(
        failure.error(),
        Error::Resource(ResourceError::Missing)
    ));
    assert_eq!(failure.origin().unwrap().document, 0);
    assert_eq!(failure.origin().unwrap().dataset, "P");
    assert_eq!(failure.documents().len(), 1);
    assert_eq!(failure.capture_reads(), 1);
}

#[test]
fn malformed_entry_and_failed_parent_raw_bytes_survive_without_a_prepared_node() {
    for parent in [false, true] {
        let study = Study::new();
        let malformed = "schema_version: '1.0'\ndomain: [\n";
        let bad = if parent { "broken.yaml" } else { "root.yaml" };
        study.write(bad, malformed);
        if parent {
            study.write(
                "root.yaml",
                "schema_version: '1.0'\nparents: [broken.yaml]\ndomain: ROOT\n",
            );
        }
        let failure = rejected(study.graph(Limits::default()));
        assert!(matches!(failure.error(), Error::Document(_)));
        assert_eq!(failure.entry(), Some("root.yaml"));
        assert!(failure.documents().is_empty());
        assert_eq!(failure.capture_reads(), if parent { 2 } else { 1 });
        let captures = failure.captured_sources().collect::<BTreeMap<_, _>>();
        assert_eq!(captures[&study.path(bad)], malformed.as_bytes());
        fs::remove_file(study.0.join(bad)).unwrap();
        assert_eq!(
            failure
                .captured_sources()
                .find(|(p, _)| p == &study.path(bad))
                .unwrap()
                .1,
            malformed.as_bytes()
        );
    }
}

#[test]
fn graph_limits_stop_before_next_capture_or_owned_preparation_copy() {
    let study = Study::new();
    leaf(&study);
    let root = yaml("{P: {path: leaf.csv, schema: leaf.yaml}}", "P", "root.csv");
    study.write("root.yaml", &root);
    for limits in [
        Limits {
            graph: yamaa_core::producer_graph::Limits {
                nodes: 1,
                ..Default::default()
            },
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
            snapshots: 1,
            ..Default::default()
        },
    ] {
        let failure = rejected(study.graph(limits));
        assert_eq!(failure.capture_reads(), 1);
        assert_eq!(failure.documents().len(), 1);
    }
    // Capture itself fits, but its owned preparation copy does not. Raw bytes
    // stay held; no prepared document is admitted under the exhausted budget.
    let failure = rejected(study.graph(Limits {
        captured_bytes: root.len(),
        ..Default::default()
    }));
    assert!(matches!(
        failure.error(),
        Error::Document(yamaa_adapters::file_preparation::Error::Resource(
            ResourceError::Limit
        ))
    ));
    assert_eq!(failure.capture_reads(), 1);
    assert!(failure.documents().is_empty());
    assert_eq!(
        failure.captured_sources().next().unwrap().1,
        root.as_bytes()
    );
    let failure = rejected(study.graph(Limits {
        graph: yamaa_core::producer_graph::Limits {
            work: 0,
            ..Default::default()
        },
        ..Default::default()
    }));
    assert_eq!(failure.capture_reads(), 0);
    assert!(failure.captured_sources().next().is_none());
}

#[test]
fn metadata_fallback_does_not_redirect_declared_artifact_locations() {
    let study = Study::new();
    study.write(
        "entry/root.yaml",
        &yaml(
            "{P: {path: ../fallback/p.csv, schema: p.yaml}}",
            "P",
            "root.csv",
        ),
    );
    study.write(
        "fallback/p.yaml",
        &yaml("{RAW: never-read.csv}", "RAW", "p.csv"),
    );
    let resources = Resources::new(
        &study.path(""),
        &study.path("entry"),
        &[study.path("fallback")],
    )
    .unwrap();
    let graph = FileGraph::prepare(
        resources,
        "root.yaml",
        environment(),
        shipped_schema::capture().unwrap(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        graph.graph().nodes()[0].producers[0].schema_identity,
        study.path("fallback/p.yaml")
    );
    assert_eq!(
        graph.graph().nodes()[0].producers[0].output_identity,
        study.path("fallback/p.csv")
    );
    assert_eq!(graph.capture_reads(), 2);
    assert!(!study.0.join("fallback/p.csv").exists());
}

#[test]
fn already_captured_changed_metadata_keeps_original_snapshot_and_native_failure() {
    let study = Study::new();
    let before = yaml("{RAW: never-read.csv}", "RAW", "root.csv");
    study.write("root.yaml", &before);
    let mut resources = study.resources();
    resources.capture("root.yaml", 65_536).unwrap();
    study.write("root.yaml", &before.replace("TEST", "NEXT"));
    let failure = rejected(FileGraph::prepare(
        resources,
        "root.yaml",
        environment(),
        shipped_schema::capture().unwrap(),
        Limits::default(),
    ));
    assert!(matches!(
        failure.error(),
        Error::Document(yamaa_adapters::file_preparation::Error::Resource(
            ResourceError::Changed
        ))
    ));
    assert_eq!(failure.capture_reads(), 1);
    assert_eq!(
        failure.captured_sources().next().unwrap().1,
        before.as_bytes()
    );
}

#[test]
fn producer_and_consumer_compilation_failures_keep_the_complete_raw_graph() {
    for producer in [false, true] {
        let study = Study::new();
        leaf(&study);
        let root = yaml("{P: {path: leaf.csv, schema: leaf.yaml}}", "P", "root.csv");
        study.write("root.yaml", &root);
        let path = if producer { "leaf.yaml" } else { "root.yaml" };
        let original = fs::read_to_string(study.0.join(path)).unwrap();
        let bad = original.replace(
            if producer { "RAW.VALUE" } else { "P.VALUE" },
            "{function: {name: missing, args: {x: 1}}}",
        );
        study.write(path, &bad);
        let failure = rejected(study.graph(Limits::default()));
        assert!(matches!(
            failure.error(),
            Error::Graph(GraphError::Graph(CoreError::Compilation { .. }))
                | Error::Graph(GraphError::Graph(CoreError::Admission {
                    error: yamaa_core::producer_admission::Error::Compilation(_),
                    ..
                }))
        ));
        assert_eq!(failure.documents().len(), 2);
        assert_eq!(failure.capture_reads(), 2);
        assert_eq!(
            failure
                .captured_sources()
                .find(|(p, _)| p == &study.path(path))
                .unwrap()
                .1,
            bad.as_bytes()
        );
        assert!(!study.0.join("never-read.csv").exists());
        assert!(!study.0.join("leaf.csv").exists());
        assert!(!study.0.join("root.csv").exists());
    }
}

#[test]
fn original_redundant_types_keep_req_0523_after_native_closure_capture() {
    let study = Study::new();
    leaf(&study);
    study.write(
        "root.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}",
            "P",
            "root.csv",
        ),
    );
    let failure = rejected(study.graph(Limits::default()));
    let Error::Graph(GraphError::Graph(CoreError::Admission {
        error: yamaa_core::producer_admission::Error::Invalid(findings),
        ..
    })) = failure.error()
    else {
        panic!("original redundant authority");
    };
    assert_eq!(findings[0].definition().requirement, Some("REQ-0523"));
    assert_eq!(findings[0].spec_paths, ["input.P.types.ID"]);
    assert_eq!(failure.capture_reads(), 2);
    assert_eq!(failure.documents().len(), 2);
}

#[cfg(unix)]
#[test]
fn symlink_and_escape_schema_failures_are_terminal_and_artifact_symlinks_are_not_opened() {
    use std::os::unix::fs::symlink;
    let study = Study::new();
    leaf(&study);
    symlink("leaf.yaml", study.0.join("link.yaml")).unwrap();
    for (schema, error) in [
        ("link.yaml", ResourceError::Symlink),
        ("../escape.yaml", ResourceError::OutsideRoots),
    ] {
        study.write(
            "root.yaml",
            &yaml(
                &format!("{{P: {{path: leaf.csv, schema: '{schema}'}}}}"),
                "P",
                "root.csv",
            ),
        );
        let failure = rejected(study.graph(Limits::default()));
        assert!(matches!(failure.error(), Error::Resource(actual) if actual == &error));
        assert_eq!(failure.capture_reads(), 1);
    }
    symlink("not-present", study.0.join("leaf.csv")).unwrap();
    symlink("not-present", study.0.join("never-read.csv")).unwrap();
    study.write(
        "root.yaml",
        &yaml("{P: {path: leaf.csv, schema: leaf.yaml}}", "P", "root.csv"),
    );
    let graph = study.graph(Limits::default()).unwrap();
    assert_eq!(graph.capture_reads(), 2);
    assert!(fs::symlink_metadata(study.0.join("leaf.csv"))
        .unwrap()
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn canonical_unix_root_with_literal_backslash_remains_an_identity() {
    let outer = Study::new();
    let root = outer.0.join("named\\root");
    fs::create_dir(&root).unwrap();
    let study = Study(root.clone());
    leaf(&study);
    study.write(
        "root.yaml",
        &yaml(
            "{P: {path: leaf.csv, schema: './leaf.yaml'}}",
            "P",
            "root.csv",
        ),
    );
    let graph = study.graph(Limits::default()).unwrap();
    assert_eq!(
        graph.graph().nodes()[0].producers[0].schema_identity,
        root.join("leaf.yaml").to_str().unwrap()
    );
    assert_eq!(graph.capture_reads(), 2);
}
