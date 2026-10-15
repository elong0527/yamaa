//! Independent raw logical arenas and opaque payloads qualify the dispatch service.
use std::{collections::BTreeMap, rc::Rc};
use yamaa_core::schema::{
    Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
    SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure,
};
use yamaa_core::{
    document_preparation as app, inheritance as graph, inheritance_capture as capture,
    inheritance_preparation as lifecycle, project_preparation::Source,
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
    Document::new(nodes, root).unwrap()
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

struct Payload {
    allocation: Box<u32>,
    marker: Rc<()>,
}
struct Raw {
    document: Document,
    locations: Box<u32>,
}
enum Failure {
    Original(Payload),
}
struct Entry {
    raw: Option<Raw>,
    error: Option<Payload>,
    calls: usize,
}
impl app::EntryCodec for Entry {
    type Raw = Raw;
    type Locations = Box<u32>;
    type Error = Failure;
    fn decode(&mut self, source: &Source) -> Result<Raw, Failure> {
        assert_eq!(source.identity, "entry");
        assert_eq!(source.bytes, b"held root");
        self.calls += 1;
        if let Some(error) = self.error.take() {
            return Err(Failure::Original(error));
        }
        Ok(self.raw.take().unwrap())
    }
    fn document(raw: &Raw) -> &Document {
        &raw.document
    }
    fn into_parts(raw: Raw) -> (Document, Box<u32>) {
        (raw.document, raw.locations)
    }
}
struct Parents {
    documents: BTreeMap<String, Raw>,
    calls: Vec<String>,
}
impl capture::DecodePort for Parents {
    type Raw = Raw;
    type Error = Failure;
    fn decode(&mut self, identity: &str, _: &[u8]) -> Result<Raw, Failure> {
        self.calls.push(identity.into());
        Ok(self.documents.remove(identity).unwrap())
    }
    fn document(raw: &Raw) -> &Document {
        &raw.document
    }
}
struct Host {
    calls: Vec<String>,
    bytes: BTreeMap<String, Vec<u8>>,
    error: Option<Payload>,
}
impl capture::CapturePort for Host {
    type Error = Payload;
    fn canonicalize(
        &mut self,
        _: &str,
        written: &str,
    ) -> Result<graph::Source, graph::SourceError<Payload>> {
        self.calls.push(format!("canonical:{written}"));
        if let Some(error) = self.error.take() {
            return Err(graph::SourceError::Raised(error));
        }
        Ok(graph::Source {
            identity: written.into(),
            display_path: format!("display/{written}"),
        })
    }
    fn capture(&mut self, source: &graph::Source) -> Result<Vec<u8>, graph::SourceError<Payload>> {
        self.calls.push(format!("capture:{}", source.identity));
        Ok(self.bytes.remove(&source.identity).unwrap())
    }
    fn rebase(
        &mut self,
        _: &graph::Source,
        _: &graph::Source,
        written: &str,
    ) -> Result<String, Payload> {
        self.calls.push(format!("rebase:{written}"));
        Ok(written.into())
    }
}
fn ports(document: Document) -> (Entry, Parents, Host) {
    (
        Entry {
            raw: Some(Raw {
                document,
                locations: Box::new(17),
            }),
            error: None,
            calls: 0,
        },
        Parents {
            documents: BTreeMap::new(),
            calls: vec![],
        },
        Host {
            calls: vec![],
            bytes: BTreeMap::new(),
            error: None,
        },
    )
}
fn source() -> Source {
    Source {
        identity: "entry".into(),
        bytes: b"held root".to_vec(),
    }
}
fn run(
    entry: &mut Entry,
    parents: &mut Parents,
    host: &mut Host,
) -> app::PreparationResult<Payload, Failure, Raw, Box<u32>> {
    app::prepare(
        &schema(),
        source(),
        "display/entry".into(),
        entry,
        parents,
        host,
    )
}
fn standalone() -> Document {
    document(map(vec![
        ("schema_version", text("1.0")),
        ("domain", text("TEST")),
        ("input", map(vec![])),
        ("keys", seq(vec![text("ID")])),
        (
            "output",
            map(vec![
                ("path", text("output.csv")),
                ("columns", seq(vec![text("ID")])),
                ("warning_log", text("warnings.csv")),
                ("verification_log", text("verification.csv")),
            ]),
        ),
        (
            "columns",
            seq(vec![map(vec![("name", text("ID")), ("type", text("int"))])]),
        ),
    ]))
}
fn finding(
    path: &str,
    condition: &'static str,
    requirement: Option<&'static str>,
    context: Vec<(&'static str, C)>,
) -> SchemaDiagnostic {
    SchemaDiagnostic {
        path: path.into(),
        code: match (condition, requirement) {
            ("schema_version_mismatch", None) => {
                yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch
            }
            ("invalid_field_type", Some("REQ-0287")) => {
                yamaa_core::diagnostic::ConditionCode::SchemaInvalidFieldType
            }
            _ => panic!("unexpected test cause"),
        },
        context,
    }
}
#[test]
fn standalone_success_moves_source_raw_and_opaque_locations_without_parent_effects() {
    let raw = standalone();
    let pointer = raw.nodes().as_ptr();
    let expected = raw.clone();
    let (mut entry, mut parents, mut host) = ports(raw);
    let locations = &*entry.raw.as_ref().unwrap().locations as *const u32;
    let source = source();
    let bytes = source.bytes.as_ptr();
    let identity = source.identity.as_ptr();
    let result = app::prepare(
        &schema(),
        source,
        "unused".into(),
        &mut entry,
        &mut parents,
        &mut host,
    );
    let Ok(prepared) = result else {
        panic!("standalone success")
    };
    assert_eq!(prepared.source().bytes.as_ptr(), bytes);
    assert_eq!(prepared.source().identity.as_ptr(), identity);
    let app::Content::Standalone {
        prepared,
        locations: held,
    } = prepared.content()
    else {
        panic!()
    };
    assert_eq!(prepared.raw().nodes().as_ptr(), pointer);
    assert_eq!(prepared.raw(), &expected);
    assert_eq!(&**held as *const u32, locations);
    assert_model(prepared.model().document());
    assert_eq!(prepared.windows(), &[]);
    assert_eq!(entry.calls, 1);
    assert!(parents.calls.is_empty());
    assert!(host.calls.is_empty());
    assert_eq!(
        prepared.origins().len(),
        prepared.model().document().nodes().len()
    );
    let schema = schema();
    let descriptor = schema
        .class_named("column_class")
        .unwrap()
        .fields
        .iter()
        .find(|field| field.name == "label")
        .unwrap()
        .descriptor;
    let default = schema.descriptors()[descriptor]
        .descriptor
        .default_node()
        .unwrap();
    let mut expected_origins = vec![None; prepared.model().document().nodes().len()];
    fn copied(
        model: &Document,
        id: usize,
        raw: &Document,
        original: usize,
        descriptor: usize,
        default: usize,
        expected: &mut [Option<SchemaOrigin>],
    ) {
        expected[id] = Some(SchemaOrigin {
            source: SchemaSource::Input,
            node: original,
            generated: false,
        });
        match (&model.nodes()[id], &raw.nodes()[original]) {
            (N::Sequence(items), N::Sequence(source)) => {
                for (&id, &original) in items.iter().zip(source) {
                    copied(model, id, raw, original, descriptor, default, expected)
                }
            }
            (N::Mapping(items), N::Mapping(source)) => {
                for &(key, value) in items {
                    if model.nodes()[key] == N::Text("label".into()) {
                        expected[key] = Some(SchemaOrigin {
                            source: SchemaSource::Default {
                                module: 0,
                                descriptor,
                            },
                            node: default,
                            generated: true,
                        });
                        expected[value] = Some(SchemaOrigin {
                            source: SchemaSource::Default {
                                module: 0,
                                descriptor,
                            },
                            node: default,
                            generated: false,
                        });
                    } else {
                        let &(original_key, original_value) = source
                            .iter()
                            .find(|&&(k, _)| raw.nodes()[k] == model.nodes()[key])
                            .unwrap();
                        copied(model, key, raw, original_key, descriptor, default, expected);
                        copied(
                            model,
                            value,
                            raw,
                            original_value,
                            descriptor,
                            default,
                            expected,
                        );
                    }
                }
            }
            (actual, original) => assert_eq!(actual, original),
        }
    }
    let model = prepared.model().document();
    copied(
        model,
        model.root(),
        &expected,
        expected.root(),
        descriptor,
        default,
        &mut expected_origins,
    );
    assert_eq!(
        prepared.origins(),
        expected_origins
            .into_iter()
            .map(Option::unwrap)
            .collect::<Vec<_>>()
    );
}
#[test]
fn inherited_success_keeps_entry_parent_allocations_and_distinct_catalog_order() {
    let (mut entry, mut parents, mut host) = ports(layer("entry", &["a", "b"], true));
    let raw = entry.raw.as_ref().unwrap().document.nodes().as_ptr();
    let locations = &*entry.raw.as_ref().unwrap().locations as *const u32;
    let mut held = vec![];
    for (name, ps) in [
        ("a", vec!["common"]),
        ("common", vec![]),
        ("b", vec!["common"]),
    ] {
        let document = layer(name, &ps, false);
        let bytes = name.as_bytes().to_vec();
        held.push((name, document.nodes().as_ptr(), bytes.as_ptr()));
        parents.documents.insert(
            name.into(),
            Raw {
                document,
                locations: Box::new(21),
            },
        );
        host.bytes.insert(name.into(), bytes);
    }
    let Ok(prepared) = run(&mut entry, &mut parents, &mut host) else {
        panic!()
    };
    let app::Content::Inherited {
        raw: actual,
        prepared,
    } = prepared.content()
    else {
        panic!()
    };
    assert_eq!(actual.document.nodes().as_ptr(), raw);
    assert_eq!(&*actual.locations as *const u32, locations);
    assert_eq!(
        prepared
            .parents
            .iter()
            .map(|p| p.source().identity.as_str())
            .collect::<Vec<_>>(),
        ["a", "common", "b"]
    );
    assert_eq!(
        prepared
            .inheritance
            .layers()
            .iter()
            .map(|l| l.source.identity.as_str())
            .collect::<Vec<_>>(),
        ["common", "a", "b", "entry"]
    );
    for (parent, (name, nodes, bytes)) in prepared.parents.iter().zip(held) {
        assert_eq!(parent.source().identity, name);
        assert_eq!(parent.raw().document.nodes().as_ptr(), nodes);
        assert_eq!(parent.source().bytes.as_ptr(), bytes);
    }
    assert_eq!(entry.calls, 1);
    assert_eq!(parents.calls, ["a", "common", "b"]);
    assert_model(prepared.inheritance.model().document());
}
#[test]
fn raw_presence_selects_inheritance_even_with_null_scalar_or_empty_list() {
    for value in [N::Null, N::Integer("7".into()), N::Sequence(vec![])] {
        let document = layer("entry", &[], true);
        let parent = document.field(document.root(), "parents").unwrap();
        let mut nodes = document.nodes().to_vec();
        nodes[parent] = value.clone();
        let (mut entry, mut parents, mut host) =
            ports(Document::new(nodes, document.root()).unwrap());
        let result = run(&mut entry, &mut parents, &mut host);
        if matches!(value, N::Sequence(_)) {
            assert!(matches!(result, Ok(app::Prepared { .. })));
        } else {
            assert!(matches!(
                result,
                Err(app::Error::Inherited(capture::Error::Preparation { .. }))
            ));
        }
        assert_eq!(entry.calls, 1);
        assert!(!host
            .calls
            .iter()
            .any(|c| c.starts_with("canonical:") || c.starts_with("capture:")));
        assert!(parents.calls.is_empty());
    }
}
#[test]
fn independent_full_version_findings_distinguish_route_checkpoints_and_hold_raw_arena() {
    for inherited in [false, true] {
        let document = if inherited {
            layer("entry", &["never"], true)
        } else {
            standalone()
        };
        let version = document.field(document.root(), "schema_version").unwrap();
        let mut nodes = document.nodes().to_vec();
        nodes[version] = N::Text("99.0".into());
        let raw = Document::new(nodes, document.root()).unwrap();
        let pointer = raw.nodes().as_ptr();
        let (mut entry, mut parents, mut host) = ports(raw);
        match run(&mut entry, &mut parents, &mut host) {
            Err(app::Error::Standalone {
                error: yamaa_core::standalone_preparation::Error::Normalize(failure),
                ..
            }) if !inherited => {
                assert_eq!(failure.input.unwrap().nodes().as_ptr(), pointer);
                assert_eq!(
                    failure.error,
                    NormalizationError::Invalid(vec![finding(
                        "schema_version",
                        "schema_version_mismatch",
                        None,
                        vec![
                            ("expected", C::Text("1.0".into())),
                            ("actual", C::InputValue(version))
                        ]
                    )])
                );
            }
            Err(app::Error::Inherited(capture::Error::Preparation {
                error,
                captured_parents,
            })) if inherited => {
                assert!(captured_parents.is_empty());
                let lifecycle::Error::Traversal(graph::Error::Version {
                    source,
                    entry: held,
                    expected,
                    actual,
                    is_entry,
                }) = *error
                else {
                    panic!("inherited version category")
                };
                assert_eq!(
                    (source, held, expected, actual, is_entry),
                    (
                        "entry".into(),
                        "entry".into(),
                        "1.0".into(),
                        "99.0".into(),
                        true
                    )
                );
            }
            _ => panic!("version route"),
        }
        assert_eq!(entry.calls, 1);
        assert!(host.calls.is_empty());
    }
}
#[test]
fn entry_opaque_failure_moves_original_allocation_before_semantic_work() {
    let (mut entry, mut parents, mut host) = ports(standalone());
    let payload = Payload {
        allocation: Box::new(99),
        marker: Rc::new(()),
    };
    let pointer = &*payload.allocation as *const u32;
    let marker = Rc::as_ptr(&payload.marker);
    entry.error = Some(payload);

    let Err(app::Error::Entry(Failure::Original(error))) = run(&mut entry, &mut parents, &mut host)
    else {
        panic!()
    };
    assert_eq!(&*error.allocation as *const u32, pointer);
    assert_eq!(Rc::as_ptr(&error.marker), marker);
    assert_eq!(entry.calls, 1);
    assert!(host.calls.is_empty());
    assert!(entry.raw.is_some());
}

#[derive(Debug, PartialEq)]
enum Logical {
    Text(String),
    Seq(Vec<Logical>),
    Map(BTreeMap<String, Logical>),
}
fn logical(doc: &Document, id: usize) -> Logical {
    match &doc.nodes()[id] {
        N::Text(text) => Logical::Text(text.clone()),
        N::Sequence(items) => Logical::Seq(items.iter().map(|&id| logical(doc, id)).collect()),
        N::Mapping(items) => Logical::Map(
            items
                .iter()
                .map(|&(k, v)| {
                    let N::Text(key) = &doc.nodes()[k] else {
                        panic!()
                    };
                    (key.clone(), logical(doc, v))
                })
                .collect(),
        ),
        _ => panic!("oracle scalar"),
    }
}
fn assert_model(actual: &Document) {
    // Authored independently: parents disappear and only column label defaults.
    let expected = document(map(vec![
        ("schema_version", text("1.0")),
        ("domain", text("TEST")),
        ("keys", seq(vec![text("ID")])),
        ("input", map(vec![])),
        (
            "output",
            map(vec![
                ("path", text("output.csv")),
                ("columns", seq(vec![text("ID")])),
                ("warning_log", text("warnings.csv")),
                ("verification_log", text("verification.csv")),
            ]),
        ),
        (
            "columns",
            seq(vec![map(vec![
                ("name", text("ID")),
                ("type", text("int")),
                ("label", text("Default label")),
            ])]),
        ),
    ]));
    assert_eq!(
        logical(actual, actual.root()),
        logical(&expected, expected.root())
    );
}

#[test]
fn nonmapping_raw_root_remains_a_standalone_raw_arena_finding_without_capture() {
    let raw = Document::new(vec![N::Integer("7".into())], 0).unwrap();
    let pointer = raw.nodes().as_ptr();
    let (mut entry, mut parents, mut host) = ports(raw);
    let Err(app::Error::Standalone {
        error: yamaa_core::standalone_preparation::Error::Normalize(failure),
        ..
    }) = run(&mut entry, &mut parents, &mut host)
    else {
        panic!()
    };
    assert_eq!(failure.input.unwrap().nodes().as_ptr(), pointer);
    assert_eq!(
        failure.error,
        NormalizationError::Invalid(vec![finding(
            "$",
            "invalid_field_type",
            Some("REQ-0287"),
            vec![
                ("expected", C::Text("root_class".into())),
                (
                    "actual",
                    C::ValueKind(yamaa_core::schema::DocumentKind::Integer)
                )
            ]
        )])
    );
    assert_eq!(entry.calls, 1);
    assert!(host.calls.is_empty());
}
