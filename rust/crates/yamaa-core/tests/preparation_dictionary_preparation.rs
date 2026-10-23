//! Preparation contracts: dictionary preparation, document preparation, file
//! preparation, inheritance and inheritance capture.
mod preparation {
    mod dictionary_preparation {
        //! Independent logical arenas and opaque effects exercise capture ownership, over a 1.0
        //! document (`prepare_document`).
        use crate::support::document::Tree as V;
        use std::{rc::Rc, sync::Arc};
        use yamaa_core::resource::ResourceFailure;
        use yamaa_core::schema::{Document, DocumentNode as N, SchemaModule, SchemaStructure};
        use yamaa_core::{
            dictionary_preparation as app,
            inheritance::{Source as Identity, SourceError},
            inheritance_capture::InheritanceDecodePort,
            project_preparation::Source,
            resolver::{Captured, Failure, File, Name, Resolver},
        };

        fn document(value: V) -> Document {
            value.document()
        }
        fn field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(name, V::map(vec![("type", V::text(kind))]))])
        }
        fn union(name: &'static str, kinds: &[&'static str]) -> V {
            V::map(vec![(
                name,
                V::map(vec![("type", V::List(kinds.iter().map(V::text).collect()))]),
            )])
        }
        fn schema() -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("yamaa", "str"),
                                field("type", "str"),
                                field("domain", "str"),
                                field("input", "dict"),
                                field("keys", "list[str]"),
                                field("output", "dict"),
                                field("columns", "list[column_class]"),
                                field("rows", "list[row_class]"),
                                field("metadata", "dict"),
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
                            "row_class",
                            V::List(vec![
                                field("id", "str"),
                                field("derivations", "dict[str, handled_expression_class]"),
                            ]),
                        ),
                        (
                            "handled_expression_class",
                            V::List(vec![
                                field("value", "expression"),
                                field("unconvertible", "literal_value"),
                            ]),
                        ),
                        (
                            "expression",
                            V::map(vec![("registry", V::text("expressions"))]),
                        ),
                        ("project_path", V::map(vec![("type", V::text("str"))])),
                        (
                            "literal_value",
                            V::map(vec![(
                                "type",
                                V::List(vec![
                                    V::text("str"),
                                    V::text("int"),
                                    V::text("float"),
                                    V::text("bool"),
                                    V::text("null"),
                                ]),
                            )]),
                        ),
                        (
                            "branch_class",
                            V::List(vec![field("when", "str"), field("value", "expression")]),
                        ),
                        (
                            "expressions",
                            V::map(vec![
                                (
                                    "mapping",
                                    V::List(vec![
                                        field("source", "str"),
                                        union(
                                            "dict",
                                            &["dict[str, literal_value]", "project_path"],
                                        ),
                                    ]),
                                ),
                                (
                                    "case",
                                    V::List(vec![field("branches", "list[branch_class]")]),
                                ),
                                ("nested", V::List(vec![field("values", "list[expression]")])),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root_class",
            )
            .unwrap()
        }
        fn mapping(dictionary: V) -> V {
            V::map(vec![(
                "mapping",
                V::map(vec![("source", V::text("SRC.X")), ("dict", dictionary)]),
            )])
        }
        fn handled(value: V) -> V {
            V::map(vec![("value", value)])
        }
        fn column(name: &'static str, value: V) -> V {
            V::map(vec![
                ("name", V::text(name)),
                ("type", V::text("str")),
                ("derivation", handled(value)),
            ])
        }
        fn input(columns: Vec<V>, rows: Vec<V>) -> V {
            V::map(vec![
                ("yamaa", V::text("1.0")),
                ("type", V::text("spec")),
                ("domain", V::text("DM")),
                (
                    "input",
                    V::map(vec![("SRC", V::map(vec![("path", V::text("src.csv"))]))]),
                ),
                ("keys", V::List(vec![V::text("ID")])),
                (
                    "output",
                    V::map(vec![
                        ("path", V::text("out.csv")),
                        ("columns", V::List(vec![V::text("X")])),
                    ]),
                ),
                ("columns", V::List(columns)),
                ("rows", V::List(rows)),
            ])
        }

        #[derive(Debug)]
        struct Host {
            tag: &'static str,
            owner: Rc<()>,
        }
        struct Port {
            events: Vec<String>,
            fail: Option<&'static str>,
            owner: Rc<()>,
        }
        impl Port {
            fn new() -> Self {
                Self {
                    events: Vec::new(),
                    fail: None,
                    owner: Rc::new(()),
                }
            }
        }
        impl Resolver for Port {
            type Error = Host;
            fn inspect(&mut self, name: Name<'_>) -> Result<Identity, Host> {
                let Name::Written {
                    file: File::Resource,
                    declaring,
                    written,
                } = name
                else {
                    panic!("parent route entered")
                };
                self.events.push(format!("resolve:{declaring}:{written}"));
                if let Some(tag) = self.fail {
                    return Err(Host {
                        tag,
                        owner: self.owner.clone(),
                    });
                }
                Ok(Identity {
                    identity: if written == "later.yaml" {
                        "later"
                    } else {
                        "dictionary"
                    }
                    .into(),
                    display_path: written.into(),
                })
            }
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Host> {
                let Name::Located {
                    file: File::Resource,
                    source,
                } = name
                else {
                    panic!("parent route entered")
                };
                self.events.push(format!("capture:{}", source.identity));
                let byte = if source.identity == "later" { 2 } else { 1 };
                Ok(Captured::resource(Arc::from(vec![byte]), true))
            }
            fn failure(&self, _: Name<'_>, error: &Host) -> Option<Failure> {
                match error.tag {
                    "missing" => Some(Failure::Resource(ResourceFailure::Missing)),
                    "directory" => Some(Failure::Resource(ResourceFailure::NotRegularFile)),
                    _ => None,
                }
            }
            fn rebase(&mut self, _: &Identity, _: &Identity, _: &str) -> Result<String, Host> {
                panic!("rebase entered")
            }
        }
        #[derive(Debug, PartialEq)]
        enum DecodeError {
            Invalid,
        }
        struct Decoder {
            events: Vec<String>,
            invalid: bool,
        }
        impl Decoder {
            fn new() -> Self {
                Self {
                    events: Vec::new(),
                    invalid: false,
                }
            }
        }
        impl InheritanceDecodePort for Decoder {
            type Raw = Document;
            type Error = DecodeError;
            fn decode(&mut self, identity: &str, bytes: &[u8]) -> Result<Document, DecodeError> {
                self.events.push(format!("{identity}:{bytes:?}"));
                if self.invalid {
                    return Err(DecodeError::Invalid);
                }
                Ok(if bytes == [2] {
                    document(V::List(vec![V::text("invalid dictionary")]))
                } else {
                    document(V::map(vec![
                        ("A", V::text("answer")),
                        ("B", V::boolean(false)),
                    ]))
                })
            }
            fn document(raw: &Document) -> &Document {
                raw
            }
        }
        fn model(paths: &[&'static str]) -> Document {
            document(input(
                paths
                    .iter()
                    .enumerate()
                    .map(|(i, path)| column(if i == 0 { "X" } else { "Y" }, mapping(V::text(path))))
                    .collect(),
                vec![],
            ))
        }
        #[test]
        fn aliases_resolve_in_use_order_but_capture_and_parse_once_without_mutating_original() {
            let schema = schema();
            let model = model(&["dict.yaml", "./dict.yaml"]);
            let original = model.clone();
            let mut port = Port::new();
            let mut decoder = Decoder::new();
            let prepared = app::prepare_document(
                &schema,
                &model,
                app::Origins::Standalone("parent/base.yaml"),
                &[],
                &mut port,
                &mut decoder,
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                port.events,
                vec![
                    "resolve:parent/base.yaml:dict.yaml",
                    "capture:dictionary",
                    "resolve:parent/base.yaml:./dict.yaml"
                ]
            );
            assert_eq!(decoder.events, vec!["dictionary:[1]"]);
            assert_eq!(prepared.captures.len(), 1);
            assert_eq!(prepared.captures[0].source().bytes, [1]);
            assert_eq!(model, original);
            assert_ne!(prepared.document, model);
        }
        #[test]
        fn already_decoded_seed_identity_is_reused_without_capture_or_decode() {
            let schema = schema();
            let model = model(&["dict.yaml"]);
            let mut port = Port::new();
            let mut decoder = Decoder::new();
            let source = Source {
                identity: "dictionary".into(),
                bytes: vec![1],
            };
            let raw = document(V::map(vec![("A", V::text("answer"))]));
            let prepared = app::prepare_document(
                &schema,
                &model,
                app::Origins::Standalone("entry"),
                &[app::Seed {
                    source: &source,
                    raw: &raw,
                }],
                &mut port,
                &mut decoder,
            )
            .unwrap()
            .unwrap();
            assert_eq!(port.events, ["resolve:entry:dict.yaml"]);
            assert!(decoder.events.is_empty());
            assert!(prepared.captures.is_empty());
        }
        #[test]
        fn classified_and_opaque_resource_errors_keep_original_payload_and_stop_without_retry() {
            for (tag, failure) in [
                ("missing", Some(ResourceFailure::Missing)),
                ("directory", Some(ResourceFailure::NotRegularFile)),
                ("opaque", None),
            ] {
                let mut port = Port::new();
                port.fail = Some(tag);
                let mut decoder = Decoder::new();
                let rejected = app::prepare_document(
                    &schema(),
                    &model(&["dict.yaml", "later.yaml"]),
                    app::Origins::Standalone("entry"),
                    &[],
                    &mut port,
                    &mut decoder,
                )
                .unwrap_err();
                let app::Cause::Capture {
                    error: SourceError::Raised(error),
                    failure: actual,
                    reference,
                } = &rejected.cause
                else {
                    panic!("failure lost")
                };
                assert_eq!(error.tag, tag);
                assert!(Rc::ptr_eq(&error.owner, &port.owner));
                assert_eq!(*actual, failure);
                assert_eq!(reference.path(), "columns.X.derivation.mapping.dict");
                assert_eq!(reference.written(), "dict.yaml");
                assert!(rejected.captures.is_empty());
                assert_eq!(port.events, ["resolve:entry:dict.yaml"]);
                assert!(decoder.events.is_empty());
            }
        }
        #[test]
        fn dictionary_contract_failure_retains_exact_first_capture_prefix_and_arena() {
            let mut port = Port::new();
            let mut decoder = Decoder::new();
            let rejected = app::prepare_document(
                &schema(),
                &model(&["dict.yaml", "later.yaml"]),
                app::Origins::Standalone("entry"),
                &[],
                &mut port,
                &mut decoder,
            )
            .unwrap_err();
            let app::Cause::Admission {
                arena: app::Arena::Captured(1),
                reference,
                ..
            } = &rejected.cause
            else {
                panic!("raw context lost")
            };
            assert_eq!(reference.path(), "columns.Y.derivation.mapping.dict");
            assert_eq!(rejected.captures.len(), 2);
            assert_eq!(
                rejected.captures[1].raw().nodes()[rejected.captures[1].raw().root()],
                N::Sequence(vec![0])
            );
            assert_eq!(decoder.events, ["dictionary:[1]", "later:[2]"]);
        }
        #[test]
        fn decode_failure_retains_entered_bytes_without_clone_and_no_later_capture() {
            let mut port = Port::new();
            let mut decoder = Decoder::new();
            decoder.invalid = true;
            let rejected = app::prepare_document(
                &schema(),
                &model(&["dict.yaml", "later.yaml"]),
                app::Origins::Standalone("entry"),
                &[],
                &mut port,
                &mut decoder,
            )
            .unwrap_err();
            let app::Cause::Decode {
                source,
                error: DecodeError::Invalid,
                ..
            } = &rejected.cause
            else {
                panic!()
            };
            assert_eq!(source.bytes, [1]);
            assert!(rejected.captures.is_empty());
            assert_eq!(decoder.events, ["dictionary:[1]"]);
        }
    }
    mod document_preparation {
        //! Independent raw logical arenas and opaque payloads qualify the dispatch service.
        //!
        //! Every test here is 0.5 only (#3447): `document_preparation::prepare`
        //! routes a 0.5 entry by its raw `parent` to the standalone or the
        //! inherited 0.5 lifecycle, and moves the entry codec's allocations.
        //! A 1.x entry is a chain of layers that `traverse_versioned` admits
        //! (LAYER-1); its non-mapping root is
        //! `inheritance::a_non_mapping_entry_is_a_finding_that_holds_its_raw_arena`.
        use crate::support::document::Tree;
        use std::{collections::BTreeMap, rc::Rc};
        use yamaa_core::schema::{
            Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
            SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure,
        };
        use yamaa_core::{
            document_preparation as app, inheritance as graph, inheritance_capture as capture,
            inheritance_preparation as lifecycle,
            project_preparation::Source,
            resolver::{self, Captured, Name, Resolver},
        };

        fn text(s: &str) -> Tree {
            Tree::text(s)
        }
        fn map(v: Vec<(&'static str, Tree)>) -> Tree {
            Tree::map(v)
        }
        fn seq(v: Vec<Tree>) -> Tree {
            Tree::List(v)
        }
        fn document(tree: Tree) -> Document {
            tree.document()
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
                                field("parent", "str"),
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
                                field("report", "str"),
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
        fn layer(name: &str, parent: Option<&str>, entry: bool) -> Document {
            let mut fields = vec![("schema_version", text("1.0"))];
            if let Some(parent) = parent {
                fields.push(("parent", text(parent)));
            }
            fields.push((
                "input",
                map(vec![(
                    "SRC",
                    map(vec![
                        ("path", text(&format!("{name}.csv"))),
                        ("schema", text(&format!("{name}-source.yaml"))),
                    ]),
                )]),
            ));
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
                            ("report", text("report.csv")),
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
        impl capture::InheritanceDecodePort for Parents {
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
        impl Resolver for Host {
            type Error = Payload;
            fn inspect(&mut self, name: Name<'_>) -> Result<graph::Source, Payload> {
                let Name::Written { written, .. } = name else {
                    panic!("unexpected request {name:?}")
                };
                self.calls.push(format!("canonical:{written}"));
                if let Some(error) = self.error.take() {
                    return Err(error);
                }
                Ok(graph::Source {
                    identity: written.into(),
                    display_path: format!("display/{written}"),
                })
            }
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Payload> {
                let Name::Located { source, .. } = name else {
                    panic!("unexpected request {name:?}")
                };
                self.calls.push(format!("capture:{}", source.identity));
                let bytes = self.bytes.remove(&source.identity).unwrap();
                Ok(Captured::resource(bytes.into(), true))
            }
            fn failure(&self, _: Name<'_>, _: &Payload) -> Option<resolver::Failure> {
                None
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
                        ("report", text("report.csv")),
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
            rule: Option<&'static str>,
            context: Vec<(&'static str, C)>,
        ) -> SchemaDiagnostic {
            SchemaDiagnostic {
                path: path.into(),
                code: match (condition, rule) {
                    ("schema_version_mismatch", None) => {
                        yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch
                    }
                    ("invalid_field_type", Some("SCHEMA-12")) => {
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
                                copied(
                                    model,
                                    key,
                                    raw,
                                    original_key,
                                    descriptor,
                                    default,
                                    expected,
                                );
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
            let (mut entry, mut parents, mut host) = ports(layer("entry", Some("b"), true));
            let raw = entry.raw.as_ref().unwrap().document.nodes().as_ptr();
            let locations = &*entry.raw.as_ref().unwrap().locations as *const u32;
            let mut held = vec![];
            for (name, parent) in [("b", Some("a")), ("a", Some("common")), ("common", None)] {
                let document = layer(name, parent, false);
                let bytes = name.as_bytes().to_vec();
                // A parent's bytes are copied out of the resolver's snapshot.
                held.push((name, document.nodes().as_ptr(), bytes.clone()));
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
                ["b", "a", "common"]
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
                assert_eq!(parent.source().bytes, bytes);
            }
            assert_eq!(entry.calls, 1);
            assert_eq!(parents.calls, ["b", "a", "common"]);
            assert_model(prepared.inheritance.model().document());
        }
        #[test]
        fn a_written_parent_selects_inheritance_and_a_null_parent_is_standalone() {
            for value in [N::Null, N::Integer("7".into()), N::Sequence(vec![])] {
                let document = layer("entry", Some("replaced"), true);
                let parent = document.field(document.root(), "parent").unwrap();
                let mut nodes = document.nodes().to_vec();
                nodes[parent] = value.clone();
                let (mut entry, mut parents, mut host) =
                    ports(Document::new(nodes, document.root()).unwrap());
                let result = run(&mut entry, &mut parents, &mut host);
                if matches!(value, N::Null) {
                    // A null parent is absent: a composition of one layer.
                    assert!(matches!(
                        result.as_ref().map(app::Prepared::content),
                        Ok(app::Content::Standalone { .. })
                    ));
                } else {
                    // Any other written parent is admitted, and rejected, as a layer.
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
                    layer("entry", Some("never"), true)
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

            let Err(app::Error::Entry(Failure::Original(error))) =
                run(&mut entry, &mut parents, &mut host)
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
                N::Sequence(items) => {
                    Logical::Seq(items.iter().map(|&id| logical(doc, id)).collect())
                }
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
            // Authored independently: the parent disappears and only column label defaults.
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
                        ("report", text("report.csv")),
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
                    Some("SCHEMA-12"),
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
    }
    mod file_preparation {
        //! Typed effect/order witnesses use independent retained parents and native policy.
        use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};
        use yamaa_core::{
            file_preparation as app,
            inheritance::{Source as Identity, SourceError},
            project_preparation::Source,
            resolver::{self, Captured, File, Name, Resolver},
        };
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum Event {
            Resolve(Option<String>, String),
            Entry(Option<String>, String),
            Service,
            Canonical(String, String),
            Parent(String, String),
            Alias(String, String),
            Rebase(String, String, String),
        }
        type Log = Rc<RefCell<Vec<Event>>>;
        // These payloads deliberately have no Debug/Clone and are neither Send nor Sync.
        struct Fault {
            allocation: Box<u32>,
            marker: Rc<()>,
        }
        impl Fault {
            fn new() -> Self {
                Self {
                    allocation: Box::new(31),
                    marker: Rc::new(()),
                }
            }
        }
        struct Native {
            log: Log,
            entry: Option<Vec<u8>>,
            parents: BTreeMap<String, Vec<u8>>,
            fault: Option<(usize, Fault)>,
            identity_pointer: Option<*const u8>,
            display_pointer: Option<*const u8>,
        }
        impl Native {
            fn event(&mut self, event: Event) -> Result<(), Fault> {
                let at = self.log.borrow().len();
                self.log.borrow_mut().push(event);
                if self.fault.as_ref().is_some_and(|(step, _)| *step == at) {
                    return Err(self.fault.take().unwrap().1);
                }
                Ok(())
            }
        }
        impl Resolver for Native {
            type Error = Fault;
            fn inspect(&mut self, name: Name<'_>) -> Result<Identity, Fault> {
                match name {
                    Name::Entry { declaring, written } => {
                        self.event(Event::Resolve(declaring.map(Into::into), written.into()))?;
                        Ok(Identity {
                            identity: "/study/entry".into(),
                            display_path: "/study/entry".into(),
                        })
                    }
                    Name::Written {
                        declaring, written, ..
                    } => {
                        self.event(Event::Canonical(declaring.into(), written.into()))?;
                        let identity = if written.starts_with('b') { "B" } else { "A" }.to_string();
                        let display_path = format!("{declaring}/{written}");
                        self.identity_pointer = Some(identity.as_ptr());
                        self.display_pointer = Some(display_path.as_ptr());
                        Ok(Identity {
                            identity,
                            display_path,
                        })
                    }
                    name => panic!("unexpected request {name:?}"),
                }
            }
            /// A spelling's first capture of its parent reads it; a later one,
            /// once the document prepares, checks the spelling as an alias. A
            /// located file no spelling reaches is refused.
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Fault> {
                match name {
                    Name::Entry { declaring, written } => {
                        self.event(Event::Entry(declaring.map(Into::into), written.into()))?;
                        Ok(Captured::resource(
                            Arc::from(self.entry.take().unwrap()),
                            true,
                        ))
                    }
                    Name::Written {
                        declaring, written, ..
                    } => {
                        let key = if written.starts_with('b') { "B" } else { "A" };
                        if self.parents.contains_key(key) {
                            self.event(Event::Parent(declaring.into(), written.into()))?;
                            let bytes = self.parents.remove(key).unwrap();
                            Ok(Captured::resource(Arc::from(bytes), true))
                        } else {
                            self.event(Event::Alias(declaring.into(), written.into()))?;
                            Ok(Captured::resource(Arc::from(Vec::new()), false))
                        }
                    }
                    Name::Located { .. } => Err(Fault::new()),
                    name => panic!("unexpected request {name:?}"),
                }
            }
            fn failure(&self, _: Name<'_>, _: &Fault) -> Option<resolver::Failure> {
                None
            }
            fn rebase(
                &mut self,
                layer: &Identity,
                entry: &Identity,
                written: &str,
            ) -> Result<String, Fault> {
                self.event(Event::Rebase(
                    layer.identity.clone(),
                    entry.identity.clone(),
                    written.into(),
                ))?;
                Ok(written.into())
            }
        }
        struct Prepared {
            source: Source,
            parents: Vec<Source>,
        }
        impl app::PreparedView for Prepared {
            type Parent = Source;
            fn parents(&self) -> &[Source] {
                &self.parents
            }
            fn parent_source(parent: &Source) -> &Source {
                parent
            }
        }
        enum Failure {
            Opaque(Fault),
            Parent(Fault),
            Unavailable,
        }
        struct Documents {
            log: Log,
            written: Vec<String>,
            rejection: Option<Fault>,
            wrong_parent: bool,
            display_pointer: Option<*const u8>,
        }
        impl app::DocumentPort<Fault> for Documents {
            type Prepared = Prepared;
            type Error = Failure;
            fn prepare<P: Resolver<Error = Fault>>(
                &mut self,
                source: Source,
                display: String,
                port: &mut P,
            ) -> Result<Prepared, Failure> {
                self.log.borrow_mut().push(Event::Service);
                assert_eq!(display, "/study/entry");
                let failed = |error| match error {
                    SourceError::Raised(error) => Failure::Parent(error),
                    SourceError::Unavailable => Failure::Unavailable,
                };
                let mut parents = vec![];
                for written in &self.written {
                    let name = Name::Written {
                        file: File::Parent,
                        declaring: &source.identity,
                        written,
                    };
                    let identity = port
                        .inspect(name)
                        .map_err(|error| failed(resolver::source_error(port, name, error)))?;
                    self.display_pointer = Some(identity.display_path.as_ptr());
                    if parents
                        .iter()
                        .any(|p: &Source| p.identity == identity.identity)
                    {
                        continue;
                    }
                    let name = Name::Located {
                        file: File::Parent,
                        source: &identity,
                    };
                    let bytes = port
                        .capture(name)
                        .map(resolver::bytes)
                        .map_err(|error| failed(resolver::source_error(port, name, error)))?;
                    parents.push(Source {
                        identity: identity.identity,
                        bytes,
                    });
                }
                if let Some(error) = self.rejection.take() {
                    return Err(Failure::Opaque(error));
                }
                if self.wrong_parent {
                    parents.push(Source {
                        identity: "ABSENT".into(),
                        bytes: vec![],
                    });
                }
                Ok(Prepared { source, parents })
            }
        }
        fn setup(written: &[&str]) -> (Native, Documents, Log) {
            let log = Rc::new(RefCell::new(vec![]));
            (
                Native {
                    log: Rc::clone(&log),
                    entry: Some(b"original-entry".to_vec()),
                    parents: BTreeMap::from([
                        ("A".into(), b"original-a".to_vec()),
                        ("B".into(), b"b".to_vec()),
                    ]),
                    fault: None,
                    identity_pointer: None,
                    display_pointer: None,
                },
                Documents {
                    log: Rc::clone(&log),
                    written: written.iter().map(|s| (*s).into()).collect(),
                    rejection: None,
                    wrong_parent: false,
                    display_pointer: None,
                },
                log,
            )
        }
        fn run(
            native: &mut Native,
            documents: &mut Documents,
        ) -> Result<Prepared, app::Error<Fault, Failure>> {
            app::prepare(native, documents, None, "root.yaml")
        }
        #[test]
        fn exact_effect_tape_preserves_duplicates_first_capture_and_success_aliases() {
            let (mut native, mut documents, log) = setup(&["a", "./a", "a", "b"]);
            let Ok(prepared) = run(&mut native, &mut documents) else {
                panic!()
            };
            assert_eq!(prepared.source.bytes, b"original-entry");
            assert_eq!(prepared.parents[0].bytes, b"original-a");
            assert_eq!(prepared.parents[1].bytes, b"b");
            assert_eq!(
                prepared
                    .parents
                    .iter()
                    .map(|p| p.identity.as_str())
                    .collect::<Vec<_>>(),
                ["A", "B"]
            );
            // /study/entry is 12 bytes. Canonical identity is one byte; display uses '/'.
            assert_eq!(
                *log.borrow(),
                vec![
                    Event::Resolve(None, "root.yaml".into()),
                    Event::Entry(None, "root.yaml".into()),
                    Event::Service,
                    Event::Canonical("/study/entry".into(), "a".into()),
                    Event::Parent("/study/entry".into(), "a".into()),
                    Event::Canonical("/study/entry".into(), "./a".into()),
                    Event::Canonical("/study/entry".into(), "a".into()),
                    Event::Canonical("/study/entry".into(), "b".into()),
                    Event::Parent("/study/entry".into(), "b".into()),
                    Event::Alias("/study/entry".into(), "a".into()),
                    Event::Alias("/study/entry".into(), "./a".into()),
                    Event::Alias("/study/entry".into(), "b".into()),
                ]
            );
        }
        #[test]
        fn declaring_entry_keeps_authored_spelling_and_service_waits_for_entry_capture() {
            let (mut native, mut documents, log) = setup(&[]);
            assert!(app::prepare(
                &mut native,
                &mut documents,
                Some("/meta/original"),
                "../study/root.yaml"
            )
            .is_ok());
            assert_eq!(
                *log.borrow(),
                vec![
                    Event::Resolve(Some("/meta/original".into()), "../study/root.yaml".into()),
                    Event::Entry(Some("/meta/original".into()), "../study/root.yaml".into()),
                    Event::Service,
                ]
            );
        }
        #[test]
        fn every_original_native_fault_moves_opaque_allocation_and_stops_later_effects() {
            let (mut native, mut documents, log) = setup(&["a", "./a", "a", "b"]);
            assert!(run(&mut native, &mut documents).is_ok());
            let tape = log.borrow().clone();
            for (step, event) in tape.iter().enumerate() {
                if *event == Event::Service {
                    continue;
                }
                let (mut native, mut documents, log) = setup(&["a", "./a", "a", "b"]);
                let fault = Fault::new();
                let pointer = &*fault.allocation as *const u32;
                let marker = Rc::as_ptr(&fault.marker);
                native.fault = Some((step, fault));
                let error = match run(&mut native, &mut documents) {
                    Err(app::Error::Entry(error))
                    | Err(app::Error::Resource(error))
                    | Err(app::Error::Preparation(Failure::Parent(error))) => error,
                    _ => panic!("native failure"),
                };
                assert_eq!(&*error.allocation as *const u32, pointer);
                assert_eq!(Rc::as_ptr(&error.marker), marker);
                assert_eq!(*log.borrow(), tape[..=step]);
            }
        }
        #[test]
        fn semantic_rejection_preserves_original_error_and_grants_no_alias_authority() {
            let (mut native, mut documents, log) = setup(&["a", "./a"]);
            let error = Fault::new();
            let pointer = &*error.allocation as *const u32;
            documents.rejection = Some(error);
            let Err(app::Error::Preparation(Failure::Opaque(error))) =
                run(&mut native, &mut documents)
            else {
                panic!()
            };
            assert_eq!(&*error.allocation as *const u32, pointer);
            assert!(!log.borrow().iter().any(|e| matches!(e, Event::Alias(..))));
        }

        #[test]
        fn existing_policy_accepts_two_hundred_independent_aliases_without_new_count_ceiling() {
            let aliases = (0..200).map(|i| format!("a{i}")).collect::<Vec<_>>();
            let written = aliases.iter().map(String::as_str).collect::<Vec<_>>();
            let (mut native, mut documents, log) = setup(&written);
            assert!(run(&mut native, &mut documents).is_ok());
            assert_eq!(
                log.borrow()
                    .iter()
                    .filter(|e| matches!(e, Event::Parent(..)))
                    .count(),
                1
            );
            assert_eq!(
                log.borrow()
                    .iter()
                    .filter(|e| matches!(e, Event::Alias(..)))
                    .count(),
                200
            );
        }
        #[test]
        fn unknown_retained_catalog_identity_refuses_before_any_alias_effect() {
            let (mut native, mut documents, log) = setup(&[]);
            documents.wrong_parent = true;
            assert!(matches!(
                run(&mut native, &mut documents),
                Err(app::Error::Resource(_))
            ));
            assert_eq!(
                *log.borrow(),
                vec![
                    Event::Resolve(None, "root.yaml".into()),
                    Event::Entry(None, "root.yaml".into()),
                    Event::Service
                ]
            );
        }

        #[test]
        fn canonical_identity_and_display_are_returned_without_replacing_original_allocations() {
            let (mut native, mut documents, _) = setup(&["a"]);
            let Ok(prepared) = run(&mut native, &mut documents) else {
                panic!()
            };
            assert_eq!(
                Some(prepared.parents[0].identity.as_ptr()),
                native.identity_pointer
            );
            // The canonical display is consumed by the document service; recording its
            // borrowed address there establishes the exact returned physical allocation.
            assert_eq!(documents.display_pointer, native.display_pointer);
        }
    }
    mod inheritance {
        //! A 1.x specification's parents are followed by `traverse_versioned`,
        //! which shares its walk with the 0.5 `traverse` (LAYER-2, LAYER-3).
        //! A test marked 0.5 exercises what only the 0.5 traversal admits, and
        //! goes with it (#3447).
        use std::collections::BTreeMap;
        use yamaa_core::inheritance::*;
        use yamaa_core::language::LanguageVersion;
        use yamaa_core::schema::{Document, NormalizationError, SchemaModule, SchemaStructure};

        use crate::support::document::Tree as V;

        fn text(value: &str) -> V {
            V::text(value)
        }
        fn document(value: V) -> Document {
            value.document()
        }
        fn field(name: &'static str, types: V) -> V {
            V::map(vec![(name, V::map(vec![("type", types)]))])
        }
        /// The layers of both languages: `yamaa` and `type` head a 1.x layer,
        /// and `schema_version` a 0.5 one.
        fn schema() -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("schema_version", text("str")),
                                field("yamaa", text("str")),
                                field("type", text("str")),
                                field("parent", text("str")),
                                field("label", text("str")),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root_class",
            )
            .unwrap()
        }
        /// A 1.x layer declaring version 1.0, naming `parent` when given.
        fn layer(parent: Option<&str>) -> Document {
            layer_with(parent, vec![])
        }
        fn layer_with(parent: Option<&str>, more: Vec<(&'static str, V)>) -> Document {
            let mut fields = vec![("yamaa", text("1.0")), ("type", text("spec"))];
            if let Some(parent) = parent {
                fields.push(("parent", text(parent)));
            }
            fields.extend(more);
            document(V::map(fields))
        }
        /// A 0.5 layer declaring `version`.
        fn layer_v0_5(version: &str, parent: Option<&str>) -> Document {
            let mut fields = vec![("schema_version", text(version))];
            if let Some(parent) = parent {
                fields.push(("parent", text(parent)));
            }
            document(V::map(fields))
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
            /// The 1.x layer `name`, naming `parent`.
            fn with(mut self, name: &str, parent: Option<&str>) -> Self {
                self.documents.insert(name.into(), layer(parent));
                self
            }
        }
        impl InheritanceSourcePort for Port {
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
        /// The 1.x chain from the entry `input`, root ancestor first.
        fn run(
            input: Document,
            port: &mut Port,
        ) -> Result<Vec<VersionedLayer>, Error<(&'static str, u32)>> {
            traverse_versioned(
                &schema(),
                LanguageVersion::new(0),
                source("entry"),
                input,
                port,
            )
            .map(|chain| chain.layers)
        }
        /// The 0.5 chain from the entry `input`.
        fn run_v0_5(
            input: Document,
            port: &mut Port,
        ) -> Result<Vec<Layer>, Error<(&'static str, u32)>> {
            traverse(&schema(), source("entry"), input, port)
        }
        /// `error`'s projection: a validation finding of `condition` and
        /// `rule` at `path`, with each text context value of `fields`.
        fn check(
            error: &Error<(&str, u32)>,
            condition: &str,
            rule: &str,
            path: &str,
            fields: &[(&str, &str)],
        ) {
            use yamaa_core::json::Value as C;
            use yamaa_core::value::Value as S;
            let diagnostic = error.diagnostic().unwrap().unwrap();
            let definition = diagnostic.definition();
            assert_eq!(
                (definition.phase, definition.condition, definition.rule),
                ("validation", condition, Some(rule))
            );
            assert_eq!(diagnostic.spec_paths, [path]);
            for &(name, value) in fields {
                assert_eq!(diagnostic.context[name], C::from(S::Str(value.into())));
            }
        }

        #[test]
        fn chain_contributes_root_ancestor_first_and_reads_each_layer_once() {
            let mut port = Port::default()
                .with("a", Some("alias"))
                .with("common", None);
            port.aliases.insert("alias".into(), "common".into());
            let layers = run(layer(Some("a")), &mut port).unwrap();
            assert_eq!(
                layers
                    .iter()
                    .map(|l| l.source.identity.as_str())
                    .collect::<Vec<_>>(),
                ["common", "a", "entry"]
            );
            assert_eq!(
                port.calls,
                [
                    "resolve:entry:a",
                    "read:a",
                    "resolve:a:alias",
                    "read:common"
                ]
            );
            assert_eq!(layers[0].document, layer(None));
        }

        /// 0.5 only (#3447): a null `parent` is absent only in a 0.5 layer.
        /// A 1.x `parent` is a path, and null fails there (LAYER-9).
        #[test]
        fn a_null_parent_is_absent_and_ends_the_chain() {
            let null_parent = |version: &str| {
                document(V::map(vec![
                    ("schema_version", text(version)),
                    ("parent", V::Scalar(yamaa_core::schema::DocumentNode::Null)),
                ]))
            };
            let mut port = Port::default();
            port.documents.insert("a".into(), null_parent("1.0"));
            let layers = run_v0_5(layer_v0_5("1.0", Some("a")), &mut port).unwrap();
            assert_eq!(port.calls, ["resolve:entry:a", "read:a"]);
            assert_eq!(layers.len(), 2);
            let mut port = Port::default();
            let layers = run_v0_5(null_parent("1.0"), &mut port).unwrap();
            assert!(port.calls.is_empty());
            assert_eq!(layers.len(), 1);
        }

        #[test]
        fn cycles_report_exact_canonical_loop_before_any_repeated_read() {
            for target in ["entry", "a"] {
                let mut port = Port::default()
                    .with("a", Some("b"))
                    .with("b", Some("alias"));
                port.aliases.insert("alias".into(), target.into());
                let error = run(layer(Some("a")), &mut port).unwrap_err();
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
        fn layer_findings_precede_all_parent_effects() {
            let input = layer_with(Some("https://bad"), vec![("unexpected", text("x"))]);
            let mut port = Port::default();
            let Error::Layer {
                source,
                entry,
                input: _,
                error: NormalizationError::Invalid(findings),
            } = run(input, &mut port).unwrap_err()
            else {
                panic!("expected structural findings")
            };
            assert_eq!(source, "entry");
            assert_eq!(entry, "entry");
            assert_eq!(findings[0].definition().condition, "unknown_field");
            assert!(port.calls.is_empty());
        }

        /// 0.5 only (#3447): the 0.5 traversal checks `schema_version` after
        /// a layer's findings. A 1.x layer's header is read first (SCHEMA-2);
        /// its findings and parent effects are the test above.
        #[test]
        fn layer_findings_precede_version_mismatch_and_all_parent_effects() {
            let input = document(V::map(vec![
                ("schema_version", text("0")),
                ("parent", text("https://bad")),
                ("unexpected", text("x")),
            ]));
            let mut port = Port::default();
            let Error::Layer {
                source,
                entry,
                input: _,
                error: NormalizationError::Invalid(findings),
            } = run_v0_5(input, &mut port).unwrap_err()
            else {
                panic!("expected structural findings")
            };
            assert_eq!(source, "entry");
            assert_eq!(entry, "entry");
            assert_eq!(findings[0].definition().condition, "unknown_field");
            assert!(port.calls.is_empty());
        }

        /// 0.5 only (#3447): a 0.5 chain's version must match the schema's
        /// exactly. A 1.x layer may declare any version the engine reads
        /// (SCHEMA-2, LAYER-4).
        #[test]
        fn version_checks_stop_before_reading_a_parent() {
            let mut port = Port::default();
            assert_eq!(
                run_v0_5(layer_v0_5("0", Some("a")), &mut port).unwrap_err(),
                Error::Version {
                    source: "entry".into(),
                    entry: "entry".into(),
                    expected: "1.0".into(),
                    actual: "0".into(),
                    is_entry: true
                }
            );
            assert!(port.calls.is_empty());
            port.documents
                .insert("a".into(), layer_v0_5("0", Some("unread")));
            assert_eq!(
                run_v0_5(layer_v0_5("1.0", Some("a")), &mut port).unwrap_err(),
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

        /// 0.5 only (#3447): `schema_version_mismatch` retires with 0.5. A
        /// 1.x layer without `yamaa` fails at its header (SCHEMA-2).
        #[test]
        fn missing_version_retains_layer_diagnostics_and_never_reads_parents() {
            let mut port = Port::default();
            let Error::Layer {
                error: NormalizationError::Invalid(findings),
                ..
            } = run_v0_5(document(V::map(vec![("parent", text("a"))])), &mut port).unwrap_err()
            else {
                panic!("expected missing version")
            };
            assert_eq!(
                findings[0].definition().condition,
                "schema_version_mismatch"
            );
            assert!(port.calls.is_empty());
        }

        #[test]
        fn a_non_mapping_entry_is_a_finding_that_holds_its_raw_arena() {
            use yamaa_core::diagnostic::ConditionCode;
            use yamaa_core::schema::{
                DocumentKind, DocumentNode as N, SchemaContext as C, SchemaDiagnostic,
            };
            let raw = Document::new(vec![N::Integer("7".into())], 0).unwrap();
            let pointer = raw.nodes().as_ptr();
            let mut port = Port::default();
            let Error::Layer {
                source,
                entry,
                input,
                error: NormalizationError::Invalid(findings),
            } = run(raw, &mut port).unwrap_err()
            else {
                panic!("expected a finding in the entry")
            };
            assert_eq!((source.as_str(), entry.as_str()), ("entry", "entry"));
            assert_eq!(input.nodes().as_ptr(), pointer);
            assert_eq!(
                findings,
                [SchemaDiagnostic {
                    path: "$".into(),
                    code: ConditionCode::SchemaInvalidFieldType,
                    context: vec![
                        ("expected", C::Text("root_class".into())),
                        ("actual", C::ValueKind(DocumentKind::Integer)),
                    ],
                }]
            );
            assert!(port.calls.is_empty());
        }

        #[test]
        fn malformed_parent_admission_precedes_its_descendants() {
            let mut port = Port::default();
            port.documents.insert(
                "a".into(),
                layer_with(Some("unread"), vec![("unexpected", text("x"))]),
            );
            assert!(
                matches!(run(layer(Some("a")), &mut port), Err(Error::Layer { source, .. }) if source == "a")
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
                    run(layer(Some(path)), &mut port).unwrap_err(),
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
                run(layer(Some("missing")), &mut port).unwrap_err(),
                Error::Unavailable {
                    declaring: "entry".into(),
                    path: "missing".into()
                }
            );
            assert_eq!(port.calls, ["resolve:entry:missing"]);
            let mut port = Port::default();
            assert_eq!(
                run(layer(Some("missing")), &mut port).unwrap_err(),
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
                run(layer(Some("raise")), &mut port).unwrap_err(),
                Error::Host(("opaque", 73))
            );
            assert_eq!(port.calls, ["resolve:entry:raise"]);
        }

        #[test]
        fn real_traversal_failures_project_canonical_causes_and_original_provenance() {
            use yamaa_core::json::Value as C;
            use yamaa_core::value::Value as S;
            let mut port = Port::default();
            let error = run(layer(Some("https://bad")), &mut port).unwrap_err();
            check(
                &error,
                "invalid_parent_path",
                "LAYER-2",
                "parent",
                &[
                    ("source", "entry"),
                    ("parent", "https://bad"),
                    ("reason", "remote_reference"),
                ],
            );
            assert!(port.calls.is_empty());

            let mut port = Port {
                unavailable: Some("missing"),
                ..Port::default()
            };
            let error = run(layer(Some("missing")), &mut port).unwrap_err();
            check(
                &error,
                "parent_not_found",
                "LAYER-2",
                "parent",
                &[("source", "entry"), ("path", "missing")],
            );
            assert_eq!(port.calls, ["resolve:entry:missing"]);

            for target in ["entry", "a"] {
                let mut port = Port::default()
                    .with("a", Some("b"))
                    .with("b", Some("alias"));
                port.aliases.insert("alias".into(), target.into());
                let error = run(layer(Some("a")), &mut port).unwrap_err();
                check(
                    &error,
                    "inheritance_cycle",
                    "LAYER-3",
                    "parent",
                    &[
                        ("source", "b"),
                        (
                            "reason",
                            if target == "entry" {
                                "parent_chain_returns_to_entry"
                            } else {
                                "parent_chain_returns_to_active_layer"
                            },
                        ),
                    ],
                );
                let expected = if target == "entry" {
                    vec!["entry", "a", "b", "entry"]
                } else {
                    vec!["a", "b", "a"]
                };
                assert_eq!(
                    error.diagnostic().unwrap().unwrap().context["cycle"],
                    C::Array(
                        expected
                            .into_iter()
                            .map(|s| C::from(S::Str(s.into())))
                            .collect()
                    )
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

        /// 0.5 only (#3447): `schema_version_mismatch` retires with 0.5. A
        /// 1.x layer's unread version fails at its header (SCHEMA-2,
        /// LAYER-4).
        #[test]
        fn real_version_mismatches_project_canonical_causes_and_original_provenance() {
            let mut port = Port::default();
            let error = run_v0_5(layer_v0_5("0", Some("later")), &mut port).unwrap_err();
            check(
                &error,
                "schema_version_mismatch",
                "SCHEMA-2",
                "schema_version",
                &[
                    ("source", "entry"),
                    ("entry", "entry"),
                    ("expected", "1.0"),
                    ("actual", "0"),
                ],
            );
            assert!(port.calls.is_empty());
            port.documents
                .insert("a".into(), layer_v0_5("0", Some("unread")));
            let error = run_v0_5(layer_v0_5("1.0", Some("a")), &mut port).unwrap_err();
            check(
                &error,
                "schema_version_mismatch",
                "LAYER-4",
                "parent",
                &[
                    ("source", "a"),
                    ("entry", "entry"),
                    ("entry_version", "1.0"),
                    ("parent_version", "0"),
                ],
            );
            assert_eq!(port.calls, ["resolve:entry:a", "read:a"]);
        }

        #[test]
        fn unclassified_failures_and_contradictory_cycles_have_no_semantic_projection() {
            struct Opaque(Box<u8>);
            let payload = Box::new(73);
            let address = &*payload as *const u8;
            let error = Error::Host(Opaque(payload));
            assert!(error.diagnostic().unwrap().is_none());
            let yamaa_core::inheritance_outcome::Outcome::Original(Error::Host(Opaque(payload))) =
                yamaa_core::inheritance_outcome::traversal_outcome(error).unwrap()
            else {
                panic!("original host failure replaced")
            };
            assert_eq!(&*payload as *const u8, address);
            for path in [vec![], vec!["one".into()], vec!["a".into(), "b".into()]] {
                assert!(Error::<()>::Cycle {
                    path,
                    returns_to_entry: true
                }
                .diagnostic()
                .is_err());
            }
            assert!(Error::<()>::InvalidIdentity.diagnostic().unwrap().is_none());
        }

        #[test]
        fn long_source_failure_and_closed_cycle_keep_complete_context_and_original_error() {
            let error = Error::<()>::Unavailable {
                declaring: "entry".into(),
                path: "p".repeat(512),
            };
            for _ in 0..2 {
                let diagnostic = error.diagnostic().unwrap().unwrap();
                assert_eq!(
                    diagnostic.context["path"],
                    yamaa_core::json::Value::from("p".repeat(512))
                );
            }
            let Error::Unavailable { declaring, path } = &error else {
                panic!()
            };
            assert_eq!(declaring, "entry");
            assert_eq!(path.len(), 512);
            let cycle = Error::<()>::Cycle {
                path: vec!["entry".into(), "base".into(), "entry".into()],
                returns_to_entry: true,
            };
            assert_eq!(
                cycle.diagnostic().unwrap().unwrap().context["cycle"],
                yamaa_core::json::Value::from(
                    ["entry", "base", "entry"].map(yamaa_core::json::Value::from)
                )
            );
        }

        #[test]
        fn fixed_parent_occurrence_boundary_counts_chain_links_before_entering_the_next_port() {
            use yamaa_core::limits::{LimitExceeded, Limits};
            for count in [
                Limits::PARENT_OCCURRENCES - 1,
                Limits::PARENT_OCCURRENCES,
                Limits::PARENT_OCCURRENCES + 1,
            ] {
                // A chain entry -> p1 -> ... -> p{count} of distinct layers.
                let mut port = Port::default();
                for index in 1..=count {
                    let parent = (index < count).then(|| format!("p{}", index + 1));
                    port.documents
                        .insert(format!("p{index}"), layer(parent.as_deref()));
                }
                let result = run(layer(Some("p1")), &mut port);
                if count <= Limits::PARENT_OCCURRENCES {
                    let layers = result.unwrap();
                    assert_eq!(layers.len(), count + 1);
                    assert_eq!(layers[0].source.identity, format!("p{count}"));
                    assert_eq!(layers[count].source.identity, "entry");
                } else {
                    assert!(matches!(result, Err(Error::Resource(LimitExceeded {
                resource: "parent_occurrences", limit: Limits::PARENT_OCCURRENCES, actual: Some(actual)
            })) if actual == count));
                }
                for prefix in ["resolve:", "read:"] {
                    assert_eq!(
                        port.calls
                            .iter()
                            .filter(|event| event.starts_with(prefix))
                            .count(),
                        count.min(Limits::PARENT_OCCURRENCES)
                    );
                }
            }
        }
    }
    mod inheritance_capture {
        //! A 1.x chain's parents are read through the host's port as
        //! `traverse_versioned` asks for them, and each layer's relative paths
        //! are respelled through the same port as `prepare_versioned` resolves
        //! the chain (LAYER-3, LAYER-7, LAYER-10). A test marked 0.5 exercises
        //! `inheritance_capture::prepare`, the 0.5 capture application with
        //! its catalog of captured parents, and goes with it (#3447).
        use crate::support::document::Tree;
        use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
        use yamaa_core::language::LanguageVersion;
        use yamaa_core::schema::{
            Document, DocumentNode as N, NormalizationError, SchemaModule, SchemaSource,
            SchemaStructure,
        };
        use yamaa_core::{
            inheritance as graph, inheritance_capture as app, inheritance_preparation as lifecycle,
            project_preparation::Source,
            resolver::{self, Captured, Name, Resolver},
        };

        fn text(s: &str) -> Tree {
            Tree::text(s)
        }
        fn map(v: Vec<(&'static str, Tree)>) -> Tree {
            Tree::map(v)
        }
        fn seq(v: Vec<Tree>) -> Tree {
            Tree::List(v)
        }
        fn document(tree: Tree) -> Document {
            tree.document()
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
                                // `yamaa` and `type` head a 1.x layer, and
                                // `schema_version` a 0.5 one.
                                field("schema_version", "str"),
                                field("yamaa", "str"),
                                field("type", "str"),
                                field("parent", "str"),
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
                                field("report", "str"),
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
        /// The 1.x layer `name`: its input `SRC` names its own data file and
        /// schema link, and the entry declares the rest of a specification.
        fn layer(name: &str, parent: Option<&str>, entry: bool) -> Document {
            layer_headed(
                vec![("yamaa", text("1.0")), ("type", text("spec"))],
                name,
                parent,
                entry,
            )
        }
        /// The same layer in 0.5.
        fn layer_v0_5(name: &str, parent: Option<&str>, entry: bool) -> Document {
            layer_headed(vec![("schema_version", text("1.0"))], name, parent, entry)
        }
        fn layer_headed(
            header: Vec<(&'static str, Tree)>,
            name: &str,
            parent: Option<&str>,
            entry: bool,
        ) -> Document {
            let mut fields = header;
            if let Some(parent) = parent {
                fields.push(("parent", text(parent)));
            }
            fields.push((
                "input",
                map(vec![(
                    "SRC",
                    map(vec![
                        ("path", text(&format!("{name}.csv"))),
                        ("schema", text(&format!("{name}-source.yaml"))),
                    ]),
                )]),
            ));
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
                            ("report", text("report.csv")),
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
            Capture(String),
            Decode(String, Vec<u8>),
            Rebase(String, String, String),
        }
        type Log = Rc<RefCell<Vec<Event>>>;
        struct Host {
            log: Log,
            bytes: BTreeMap<String, Vec<u8>>,
            aliases: BTreeMap<String, String>,
            failure: Option<(usize, Payload)>,
            unavailable: Option<usize>,
            /// Whether the last request failed as unavailable.
            missing: bool,
        }
        impl Host {
            fn failure(&mut self, index: usize) -> Option<Payload> {
                if self.failure.as_ref().is_some_and(|(at, _)| *at == index) {
                    self.failure.take().map(|(_, p)| p)
                } else {
                    None
                }
            }
            /// The scripted failure of request `index`: its payload, or an
            /// unavailable file.
            fn refusal(&mut self, index: usize) -> Option<Payload> {
                self.missing = self.unavailable == Some(index);
                self.failure(index).or_else(|| {
                    self.missing.then(|| Payload {
                        identity: Rc::new(()),
                        allocation: Box::new(0),
                    })
                })
            }
        }
        impl Resolver for Host {
            type Error = Payload;
            fn inspect(&mut self, name: Name<'_>) -> Result<graph::Source, Payload> {
                let Name::Written {
                    declaring, written, ..
                } = name
                else {
                    panic!("unexpected request {name:?}")
                };
                let index = self.log.borrow().len();
                self.log
                    .borrow_mut()
                    .push(Event::Resolve(declaring.into(), written.into()));
                if let Some(error) = self.refusal(index) {
                    return Err(error);
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
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Payload> {
                let Name::Located { source, .. } = name else {
                    panic!("unexpected request {name:?}")
                };
                let index = self.log.borrow().len();
                self.log
                    .borrow_mut()
                    .push(Event::Capture(source.identity.clone()));
                if let Some(error) = self.refusal(index) {
                    return Err(error);
                }
                let bytes = self
                    .bytes
                    .remove(&source.identity)
                    .expect("only one capture per identity");
                Ok(Captured::resource(bytes.into(), true))
            }
            fn failure(&self, _: Name<'_>, _: &Payload) -> Option<resolver::Failure> {
                self.missing.then_some(resolver::Failure::Unreadable)
            }
            fn rebase(
                &mut self,
                layer: &graph::Source,
                entry: &graph::Source,
                written: &str,
            ) -> Result<String, Payload> {
                let index = self.log.borrow().len();
                self.log.borrow_mut().push(Event::Rebase(
                    layer.identity.clone(),
                    entry.identity.clone(),
                    written.into(),
                ));
                if let Some(error) = self.failure(index) {
                    return Err(error);
                }
                Ok(written.into())
            }
        }
        enum CodecFailure {
            Original(Payload),
        }
        struct Decoder {
            log: Log,
            documents: BTreeMap<String, Raw>,
            failure: Option<Payload>,
        }
        impl app::InheritanceDecodePort for Decoder {
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
                missing: false,
            };
            let decoder = Decoder {
                log,
                documents: [
                    ("b", layer_v0_5("b", Some("a"), false)),
                    ("a", layer_v0_5("a", Some("alias"), false)),
                    ("common", layer_v0_5("common", None, false)),
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
                    document: layer_v0_5("entry", Some("b"), true),
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
        ) -> app::PreparationResult<Payload, CodecFailure, Raw> {
            app::prepare(
                &schema(),
                source,
                "written/entry".into(),
                raw,
                host,
                decoder,
            )
        }
        fn expected_effects() -> Vec<Event> {
            let mut events = vec![
                Event::Resolve("entry".into(), "b".into()),
                Event::Capture("b".into()),
                Event::Decode("b".into(), vec![4, 5, 6]),
                Event::Resolve("b".into(), "a".into()),
                Event::Capture("a".into()),
                Event::Decode("a".into(), vec![1, 2]),
                Event::Resolve("a".into(), "alias".into()),
                Event::Capture("common".into()),
                Event::Decode("common".into(), vec![3]),
            ];
            for (layer, paths) in [
                ("common", vec!["common.csv", "common-source.yaml"]),
                ("a", vec!["a.csv", "a-source.yaml"]),
                ("b", vec!["b.csv", "b-source.yaml"]),
                (
                    "entry",
                    vec!["entry.csv", "entry-source.yaml", "output.csv", "report.csv"],
                ),
            ] {
                for path in paths {
                    events.push(Event::Rebase(layer.into(), "entry".into(), path.into()));
                }
            }
            events
        }
        /// 0.5 only (#3447): the catalog of captured parents is the 0.5 capture
        /// application's. Its effects, layers and defaults on a 1.x chain are
        /// `complete_effects_defaults_and_distinct_orders_are_engine_owned`.
        #[test]
        fn complete_effects_catalog_allocations_defaults_and_distinct_orders_are_engine_owned() {
            for _ in 0..2 {
                let (source, raw, mut host, mut decoder) = fixture();
                let held: Vec<_> = ["b", "a", "common"]
                    .into_iter()
                    .map(|name| {
                        let d = &decoder.documents[name];
                        // A parent's bytes are copied out of the resolver's
                        // snapshot; its decoded arena keeps its allocation.
                        (
                            name,
                            host.bytes[name].clone(),
                            d.document.nodes().as_ptr(),
                            (&*d.marker) as *const u32,
                        )
                    })
                    .collect();
                let prepared = match run(&source, &raw, &mut host, &mut decoder) {
                    Ok(p) => p,
                    Err(app::Error::Preparation { error, .. }) => match *error {
                        lifecycle::Error::Traversal(graph::Error::Layer { error, .. }) => {
                            panic!("fixture layer: {error:?}")
                        }
                        _ => panic!("fixture later phase"),
                    },
                };
                assert_eq!(*host.log.borrow(), expected_effects());
                assert_eq!(
                    prepared
                        .parents
                        .iter()
                        .map(|p| p.source().identity.as_str())
                        .collect::<Vec<_>>(),
                    ["b", "a", "common"]
                );
                for (parent, (name, bytes, nodes, marker)) in prepared.parents.iter().zip(held) {
                    assert_eq!(parent.source().identity, name);
                    assert_eq!(parent.source().bytes, bytes);
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

        /// 0.5 only (#3447): the catalog prefix is the 0.5 capture
        /// application's. Its opaque effect errors on a 1.x chain are
        /// `every_opaque_effect_error_keeps_original_allocation_and_exact_completed_prefix`.
        #[test]
        fn every_opaque_effect_error_keeps_original_allocation_and_exact_completed_catalog_prefix()
        {
            for index in [0, 1, 3, 4, 6, 7, 9, 10, 18] {
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
                }) = run(&source, &raw, &mut host, &mut decoder)
                else {
                    panic!()
                };
                let original = match *error {
                    lifecycle::Error::Traversal(graph::Error::Host(app::SourceFailure::Host(
                        p,
                    )))
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
        /// 0.5 only (#3447): the decoder is the 0.5 capture application's own
        /// port. A 1.x port's failed read is a test below, and its
        /// unavailable parents are
        /// `inheritance::source_failure_retains_written_spelling_and_stops_later_access`.
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
            }) = run(&source, &raw, &mut host, &mut decoder)
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
                }) = run(&source, &raw, &mut host, &mut decoder)
                else {
                    panic!()
                };
                let lifecycle::Error::Traversal(graph::Error::Unavailable { declaring, path }) =
                    *error
                else {
                    panic!()
                };
                assert_eq!(declaring, "entry");
                assert_eq!(path, if index == 0 { "b" } else { "display/b" });
                assert!(captured_parents.is_empty());
                assert_eq!(&*host.log.borrow(), &expected_effects()[..=index]);
            }
        }
        /// 0.5 only (#3447): the 0.5 capture application keeps each parent it
        /// decoded in its catalog. A 1.x parent's failed admission is
        /// `inheritance::malformed_parent_admission_precedes_its_descendants`.
        #[test]
        fn admitted_parent_raw_is_retained_when_later_normalization_fails_before_descendants() {
            let (source, raw, mut host, mut decoder) = fixture();
            let bad = document(map(vec![
                ("schema_version", text("1.0")),
                ("unknown", text("kept")),
                ("parent", text("not-read")),
            ]));
            let nodes = bad.nodes().as_ptr();
            decoder.documents.get_mut("b").unwrap().document = bad;
            let Err(app::Error::Preparation {
                error,
                captured_parents,
            }) = run(&source, &raw, &mut host, &mut decoder)
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
            assert_eq!((source.as_str(), entry.as_str()), ("b", "entry"));
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].definition().condition, "unknown_field");
            assert_eq!(captured_parents.len(), 1);
            assert_eq!(captured_parents[0].raw().document.nodes().as_ptr(), nodes);
            assert_eq!(&*host.log.borrow(), &expected_effects()[..3]);
        }

        /// What resolving a 1.x chain asks of its host's port, in order.
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum Effect {
            Resolve(String, String),
            Read(String),
            Rebase(String, String, String),
        }
        /// A host's port over 1.x layers held by identity. It resolves a
        /// written parent, reads a parent, and respells a path from the
        /// entry; one effect may raise an opaque payload instead.
        struct Chain {
            log: Vec<Effect>,
            documents: BTreeMap<String, Document>,
            aliases: BTreeMap<String, String>,
            failure: Option<(usize, Payload)>,
        }
        impl Chain {
            /// The entry's parents: `b`, then `a`, whose `alias` resolves to
            /// the root ancestor `common`.
            fn new() -> Self {
                Self {
                    log: Vec::new(),
                    documents: [
                        ("b", layer("b", Some("a"), false)),
                        ("a", layer("a", Some("alias"), false)),
                        ("common", layer("common", None, false)),
                    ]
                    .into_iter()
                    .map(|(name, document)| (name.into(), document))
                    .collect(),
                    aliases: [("alias".into(), "common".into())].into_iter().collect(),
                    failure: None,
                }
            }
            /// Record `effect`, and the payload it raises, if any.
            fn effect(&mut self, effect: Effect) -> Option<Payload> {
                let index = self.log.len();
                self.log.push(effect);
                if self.failure.as_ref().is_some_and(|(at, _)| *at == index) {
                    self.failure.take().map(|(_, payload)| payload)
                } else {
                    None
                }
            }
        }
        impl graph::InheritanceSourcePort for Chain {
            type Error = Payload;
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<graph::Source, graph::SourceError<Payload>> {
                if let Some(payload) =
                    self.effect(Effect::Resolve(declaring.into(), written.into()))
                {
                    return Err(graph::SourceError::Raised(payload));
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
            fn read(
                &mut self,
                source: &graph::Source,
            ) -> Result<Document, graph::SourceError<Payload>> {
                if let Some(payload) = self.effect(Effect::Read(source.identity.clone())) {
                    return Err(graph::SourceError::Raised(payload));
                }
                Ok(self
                    .documents
                    .remove(&source.identity)
                    .expect("each parent is read once"))
            }
        }
        impl lifecycle::PathPort for Chain {
            fn rebase(
                &mut self,
                layer: &graph::Source,
                entry: &graph::Source,
                written: &str,
            ) -> Result<String, Payload> {
                match self.effect(Effect::Rebase(
                    layer.identity.clone(),
                    entry.identity.clone(),
                    written.into(),
                )) {
                    Some(payload) => Err(payload),
                    None => Ok(written.into()),
                }
            }
        }
        /// Why a 1.x chain did not resolve.
        enum Failure {
            Traversal(graph::Error<Payload>),
            Resolution(lifecycle::VersionedError<Payload>),
        }
        /// Traverse the entry's chain and resolve it, as a 1.x
        /// specification's preparation does.
        fn resolve(
            chain: &mut Chain,
        ) -> Result<(graph::VersionedChain, lifecycle::ResolvedVersioned), Failure> {
            let schema = schema();
            let entry = graph::Source {
                identity: "entry".into(),
                display_path: "written/entry".into(),
            };
            let layers = graph::traverse_versioned(
                &schema,
                LanguageVersion::new(0),
                entry,
                layer("entry", Some("b"), true),
                chain,
            )
            .map_err(Failure::Traversal)?;
            let resolved = lifecycle::prepare_versioned(&schema, &layers, chain)
                .map_err(Failure::Resolution)?;
            Ok((layers, resolved))
        }
        /// Every effect of resolving the chain: each parent resolved, then
        /// read, from the entry up; then each layer's relative paths
        /// respelled from the entry, root ancestor first (LAYER-10).
        fn effects() -> Vec<Effect> {
            let mut effects = vec![
                Effect::Resolve("entry".into(), "b".into()),
                Effect::Read("b".into()),
                Effect::Resolve("b".into(), "a".into()),
                Effect::Read("a".into()),
                Effect::Resolve("a".into(), "alias".into()),
                Effect::Read("common".into()),
            ];
            for (layer, paths) in [
                ("common", vec!["common.csv", "common-source.yaml"]),
                ("a", vec!["a.csv", "a-source.yaml"]),
                ("b", vec!["b.csv", "b-source.yaml"]),
                (
                    "entry",
                    vec!["entry.csv", "entry-source.yaml", "output.csv", "report.csv"],
                ),
            ] {
                for path in paths {
                    effects.push(Effect::Rebase(layer.into(), "entry".into(), path.into()));
                }
            }
            effects
        }

        #[test]
        fn complete_effects_defaults_and_distinct_orders_are_engine_owned() {
            for _ in 0..2 {
                let mut chain = Chain::new();
                let Ok((layers, resolved)) = resolve(&mut chain) else {
                    panic!("the chain resolves")
                };
                assert_eq!(chain.log, effects());
                // Parents are read from the entry up, and the layers
                // contribute root ancestor first (LAYER-3).
                assert_eq!(
                    layers
                        .layers
                        .iter()
                        .map(|layer| layer.source.identity.as_str())
                        .collect::<Vec<_>>(),
                    ["common", "a", "b", "entry"]
                );
                let d = &resolved.document;
                let columns = d.field(d.root(), "columns").unwrap();
                let N::Sequence(columns) = &d.nodes()[columns] else {
                    panic!()
                };
                let label = d.field(columns[0], "label").unwrap();
                assert_eq!(d.nodes()[label], N::Text("Default label".into()));
                let origin = resolved.origins[label];
                assert_eq!(origin.layer, None);
                assert!(matches!(
                    origin.origin.source,
                    SchemaSource::Default { module: 0, .. }
                ));
                assert_eq!(resolved.origins.len(), d.nodes().len());
            }
        }

        #[test]
        fn every_opaque_effect_error_keeps_original_allocation_and_exact_completed_prefix() {
            // Each parent's resolution and read, and the first, second and
            // last respelling.
            for index in [0, 1, 2, 3, 4, 5, 6, 7, 15] {
                let mut chain = Chain::new();
                let identity = Rc::new(());
                let allocation = Box::new(99);
                let pointer = (&*allocation) as *const u32;
                chain.failure = Some((
                    index,
                    Payload {
                        identity: identity.clone(),
                        allocation,
                    },
                ));
                let original = match resolve(&mut chain) {
                    Err(Failure::Traversal(graph::Error::Host(payload)))
                    | Err(Failure::Resolution(lifecycle::VersionedError::Path(payload))) => payload,
                    _ => panic!("effect {index} raises its payload"),
                };
                assert!(Rc::ptr_eq(&original.identity, &identity));
                assert_eq!((&*original.allocation) as *const u32, pointer);
                assert_eq!(chain.log, &effects()[..=index]);
            }
        }
    }
}
