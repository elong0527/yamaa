//! Preparation contracts.
#[allow(dead_code)]
mod support;
mod preparation {
    mod dictionary_preparation {
        //! Independent logical arenas and opaque effects exercise capture ownership.
        use crate::support::document::Tree as V;
        use std::rc::Rc;
        use yamaa_core::resource::ResourceFailure;
        use yamaa_core::schema::{
            Document, DocumentNode as N, SchemaModule, SchemaStructure, SpecificationDocument,
        };
        use yamaa_core::{
            dictionary_preparation as app,
            inheritance::{Source as Identity, SourceError},
            inheritance_capture::{CapturePort, DecodePort},
            project_preparation::Source,
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
                                field("schema_version", "str"),
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
                ("schema_version", V::text("1.0")),
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
        impl CapturePort for Port {
            type Error = Host;
            fn canonicalize(&mut self, _: &str, _: &str) -> Result<Identity, SourceError<Host>> {
                panic!("parent route entered")
            }
            fn capture(&mut self, _: &Identity) -> Result<Vec<u8>, SourceError<Host>> {
                panic!("parent route entered")
            }
            fn canonicalize_resource(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<Identity, SourceError<Host>> {
                self.events.push(format!("resolve:{declaring}:{written}"));
                if let Some(tag) = self.fail {
                    return Err(SourceError::Raised(Host {
                        tag,
                        owner: self.owner.clone(),
                    }));
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
            fn capture_resource(
                &mut self,
                source: &Identity,
            ) -> Result<Vec<u8>, SourceError<Host>> {
                self.events.push(format!("capture:{}", source.identity));
                Ok(vec![if source.identity == "later" { 2 } else { 1 }])
            }
            fn resource_failure(&self, error: &Host) -> Option<ResourceFailure> {
                match error.tag {
                    "missing" => Some(ResourceFailure::Missing),
                    "directory" => Some(ResourceFailure::NotRegularFile),
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
        impl DecodePort for Decoder {
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
        fn model(paths: &[&'static str]) -> SpecificationDocument {
            let doc = document(input(
                paths
                    .iter()
                    .enumerate()
                    .map(|(i, path)| column(if i == 0 { "X" } else { "Y" }, mapping(V::text(path))))
                    .collect(),
                vec![],
            ));
            SpecificationDocument::admit(doc).unwrap().unwrap()
        }
        #[test]
        fn aliases_resolve_in_use_order_but_capture_and_parse_once_without_mutating_original() {
            let schema = schema();
            let model = model(&["dict.yaml", "./dict.yaml"]);
            let original = model.document().clone();
            let mut port = Port::new();
            let mut decoder = Decoder::new();
            let prepared = app::prepare(
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
            assert_eq!(model.document(), &original);
            assert_ne!(prepared.model.document(), model.document());
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
            let prepared = app::prepare(
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
                let rejected = app::prepare(
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
            let rejected = app::prepare(
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
            let rejected = app::prepare(
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
        use crate::support::document::Tree;
        use std::{collections::BTreeMap, rc::Rc};
        use yamaa_core::schema::{
            Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
            SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure,
        };
        use yamaa_core::{
            document_preparation as app, inheritance as graph, inheritance_capture as capture,
            inheritance_preparation as lifecycle, project_preparation::Source,
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
            fn capture(
                &mut self,
                source: &graph::Source,
            ) -> Result<Vec<u8>, graph::SourceError<Payload>> {
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
    }
    mod file_preparation {
        //! Typed effect/order witnesses use independent retained parents and native policy.
        use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
        use yamaa_core::{
            file_preparation as app,
            inheritance::{Source as Identity, SourceError},
            inheritance_capture::CapturePort,
            project_preparation::Source,
        };
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum Event {
            Resolve(Option<String>, String),
            Entry(Option<String>, String),
            Service,
            Canonical(String, String),
            Parent(String, String),
            Alias(String, String),
            Base(String),
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
        impl app::NativePort for Native {
            type Error = Fault;
            fn resolve_entry(
                &mut self,
                declaring: Option<&str>,
                written: &str,
            ) -> Result<String, Fault> {
                self.event(Event::Resolve(declaring.map(Into::into), written.into()))?;
                Ok("/study/entry".into())
            }
            fn capture_entry(
                &mut self,
                declaring: Option<&str>,
                written: &str,
            ) -> Result<Vec<u8>, Fault> {
                self.event(Event::Entry(declaring.map(Into::into), written.into()))?;
                Ok(self.entry.take().unwrap())
            }
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<Identity, SourceError<Fault>> {
                self.event(Event::Canonical(declaring.into(), written.into()))
                    .map_err(SourceError::Raised)?;
                let identity = if written.starts_with('b') { "B" } else { "A" }.to_string();
                let display_path = format!("{declaring}/{written}");
                self.identity_pointer = Some(identity.as_ptr());
                self.display_pointer = Some(display_path.as_ptr());
                Ok(Identity {
                    identity,
                    display_path,
                })
            }

            fn capture_parent(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<Vec<u8>, SourceError<Fault>> {
                self.event(Event::Parent(declaring.into(), written.into()))
                    .map_err(SourceError::Raised)?;
                let key = if written.starts_with('b') { "B" } else { "A" };
                Ok(self.parents.remove(key).unwrap())
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
            fn accept_alias(&mut self, declaring: &str, written: &str) -> Result<(), Fault> {
                self.event(Event::Alias(declaring.into(), written.into()))
            }
            fn select_entry_base(&mut self, identity: &str) -> Result<(), Fault> {
                self.event(Event::Base(identity.into()))
            }
            fn missing_alias() -> Fault {
                Fault::new()
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
            source_pointer: Option<*const u8>,
            display_pointer: Option<*const u8>,
            parent_pointer: Option<*const u8>,
        }
        impl app::DocumentPort<Fault> for Documents {
            type Prepared = Prepared;
            type Error = Failure;
            fn prepare<P: CapturePort<Error = Fault>>(
                &mut self,
                source: Source,
                display: String,
                port: &mut P,
            ) -> Result<Prepared, Failure> {
                self.log.borrow_mut().push(Event::Service);
                assert_eq!(display, "/study/entry");
                self.source_pointer = Some(source.bytes.as_ptr());
                let mut parents = vec![];
                for written in &self.written {
                    let identity = match port.canonicalize(&source.identity, written) {
                        Ok(source) => source,
                        Err(SourceError::Raised(error)) => {
                            return Err(Failure::Parent(error));
                        }
                        Err(SourceError::Unavailable) => return Err(Failure::Unavailable),
                    };
                    self.display_pointer = Some(identity.display_path.as_ptr());
                    if parents
                        .iter()
                        .any(|p: &Source| p.identity == identity.identity)
                    {
                        continue;
                    }
                    let bytes = port.capture(&identity).map_err(|error| match error {
                        SourceError::Raised(error) => Failure::Parent(error),
                        SourceError::Unavailable => Failure::Unavailable,
                    })?;
                    self.parent_pointer = Some(bytes.as_ptr());
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
                    source_pointer: None,
                    display_pointer: None,
                    parent_pointer: None,
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
        fn exact_effect_tape_preserves_duplicates_first_capture_and_success_aliases_before_base() {
            let (mut native, mut documents, log) = setup(&["a", "./a", "a", "b"]);
            let root = native.entry.as_ref().unwrap().as_ptr();
            let a = native.parents["A"].as_ptr();
            let b = native.parents["B"].as_ptr();
            let Ok(prepared) = run(&mut native, &mut documents) else {
                panic!()
            };
            assert_eq!(prepared.source.bytes.as_ptr(), root);
            assert_eq!(prepared.parents[0].bytes.as_ptr(), a);
            assert_eq!(prepared.parents[1].bytes.as_ptr(), b);
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
                    Event::Base("/study/entry".into()),
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
                    Event::Base("/study/entry".into())
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
                    Err(app::Error::Resource(error))
                    | Err(app::Error::Preparation(Failure::Parent(error))) => error,
                    _ => panic!("native failure"),
                };
                assert_eq!(&*error.allocation as *const u32, pointer);
                assert_eq!(Rc::as_ptr(&error.marker), marker);
                assert_eq!(*log.borrow(), tape[..=step]);
            }
        }
        #[test]
        fn semantic_rejection_preserves_original_error_and_grants_no_alias_or_base_authority() {
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
            assert!(!log
                .borrow()
                .iter()
                .any(|e| matches!(e, Event::Alias(..) | Event::Base(..))));
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
        fn unknown_retained_catalog_identity_refuses_before_any_later_base_effect() {
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
        use std::collections::BTreeMap;
        use yamaa_core::inheritance::*;
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
                                field("parents", V::List(vec![text("str"), text("list[str]")])),
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
        fn layer(version: &str, parents: &[&str]) -> Document {
            document(V::map(vec![
                ("schema_version", text(version)),
                (
                    "parents",
                    V::List(parents.iter().map(|p| text(p)).collect()),
                ),
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
            traverse(&schema(), source("entry"), input, port)
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
            let input = document(V::map(vec![
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
            let input = document(V::map(vec![
                ("schema_version", text("0")),
                ("parents", V::List(vec![text("https://bad")])),
                ("unexpected", text("x")),
            ]));
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
            } = run(document(V::map(vec![("parents", text("a"))])), &mut port).unwrap_err()
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
        fn malformed_parent_admission_precedes_its_descendants() {
            let mut port = Port::default();
            port.documents.insert(
                "a".into(),
                document(V::map(vec![
                    ("schema_version", text("1.0")),
                    ("parents", V::List(vec![text("unread")])),
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
        fn real_traversal_failures_project_canonical_causes_and_original_provenance() {
            use yamaa_core::json::Value as C;
            use yamaa_core::value::Value as S;
            fn check(
                error: &Error<(&str, u32)>,
                condition: &str,
                requirement: &str,
                path: &str,
                fields: &[(&str, &str)],
            ) {
                let diagnostic = error.diagnostic().unwrap().unwrap();
                let definition = diagnostic.definition();
                assert_eq!(
                    (
                        definition.phase,
                        definition.condition,
                        definition.requirement
                    ),
                    ("validation", condition, Some(requirement))
                );
                assert_eq!(diagnostic.spec_paths, [path]);
                for &(name, value) in fields {
                    assert_eq!(diagnostic.context[name], C::from(S::Str(value.into())));
                }
            }
            let mut port = Port::default();
            let error = run(layer("1.0", &["https://bad", "later"]), &mut port).unwrap_err();
            check(
                &error,
                "invalid_parent_path",
                "REQ-0653",
                "parents",
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
            let error = run(layer("1.0", &["missing", "later"]), &mut port).unwrap_err();
            check(
                &error,
                "parent_not_found",
                "REQ-0654",
                "parents",
                &[("source", "entry"), ("path", "missing")],
            );
            assert_eq!(port.calls, ["resolve:entry:missing"]);

            for target in ["entry", "a"] {
                let mut port = Port::default().with("a", &["b"]).with("b", &["alias"]);
                port.aliases.insert("alias".into(), target.into());
                let error = run(layer("1.0", &["a", "later"]), &mut port).unwrap_err();
                check(
                    &error,
                    "inheritance_cycle",
                    "REQ-0655",
                    "parents",
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

            let mut port = Port::default();
            let error = run(layer("0", &["later"]), &mut port).unwrap_err();
            check(
                &error,
                "schema_version_mismatch",
                "REQ-0245",
                "schema_version",
                &[
                    ("source", "entry"),
                    ("entry", "entry"),
                    ("expected", "1.0"),
                    ("actual", "0"),
                ],
            );
            assert!(port.calls.is_empty());
            port.documents.insert("a".into(), layer("0", &["unread"]));
            let error = run(layer("1.0", &["a", "later"]), &mut port).unwrap_err();
            check(
                &error,
                "schema_version_mismatch",
                "REQ-0656",
                "parents",
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
        fn fixed_parent_occurrence_boundary_counts_aliases_before_entering_the_next_port() {
            use yamaa_core::limits::{LimitExceeded, Limits};
            for count in [
                Limits::PARENT_OCCURRENCES - 1,
                Limits::PARENT_OCCURRENCES,
                Limits::PARENT_OCCURRENCES + 1,
            ] {
                let mut port = Port::default().with("a", &[]);
                let parents = vec!["a"; count];
                let result = run(layer("1.0", &parents), &mut port);
                if count <= Limits::PARENT_OCCURRENCES {
                    let layers = result.unwrap();
                    assert_eq!(
                        layers
                            .iter()
                            .map(|layer| layer.source.identity.as_str())
                            .collect::<Vec<_>>(),
                        ["a", "entry"]
                    );
                } else {
                    assert!(matches!(result, Err(Error::Resource(LimitExceeded {
                resource: "parent_occurrences", limit: Limits::PARENT_OCCURRENCES, actual: Some(actual)
            })) if actual == count));
                }
                assert_eq!(
                    port.calls
                        .iter()
                        .filter(|event| event.starts_with("resolve:"))
                        .count(),
                    count.min(Limits::PARENT_OCCURRENCES)
                );
                assert_eq!(
                    port.calls
                        .iter()
                        .filter(|event| event.starts_with("read:"))
                        .count(),
                    1
                );
            }
        }
    }
    mod inheritance_capture {
        use crate::support::document::Tree;
        use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
        use yamaa_core::schema::{
            Document, DocumentNode as N, NormalizationError, SchemaModule, SchemaStructure,
        };
        use yamaa_core::{
            inheritance as graph, inheritance_capture as app, inheritance_preparation as lifecycle,
            project_preparation::Source,
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
            ) -> Result<Vec<u8>, graph::SourceError<Payload>> {
                let index = self.log.borrow().len();
                self.log
                    .borrow_mut()
                    .push(Event::Capture(source.identity.clone()));
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
                Event::Resolve("entry".into(), "a".into()),
                Event::Capture("a".into()),
                Event::Decode("a".into(), vec![1, 2]),
                Event::Resolve("a".into(), "common".into()),
                Event::Capture("common".into()),
                Event::Decode("common".into(), vec![3]),
                Event::Resolve("entry".into(), "b".into()),
                Event::Capture("b".into()),
                Event::Decode("b".into(), vec![4, 5, 6]),
                Event::Resolve("b".into(), "alias".into()),
            ];
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
                    events.push(Event::Rebase(layer.into(), "entry".into(), path.into()));
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
        fn every_opaque_effect_error_keeps_original_allocation_and_exact_completed_catalog_prefix()
        {
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
            assert_eq!((source.as_str(), entry.as_str()), ("a", "entry"));
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].definition().condition, "unknown_field");
            assert_eq!(captured_parents.len(), 1);
            assert_eq!(captured_parents[0].raw().document.nodes().as_ptr(), nodes);
            assert_eq!(&*host.log.borrow(), &expected_effects()[..3]);
        }
    }
}
