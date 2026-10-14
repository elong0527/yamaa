use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use yamaa_core::schema::{
    BundleError, BundleIssue, BundleIssueKind, BundleLimits, Document, DocumentLimits,
    DocumentNode as N, NormalizationBudget, NormalizationError, SchemaOrigin, SchemaSource,
};
use yamaa_engine::{
    project_preparation::Source,
    schema_capture::{
        self, CapturedClosure, DecodePort, DecodedModule, Error, Limits, Resource, RootSelection,
    },
};

enum Tree {
    Text(String),
    Bool(bool),
    List(Vec<Tree>),
    Map(Vec<(String, Tree)>),
}
fn text(s: &str) -> Tree {
    Tree::Text(s.into())
}
fn map(items: Vec<(&str, Tree)>) -> Tree {
    Tree::Map(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn document(tree: Tree) -> Document {
    fn push(t: Tree, nodes: &mut Vec<N>) -> usize {
        let node = match t {
            Tree::Text(s) => N::Text(s),
            Tree::Bool(b) => N::Boolean(b),
            Tree::List(v) => N::Sequence(v.into_iter().map(|x| push(x, nodes)).collect()),
            Tree::Map(v) => N::Mapping(
                v.into_iter()
                    .map(|(k, v)| (push(Tree::Text(k), nodes), push(v, nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    let mut nodes = vec![];
    let root = push(tree, &mut nodes);
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}
fn module(declarations: Vec<(&str, Tree)>, includes: &[&str]) -> Document {
    let mut fields = vec![
        ("version", text("1.0")),
        (
            "includes",
            Tree::List(includes.iter().map(|s| text(s)).collect()),
        ),
    ];
    fields.extend(declarations);
    document(map(fields))
}
fn descriptor(kind: &str) -> Tree {
    map(vec![("type", text(kind))])
}
fn fixtures() -> (Vec<Source>, Decoder) {
    let shared = module(
        vec![(
            "shared",
            Tree::List(vec![
                map(vec![(
                    "id",
                    map(vec![("type", text("str")), ("required", Tree::Bool(true))]),
                )]),
                map(vec![(
                    "label",
                    map(vec![("type", text("str")), ("default", text("untitled"))]),
                )]),
            ]),
        )],
        &[],
    );
    let root = module(
        vec![(
            "root_class",
            Tree::List(vec![
                map(vec![("fields_from", text("shared"))]),
                map(vec![("schema_version", descriptor("str"))]),
            ]),
        )],
        &["schema_s.yaml"],
    );
    (
        vec![source("schema_s.yaml", vec![10, 11]), source("r", vec![12])],
        Decoder::new(vec![shared, root]),
    )
}
fn source(identity: &str, bytes: Vec<u8>) -> Source {
    Source {
        identity: identity.into(),
        bytes,
    }
}
struct Payload {
    allocation: Box<u32>,
    _native: Rc<Cell<usize>>,
}
struct Locations {
    positions: Vec<(usize, usize)>,
    _native: Rc<Cell<usize>>,
}
type Trace = Rc<RefCell<Vec<(String, Vec<u8>)>>>;
struct Decoder {
    documents: VecDeque<Document>,
    locations: VecDeque<Locations>,
    trace: Trace,
    fail_at: Option<usize>,
    failure: Option<Payload>,
}
impl Decoder {
    fn new(documents: Vec<Document>) -> Self {
        let locations = (0..documents.len())
            .map(|i| Locations {
                positions: vec![(i + 1, 7)],
                _native: Rc::new(Cell::new(i)),
            })
            .collect();
        Self {
            documents: documents.into(),
            locations,
            trace: Rc::new(RefCell::new(vec![])),
            fail_at: None,
            failure: None,
        }
    }
}
impl DecodePort for Decoder {
    type Error = Payload;
    type Locations = Locations;
    fn decode(
        &mut self,
        identity: &str,
        bytes: &[u8],
    ) -> Result<DecodedModule<Locations>, Payload> {
        self.trace
            .borrow_mut()
            .push((identity.into(), bytes.to_vec()));
        if self.fail_at == Some(self.trace.borrow().len()) {
            return Err(self.failure.take().unwrap());
        }
        Ok(DecodedModule {
            document: self.documents.pop_front().unwrap(),
            locations: self.locations.pop_front().unwrap(),
        })
    }
}
fn accepted(
    result: Result<CapturedClosure<Locations>, Error<Payload>>,
) -> CapturedClosure<Locations> {
    match result {
        Ok(result) => result,
        Err(Error::Bundle(e)) => panic!("unexpected core bundle failure: {e:?}"),
        Err(_) => panic!("unexpected capture refusal"),
    }
}
#[test]
fn complete_source_decode_order_allocations_descriptor_defaults_and_origins_survive() {
    for _ in 0..2 {
        let (sources, mut decoder) = fixtures();
        let source_arena = sources.as_ptr();
        let identities = sources
            .iter()
            .map(|s| s.identity.as_ptr())
            .collect::<Vec<_>>();
        let bytes = sources.iter().map(|s| s.bytes.as_ptr()).collect::<Vec<_>>();
        let arenas = decoder
            .documents
            .iter()
            .map(|d| d.nodes().as_ptr())
            .collect::<Vec<_>>();
        let position_pointers = decoder
            .locations
            .iter()
            .map(|l| l.positions.as_ptr())
            .collect::<Vec<_>>();
        let captured = accepted(schema_capture::admit(
            sources,
            1,
            RootSelection::Default,
            Limits::default(),
            &mut decoder,
        ));
        assert_eq!(
            *decoder.trace.borrow(),
            [
                ("schema_s.yaml".into(), vec![10, 11]),
                ("r".into(), vec![12])
            ]
        );
        assert_eq!(captured.sources().as_ptr(), source_arena);
        for i in 0..2 {
            assert_eq!(captured.sources()[i].identity.as_ptr(), identities[i]);
            assert_eq!(captured.sources()[i].bytes.as_ptr(), bytes[i]);
            assert_eq!(
                captured.structure().modules()[i].document.nodes().as_ptr(),
                arenas[i]
            );
            assert_eq!(
                captured.locations()[i].positions.as_ptr(),
                position_pointers[i]
            );
            assert_eq!(captured.locations()[i].positions, [(i + 1, 7)]);
        }
        let schema = captured.structure();
        assert_eq!(schema.version(), "1.0");
        assert_eq!(
            schema
                .root_class()
                .fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "label", "schema_version"]
        );
        let label = schema.root_class().fields[1].descriptor;
        let origin = &schema.descriptors()[label];
        assert_eq!(
            (origin.module, origin.path.as_str()),
            (0, "root_class.label")
        );
        let default = origin.descriptor.default_node().unwrap();
        assert_eq!(
            schema.modules()[0].document.nodes()[default],
            N::Text("untitled".into())
        );
        let input = document(map(vec![
            ("schema_version", text("1.0")),
            ("id", text("ID01")),
        ]));
        let normalized = schema
            .normalize_document(&input, &mut NormalizationBudget::new(Default::default()))
            .unwrap();
        let value = normalized
            .document
            .field(normalized.document.root(), "label")
            .unwrap();
        assert_eq!(
            normalized.document.nodes()[value],
            N::Text("untitled".into())
        );
        assert_eq!(
            normalized.origins[value],
            SchemaOrigin {
                source: SchemaSource::Default {
                    module: 0,
                    descriptor: label
                },
                node: default,
                generated: false
            }
        );
        let (sources, locations, structure) = captured.into_parts();
        assert_eq!(sources.as_ptr(), source_arena);
        assert_eq!(locations[0].positions.as_ptr(), position_pointers[0]);
        assert_eq!(structure.modules()[0].document.nodes().as_ptr(), arenas[0]);
    }
}
#[test]
fn whole_closure_quota_admission_precedes_every_codec_with_exact_equality() {
    let (sources, mut decoder) = fixtures();
    accepted(schema_capture::admit(
        sources,
        1,
        RootSelection::Default,
        Limits {
            captured_bytes: 3,
            identity_bytes: 14,
            ..Default::default()
        },
        &mut decoder,
    ));
    assert_eq!(decoder.trace.borrow().len(), 2);
    for (limits, root, resource) in [
        (
            Limits {
                captured_bytes: 0,
                identity_bytes: 0,
                bundle: BundleLimits {
                    modules: 0,
                    ..Default::default()
                },
            },
            RootSelection::Named("long"),
            Resource::RootClass,
        ),
        (
            Limits {
                captured_bytes: 0,
                identity_bytes: 0,
                bundle: BundleLimits {
                    modules: 1,
                    ..Default::default()
                },
            },
            RootSelection::Default,
            Resource::SchemaModules,
        ),
        (
            Limits {
                captured_bytes: 1,
                identity_bytes: 0,
                ..Default::default()
            },
            RootSelection::Default,
            Resource::CapturedBytes,
        ),
        (
            Limits {
                captured_bytes: 3,
                identity_bytes: 0,
                ..Default::default()
            },
            RootSelection::Default,
            Resource::IdentityBytes,
        ),
        (
            Limits {
                captured_bytes: 2,
                identity_bytes: 13,
                ..Default::default()
            },
            RootSelection::Default,
            Resource::CapturedBytes,
        ),
        (
            Limits {
                captured_bytes: 3,
                identity_bytes: 13,
                ..Default::default()
            },
            RootSelection::Default,
            Resource::IdentityBytes,
        ),
    ] {
        let (sources, mut decoder) = fixtures();
        let result = schema_capture::admit(sources, 1, root, limits, &mut decoder);
        assert!(matches!(result,Err(Error::Limit(actual)) if actual==resource));
        assert!(decoder.trace.borrow().is_empty());
        assert_eq!(decoder.documents.len(), 2);
    }
}
#[test]
fn default_root_skips_named_charge_and_named_text_is_separate_and_inclusive() {
    for (root, limit, success) in [
        (RootSelection::Default, 1, true),
        (RootSelection::Named("root_class"), 1, false),
        (RootSelection::Named("root_class"), 10, true),
    ] {
        let sources = vec![source("r", vec![1])];
        let mut decoder = Decoder::new(vec![module(
            vec![(
                "root_class",
                Tree::List(vec![map(vec![("value", descriptor("str"))])]),
            )],
            &[],
        )]);
        let result = schema_capture::admit(
            sources,
            0,
            root,
            Limits {
                identity_bytes: limit,
                ..Default::default()
            },
            &mut decoder,
        );
        if success {
            accepted(result);
            assert_eq!(decoder.trace.borrow().len(), 1);
        } else {
            assert!(matches!(result, Err(Error::Limit(Resource::RootClass))));
            assert!(decoder.trace.borrow().is_empty());
        }
    }
}
#[test]
fn late_original_opaque_codec_failure_precedes_bundle_entry_or_duplicate_defects() {
    for entry in [0, usize::MAX] {
        let mut decoder = Decoder::new(vec![
            document(Tree::Text("invalid schema".into())),
            module(vec![], &[]),
            module(vec![], &[]),
        ]);
        let allocation = Box::new(72);
        let pointer = (&*allocation) as *const u32;
        decoder.fail_at = Some(2);
        decoder.failure = Some(Payload {
            allocation,
            _native: Rc::new(Cell::new(0)),
        });
        let sources = vec![
            source("schema_s.yaml", vec![1]),
            source("schema_s.yaml", vec![2]),
            source("late", vec![3]),
        ];
        let result = schema_capture::admit(
            sources,
            entry,
            RootSelection::Default,
            Limits::default(),
            &mut decoder,
        );
        let Err(Error::Decode { identity, error }) = result else {
            panic!("original second codec error expected")
        };
        assert_eq!(identity, "schema_s.yaml");
        assert_eq!((&*error.allocation) as *const u32, pointer);
        assert_eq!(
            *decoder.trace.borrow(),
            [
                ("schema_s.yaml".into(), vec![1]),
                ("schema_s.yaml".into(), vec![2])
            ]
        );
        assert_eq!(decoder.documents.len(), 2);
    }
}
#[test]
fn all_codecs_complete_before_exact_ordered_bundle_reports_and_entry_errors() {
    let root = module(
        vec![(
            "root_class",
            Tree::List(vec![
                map(vec![("first", descriptor("unknown_one"))]),
                map(vec![("second", descriptor("unknown_two"))]),
            ]),
        )],
        &[],
    );
    let mut decoder = Decoder::new(vec![root]);
    let result = schema_capture::admit(
        vec![source("r", vec![1])],
        0,
        RootSelection::Default,
        Limits::default(),
        &mut decoder,
    );
    let Err(Error::Bundle(error)) = result else {
        panic!("full bundle issues expected")
    };
    assert_eq!(
        error,
        BundleError::Invalid(vec![
            BundleIssue {
                module: 0,
                path: "root_class.first".into(),
                kind: BundleIssueKind::UnknownType("unknown_one".into())
            },
            BundleIssue {
                module: 0,
                path: "root_class.second".into(),
                kind: BundleIssueKind::UnknownType("unknown_two".into())
            },
        ])
    );
    assert_eq!(*decoder.trace.borrow(), [("r".into(), vec![1])]);
    for entry in [0, usize::MAX] {
        let (mut sources, mut decoder) = fixtures();
        sources[1].identity = "schema_s.yaml".into();
        let result = schema_capture::admit(
            sources,
            entry,
            RootSelection::Default,
            Limits::default(),
            &mut decoder,
        );
        let Err(Error::Bundle(error)) = result else {
            panic!("bundle defect expected after both codecs")
        };
        let issue = if entry == 0 {
            BundleIssue {
                module: 1,
                path: "$".into(),
                kind: BundleIssueKind::DuplicateModule("schema_s.yaml".into()),
            }
        } else {
            BundleIssue {
                module: usize::MAX,
                path: "$".into(),
                kind: BundleIssueKind::ModuleName,
            }
        };
        assert_eq!(error, BundleError::Invalid(vec![issue]));
        assert_eq!(decoder.trace.borrow().len(), 2);
    }
}
#[test]
fn invalid_defaults_remain_a_later_normalization_gate() {
    let schema = module(
        vec![(
            "root_class",
            Tree::List(vec![map(vec![(
                "value",
                map(vec![
                    ("type", text("int")),
                    ("default", text("invalid integer")),
                ]),
            )])]),
        )],
        &[],
    );
    let mut decoder = Decoder::new(vec![schema]);
    let captured = accepted(schema_capture::admit(
        vec![source("r", vec![1])],
        0,
        RootSelection::Default,
        Limits::default(),
        &mut decoder,
    ));
    let input = document(map(vec![]));
    assert!(matches!(
        captured
            .structure()
            .normalize_document(&input, &mut NormalizationBudget::new(Default::default())),
        Err(NormalizationError::Defaults(_))
    ));
    assert_eq!(decoder.trace.borrow().len(), 1);
}
#[test]
fn configured_large_closure_keeps_its_existing_capacity_without_a_new_source_ceiling() {
    for maximum in [128, 201] {
        let names = (0..200)
            .map(|i| format!("schema_m{i}.yaml"))
            .collect::<Vec<_>>();
        let mut sources = names
            .iter()
            .map(|name| source(name, vec![1]))
            .collect::<Vec<_>>();
        sources.push(source("root", vec![2]));
        let mut documents = names
            .iter()
            .map(|_| module(vec![], &[]))
            .collect::<Vec<_>>();
        documents.push(module(
            vec![(
                "root_class",
                Tree::List(vec![map(vec![("value", descriptor("str"))])]),
            )],
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
        ));
        let mut decoder = Decoder::new(documents);
        let result = schema_capture::admit(
            sources,
            200,
            RootSelection::Default,
            Limits {
                bundle: BundleLimits {
                    modules: maximum,
                    ..Default::default()
                },
                ..Default::default()
            },
            &mut decoder,
        );
        if maximum == 128 {
            assert!(matches!(result, Err(Error::Limit(Resource::SchemaModules))));
            assert!(decoder.trace.borrow().is_empty());
        } else {
            let captured = accepted(result);
            assert_eq!(captured.sources().len(), 201);
            assert_eq!(captured.structure().modules().len(), 201);
            assert_eq!(decoder.trace.borrow().len(), 201);
        }
    }
}
