use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use yamaa_core::schema::{
    BundleLimits, Document, DocumentNode as N, NormalizationError, SchemaModule, SchemaStructure,
};
use yamaa_engine::{
    inheritance as graph, inheritance_capture as app, inheritance_preparation as lifecycle,
    project_preparation::Source,
};
#[derive(Clone)]
enum Tree {
    Text(String),
    Seq(Vec<Tree>),
    Map(Vec<(&'static str, Tree)>),
}
fn text(s: &str) -> Tree {
    Tree::Text(s.into())
}
fn map(v: Vec<(&'static str, Tree)>) -> Tree {
    Tree::Map(v)
}
fn seq(v: Vec<Tree>) -> Tree {
    Tree::Seq(v)
}
fn document(tree: Tree) -> Document {
    fn push(tree: Tree, nodes: &mut Vec<N>) -> usize {
        let node = match tree {
            Tree::Text(s) => N::Text(s),
            Tree::Seq(items) => N::Sequence(items.into_iter().map(|v| push(v, nodes)).collect()),
            Tree::Map(items) => N::Mapping(
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
    let root = push(tree, &mut nodes);
    Document::new(nodes, root, Default::default()).unwrap()
}
fn field(name: &'static str, kind: &str) -> Tree {
    map(vec![(name, map(vec![("type", text(kind))]))])
}
fn schema() -> SchemaStructure {
    SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(map(vec![
                ("version", text("1.0")),
                ("identifier", map(vec![("type", text("str"))])),
                (
                    "root_class",
                    seq(vec![
                        field("schema_version", "str"),
                        map(vec![(
                            "parents",
                            map(vec![("type", seq(vec![text("str"), text("list[str]")]))]),
                        )]),
                        field("domain", "str"),
                        field("keys", "list[str]"),
                        field("input", "dict[str,dataset_class]"),
                        field("output", "output_class"),
                        field("columns", "list[column_class]"),
                    ]),
                ),
                (
                    "dataset_class",
                    seq(vec![field("path", "str"), field("schema", "str")]),
                ),
                (
                    "output_class",
                    seq(vec![
                        field("path", "str"),
                        field("columns", "list[str]"),
                        field("warning_log", "str"),
                        field("verification_log", "str"),
                    ]),
                ),
                (
                    "column_class",
                    seq(vec![
                        field("name", "str"),
                        field("type", "str"),
                        map(vec![(
                            "label",
                            map(vec![
                                ("type", text("str")),
                                ("default", text("Default label")),
                            ]),
                        )]),
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
fn layer(name: &str, parents: &[&str], entry: bool) -> Document {
    let mut fields = vec![
        ("schema_version", text("1.0")),
        ("parents", seq(parents.iter().map(|s| text(s)).collect())),
        (
            "input",
            map(vec![(
                "SRC",
                map(vec![
                    ("path", text(&format!("{name}.csv"))),
                    ("schema", text(&format!("{name}-source.yaml"))),
                ]),
            )]),
        ),
    ];
    if entry {
        fields.extend([
            ("domain", text("TEST")),
            ("keys", seq(vec![text("ID")])),
            (
                "columns",
                seq(vec![map(vec![("name", text("ID")), ("type", text("int"))])]),
            ),
            (
                "output",
                map(vec![
                    ("path", text("output.csv")),
                    ("columns", seq(vec![text("ID")])),
                    ("warning_log", text("warnings.csv")),
                    ("verification_log", text("verification.csv")),
                ]),
            ),
        ]);
    }
    document(map(fields))
}
// No Clone, Debug, Send or Sync: errors are moved, never inspected by this service.
struct Payload {
    identity: Rc<()>,
    allocation: Box<u32>,
}
struct Raw {
    document: Document,
    marker: Box<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Resolve(String, String),
    Capture(String, usize),
    Decode(String, Vec<u8>),
    Rebase(String, String, String, usize),
}
type Log = Rc<RefCell<Vec<Event>>>;
struct Host {
    log: Log,
    bytes: BTreeMap<String, Vec<u8>>,
    aliases: BTreeMap<String, String>,
    failure: Option<(usize, Payload)>,
    unavailable: Option<usize>,
}
impl Host {
    fn failure(&mut self, index: usize) -> Option<Payload> {
        if self.failure.as_ref().is_some_and(|(at, _)| *at == index) {
            self.failure.take().map(|(_, p)| p)
        } else {
            None
        }
    }
}
impl app::CapturePort for Host {
    type Error = Payload;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<graph::Source, graph::SourceError<Payload>> {
        let index = self.log.borrow().len();
        self.log
            .borrow_mut()
            .push(Event::Resolve(declaring.into(), written.into()));
        if let Some(error) = self.failure(index) {
            return Err(graph::SourceError::Raised(error));
        }
        if self.unavailable == Some(index) {
            return Err(graph::SourceError::Unavailable);
        }
        Ok(graph::Source {
            identity: self
                .aliases
                .get(written)
                .cloned()
                .unwrap_or_else(|| written.into()),
            display_path: format!("display/{written}"),
        })
    }
    fn capture(
        &mut self,
        source: &graph::Source,
        maximum: usize,
    ) -> Result<Vec<u8>, graph::SourceError<Payload>> {
        let index = self.log.borrow().len();
        self.log
            .borrow_mut()
            .push(Event::Capture(source.identity.clone(), maximum));
        if let Some(error) = self.failure(index) {
            return Err(graph::SourceError::Raised(error));
        }
        if self.unavailable == Some(index) {
            return Err(graph::SourceError::Unavailable);
        }
        Ok(self
            .bytes
            .remove(&source.identity)
            .expect("only one capture per identity"))
    }
    fn rebase(
        &mut self,
        layer: &graph::Source,
        entry: &graph::Source,
        written: &str,
        maximum: usize,
    ) -> Result<String, Payload> {
        let index = self.log.borrow().len();
        self.log.borrow_mut().push(Event::Rebase(
            layer.identity.clone(),
            entry.identity.clone(),
            written.into(),
            maximum,
        ));
        if let Some(error) = self.failure(index) {
            return Err(error);
        }
        Ok(written.into())
    }
}
enum CodecFailure {
    Original(Payload),
    Limit(app::CaptureResource),
}
struct Decoder {
    log: Log,
    documents: BTreeMap<String, Raw>,
    failure: Option<Payload>,
}
impl app::DecodePort for Decoder {
    type Raw = Raw;
    type Error = CodecFailure;
    fn decode(&mut self, identity: &str, bytes: &[u8]) -> Result<Raw, CodecFailure> {
        self.log
            .borrow_mut()
            .push(Event::Decode(identity.into(), bytes.to_vec()));
        if let Some(error) = self.failure.take() {
            return Err(CodecFailure::Original(error));
        }
        Ok(self
            .documents
            .remove(identity)
            .expect("entry is never decoded again"))
    }
    fn document(raw: &Raw) -> &Document {
        &raw.document
    }
    fn limit_error(resource: app::CaptureResource) -> CodecFailure {
        CodecFailure::Limit(resource)
    }
}
fn fixture() -> (Source, Raw, Host, Decoder) {
    let log = Rc::new(RefCell::new(vec![]));
    let host = Host {
        log: log.clone(),
        bytes: [("a", vec![1, 2]), ("common", vec![3]), ("b", vec![4, 5, 6])]
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect(),
        aliases: [("alias".into(), "common".into())].into_iter().collect(),
        failure: None,
        unavailable: None,
    };
    let decoder = Decoder {
        log,
        documents: [
            ("a", layer("a", &["common"], false)),
            ("common", layer("common", &[], false)),
            ("b", layer("b", &["alias"], false)),
        ]
        .into_iter()
        .map(|(k, d)| {
            (
                k.into(),
                Raw {
                    document: d,
                    marker: Box::new(73),
                },
            )
        })
        .collect(),
        failure: None,
    };
    (
        Source {
            identity: "entry".into(),
            bytes: vec![9, 8],
        },
        Raw {
            document: layer("entry", &["a", "b"], true),
            marker: Box::new(71),
        },
        host,
        decoder,
    )
}
fn run(
    source: &Source,
    raw: &Raw,
    host: &mut Host,
    decoder: &mut Decoder,
    limits: app::Limits,
) -> app::PreparationResult<Payload, CodecFailure, Raw> {
    app::prepare(
        &schema(),
        source,
        "written/entry".into(),
        raw,
        host,
        decoder,
        limits,
    )
}
fn expected_effects() -> Vec<Event> {
    let mut events = vec![
        Event::Resolve("entry".into(), "a".into()),
        Event::Capture("a".into(), 16_777_214),
        Event::Decode("a".into(), vec![1, 2]),
        Event::Resolve("a".into(), "common".into()),
        Event::Capture("common".into(), 16_777_212),
        Event::Decode("common".into(), vec![3]),
        Event::Resolve("entry".into(), "b".into()),
        Event::Capture("b".into(), 16_777_211),
        Event::Decode("b".into(), vec![4, 5, 6]),
        Event::Resolve("b".into(), "alias".into()),
    ];
    let mut remaining = 1_048_576;
    for (layer, paths) in [
        ("common", vec!["common.csv", "common-source.yaml"]),
        ("a", vec!["a.csv", "a-source.yaml"]),
        ("b", vec!["b.csv", "b-source.yaml"]),
        (
            "entry",
            vec![
                "entry.csv",
                "entry-source.yaml",
                "output.csv",
                "warnings.csv",
                "verification.csv",
            ],
        ),
    ] {
        for path in paths {
            events.push(Event::Rebase(
                layer.into(),
                "entry".into(),
                path.into(),
                remaining,
            ));
            remaining -= path.len();
        }
    }
    events
}
#[test]
fn complete_effects_catalog_allocations_defaults_and_distinct_orders_are_engine_owned() {
    for _ in 0..2 {
        let (source, raw, mut host, mut decoder) = fixture();
        let held: Vec<_> = ["a", "common", "b"]
            .into_iter()
            .map(|name| {
                let d = &decoder.documents[name];
                (
                    name,
                    host.bytes[name].as_ptr(),
                    d.document.nodes().as_ptr(),
                    (&*d.marker) as *const u32,
                )
            })
            .collect();
        let prepared = match run(&source, &raw, &mut host, &mut decoder, Default::default()) {
            Ok(p) => p,
            Err(app::Error::Preparation { error, .. }) => match *error {
                lifecycle::Error::Traversal(graph::Error::Layer { error, .. }) => {
                    panic!("fixture layer: {error:?}")
                }
                _ => panic!("fixture later phase"),
            },
            Err(_) => panic!("fixture entry"),
        };
        assert_eq!(*host.log.borrow(), expected_effects());
        assert_eq!(
            prepared
                .parents
                .iter()
                .map(|p| p.source().identity.as_str())
                .collect::<Vec<_>>(),
            ["a", "common", "b"]
        );
        for (parent, (name, bytes, nodes, marker)) in prepared.parents.iter().zip(held) {
            assert_eq!(parent.source().identity, name);
            assert_eq!(parent.source().bytes.as_ptr(), bytes);
            assert_eq!(parent.raw().document.nodes().as_ptr(), nodes);
            assert_eq!((&*parent.raw().marker) as *const u32, marker);
        }
        assert_eq!(
            prepared
                .inheritance
                .layers()
                .iter()
                .map(|p| p.source.identity.as_str())
                .collect::<Vec<_>>(),
            ["common", "a", "b", "entry"]
        );
        let d = prepared.inheritance.model().document();
        let columns = d.field(d.root(), "columns").unwrap();
        let N::Sequence(columns) = &d.nodes()[columns] else {
            panic!()
        };
        let label = d.field(columns[0], "label").unwrap();
        assert_eq!(d.nodes()[label], N::Text("Default label".into()));
        let origin = prepared.inheritance.origins()[label];
        assert!(matches!(
            origin.source,
            yamaa_core::schema::SchemaSource::Default { module: 0, .. }
        ));
        assert_eq!(prepared.inheritance.origins().len(), d.nodes().len());
        assert_eq!(source.bytes, [9, 8]);
        assert_eq!(*raw.marker, 71);
    }
}
#[test]
fn raw_capture_boundaries_have_exact_equality_and_no_disallowed_decoder_effects() {
    for (bytes, identities, expected_capture, expected_decode, resource) in [
        (1, 65_536, 0, 0, app::CaptureResource::CapturedBytes),
        (8, 4, 0, 0, app::CaptureResource::IdentityBytes),
        (8, 5, 0, 0, app::CaptureResource::IdentityBytes),
        (3, 65_536, 1, 0, app::CaptureResource::CapturedBytes),
        (8, 12, 2, 2, app::CaptureResource::IdentityBytes),
    ] {
        let (source, raw, mut host, mut decoder) = fixture();
        let result = run(
            &source,
            &raw,
            &mut host,
            &mut decoder,
            app::Limits {
                captured_bytes: bytes,
                identity_bytes: identities,
                ..Default::default()
            },
        );
        let actual = match result {
            Err(app::Error::Entry(CodecFailure::Limit(r))) => r,
            Err(app::Error::Preparation { error, .. }) => match *error {
                lifecycle::Error::Traversal(graph::Error::Host(app::SourceFailure::Admission(
                    CodecFailure::Limit(r),
                ))) => r,
                _ => panic!("expected capture limit"),
            },
            _ => panic!("expected refusal"),
        };
        assert_eq!(actual, resource);
        let events = host.log.borrow();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Capture(..)))
                .count(),
            expected_capture
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Decode(..)))
                .count(),
            expected_decode
        );
        assert!(!events.iter().any(|e| matches!(e, Event::Rebase(..))));
    }
    let (source, raw, mut host, mut decoder) = fixture();
    assert!(run(
        &source,
        &raw,
        &mut host,
        &mut decoder,
        app::Limits {
            captured_bytes: 8,
            identity_bytes: 13,
            ..Default::default()
        }
    )
    .is_ok());
}
#[test]
fn every_opaque_effect_error_keeps_original_allocation_and_exact_completed_catalog_prefix() {
    for index in [0, 1, 3, 4, 6, 7, 9, 10, 20] {
        let (source, raw, mut host, mut decoder) = fixture();
        let identity = Rc::new(());
        let allocation = Box::new(99);
        let pointer = (&*allocation) as *const u32;
        host.failure = Some((
            index,
            Payload {
                identity: identity.clone(),
                allocation,
            },
        ));
        let Err(app::Error::Preparation {
            error,
            captured_parents,
        }) = run(&source, &raw, &mut host, &mut decoder, Default::default())
        else {
            panic!()
        };
        let original = match *error {
            lifecycle::Error::Traversal(graph::Error::Host(app::SourceFailure::Host(p)))
            | lifecycle::Error::Path(app::SourceFailure::Host(p)) => p,
            _ => panic!(),
        };
        assert!(Rc::ptr_eq(&original.identity, &identity));
        assert_eq!((&*original.allocation) as *const u32, pointer);
        assert_eq!(&*host.log.borrow(), &expected_effects()[..=index]);
        let count = expected_effects()[..index]
            .iter()
            .filter(|e| matches!(e, Event::Decode(..)))
            .count();
        assert_eq!(captured_parents.len(), count);
    }
}
#[test]
fn original_decoder_failure_and_unavailable_roles_stop_before_later_authority() {
    let (source, raw, mut host, mut decoder) = fixture();
    let identity = Rc::new(());
    let allocation = Box::new(99);
    let pointer = (&*allocation) as *const u32;
    decoder.failure = Some(Payload {
        identity: identity.clone(),
        allocation,
    });
    let Err(app::Error::Preparation {
        error,
        captured_parents,
    }) = run(&source, &raw, &mut host, &mut decoder, Default::default())
    else {
        panic!()
    };
    let lifecycle::Error::Traversal(graph::Error::Host(app::SourceFailure::Admission(
        CodecFailure::Original(original),
    ))) = *error
    else {
        panic!()
    };
    assert!(Rc::ptr_eq(&original.identity, &identity));
    assert_eq!((&*original.allocation) as *const u32, pointer);
    assert!(captured_parents.is_empty());
    assert_eq!(&*host.log.borrow(), &expected_effects()[..3]);
    for index in [0, 1] {
        let (source, raw, mut host, mut decoder) = fixture();
        host.unavailable = Some(index);
        let Err(app::Error::Preparation {
            error,
            captured_parents,
        }) = run(&source, &raw, &mut host, &mut decoder, Default::default())
        else {
            panic!()
        };
        let lifecycle::Error::Traversal(graph::Error::Unavailable { declaring, path }) = *error
        else {
            panic!()
        };
        assert_eq!(declaring, "entry");
        assert_eq!(path, if index == 0 { "a" } else { "display/a" });
        assert!(captured_parents.is_empty());
        assert_eq!(&*host.log.borrow(), &expected_effects()[..=index]);
    }
}
#[test]
fn admitted_parent_raw_is_retained_when_later_normalization_fails_before_descendants() {
    let (source, raw, mut host, mut decoder) = fixture();
    let bad = document(map(vec![
        ("schema_version", text("1.0")),
        ("unknown", text("kept")),
        ("parents", seq(vec![text("not-read")])),
    ]));
    let nodes = bad.nodes().as_ptr();
    decoder.documents.get_mut("a").unwrap().document = bad;
    let Err(app::Error::Preparation {
        error,
        captured_parents,
    }) = run(&source, &raw, &mut host, &mut decoder, Default::default())
    else {
        panic!()
    };
    let lifecycle::Error::Traversal(graph::Error::Layer {
        source,
        entry,
        error: NormalizationError::Invalid(findings),
        ..
    }) = *error
    else {
        panic!()
    };
    assert_eq!((source.as_str(), entry.as_str()), ("a", "entry"));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].definition().condition, "unknown_field");
    assert_eq!(captured_parents.len(), 1);
    assert_eq!(captured_parents[0].raw().document.nodes().as_ptr(), nodes);
    assert_eq!(&*host.log.borrow(), &expected_effects()[..3]);
}
#[test]
fn traversal_and_rebase_budgets_remain_separate_from_raw_capture_policies() {
    let (source, raw, mut host, mut decoder) = fixture();
    let Err(app::Error::Preparation {
        error,
        captured_parents,
    }) = run(
        &source,
        &raw,
        &mut host,
        &mut decoder,
        app::Limits {
            preparation: lifecycle::Limits {
                traversal: graph::Limits {
                    layers: 1,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        },
    )
    else {
        panic!()
    };
    assert!(matches!(
        *error,
        lifecycle::Error::Traversal(graph::Error::Resource(graph::Exhausted {
            resource: graph::Resource::Layers,
            limit: 1
        }))
    ));
    assert!(captured_parents.is_empty());
    assert_eq!(&*host.log.borrow(), &expected_effects()[..1]);
    let (source, raw, mut host, mut decoder) = fixture();
    let Err(app::Error::Preparation {
        error,
        captured_parents,
    }) = run(
        &source,
        &raw,
        &mut host,
        &mut decoder,
        app::Limits {
            preparation: lifecycle::Limits {
                rebased_bytes: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    else {
        panic!()
    };
    assert!(matches!(*error, lifecycle::Error::PathBytes));
    assert_eq!(captured_parents.len(), 3);
    let events = host.log.borrow();
    assert_eq!(&events[..10], &expected_effects()[..10]);
    assert_eq!(
        events[10],
        Event::Rebase("common".into(), "entry".into(), "common.csv".into(), 0)
    );
}
