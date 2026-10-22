//! Preparation contracts: schema capture, source profile inspection,
//! standalone preparation and structural preparation.
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

    mod standalone_preparation {
        //! Independent logical documents exercise the complete standalone application.
        //!
        //! Every test here is 0.5 only (#3447): `standalone_preparation::prepare`
        //! prepares a 0.5 entry without a `parent`, through the 0.5
        //! normalization and the 0.5 model contract. A 1.x specification is
        //! a chain of layers, resolved by `prepare_versioned` (LAYER-1); its
        //! window expansion is `schema_windows`', and its defaults
        //! `inheritance_capture::complete_effects_defaults_and_distinct_orders_are_engine_owned`.
        use yamaa_core::schema::{
            Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
            SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure, WindowReference,
        };
        use yamaa_core::standalone_preparation::{prepare, Error};

        use crate::support::document::Tree as V;

        fn document(value: V) -> Document {
            value.document()
        }
        #[derive(Debug, PartialEq)]
        enum Logical {
            Null,
            Int(String),
            Text(String),
            List(Vec<Logical>),
            Map(Vec<(String, Logical)>),
        }
        fn logical(document: &Document, node: usize) -> Logical {
            match &document.nodes()[node] {
                N::Null => Logical::Null,
                N::Integer(text) => Logical::Int(text.clone()),
                N::Text(text) => Logical::Text(text.clone()),
                N::Sequence(items) => {
                    Logical::List(items.iter().map(|&id| logical(document, id)).collect())
                }
                N::Mapping(items) => Logical::Map(
                    items
                        .iter()
                        .map(|&(k, v)| {
                            let N::Text(key) = &document.nodes()[k] else {
                                panic!("text fixture keys")
                            };
                            (key.clone(), logical(document, v))
                        })
                        .collect(),
                ),
                _ => panic!("fixture scalar"),
            }
        }
        fn field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(name, V::map(vec![("type", V::text(kind))]))])
        }
        fn schema(default_domain: bool, loose_domain: bool) -> SchemaStructure {
            let mut domain = vec![(
                "type",
                if loose_domain {
                    V::List(vec![V::text("str"), V::text("int")])
                } else {
                    V::text("str")
                },
            )];
            if default_domain {
                domain.push(("default", V::text("DEFAULT")));
            }
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root",
                            V::List(vec![
                                field("schema_version", "str"),
                                V::map(vec![("domain", V::map(domain))]),
                                field("input", "dict"),
                                field("keys", "list[str]"),
                                field("output", "dict"),
                                field("columns", "list[column_class]"),
                                field("windows", "dict[str, window_spec]"),
                            ]),
                        ),
                        (
                            "column_class",
                            V::List(vec![
                                field("name", "str"),
                                field("type", "str"),
                                field("derivation", "handled_expression_class"),
                            ]),
                        ),
                        (
                            "handled_expression_class",
                            V::List(vec![field("value", "expression")]),
                        ),
                        (
                            "expression",
                            V::map(vec![("registry", V::text("expressions"))]),
                        ),
                        (
                            "expressions",
                            V::map(vec![(
                                "row_number",
                                V::List(vec![field("window", "window_selection")]),
                            )]),
                        ),
                        (
                            "window_selection",
                            V::map(vec![(
                                "type",
                                V::List(vec![V::text("str"), V::text("window_spec")]),
                            )]),
                        ),
                        (
                            "window_spec",
                            V::List(vec![
                                field("group_by", "list[str]"),
                                field("order_by", "list[order_term]"),
                            ]),
                        ),
                        (
                            "order_term",
                            V::List(vec![
                                field("variable", "str"),
                                field("direction", "str"),
                                field("nulls", "str"),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root",
            )
            .unwrap()
        }
        fn basic_fields() -> Vec<(&'static str, V)> {
            vec![
                ("schema_version", V::text("1.0")),
                ("domain", V::text("X")),
                ("input", V::map(vec![])),
                ("keys", V::List(vec![])),
                (
                    "output",
                    V::map(vec![
                        ("path", V::text("output.csv")),
                        ("columns", V::List(vec![])),
                    ]),
                ),
                ("columns", V::List(vec![])),
            ]
        }

        fn column(name: &'static str, window: V) -> V {
            V::map(vec![
                ("name", V::text(name)),
                ("type", V::text("int")),
                (
                    "derivation",
                    V::map(vec![(
                        "value",
                        V::map(vec![("row_number", V::map(vec![("window", window)]))]),
                    )]),
                ),
            ])
        }
        fn window() -> V {
            V::map(vec![(
                "order_by",
                V::List(vec![V::map(vec![("variable", V::text("SEQ"))])]),
            )])
        }
        fn field_id(document: &Document, path: &[&str]) -> usize {
            path.iter().fold(document.root(), |id, name| {
                document.field(id, name).unwrap()
            })
        }
        fn item(document: &Document, sequence: usize, index: usize) -> usize {
            let N::Sequence(items) = &document.nodes()[sequence] else {
                panic!()
            };
            items[index]
        }
        fn window_id(document: &Document, index: usize) -> usize {
            ["derivation", "value", "row_number", "window"]
                .into_iter()
                .fold(
                    item(document, field_id(document, &["columns"]), index),
                    |id, name| document.field(id, name).unwrap(),
                )
        }
        fn unknown(name: &str, window: &str) -> SchemaDiagnostic {
            SchemaDiagnostic {
                path: format!("columns.{name}.derivation.row_number.window"),
                code: yamaa_core::diagnostic::ConditionCode::SchemaUnknownWindow,
                context: vec![("window", C::Text(window.into()))],
            }
        }

        #[test]
        fn success_moves_raw_arena_and_retains_complete_default_and_window_provenance() {
            let mut fields = basic_fields();
            fields.remove(1);
            // Keys name both columns, so pruning (LAYER-11) keeps them live.
            fields[2].1 = V::List(vec![V::text("Z"), V::text("A")]);
            fields.last_mut().unwrap().1 = V::List(vec![
                column("Z", V::text("VISITS")),
                column("A", V::text("VISITS")),
            ]);
            fields.push(("windows", V::map(vec![("VISITS", window())])));
            let raw = document(V::map(fields));
            let raw_pointer = raw.nodes().as_ptr();
            let raw_expected = raw.clone();
            let schema = schema(true, false);
            let prepared = prepare(&schema, raw).unwrap();
            assert_eq!(prepared.raw().nodes().as_ptr(), raw_pointer);
            assert_eq!(prepared.raw(), &raw_expected);
            let mut expected = basic_fields();
            expected[1].1 = V::text("DEFAULT");
            expected[3].1 = V::List(vec![V::text("Z"), V::text("A")]);
            expected.last_mut().unwrap().1 =
                V::List(vec![column("Z", window()), column("A", window())]);
            let model = prepared.model().document();
            let expected = document(V::map(expected));
            assert_eq!(
                logical(model, model.root()),
                logical(&expected, expected.root())
            );
            assert_eq!(
                prepared.windows(),
                &[
                    WindowReference {
                        path: "columns.Z.derivation.value.row_number.window".into(),
                        definition: "VISITS".into()
                    },
                    WindowReference {
                        path: "columns.A.derivation.value.row_number.window".into(),
                        definition: "VISITS".into()
                    },
                ]
            );
            assert_eq!(prepared.origins().len(), model.nodes().len());
            let descriptor = schema
                .class_named("root")
                .unwrap()
                .fields
                .iter()
                .find(|f| f.name == "domain")
                .unwrap()
                .descriptor;
            let located = &schema.descriptors()[descriptor];
            let default = located.descriptor.default_node().unwrap();
            let default_origin = SchemaOrigin {
                source: SchemaSource::Default {
                    module: 0,
                    descriptor,
                },
                node: default,
                generated: false,
            };
            let domain = model.field(model.root(), "domain").unwrap();
            assert_eq!(prepared.origins()[domain], default_origin);
            let N::Mapping(entries) = &model.nodes()[model.root()] else {
                panic!()
            };
            let domain_key = entries.iter().find(|(_, v)| *v == domain).unwrap().0;
            assert_eq!(
                prepared.origins()[domain_key],
                SchemaOrigin {
                    generated: true,
                    ..default_origin
                }
            );
            assert_eq!(
                prepared.origins()[model.root()],
                SchemaOrigin {
                    source: SchemaSource::Input,
                    node: raw_expected.root(),
                    generated: false
                }
            );
            for index in 0..2 {
                let selected = window_id(model, index);
                let original = window_id(&raw_expected, index);
                assert_eq!(
                    prepared.origins()[selected],
                    SchemaOrigin {
                        source: SchemaSource::Input,
                        node: original,
                        generated: false
                    }
                );
                let copied = model.field(selected, "order_by").unwrap();
                let definition = field_id(&raw_expected, &["windows", "VISITS", "order_by"]);
                assert_eq!(
                    prepared.origins()[copied],
                    SchemaOrigin {
                        source: SchemaSource::Input,
                        node: definition,
                        generated: false
                    }
                );
            }
            // Independent complete provenance oracle: copy authored occurrences, replace
            // only the named selection's children from its definition, and materialize
            // the sole default key/value from its descriptor's captured schema arena.
            fn copied(
                model: &Document,
                id: usize,
                raw: &Document,
                input: usize,
                expected: &mut [Option<SchemaOrigin>],
            ) {
                expected[id] = Some(SchemaOrigin {
                    source: SchemaSource::Input,
                    node: input,
                    generated: false,
                });
                match (&model.nodes()[id], &raw.nodes()[input]) {
                    (N::Sequence(actual), N::Sequence(original)) => {
                        assert_eq!(actual.len(), original.len());
                        for (&actual, &original) in actual.iter().zip(original) {
                            copied(model, actual, raw, original, expected);
                        }
                    }
                    (N::Mapping(actual), N::Mapping(original)) => {
                        for &(key, value) in actual {
                            let &(original_key, original_value) = original
                                .iter()
                                .find(|&&(k, _)| raw.nodes()[k] == model.nodes()[key])
                                .unwrap();
                            copied(model, key, raw, original_key, expected);
                            copied(model, value, raw, original_value, expected);
                        }
                    }
                    (N::Mapping(_), N::Text(name)) if name == "VISITS" => {
                        let definition = field_id(raw, &["windows", "VISITS"]);
                        copied(model, id, raw, definition, expected);
                        expected[id] = Some(SchemaOrigin {
                            source: SchemaSource::Input,
                            node: input,
                            generated: false,
                        });
                    }
                    (actual, original) => assert_eq!(actual, original),
                }
            }
            let mut expected_origins = vec![None; model.nodes().len()];
            expected_origins[model.root()] = Some(SchemaOrigin {
                source: SchemaSource::Input,
                node: raw_expected.root(),
                generated: false,
            });
            let N::Mapping(raw_fields) = &raw_expected.nodes()[raw_expected.root()] else {
                panic!()
            };
            for &(key, value) in entries {
                if value == domain {
                    expected_origins[value] = Some(default_origin);
                    expected_origins[key] = Some(SchemaOrigin {
                        generated: true,
                        ..default_origin
                    });
                } else {
                    let &(raw_key, raw_value) = raw_fields
                        .iter()
                        .find(|&&(k, _)| raw_expected.nodes()[k] == model.nodes()[key])
                        .unwrap();
                    copied(model, key, &raw_expected, raw_key, &mut expected_origins);
                    copied(
                        model,
                        value,
                        &raw_expected,
                        raw_value,
                        &mut expected_origins,
                    );
                }
            }
            assert_eq!(
                prepared.origins(),
                expected_origins
                    .into_iter()
                    .map(Option::unwrap)
                    .collect::<Vec<_>>()
            );
            let (raw, model, origins, windows) = prepared.into_parts();
            assert_eq!(raw.nodes().as_ptr(), raw_pointer);
            assert_eq!(origins.len(), model.document().nodes().len());
            assert_eq!(windows.len(), 2);
        }

        #[test]
        fn parent_presence_selects_inheritance_before_standalone_semantic_checks() {
            let schema = schema(false, false);
            for parent in [V::List(vec![]), V::text("parent.yaml")] {
                let raw = document(V::map(vec![
                    ("schema_version", V::text("99.0")),
                    ("parent", parent),
                    ("domain", V::integer("8")),
                ]));

                assert!(matches!(
                    prepare(&schema, raw),
                    Err(Error::InheritanceRequired)
                ));
            }
            // A null parent is absent: the entry takes the standalone route,
            // whose version gate refuses it.
            let raw = document(V::map(vec![
                ("schema_version", V::text("99.0")),
                ("parent", V::Scalar(N::Null)),
                ("domain", V::integer("8")),
            ]));
            let actual_node = field_id(&raw, &["schema_version"]);
            let Err(Error::Normalize(failure)) = prepare(&schema, raw) else {
                panic!("a null parent takes the standalone route")
            };
            assert_eq!(
                failure.error,
                NormalizationError::Invalid(vec![SchemaDiagnostic {
                    path: "schema_version".into(),
                    code: yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch,
                    context: vec![
                        ("expected", C::Text("1.0".into())),
                        ("actual", C::InputValue(actual_node))
                    ]
                }])
            );
        }

        #[test]
        fn normalization_findings_keep_original_raw_allocation_and_full_version_precedence() {
            let raw = document(V::map(vec![
                (
                    "schema_version",
                    V::integer("92233720368547758081234567890"),
                ),
                ("domain", V::integer("8")),
                ("windows", V::map(vec![("BAD", V::text("broken"))])),
            ]));
            let pointer = raw.nodes().as_ptr();
            let expected = raw.clone();
            let actual_node = field_id(&raw, &["schema_version"]);
            let Err(Error::Normalize(failure)) = prepare(&schema(false, false), raw) else {
                panic!()
            };
            let held = failure.input.unwrap();
            assert_eq!(held.nodes().as_ptr(), pointer);
            assert_eq!(held, expected);
            assert_eq!(
                failure.error,
                NormalizationError::Invalid(vec![SchemaDiagnostic {
                    path: "schema_version".into(),
                    code: yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch,
                    context: vec![
                        ("expected", C::Text("1.0".into())),
                        ("actual", C::InputValue(actual_node))
                    ]
                }])
            );
        }

        #[test]
        fn strict_windows_preserve_complete_order_and_precede_model_findings() {
            let mut fields = basic_fields();
            fields[4].1 = V::map(vec![]);
            // Keys name both columns, so pruning (LAYER-11) keeps them live
            // and strict window expansion still reports their unknown windows.
            fields[3].1 = V::List(vec![V::text("Z"), V::text("A")]);
            fields.last_mut().unwrap().1 = V::List(vec![
                column("Z", V::text("SECOND")),
                column("A", V::text("FIRST")),
            ]);
            let Err(Error::Normalize(failure)) =
                prepare(&schema(false, false), document(V::map(fields)))
            else {
                panic!()
            };
            assert_eq!(
                failure.error,
                NormalizationError::Invalid(vec![unknown("Z", "SECOND"), unknown("A", "FIRST")])
            );
            assert!(failure.input.is_some());
        }

        #[test]
        fn model_findings_remain_complete_and_have_no_document_value_references() {
            let mut fields = basic_fields();
            fields[1].1 = V::integer("7");
            fields[4].1 = V::map(vec![]);
            let Err(Error::Model(findings)) =
                prepare(&schema(false, true), document(V::map(fields)))
            else {
                panic!()
            };
            let finding = |path: &str, reason: &str| SchemaDiagnostic {
                path: path.into(),
                code: if reason == "Field required" {
                    yamaa_core::diagnostic::ConditionCode::SchemaModelMissingField
                } else {
                    yamaa_core::diagnostic::ConditionCode::SchemaModelInvalidValue
                },
                context: vec![("reason", C::Text(reason.into()))],
            };
            assert_eq!(
                findings,
                vec![
                    finding("domain", "Input should be a valid string"),
                    finding("output.path", "Field required"),
                    finding("output.columns", "Field required")
                ]
            );
        }
    }

    mod structural_preparation {
        //! Independent logical truth and physical codec effects for live metadata preparation.
        //!
        //! Every test here is 0.5 only (#3447): `structural_preparation::prepare`
        //! admits the files the 0.5 closures read (a 0.5 project, environment
        //! file, definition file or codelist source). A 1.x file is admitted
        //! against its type's closure once its header is read (SCHEMA-2).
        use std::rc::Rc;
        use yamaa_core::schema::{
            Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
            SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure,
        };
        use yamaa_core::{
            project_preparation::Source,
            structural_preparation::{
                self as application, Decoded, Error, Selection, StructuralDecodePort,
            },
        };

        use crate::support::document::Tree as V;

        fn document(value: V) -> Document {
            value.document()
        }
        #[derive(Debug, PartialEq)]
        enum Logical {
            Text(String),
            Int(String),
            Bool(bool),
            List(Vec<Logical>),
            Map(Vec<(String, Logical)>),
        }
        fn logical(d: &Document, id: usize) -> Logical {
            match &d.nodes()[id] {
                N::Text(v) => Logical::Text(v.clone()),
                N::Integer(v) => Logical::Int(v.clone()),
                N::Boolean(v) => Logical::Bool(*v),
                N::Sequence(items) => {
                    Logical::List(items.iter().map(|&id| logical(d, id)).collect())
                }
                N::Mapping(items) => Logical::Map(
                    items
                        .iter()
                        .map(|&(k, v)| {
                            let N::Text(k) = &d.nodes()[k] else { panic!() };
                            (k.clone(), logical(d, v))
                        })
                        .collect(),
                ),
                _ => panic!(),
            }
        }
        fn field(name: &'static str, kind: &'static str, required: bool) -> V {
            V::map(vec![(
                name,
                V::map(vec![
                    ("type", V::text(kind)),
                    ("required", V::boolean(required)),
                ]),
            )])
        }
        fn schema(bad_default: bool) -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "environment_class",
                            V::List(vec![
                                field("schema_version", "str", true),
                                field("lock", "str", false),
                            ]),
                        ),
                        (
                            "function_definition_class",
                            V::List(vec![
                                field("function", "str", true),
                                field("returns", "str", true),
                                V::map(vec![(
                                    "flag",
                                    V::map(vec![
                                        ("type", V::text("bool")),
                                        (
                                            "default",
                                            if bad_default {
                                                V::integer("7")
                                            } else {
                                                V::boolean(false)
                                            },
                                        ),
                                    ]),
                                )]),
                            ]),
                        ),
                        (
                            "versioned_other",
                            V::List(vec![field("schema_version", "str", false)]),
                        ),
                        (
                            "function_alias",
                            V::map(vec![("type", V::text("function_definition_class"))]),
                        ),
                    ])),
                }],
                0,
                "environment_class",
            )
            .unwrap()
        }
        fn source() -> Source {
            Source {
                identity: "held/function.yaml".into(),
                bytes: b"original captured representation".to_vec(),
            }
        }
        fn function() -> Document {
            document(V::map(vec![
                ("function", V::text("program.constant")),
                ("returns", V::text("int")),
            ]))
        }
        // Intentionally no Clone/Debug/Send/Sync: the actual allocation must move through
        // the new application rather than be interpreted, copied or formatted.
        struct Opaque {
            value: Box<str>,
            _thread: Rc<()>,
        }
        struct Codec {
            reply: Option<Result<Decoded<Vec<usize>>, Opaque>>,
            calls: Vec<(usize, usize)>,
        }
        impl Codec {
            fn success(document: Document) -> Self {
                Self {
                    reply: Some(Ok(Decoded {
                        locations: (0..document.nodes().len()).collect(),
                        document,
                    })),
                    calls: vec![],
                }
            }
            fn raised(value: Opaque) -> Self {
                Self {
                    reply: Some(Err(value)),
                    calls: vec![],
                }
            }
        }
        impl StructuralDecodePort for Codec {
            type Error = Opaque;
            type Locations = Vec<usize>;
            fn decode(&mut self, source: &Source) -> Result<Decoded<Vec<usize>>, Opaque> {
                self.calls.push((
                    source.bytes.as_ptr() as usize,
                    source.identity.as_ptr() as usize,
                ));
                self.reply.take().expect("codec must execute once")
            }
        }
        fn unwrap<T>(value: Result<T, Error<Opaque>>) -> T {
            value.unwrap_or_else(|_| panic!("unexpected preparation failure"))
        }
        fn unknown(path: &str, name: &str, class: &str) -> SchemaDiagnostic {
            SchemaDiagnostic {
                path: path.into(),
                code: yamaa_core::diagnostic::ConditionCode::SchemaUnknownField,
                context: vec![
                    ("field", C::Text(name.into())),
                    ("class", C::Text(class.into())),
                ],
            }
        }

        #[test]
        fn named_authority_guards_precede_decode_and_defaults() {
            let schema = schema(true);

            for name in ["missing", "str", "function_alias", ""] {
                let mut codec = Codec::success(function());
                let error = application::prepare(
                    &schema,
                    source(),
                    Selection::NamedClass(name),
                    &mut codec,
                );
                assert!(matches!(error,Err(Error::UnknownClass(actual)) if actual==name));
                assert!(codec.calls.is_empty());
                assert!(codec.reply.is_some());
            }
            for name in ["environment_class", "versioned_other"] {
                let mut codec = Codec::success(function());
                assert!(matches!(
                    application::prepare(
                        &schema,
                        source(),
                        Selection::NamedClass(name),
                        &mut codec
                    ),
                    Err(Error::VersionedRootRequired)
                ));
                assert!(codec.calls.is_empty());
            }
        }

        #[test]
        fn physical_codec_runs_once_and_original_opaque_error_precedes_query_and_defaults() {
            let schema = schema(true);
            for selection in [
                Selection::StructuralRoot,
                Selection::NamedClass("function_definition_class"),
            ] {
                let value = Opaque {
                    value: "original opaque codec failure".into(),
                    _thread: Rc::new(()),
                };
                let pointer = value.value.as_ptr();
                let entry = source();
                let bytes = entry.bytes.as_ptr() as usize;
                let identity = entry.identity.as_ptr() as usize;
                let mut codec = Codec::raised(value);

                let Err(Error::Decode(original)) =
                    application::prepare(&schema, entry, selection, &mut codec)
                else {
                    panic!()
                };
                assert_eq!(original.value.as_ptr(), pointer);
                assert_eq!(&*original.value, "original opaque codec failure");
                assert_eq!(codec.calls, [(bytes, identity)]);
                assert!(codec.reply.is_none());
            }
        }

        #[test]
        fn success_moves_every_original_allocation_and_complete_default_origin_vector() {
            let schema = schema(false);
            let entry = source();
            let bytes = entry.bytes.as_ptr();
            let identity = entry.identity.as_ptr();
            let raw = function();
            let expected_raw = raw.clone();
            let raw_pointer = raw.nodes().as_ptr();
            let mut codec = Codec::success(raw);
            let location_pointer = codec
                .reply
                .as_ref()
                .unwrap()
                .as_ref()
                .ok()
                .unwrap()
                .locations
                .as_ptr();
            let result = unwrap(application::prepare(
                &schema,
                entry,
                Selection::NamedClass("function_definition_class"),
                &mut codec,
            ));
            assert_eq!(result.source().bytes.as_ptr(), bytes);
            assert_eq!(result.source().identity.as_ptr(), identity);
            assert_eq!(result.raw().nodes().as_ptr(), raw_pointer);
            assert_eq!(result.locations().as_ptr(), location_pointer);
            assert_eq!(result.raw(), &expected_raw);
            assert_eq!(result.class_name(), "function_definition_class");
            assert_eq!(codec.calls.len(), 1);
            let expected = document(V::map(vec![
                ("function", V::text("program.constant")),
                ("returns", V::text("int")),
                ("flag", V::boolean(false)),
            ]));
            let normalized = &result.normalized().document;
            assert_eq!(
                logical(normalized, normalized.root()),
                logical(&expected, expected.root())
            );
            assert!(normalized
                .field(normalized.root(), "schema_version")
                .is_none());
            let descriptor = schema
                .class_named("function_definition_class")
                .unwrap()
                .fields
                .iter()
                .find(|f| f.name == "flag")
                .unwrap()
                .descriptor;
            let default = schema.descriptors()[descriptor]
                .descriptor
                .default_node()
                .unwrap();
            let origin = SchemaOrigin {
                source: SchemaSource::Default {
                    module: 0,
                    descriptor,
                },
                node: default,
                generated: false,
            };
            // Complete independently declared origin truth, aligned to independently
            // checked output nodes. Authored pair IDs are 0..3 and root ID is 4.
            let input = |node| SchemaOrigin {
                source: SchemaSource::Input,
                node,
                generated: false,
            };
            assert_eq!(
                result.normalized().origins,
                [
                    input(0),
                    input(1),
                    input(2),
                    input(3),
                    origin,
                    SchemaOrigin {
                        generated: true,
                        ..origin
                    },
                    input(4)
                ]
            );
            let (source, raw, locations, class_name, normalized) = result.into_parts();
            assert_eq!(source.bytes.as_ptr(), bytes);
            assert_eq!(source.identity.as_ptr(), identity);
            assert_eq!(raw.nodes().as_ptr(), raw_pointer);
            assert_eq!(locations.as_ptr(), location_pointer);
            assert_eq!(class_name, "function_definition_class");
            assert_eq!(normalized.origins.len(), 7);
        }

        #[test]
        fn semantic_failures_move_raw_arena_and_full_ordered_findings_without_partial_output() {
            let schema = schema(false);
            for (selection, raw, expected) in [
                (
                    Selection::StructuralRoot,
                    document(V::map(vec![
                        (
                            "schema_version",
                            V::integer("92233720368547758081234567890"),
                        ),
                        ("extra", V::text("late")),
                    ])),
                    vec![SchemaDiagnostic {
                        path: "schema_version".into(),
                        code: yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch,
                        context: vec![
                            ("expected", C::Text("1.0".into())),
                            ("actual", C::InputValue(1)),
                        ],
                    }],
                ),
                (
                    Selection::NamedClass("function_definition_class"),
                    document(V::map(vec![
                        ("function", V::text("program.constant")),
                        ("returns", V::text("int")),
                        ("schema_version", V::text("1.0")),
                        ("second", V::text("extra")),
                    ])),
                    vec![
                        unknown(
                            "<normalization>.schema_version",
                            "schema_version",
                            "function_definition_class",
                        ),
                        unknown(
                            "<normalization>.second",
                            "second",
                            "function_definition_class",
                        ),
                    ],
                ),
            ] {
                let pointer = raw.nodes().as_ptr();
                let raw_expected = raw.clone();
                let entry = source();
                let source_pointer = entry.bytes.as_ptr();
                let identity = entry.identity.as_ptr();
                let mut codec = Codec::success(raw);
                let Err(Error::Findings(held)) =
                    application::prepare(&schema, entry, selection, &mut codec)
                else {
                    panic!()
                };
                assert_eq!(held.document().nodes().as_ptr(), pointer);
                assert_eq!(held.document(), &raw_expected);
                assert_eq!(held.source().bytes.as_ptr(), source_pointer);
                assert_eq!(held.source().identity.as_ptr(), identity);
                assert_eq!(held.findings(), expected);
                assert_eq!(codec.calls.len(), 1);
                let (source, document, findings) = held.into_parts();
                assert_eq!(source.bytes.as_ptr(), source_pointer);
                assert_eq!(document.nodes().as_ptr(), pointer);
                assert_eq!(findings, expected);
            }
        }

        #[test]
        fn structural_root_has_no_named_name_guard_and_never_admits_a_model_or_window() {
            let schema = schema(false);
            let raw = document(V::map(vec![
                ("schema_version", V::text("1.0")),
                ("lock", V::text("uv.lock")),
            ]));
            let expected = raw.clone();
            let pointer = raw.nodes().as_ptr();
            let mut codec = Codec::success(raw);
            let result = unwrap(application::prepare(
                &schema,
                source(),
                Selection::StructuralRoot,
                &mut codec,
            ));
            assert_eq!(result.raw().nodes().as_ptr(), pointer);
            assert_eq!(result.class_name(), "environment_class");
            let normalized = &result.normalized().document;
            assert_eq!(
                logical(normalized, normalized.root()),
                logical(&expected, expected.root())
            );
            assert!(normalized.field(normalized.root(), "domain").is_none());
            assert_eq!(codec.calls.len(), 1);
            assert_eq!(
                result.normalized().origins,
                (0..expected.nodes().len())
                    .map(|node| SchemaOrigin {
                        source: SchemaSource::Input,
                        node,
                        generated: false
                    })
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn defaults_keep_their_post_decode_post_query_checkpoint_and_original_typed_payload() {
            let schema = schema(true);
            for selection in [
                Selection::StructuralRoot,
                Selection::NamedClass("function_definition_class"),
            ] {
                let raw = document(V::map(vec![("schema_version", V::text("99.0"))]));
                let mut codec = Codec::success(raw);
                let Err(Error::Normalize(NormalizationError::Defaults(defaults))) =
                    application::prepare(&schema, source(), selection, &mut codec)
                else {
                    panic!()
                };
                assert_eq!(defaults.len(), 1);
                assert_eq!(
                    defaults[0].diagnostics,
                    vec![SchemaDiagnostic {
                        path: "function_definition_class.flag.default".into(),
                        code: yamaa_core::diagnostic::ConditionCode::SchemaInvalidFieldType,
                        context: vec![
                            ("expected", C::Text("bool".into())),
                            (
                                "actual",
                                C::ValueKind(yamaa_core::schema::DocumentKind::Integer)
                            )
                        ]
                    }]
                );
                assert_eq!(codec.calls.len(), 1);
            }
        }
    }
}
