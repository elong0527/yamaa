//! Preparation contracts: schema capture and source profile inspection.
mod preparation {
    mod schema_capture {
        use std::{
            cell::{Cell, RefCell},
            collections::VecDeque,
            rc::Rc,
        };
        use yamaa_core::schema::{
            BundleError, BundleIssue, BundleIssueKind, Document, DocumentNode as N,
            NormalizationError, SchemaOrigin, SchemaSource,
        };
        use yamaa_core::{
            project_preparation::Source,
            schema_capture::{
                self, CapturedClosure, DecodedModule, Error, RootSelection, SchemaDecodePort,
            },
        };

        use crate::support::document::Tree;

        fn text(s: &str) -> Tree {
            Tree::text(s)
        }
        fn map(items: Vec<(&str, Tree)>) -> Tree {
            Tree::Map(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
        }
        fn document(tree: Tree) -> Document {
            tree.document()
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
                            map(vec![
                                ("type", text("str")),
                                ("required", Tree::boolean(true)),
                            ]),
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
                        map(vec![("yamaa", descriptor("str"))]),
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
        impl SchemaDecodePort for Decoder {
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
                    ["id", "label", "yamaa"]
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
                // A complete 1.x document, normalized once its header is read.
                let input = document(map(vec![("yamaa", text("1.0")), ("id", text("ID01"))]));
                let normalized = schema.normalize_root(&input).unwrap();
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
        fn late_original_opaque_codec_failure_precedes_bundle_entry_or_duplicate_defects() {
            for entry in [0, usize::MAX] {
                let mut decoder = Decoder::new(vec![
                    document(Tree::text("invalid schema")),
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
                let result =
                    schema_capture::admit(sources, entry, RootSelection::Default, &mut decoder);
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
                let result =
                    schema_capture::admit(sources, entry, RootSelection::Default, &mut decoder);
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
                &mut decoder,
            ));
            let input = document(map(vec![]));
            assert!(matches!(
                captured.structure().normalize_root(&input),
                Err(NormalizationError::Defaults(_))
            ));
            assert_eq!(decoder.trace.borrow().len(), 1);
        }
        #[test]
        fn large_closed_schema_keeps_every_original_source_and_decode() {
            {
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
                let result =
                    schema_capture::admit(sources, 200, RootSelection::Default, &mut decoder);
                let captured = accepted(result);
                assert_eq!(captured.sources().len(), 201);
                assert_eq!(captured.structure().modules().len(), 201);
                assert_eq!(decoder.trace.borrow().len(), 201);
            }
        }
    }

    mod source_profile_inspection {
        use yamaa_core::source_profile_inspection;
        use yamaa_core::{
            resource::ResourceFailure,
            specification::{PreflightFinding, PrepareError, UnsupportedFeature},
        };

        fn unknown() -> PrepareError {
            PrepareError::Invalid(vec![PreflightFinding::UnknownSourceProfile {
                dataset: "SRC".into(),
                path: "input.unknown".into(),
            }])
        }

        #[test]
        fn physical_kind_refines_unknown_profile_without_byte_authority() {
            for (failure, condition) in [
                (
                    ResourceFailure::NotRegularFile,
                    "resource_path_not_regular_file",
                ),
                (ResourceFailure::Symlink, "resource_path_symlink"),
                (ResourceFailure::Missing, "source_profile_unknown"),
            ] {
                let mut inspected = Vec::new();
                let error = source_profile_inspection::refine(unknown(), |dataset, path| {
                    inspected.push((dataset.to_owned(), path.to_owned()));
                    Some((failure, "../input.unknown".into()))
                });
                assert_eq!(inspected, [("SRC".into(), "input.unknown".into())]);
                let PrepareError::Invalid(findings) = error else {
                    panic!("invalid findings")
                };
                let issue = findings[0].diagnostic();
                assert_eq!(issue.definition().condition, condition);
                assert_eq!(issue.spec_paths, ["input.SRC.path"]);
            }
            let PrepareError::Invalid(findings) =
                source_profile_inspection::refine(unknown(), |_, _| None)
            else {
                panic!("invalid findings")
            };
            assert_eq!(
                findings[0].diagnostic().definition().condition,
                "source_profile_unknown"
            );
        }

        #[test]
        fn unsupported_and_other_static_findings_do_not_enter_inspection() {
            let unsupported = PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "future_operation".into(),
                path: "columns.X".into(),
            }]);
            assert!(matches!(
                source_profile_inspection::refine(unsupported, |_, _| panic!(
                    "unsupported entered study metadata"
                )),
                PrepareError::Unsupported(_)
            ));
            let original = PreflightFinding::UndeclaredKey {
                position: 0,
                column: "ID".into(),
            };
            let expected = original.diagnostic();
            let PrepareError::Invalid(findings) =
                source_profile_inspection::refine(PrepareError::Invalid(vec![original]), |_, _| {
                    panic!("unrelated finding entered study metadata")
                })
            else {
                panic!("invalid findings")
            };
            assert_eq!(findings[0].diagnostic(), expected);
        }
    }
}
