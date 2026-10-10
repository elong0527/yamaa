use std::sync::Arc;
use yamaa_adapters::{
    producer_admission::SuppliedProducer,
    producer_graph::{prepare, Error, Limits, SuppliedNode},
    shipped_schema,
    specification_source::{InheritancePort, PreparedDocument, Source},
};
use yamaa_core::{
    producer_graph::Error as CoreError,
    project_environment::{Draft, Environment, ExecutionEnvironment},
    project_function::Language,
    specification::PrepareError,
};
use yamaa_engine::inheritance::{Source as Identity, SourceError};

fn environment() -> ExecutionEnvironment {
    Environment::admit(
        Language::Python,
        Draft {
            language: None,
            lock: None,
            functions: None,
            codelists: vec![],
            has_study: false,
            submissions: vec![],
        },
    )
    .unwrap()
    .into_execution()
}
fn text(input: &str, base: &str, output: &str) -> String {
    format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - {{name: VALUE, type: float, label: Reported value, derivation: {base}.VALUE}}\noutput: {{path: {output}, columns: [ID, VALUE], decimals: 2}}\n")
}
fn document(identity: &str, text: &str) -> Arc<PreparedDocument> {
    Arc::new(
        shipped_schema::capture()
            .unwrap()
            .prepare_standalone(Source {
                identity: identity.into(),
                bytes: text.as_bytes().to_vec(),
            })
            .unwrap(),
    )
}
fn link(dataset: &str, document: &Arc<PreparedDocument>, artifact: &str) -> SuppliedProducer {
    SuppliedProducer {
        dataset: dataset.into(),
        schema_identity: document.source().identity.clone(),
        source_identity: artifact.into(),
        output_identity: artifact.into(),
        document: Arc::clone(document),
    }
}
fn node(document: &Arc<PreparedDocument>, producers: Vec<SuppliedProducer>) -> SuppliedNode {
    SuppliedNode {
        document: Arc::clone(document),
        producers,
    }
}

#[test]
fn recursive_original_documents_are_owned_once_and_all_alias_origins_remain_held() {
    let leaf_text = text("{RAW: never-read.csv}", "RAW", "leaf.csv");
    let left_text = text(
        "{LEAF: {path: leaf.csv, schema: './leaf.yaml'}}",
        "LEAF",
        "left.csv",
    );
    let right_text = text(
        "{LEAF: {path: leaf.csv, schema: leaf.yaml}}",
        "LEAF",
        "right.csv",
    );
    let root_text = text("{RIGHT: {path: right.csv, schema: right.yaml}, LEFT: {path: left.csv, schema: left.yaml}, AGAIN: {path: left.csv, schema: './left.yaml'}}", "RIGHT", "root.csv");
    let leaf = document("/leaf.yaml", &leaf_text);
    let left = document("/left.yaml", &left_text);
    let right = document("/right.yaml", &right_text);
    let root = document("/root.yaml", &root_text);
    let graph = prepare(
        "/root.yaml",
        vec![
            node(&left, vec![link("LEAF", &leaf, "/leaf.csv")]),
            node(
                &root,
                vec![
                    link("AGAIN", &left, "/left.csv"),
                    link("LEFT", &left, "/left.csv"),
                    link("RIGHT", &right, "/right.csv"),
                ],
            ),
            node(&leaf, vec![]),
            node(&right, vec![link("LEAF", &leaf, "/leaf.csv")]),
        ],
        environment(),
        Limits::default(),
    )
    .unwrap();
    drop((root, left, right, leaf));
    assert_eq!(graph.checked().metadata().root(), 1);
    assert_eq!(graph.checked().metadata().order(), [2, 3, 0, 1]);
    assert_eq!(graph.nodes().len(), 4);
    assert_eq!(
        graph.nodes()[1].document.source().bytes,
        root_text.as_bytes()
    );
    assert_eq!(
        graph.nodes()[2].document.source().bytes,
        leaf_text.as_bytes()
    );
    assert!(Arc::ptr_eq(
        &graph.nodes()[0].producers[0].document,
        &graph.nodes()[2].document
    ));
    assert!(Arc::ptr_eq(
        &graph.nodes()[3].producers[0].document,
        &graph.nodes()[2].document
    ));
    let aliases = graph.checked().metadata().nodes()[1].producers();
    assert_eq!(
        aliases
            .iter()
            .map(|p| (
                p.dataset(),
                p.schema_origin().declaring_source(),
                p.schema_origin().written()
            ))
            .collect::<Vec<_>>(),
        [
            ("RIGHT", "/root.yaml", "right.yaml"),
            ("LEFT", "/root.yaml", "left.yaml"),
            ("AGAIN", "/root.yaml", "./left.yaml")
        ]
    );
    assert!(matches!(
        graph.checked().execution_capability(),
        Err(PrepareError::Unsupported(_))
    ));
    assert!(matches!(
        yamaa_engine::domain::check(graph.nodes()[1].document.model()),
        Err(PrepareError::Unsupported(_))
    ));
}

#[test]
fn incomplete_duplicate_replaced_and_unreachable_supplied_nodes_are_denied() {
    let p = document("/p.yaml", &text("{RAW: never-read.csv}", "RAW", "p.csv"));
    let root = document(
        "/root.yaml",
        &text("{P: {path: p.csv, schema: p.yaml}}", "P", "root.csv"),
    );
    let replaced = document(
        "/p.yaml",
        &text("{RAW: different-never-read.csv}", "RAW", "p.csv"),
    );
    let extra = document(
        "/extra.yaml",
        &text("{RAW: never-read.csv}", "RAW", "extra.csv"),
    );
    for (nodes, reason) in [
        (
            vec![node(&root, vec![link("P", &p, "/p.csv")])],
            "missing_producer_node",
        ),
        (
            vec![
                node(&root, vec![link("P", &p, "/p.csv")]),
                node(&p, vec![]),
                node(&p, vec![]),
            ],
            "duplicate_identity",
        ),
        (
            vec![
                node(&root, vec![link("P", &replaced, "/p.csv")]),
                node(&p, vec![]),
            ],
            "contradictory_producer_document",
        ),
    ] {
        assert!(
            matches!(prepare("/root.yaml", nodes, environment(), Limits::default()), Err(Error::Boundary { reason: actual, .. }) if actual == reason)
        );
    }
    assert!(matches!(
        prepare(
            "/root.yaml",
            vec![
                node(&root, vec![link("P", &p, "/p.csv")]),
                node(&p, vec![]),
                node(&extra, vec![])
            ],
            environment(),
            Limits::default()
        ),
        Err(Error::Graph(CoreError::Boundary {
            reason: "unreachable_node",
            ..
        }))
    ));
}

struct Parent {
    bytes: Vec<u8>,
    calls: usize,
}
impl InheritancePort for Parent {
    type Error = ();
    fn canonicalize(&mut self, _: &str, written: &str) -> Result<Identity, SourceError<()>> {
        self.calls += 1;
        assert_eq!(written, "parent.yaml");
        Ok(Identity {
            identity: "/shared-parent.yaml".into(),
            display_path: "parent.yaml".into(),
        })
    }
    fn capture(&mut self, _: &Identity, _: usize) -> Result<Vec<u8>, SourceError<()>> {
        self.calls += 1;
        Ok(self.bytes.clone())
    }
    fn rebase(
        &mut self,
        _: &Identity,
        _: &Identity,
        written: &str,
        _: usize,
    ) -> Result<String, ()> {
        self.calls += 1;
        Ok(written.into())
    }
}
fn inherited(identity: &str, text: &str, parent: &[u8]) -> (Arc<PreparedDocument>, Parent) {
    let mut port = Parent {
        bytes: parent.into(),
        calls: 0,
    };
    let bytes = text.replace("domain: TEST", "parents: parent.yaml\ndomain: TEST");
    let prepared = shipped_schema::prepare(
        Source {
            identity: identity.into(),
            bytes: bytes.into_bytes(),
        },
        identity.into(),
        &mut port,
    )
    .unwrap();
    (Arc::new(prepared), port)
}

#[test]
fn snapshots_shared_only_across_distinct_branches_must_hold_identical_bytes() {
    let (a, a_port) = inherited(
        "/a.yaml",
        &text("{RAW: never-read.csv}", "RAW", "a.csv"),
        b"schema_version: '1.0'\n# first retained parent\n",
    );
    let (b, b_port) = inherited(
        "/b.yaml",
        &text("{RAW: never-read.csv}", "RAW", "b.csv"),
        b"schema_version: '1.0'\n# different retained parent\n",
    );
    let left = document(
        "/left.yaml",
        &text("{A: {path: a.csv, schema: a.yaml}}", "A", "left.csv"),
    );
    let right = document(
        "/right.yaml",
        &text("{B: {path: b.csv, schema: b.yaml}}", "B", "right.csv"),
    );
    let root = document("/root.yaml", &text("{LEFT: {path: left.csv, schema: left.yaml}, RIGHT: {path: right.csv, schema: right.yaml}}", "LEFT", "root.csv"));
    let before = (a_port.calls, b_port.calls);
    assert!(matches!(
        prepare(
            "/root.yaml",
            vec![
                node(
                    &root,
                    vec![
                        link("LEFT", &left, "/left.csv"),
                        link("RIGHT", &right, "/right.csv")
                    ]
                ),
                node(&left, vec![link("A", &a, "/a.csv")]),
                node(&right, vec![link("B", &b, "/b.csv")]),
                node(&a, vec![]),
                node(&b, vec![])
            ],
            environment(),
            Limits::default()
        ),
        Err(Error::Boundary {
            reason: "contradictory_snapshot",
            ..
        })
    ));
    assert_eq!(before, (a_port.calls, b_port.calls));
}

#[test]
fn equal_shared_parent_snapshots_and_written_inherited_origins_remain_retained() {
    let producer_text = text("{RAW: never-read.csv}", "RAW", "p.csv").replace(
        "output: {path: p.csv, columns: [ID, VALUE], decimals: 2}\n",
        "",
    );
    let parent = b"schema_version: '1.0'\noutput: {path: p.csv, columns: [ID, VALUE], decimals: 2}\n# retained parent\n";
    let (p, port) = inherited("/p.yaml", &producer_text, parent);
    let (root, root_port) = inherited(
        "/root.yaml",
        &text("{P: {path: p.csv, schema: p.yaml}}", "P", "root.csv"),
        parent,
    );
    let before = (port.calls, root_port.calls);
    let graph = prepare(
        "/root.yaml",
        vec![node(&root, vec![link("P", &p, "/p.csv")]), node(&p, vec![])],
        environment(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        graph.nodes()[1].document.parents()[0].source().bytes,
        parent
    );
    assert_eq!(
        graph.nodes()[0].document.parents()[0].source().bytes,
        parent
    );
    assert_eq!(
        graph.checked().metadata().nodes()[0].producers()[0]
            .output_origin()
            .declaring_source(),
        "/shared-parent.yaml"
    );
    assert_eq!(
        graph.checked().metadata().nodes()[0].producers()[0]
            .output_origin()
            .written(),
        "p.csv"
    );
    assert_eq!(before, (port.calls, root_port.calls));
}

#[test]
fn changed_schema_bytes_are_rejected_and_aggregate_raw_quotas_precede_capability() {
    let p_text = text("{RAW: never-read.csv}", "RAW", "p.csv");
    let p = document("/p.yaml", &p_text);
    let root = document(
        "/root.yaml",
        &text("{P: {path: p.csv, schema: p.yaml}}", "P", "root.csv"),
    );
    let mut sources = shipped_schema::capture().unwrap().sources().to_vec();
    sources[0]
        .bytes
        .extend_from_slice(b"\n# changed schema closure\n");
    let schema =
        yamaa_adapters::specification_source::CapturedSchema::admit(sources, 0, Default::default())
            .unwrap();
    let changed = Arc::new(
        schema
            .prepare_standalone(Source {
                identity: "/p.yaml".into(),
                bytes: p_text.into_bytes(),
            })
            .unwrap(),
    );
    assert!(matches!(
        prepare(
            "/root.yaml",
            vec![
                node(&root, vec![link("P", &changed, "/p.csv")]),
                node(&changed, vec![])
            ],
            environment(),
            Limits::default()
        ),
        Err(Error::Boundary {
            reason: "schema_mismatch",
            ..
        })
    ));
    for (limits, resource) in [
        (
            Limits {
                captured_bytes: 0,
                ..Default::default()
            },
            "producer_graph_captured_bytes",
        ),
        (
            Limits {
                schema_bytes: 0,
                ..Default::default()
            },
            "producer_graph_schema_bytes",
        ),
        (
            Limits {
                schema_modules: 0,
                ..Default::default()
            },
            "producer_graph_schema_modules",
        ),
        (
            Limits {
                snapshots: 0,
                ..Default::default()
            },
            "producer_graph_snapshots",
        ),
    ] {
        assert!(
            matches!(prepare("/root.yaml", vec![node(&root, vec![link("P", &p, "/p.csv")]), node(&p, vec![])], environment(), limits), Err(Error::Limit(actual)) if actual == resource)
        );
    }
}

#[test]
fn original_inline_type_conflict_is_not_erased_by_complete_graph_admission() {
    let p = document("/p.yaml", &text("{RAW: never-read.csv}", "RAW", "p.csv"));
    let root = document(
        "/root.yaml",
        &text(
            "{P: {path: p.csv, schema: p.yaml, types: {ID: int}}}",
            "P",
            "root.csv",
        ),
    );
    let Err(Error::Graph(CoreError::Admission {
        error: yamaa_core::producer_admission::Error::Invalid(findings),
        ..
    })) = prepare(
        "/root.yaml",
        vec![node(&root, vec![link("P", &p, "/p.csv")]), node(&p, vec![])],
        environment(),
        Limits::default(),
    )
    else {
        panic!("original conflicting authority")
    };
    assert_eq!(findings[0].definition().condition, "redundant_field_type");
    assert_eq!(findings[0].definition().requirement, Some("REQ-0523"));
    assert_eq!(findings[0].spec_paths, ["input.P.types.ID"]);
}
