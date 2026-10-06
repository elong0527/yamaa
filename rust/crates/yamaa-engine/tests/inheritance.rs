use std::collections::BTreeMap;
use yamaa_core::schema::{
    BundleLimits, Document, DocumentLimits, DocumentNode as N, NormalizationBudget,
    NormalizationError, NormalizationLimits, SchemaModule, SchemaStructure,
};
use yamaa_engine::inheritance::*;

enum V {
    Text(String),
    Seq(Vec<V>),
    Map(Vec<(&'static str, V)>),
}
use V::{Map, Seq};
fn text(value: &str) -> V {
    V::Text(value.into())
}
fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            V::Text(s) => N::Text(s),
            Seq(items) => N::Sequence(items.into_iter().map(|v| push(v, nodes)).collect()),
            Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (push(text(k), nodes), push(v, nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    let mut nodes = vec![];
    let root = push(value, &mut nodes);
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}
fn field(name: &'static str, types: V) -> V {
    Map(vec![(name, Map(vec![("type", types)]))])
}
fn schema() -> SchemaStructure {
    SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Map(vec![
                ("version", text("1.0")),
                (
                    "root_class",
                    Seq(vec![
                        field("schema_version", text("str")),
                        field("parents", Seq(vec![text("str"), text("list[str]")])),
                        field("label", text("str")),
                    ]),
                ),
            ])),
        }],
        0,
        "root_class",
        BundleLimits::default(),
    )
    .unwrap()
}
fn layer(version: &str, parents: &[&str]) -> Document {
    document(Map(vec![
        ("schema_version", text(version)),
        ("parents", Seq(parents.iter().map(|p| text(p)).collect())),
    ]))
}
fn source(identity: &str) -> Source {
    Source {
        identity: identity.into(),
        display_path: identity.into(),
    }
}
#[derive(Default)]
struct Port {
    documents: BTreeMap<String, Document>,
    aliases: BTreeMap<String, String>,
    calls: Vec<String>,
    unavailable: Option<&'static str>,
    raised: Option<&'static str>,
}
impl Port {
    fn with(mut self, name: &str, parents: &[&str]) -> Self {
        self.documents.insert(name.into(), layer("1.0", parents));
        self
    }
}
impl SourcePort for Port {
    type Error = (&'static str, u32);
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Source, SourceError<Self::Error>> {
        self.calls.push(format!("resolve:{declaring}:{written}"));
        if self.unavailable == Some(written) {
            return Err(SourceError::Unavailable);
        }
        if self.raised == Some(written) {
            return Err(SourceError::Raised(("opaque", 73)));
        }
        Ok(Source {
            identity: self
                .aliases
                .get(written)
                .map(String::as_str)
                .unwrap_or(written)
                .into(),
            display_path: format!("written/{written}"),
        })
    }
    fn read(&mut self, source: &Source) -> Result<Document, SourceError<Self::Error>> {
        self.calls.push(format!("read:{}", source.identity));
        self.documents
            .get(&source.identity)
            .cloned()
            .ok_or(SourceError::Unavailable)
    }
}
fn run(input: Document, port: &mut Port) -> Result<Vec<Layer>, Error<(&'static str, u32)>> {
    traverse(
        &schema(),
        source("entry"),
        input,
        port,
        &mut NormalizationBudget::new(NormalizationLimits::default()),
        &mut Budget::new(Limits::default()),
    )
}

#[test]
fn diamond_is_left_to_right_postorder_and_reads_each_canonical_source_once() {
    let mut port = Port::default()
        .with("a", &["common"])
        .with("b", &["alias"])
        .with("common", &[]);
    port.aliases.insert("alias".into(), "common".into());
    let layers = run(layer("1.0", &["a", "b"]), &mut port).unwrap();
    assert_eq!(
        layers
            .iter()
            .map(|l| l.source.identity.as_str())
            .collect::<Vec<_>>(),
        ["common", "a", "b", "entry"]
    );
    assert_eq!(
        port.calls,
        [
            "resolve:entry:a",
            "read:a",
            "resolve:a:common",
            "read:common",
            "resolve:entry:b",
            "read:b",
            "resolve:b:alias"
        ]
    );
    assert_eq!(layers[0].document, layer("1.0", &[]));
}

#[test]
fn parent_shorthand_is_normalized_before_requesting_the_source() {
    let input = document(Map(vec![
        ("schema_version", text("1.0")),
        ("parents", text("a")),
    ]));
    let mut port = Port::default().with("a", &[]);
    let layers = run(input, &mut port).unwrap();
    assert_eq!(port.calls, ["resolve:entry:a", "read:a"]);
    assert_eq!(layers.last().unwrap().document, layer("1.0", &["a"]));
}

#[test]
fn cycles_report_exact_canonical_loop_before_any_repeated_read() {
    for target in ["entry", "a"] {
        let mut port = Port::default().with("a", &["b"]).with("b", &["alias"]);
        port.aliases.insert("alias".into(), target.into());
        let error = run(layer("1.0", &["a"]), &mut port).unwrap_err();
        let names = if target == "entry" {
            vec!["entry", "a", "b", "entry"]
        } else {
            vec!["a", "b", "a"]
        };
        assert_eq!(
            error,
            Error::Cycle {
                path: names.into_iter().map(str::to_owned).collect(),
                returns_to_entry: target == "entry"
            }
        );
        assert_eq!(
            port.calls,
            [
                "resolve:entry:a",
                "read:a",
                "resolve:a:b",
                "read:b",
                "resolve:b:alias"
            ]
        );
    }
}

#[test]
fn layer_findings_precede_version_mismatch_and_all_parent_effects() {
    let input = document(Map(vec![
        ("schema_version", text("0")),
        ("parents", Seq(vec![text("https://bad")])),
        ("unexpected", text("x")),
    ]));
    let mut port = Port::default();
    let Error::Layer {
        source,
        input: _,
        error: NormalizationError::Invalid(findings),
    } = run(input, &mut port).unwrap_err()
    else {
        panic!("expected structural findings")
    };
    assert_eq!(source, "entry");
    assert_eq!(findings[0].condition, "unknown_field");
    assert!(port.calls.is_empty());
}

#[test]
fn version_checks_stop_before_reading_children_or_later_siblings() {
    let mut port = Port::default();
    assert_eq!(
        run(layer("0", &["a"]), &mut port).unwrap_err(),
        Error::Version {
            source: "entry".into(),
            entry: "entry".into(),
            expected: "1.0".into(),
            actual: "0".into(),
            is_entry: true
        }
    );
    assert!(port.calls.is_empty());
    port.documents.insert("a".into(), layer("0", &["unread"]));
    assert_eq!(
        run(layer("1.0", &["a", "later"]), &mut port).unwrap_err(),
        Error::Version {
            source: "a".into(),
            entry: "entry".into(),
            expected: "1.0".into(),
            actual: "0".into(),
            is_entry: false
        }
    );
    assert_eq!(port.calls, ["resolve:entry:a", "read:a"]);
}

#[test]
fn missing_version_retains_layer_diagnostics_and_never_reads_parents() {
    let mut port = Port::default();
    let Error::Layer {
        error: NormalizationError::Invalid(findings),
        ..
    } = run(document(Map(vec![("parents", text("a"))])), &mut port).unwrap_err()
    else {
        panic!("expected missing version")
    };
    assert_eq!(findings[0].condition, "schema_version_mismatch");
    assert!(port.calls.is_empty());
}

#[test]
fn malformed_parent_admission_precedes_its_descendants() {
    let mut port = Port::default();
    port.documents.insert(
        "a".into(),
        document(Map(vec![
            ("schema_version", text("1.0")),
            ("parents", Seq(vec![text("unread")])),
            ("unexpected", text("x")),
        ])),
    );
    assert!(
        matches!(run(layer("1.0",&["a","later"]),&mut port),Err(Error::Layer{source,..}) if source=="a")
    );
    assert_eq!(port.calls, ["resolve:entry:a", "read:a"]);
}

#[test]
fn local_path_policy_is_closed_before_calling_the_filesystem_port() {
    for path in [
        "",
        "https://example/a",
        "file:///a",
        "git+ssh:project",
        "C:relative",
    ] {
        assert!(!is_local_parent(path), "{path}");
        let mut port = Port::default();
        assert_eq!(
            run(layer("1.0", &[path]), &mut port).unwrap_err(),
            Error::InvalidParent {
                declaring: "entry".into(),
                path: path.into()
            }
        );
        assert!(port.calls.is_empty());
    }
    for path in [
        "a.yaml",
        "../a.yaml",
        "/a.yaml",
        "C:/a.yaml",
        r"C:\a.yaml",
        "./a:b",
        "1:a",
    ] {
        assert!(is_local_parent(path), "{path}");
    }
}

#[test]
fn source_failure_retains_written_spelling_and_stops_later_access() {
    let mut port = Port {
        unavailable: Some("missing"),
        ..Port::default()
    };
    assert_eq!(
        run(layer("1.0", &["missing", "later"]), &mut port).unwrap_err(),
        Error::Unavailable {
            declaring: "entry".into(),
            path: "missing".into()
        }
    );
    assert_eq!(port.calls, ["resolve:entry:missing"]);
    let mut port = Port::default();
    assert_eq!(
        run(layer("1.0", &["missing", "later"]), &mut port).unwrap_err(),
        Error::Unavailable {
            declaring: "entry".into(),
            path: "written/missing".into()
        }
    );
    assert_eq!(port.calls, ["resolve:entry:missing", "read:missing"]);
    let mut port = Port {
        raised: Some("raise"),
        ..Port::default()
    };
    assert_eq!(
        run(layer("1.0", &["raise", "later"]), &mut port).unwrap_err(),
        Error::Host(("opaque", 73))
    );
    assert_eq!(port.calls, ["resolve:entry:raise"]);
}

#[test]
fn graph_limits_fail_before_disallowed_port_effects() {
    for (limits, resource, calls) in [
        (
            Limits {
                layers: 1,
                ..Limits::default()
            },
            Resource::Layers,
            vec!["resolve:entry:a"],
        ),
        (
            Limits {
                depth: 1,
                ..Limits::default()
            },
            Resource::Depth,
            vec!["resolve:entry:a"],
        ),
        (
            Limits {
                parent_visits: 0,
                ..Limits::default()
            },
            Resource::ParentVisits,
            vec![],
        ),
    ] {
        let mut port = Port::default().with("a", &[]);
        let error = traverse(
            &schema(),
            source("entry"),
            layer("1.0", &["a"]),
            &mut port,
            &mut NormalizationBudget::new(NormalizationLimits::default()),
            &mut Budget::new(limits),
        )
        .unwrap_err();
        assert!(matches!(error,Error::Resource(e) if e.resource==resource));
        assert_eq!(port.calls, calls);
    }
}

#[test]
fn failed_attempts_retain_charges_and_fresh_requests_recover() {
    let mut port = Port::default();
    let mut budget = Budget::new(Limits {
        layers: 1,
        ..Limits::default()
    });
    let mut normalization = NormalizationBudget::new(NormalizationLimits::default());
    assert!(matches!(
        traverse(
            &schema(),
            source("entry"),
            layer("0", &[]),
            &mut port,
            &mut normalization,
            &mut budget
        ),
        Err(Error::Version { .. })
    ));
    assert!(matches!(
        traverse(
            &schema(),
            source("entry"),
            layer("1.0", &[]),
            &mut port,
            &mut normalization,
            &mut budget
        ),
        Err(Error::Resource(Exhausted {
            resource: Resource::Layers,
            ..
        }))
    ));
    assert_eq!(budget.used(Resource::Layers), 2);
    assert!(run(layer("1.0", &[]), &mut port).is_ok());
}

#[test]
fn cumulative_input_budget_includes_all_unique_layers() {
    let first = layer("1.0", &["a"]);
    let mut port = Port::default().with("a", &[]);
    let mut budget = Budget::new(Limits {
        input_nodes: first.nodes().len(),
        ..Limits::default()
    });
    assert!(matches!(
        traverse(
            &schema(),
            source("entry"),
            first,
            &mut port,
            &mut NormalizationBudget::new(NormalizationLimits::default()),
            &mut budget
        ),
        Err(Error::Resource(Exhausted {
            resource: Resource::InputNodes,
            ..
        }))
    ));
    assert_eq!(port.calls, ["resolve:entry:a", "read:a"]);
}
