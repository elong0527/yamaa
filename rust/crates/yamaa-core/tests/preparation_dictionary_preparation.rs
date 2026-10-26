//! Preparation contracts: dictionary preparation, file preparation,
//! inheritance and inheritance capture.
//!
//! The seam is the resolver and decoder ports, scripted per step: the order
//! of their calls, the original opaque payloads and arenas they keep, and the
//! layers `traverse_versioned` and `prepare_versioned` return, which neither
//! the benchmark reader nor the host fixtures observe.
mod preparation {
    mod dictionary_preparation {
        //! Independent logical arenas and opaque effects exercise capture ownership, over a 1.0
        //! document (`prepare_document`).
        use crate::support::document::Tree as V;
        use std::{rc::Rc, sync::Arc};
        use yamaa_core::resource::ResourceFailure;
        use yamaa_core::schema::{
            Document, DocumentNode as N, LayerProvenance, SchemaModule, SchemaStructure,
        };
        use yamaa_core::{
            decoder, dictionary_preparation as app,
            inheritance::{Source as Identity, SourceError, VersionedLayer},
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
        impl decoder::Decoder for Decoder {
            crate::support::ports::no_tables!();
            crate::support::ports::no_project_files!();
            type Raw = Document;
            type DocumentError = DecodeError;
            fn raw(&mut self, identity: &str, bytes: &[u8]) -> Result<Document, DecodeError> {
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
            fn raw_document(raw: &Document) -> &Document {
                raw
            }
        }
        /// A specification `source` declares alone: one layer, which wrote
        /// every column.
        struct Alone(Vec<LayerProvenance>, Vec<VersionedLayer>);
        impl Alone {
            fn new(source: &str) -> Self {
                Self(
                    vec![LayerProvenance {
                        path: "columns".into(),
                        layer: 0,
                    }],
                    vec![VersionedLayer {
                        source: Identity {
                            identity: source.into(),
                            display_path: source.into(),
                        },
                        document: document(V::map(vec![])),
                        version: yamaa_core::language::LANGUAGE_VERSION,
                    }],
                )
            }
            fn origins(&self) -> app::Origins<'_> {
                app::Origins {
                    provenance: &self.0,
                    layers: &self.1,
                }
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
                Alone::new("parent/base.yaml").origins(),
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
                Alone::new("entry").origins(),
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
                    Alone::new("entry").origins(),
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
                Alone::new("entry").origins(),
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
                Alone::new("entry").origins(),
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
        impl Documents {
            /// Prepare the entry `source` as a document service does, reading
            /// each written parent through `port`.
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
        /// Prepare `written` from `declaring` in the order a caller takes the
        /// steps: the entry is captured, the document prepares through an
        /// alias cache, and every alias of each parent it holds is captured
        /// again once it has prepared.
        fn prepare(
            native: &mut Native,
            documents: &mut Documents,
            declaring: Option<&str>,
            written: &str,
        ) -> Result<Prepared, app::Error<Fault, Failure>> {
            let source = app::capture_entry(native, declaring, written)?;
            let display = source.identity.clone();
            let mut aliases = app::AliasCache::new(native);
            let prepared = documents
                .prepare(source, display, &mut aliases)
                .map_err(|error| aliases.refused(error))?;
            aliases.accept(
                prepared
                    .parents
                    .iter()
                    .map(|parent| parent.identity.as_str()),
                |_, _, _| None,
            )?;
            Ok(prepared)
        }
        fn run(
            native: &mut Native,
            documents: &mut Documents,
        ) -> Result<Prepared, app::Error<Fault, Failure>> {
            prepare(native, documents, None, "root.yaml")
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
            assert!(prepare(
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
        //! A 1.x specification's parents are followed, and the fragments its
        //! layers include are read, by `traverse_versioned` (LAYER-2 to
        //! LAYER-4).
        use std::collections::BTreeMap;
        use yamaa_core::decoder;
        use yamaa_core::inheritance::*;
        use yamaa_core::inheritance_capture::SourceFailure;
        use yamaa_core::language::LanguageVersion;
        use yamaa_core::resolver::{Captured, Failure, File, Located, Name, Resolver};
        use yamaa_core::resource::ResourceFailure;
        use yamaa_core::schema::{Document, NormalizationError, SchemaModule, SchemaStructure};

        use crate::support::document::Tree as V;
        use crate::support::ports::Never;

        type Host = (&'static str, u32);
        /// A traversal's failure, of which no decoding fails here.
        type Failed = Error<SourceFailure<Host, Never>>;
        /// What the port refuses a file it does not hold with.
        const MISSING: Host = ("missing", 0);

        fn text(value: &str) -> V {
            V::text(value)
        }
        fn document(value: V) -> Document {
            value.document()
        }
        fn field(name: &'static str, types: V) -> V {
            V::map(vec![(name, V::map(vec![("type", types)]))])
        }
        /// The layers of a 1.x specification, headed by `yamaa` and `type`.
        fn schema() -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("yamaa", text("str")),
                                field("type", text("str")),
                                field("parent", text("str")),
                                field("include", text("list[str]")),
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
        /// A 1.x layer naming `parent` when given and including `include`.
        fn including(parent: Option<&str>, include: &[&str]) -> Document {
            layer_with(
                parent,
                vec![(
                    "include",
                    V::List(include.iter().map(|path| text(path)).collect()),
                )],
            )
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
            /// The layer `name`, holding `document`.
            fn holding(mut self, name: &str, document: Document) -> Self {
                self.documents.insert(name.into(), document);
                self
            }
        }
        impl Resolver for Port {
            type Error = Host;
            fn inspect(&mut self, name: Name<'_>) -> Result<Located, Host> {
                // A parent is named as a parent, and a fragment as a file a
                // layer reads for its content.
                let (verb, declaring, written) = match name {
                    Name::Written {
                        file: File::Parent,
                        declaring,
                        written,
                    } => ("resolve", declaring, written),
                    Name::Written {
                        file: File::Resource,
                        declaring,
                        written,
                    } => ("include", declaring, written),
                    // A parent a caller names comes from no declaring file.
                    Name::Entry {
                        declaring: None,
                        written,
                    } => ("caller", "", written),
                    _ => panic!("unexpected inspection {name:?}"),
                };
                self.calls.push(if verb == "caller" {
                    format!("{verb}:{written}")
                } else {
                    format!("{verb}:{declaring}:{written}")
                });
                if self.unavailable == Some(written) {
                    return Err(MISSING);
                }
                if self.raised == Some(written) {
                    return Err(("opaque", 73));
                }
                Ok(Located {
                    identity: self
                        .aliases
                        .get(written)
                        .map(String::as_str)
                        .unwrap_or(written)
                        .into(),
                    display_path: format!("written/{written}"),
                })
            }
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Host> {
                let identity = match name {
                    Name::Located {
                        file: File::Parent | File::Resource,
                        source,
                    } => source.identity.clone(),
                    // A parent a caller names is read by that name.
                    Name::Entry {
                        declaring: None,
                        written,
                    } => self
                        .aliases
                        .get(written)
                        .map_or(written, String::as_str)
                        .to_owned(),
                    _ => panic!("unexpected capture {name:?}"),
                };
                self.calls.push(format!("read:{identity}"));
                if self.documents.contains_key(&identity) {
                    Ok(Captured::resource(identity.as_bytes().into(), true))
                } else {
                    Err(MISSING)
                }
            }
            fn failure(&self, _: Name<'_>, error: &Host) -> Option<Failure> {
                (*error == MISSING).then_some(Failure::Resource(ResourceFailure::Missing))
            }
            fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Host> {
                panic!("a traversal respells no path")
            }
        }
        /// Each layer's document, by the identity its held bytes name.
        struct Layers(BTreeMap<String, Document>);
        impl decoder::Decoder for Layers {
            crate::support::ports::no_tables!();
            crate::support::ports::no_project_files!();
            type Raw = Document;
            type DocumentError = Never;
            fn raw(&mut self, identity: &str, bytes: &[u8]) -> Result<Document, Never> {
                assert_eq!(bytes, identity.as_bytes(), "the held bytes of {identity}");
                Ok(self.0[identity].clone())
            }
            fn raw_document(raw: &Document) -> &Document {
                raw
            }
        }
        /// The 1.x chain from the entry `input`, root ancestor first.
        fn run(input: Document, port: &mut Port) -> Result<Vec<VersionedLayer>, Failed> {
            traverse(input, None, port)
        }
        /// The chain of the entry `input` checked as a fragment on the parent
        /// a caller names `parent`, which the caller gave as
        /// `given/<parent>`.
        fn run_on(
            input: Document,
            parent: &str,
            port: &mut Port,
        ) -> Result<Vec<VersionedLayer>, Failed> {
            let given = format!("given/{parent}");
            traverse(
                input,
                Some(CallerParent {
                    written: parent,
                    given: &given,
                }),
                port,
            )
        }
        fn traverse(
            input: Document,
            parent: Option<CallerParent<'_>>,
            port: &mut Port,
        ) -> Result<Vec<VersionedLayer>, Failed> {
            let mut layers = Layers(port.documents.clone());
            traverse_versioned(
                &schema(),
                LanguageVersion::new(0),
                source("entry"),
                input,
                parent,
                port,
                &mut layers,
            )
            .map(|traversal| traversal.chain.layers)
        }
        /// `error`'s projection: a validation finding of `condition` and
        /// `rule` at `path`, with each text context value of `fields`.
        fn check(error: &Failed, condition: &str, rule: &str, path: &str, fields: &[(&str, &str)]) {
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
                        returns_to_entry: target == "entry",
                        named: Named::Parent,
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
                Error::Host(SourceFailure::Host(("opaque", 73)))
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
                    returns_to_entry: true,
                    named: Named::Parent,
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
                named: Named::Parent,
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

        /// The identities of `layers`, in contribution order.
        fn identities(layers: &[VersionedLayer]) -> Vec<&str> {
            layers.iter().map(|l| l.source.identity.as_str()).collect()
        }

        #[test]
        fn fragments_contribute_after_the_parent_chain_and_before_their_layer() {
            // entry -> a -> root, where a includes g and the entry includes
            // f1 then f2: at each layer its parent chain, then its fragments
            // in list order, then the layer itself (LAYER-3). The chain is
            // followed to its root before any fragment is read.
            let mut port = Port::default()
                .holding("a", including(Some("root"), &["g"]))
                .with("root", None)
                .with("g", None)
                .with("f1", None)
                .with("f2", None);
            let layers = run(including(Some("a"), &["f1", "f2"]), &mut port).unwrap();
            assert_eq!(identities(&layers), ["root", "g", "a", "f1", "f2", "entry"]);
            assert_eq!(
                port.calls,
                [
                    "resolve:entry:a",
                    "read:a",
                    "resolve:a:root",
                    "read:root",
                    "include:a:g",
                    "read:g",
                    "include:entry:f1",
                    "read:f1",
                    "include:entry:f2",
                    "read:f2",
                ]
            );
            // A layer without a parent composes its fragments before itself.
            let mut port = Port::default().with("f", None);
            let layers = run(including(None, &["f"]), &mut port).unwrap();
            assert_eq!(identities(&layers), ["f", "entry"]);
        }

        /// A study of layers that the port holds, by name: an entry
        /// document and the layers it reaches.
        struct Study {
            label: &'static str,
            entry: Document,
            layers: Vec<(&'static str, Document)>,
        }
        impl Study {
            /// A port that holds the study's layers, each alias naming the
            /// layer it reaches.
            fn port(&self, aliases: &[(&str, &str)]) -> Port {
                let mut port = Port::default();
                for (name, document) in &self.layers {
                    port = port.holding(name, document.clone());
                }
                for (alias, target) in aliases {
                    port.aliases.insert((*alias).into(), (*target).into());
                }
                port
            }
        }

        #[test]
        fn an_include_that_reaches_a_layer_still_composing_is_a_cycle() {
            // Each study, the cycle its include entry `include[0]` closes,
            // whether it returns to the entry, and the calls made: the layer
            // reached again is never read again.
            let cases = [
                (
                    Study {
                        label: "the entry includes itself",
                        entry: including(None, &["self"]),
                        layers: vec![],
                    },
                    vec!["entry", "entry"],
                    true,
                    vec!["include:entry:self"],
                ),
                (
                    Study {
                        label: "a parent includes the entry",
                        entry: layer(Some("a")),
                        layers: vec![("a", including(None, &["back"]))],
                    },
                    vec!["entry", "a", "entry"],
                    true,
                    vec!["resolve:entry:a", "read:a", "include:a:back"],
                ),
                (
                    Study {
                        label: "the root includes the layer below it",
                        entry: layer(Some("a")),
                        layers: vec![("a", layer(Some("b"))), ("b", including(None, &["down"]))],
                    },
                    vec!["a", "b", "a"],
                    false,
                    vec![
                        "resolve:entry:a",
                        "read:a",
                        "resolve:a:b",
                        "read:b",
                        "include:b:down",
                    ],
                ),
            ];
            for (study, cycle, returns_to_entry, calls) in cases {
                let label = study.label;
                let mut port = study.port(&[("self", "entry"), ("back", "entry"), ("down", "a")]);
                let error = run(study.entry, &mut port).unwrap_err();
                assert_eq!(
                    error,
                    Error::Cycle {
                        path: cycle.iter().map(|s| s.to_string()).collect(),
                        returns_to_entry,
                        named: Named::Include(0),
                    },
                    "{label}"
                );
                assert_eq!(port.calls, calls, "{label}");
                check(
                    &error,
                    "inheritance_cycle",
                    "LAYER-3",
                    "include[0]",
                    &[
                        ("source", cycle[cycle.len() - 2]),
                        (
                            "reason",
                            if returns_to_entry {
                                "parent_chain_returns_to_entry"
                            } else {
                                "parent_chain_returns_to_active_layer"
                            },
                        ),
                    ],
                );
            }
        }

        #[test]
        fn an_include_that_reaches_a_layer_already_composed_repeats_it() {
            // Each study, the layer whose include entry repeats a layer, the
            // entry's index, the path as written and the layer reached.
            let cases = [
                (
                    Study {
                        label: "one list names a fragment twice",
                        entry: including(None, &["f", "again"]),
                        layers: vec![("f", layer(None))],
                    },
                    "entry",
                    1,
                    "again",
                    "f",
                ),
                (
                    Study {
                        label: "the entry includes its parent",
                        entry: including(Some("a"), &["a"]),
                        layers: vec![("a", layer(None))],
                    },
                    "entry",
                    0,
                    "a",
                    "a",
                ),
                (
                    Study {
                        label: "a parent and the entry include one fragment",
                        entry: including(Some("a"), &["again"]),
                        layers: vec![("a", including(None, &["f"])), ("f", layer(None))],
                    },
                    "entry",
                    0,
                    "again",
                    "f",
                ),
            ];
            for (study, declaring, index, path, reached) in cases {
                let label = study.label;
                let mut port = study.port(&[("again", "f")]);
                let error = run(study.entry, &mut port).unwrap_err();
                assert_eq!(
                    error,
                    Error::Repeated {
                        declaring: declaring.into(),
                        index,
                        path: path.into(),
                        layer: reached.into(),
                    },
                    "{label}"
                );
                // The repeated file is located, never read again.
                assert_eq!(
                    port.calls.last().map(String::as_str),
                    Some(format!("include:{declaring}:{path}").as_str()),
                    "{label}"
                );
                check(
                    &error,
                    "duplicate_layer",
                    "LAYER-3",
                    &format!("include[{index}]"),
                    &[("source", declaring), ("path", path), ("layer", reached)],
                );
            }
        }

        #[test]
        fn a_fragment_declares_neither_parent_nor_include() {
            for (field, fragment) in [
                ("parent", layer(Some("x"))),
                ("include", including(None, &["x"])),
            ] {
                let mut port = Port::default().holding("f", fragment);
                let error = run(including(None, &["f"]), &mut port).unwrap_err();
                assert_eq!(
                    error,
                    Error::Fragment {
                        source: "f".into(),
                        entry: "entry".into(),
                        field,
                    },
                    "{field}"
                );
                // What the fragment names is never read.
                assert_eq!(port.calls, ["include:entry:f", "read:f"], "{field}");
                check(
                    &error,
                    "prohibited_construct",
                    "LAYER-4",
                    field,
                    &[("construct", field), ("source", "f"), ("entry", "entry")],
                );
            }
        }

        #[test]
        fn an_include_that_reaches_no_file_fails_at_its_entry() {
            let mut port = Port::default().with("a", None);
            let error = run(including(Some("a"), &["gone"]), &mut port).unwrap_err();
            assert_eq!(
                error,
                Error::Included {
                    declaring: "entry".into(),
                    index: 0,
                    path: "gone".into(),
                    failure: ResourceFailure::Missing,
                }
            );
            // The layer that writes the entry is named, so the finding is
            // located there whichever layers also include (P-ISSUE-1).
            check(
                &error,
                "resource_path_missing",
                "FILE-7",
                "include[0]",
                &[("path", "gone"), ("source", "entry")],
            );
            // A failure the port does not classify stays its own.
            let mut port = Port {
                raised: Some("raise"),
                ..Port::default()
            };
            assert_eq!(
                run(including(None, &["raise"]), &mut port).unwrap_err(),
                Error::Host(SourceFailure::Host(("opaque", 73)))
            );
        }

        #[test]
        fn a_fragment_with_an_unread_header_fails_at_the_include_entry_naming_it() {
            let fragment = document(V::map(vec![
                ("yamaa", text("1.0")),
                ("type", text("project")),
            ]));
            // The entry includes the fragment, or the entry's parent does:
            // the finding names the layer that writes the include entry.
            for (input, declaring) in [
                (including(None, &["first", "f"]), "entry"),
                (layer(Some("base")), "base"),
            ] {
                let mut port = Port::default()
                    .with("first", None)
                    .holding("base", including(None, &["first", "f"]))
                    .holding("f", fragment.clone());
                let error = run(input, &mut port).unwrap_err();
                assert_eq!(
                    error,
                    Error::Header {
                        source: "f".into(),
                        entry: "entry".into(),
                        mismatch: Box::new(HeaderMismatch::FileType {
                            file_type: "project".into()
                        }),
                        named: Named::Include(1),
                        declaring: Some(declaring.into()),
                    },
                    "{declaring}"
                );
                check(
                    &error,
                    "unexpected_file_type",
                    "LAYER-4",
                    "include[1]",
                    &[
                        ("source", "f"),
                        ("entry", "entry"),
                        ("declaring_source", declaring),
                        ("type", "project"),
                        ("expected", "spec"),
                    ],
                );
            }
        }

        #[test]
        fn a_parent_a_caller_names_contributes_before_the_fragment() {
            // The fragment checked on the parent b, where b names root as its
            // own parent and includes g: b's layers contribute before the
            // fragment, as they do before a child that names b as its parent
            // and includes the fragment (LAYER-3, #3694). b is asked for as a
            // file the caller names, and the chain continues from b's parent.
            let mut port = Port::default()
                .holding("b", including(Some("root"), &["g"]))
                .with("root", None)
                .with("g", None);
            let layers = run_on(layer(None), "b", &mut port).unwrap();
            assert_eq!(identities(&layers), ["root", "g", "b", "entry"]);
            assert_eq!(
                port.calls,
                [
                    "caller:b",
                    "read:b",
                    "resolve:b:root",
                    "read:root",
                    "include:b:g",
                    "read:g",
                ]
            );
        }

        #[test]
        fn a_fragment_that_names_a_layer_refuses_the_given_parent_before_it_is_read() {
            for (field, entry) in [
                ("parent", layer(Some("x"))),
                ("include", including(None, &["x"])),
            ] {
                let mut port = Port::default().with("b", None).with("x", None);
                let error = run_on(entry, "b", &mut port).unwrap_err();
                assert_eq!(
                    error,
                    Error::NotFragment {
                        field,
                        value: "given/b".into(),
                    },
                    "{field}"
                );
                // Neither the given parent nor what the entry names is read.
                assert!(port.calls.is_empty(), "{field}: {:?}", port.calls);
                // The value is the parent as the caller gave it.
                check(
                    &error,
                    "value_not_permitted",
                    "LAYER-4",
                    "parent",
                    &[("construct", field), ("value", "given/b")],
                );
            }
        }

        #[test]
        fn a_given_parent_that_cannot_be_read_is_the_caller_s_path_at_no_field() {
            use yamaa_core::json::Value as C;
            let mut port = Port {
                unavailable: Some("gone"),
                ..Port::default()
            };
            let error = run_on(layer(None), "gone", &mut port).unwrap_err();
            assert_eq!(
                error,
                Error::CallerParent {
                    path: "given/gone".into(),
                    failure: ResourceFailure::Missing,
                }
            );
            // FILE-1: the path as the caller gave it, and no spec path.
            let diagnostic = error.diagnostic().unwrap().unwrap();
            let definition = diagnostic.definition();
            assert_eq!(
                (
                    definition.phase,
                    definition.condition,
                    definition.rule,
                    diagnostic.spec_paths.as_slice(),
                    &diagnostic.context,
                ),
                (
                    "validation",
                    "resource_path_missing",
                    Some("FILE-7"),
                    &[][..],
                    &[("path".to_owned(), C::String("given/gone".into()))]
                        .into_iter()
                        .collect(),
                )
            );
            // A failure the port does not classify stays its own.
            let mut port = Port {
                raised: Some("raise"),
                ..Port::default()
            };
            assert_eq!(
                run_on(layer(None), "raise", &mut port).unwrap_err(),
                Error::Host(SourceFailure::Host(("opaque", 73)))
            );
        }

        #[test]
        fn a_given_parent_that_is_the_fragment_itself_is_a_cycle_at_parent() {
            let mut port = Port::default();
            port.aliases.insert("self".into(), "entry".into());
            let error = run_on(layer(None), "self", &mut port).unwrap_err();
            assert_eq!(
                error,
                Error::Cycle {
                    path: vec!["entry".into(), "entry".into()],
                    returns_to_entry: true,
                    named: Named::CallerParent,
                }
            );
            assert_eq!(port.calls, ["caller:self"]);
            check(
                &error,
                "inheritance_cycle",
                "LAYER-3",
                "parent",
                &[
                    ("reason", "parent_chain_returns_to_entry"),
                    ("source", "entry"),
                ],
            );
        }

        #[test]
        fn a_given_parent_with_an_unread_header_fails_in_that_file() {
            // No field names the given parent, so its header is refused at
            // its own `type`, as the entry's is (SCHEMA-2).
            let parent = document(V::map(vec![
                ("yamaa", text("1.0")),
                ("type", text("project")),
            ]));
            let mut port = Port::default().holding("b", parent);
            let error = run_on(layer(None), "b", &mut port).unwrap_err();
            assert_eq!(
                error,
                Error::Header {
                    source: "b".into(),
                    entry: "entry".into(),
                    mismatch: Box::new(HeaderMismatch::FileType {
                        file_type: "project".into()
                    }),
                    named: Named::CallerParent,
                    declaring: None,
                }
            );
            check(
                &error,
                "unexpected_file_type",
                "SCHEMA-2",
                "type",
                &[
                    ("source", "b"),
                    ("entry", "entry"),
                    ("type", "project"),
                    ("expected", "spec"),
                ],
            );
        }
    }
    mod inheritance_capture {
        //! A 1.x chain's parents are read through the host's port as
        //! `traverse_versioned` asks for them, and each layer's relative paths
        //! are respelled through the same port as `prepare_versioned` resolves
        //! the chain (LAYER-3, LAYER-7, LAYER-10).
        use crate::support::document::Tree;
        use crate::support::ports::Never;
        use std::{collections::BTreeMap, rc::Rc};
        use yamaa_core::language::LanguageVersion;
        use yamaa_core::resolver::{self, Captured, File, Located, Name, Resolver};
        use yamaa_core::schema::{
            Document, DocumentNode as N, SchemaModule, SchemaSource, SchemaStructure,
        };
        use yamaa_core::{
            decoder, inheritance as graph, inheritance_capture::SourceFailure,
            inheritance_preparation as lifecycle,
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
            let mut fields = vec![("yamaa", text("1.0")), ("type", text("spec"))];
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
        /// What resolving a 1.x chain asks of its host's port, in order.
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum Effect {
            Resolve(String, String),
            Read(String),
            Rebase(String, String, String),
        }
        /// A host's resolver over 1.x layers held by identity. It locates a
        /// written parent, captures a parent, and respells a path from the
        /// entry; one effect may raise an opaque payload instead.
        struct Chain {
            log: Vec<Effect>,
            aliases: BTreeMap<String, String>,
            failure: Option<(usize, Payload)>,
        }
        impl Chain {
            /// The entry's parents are `b`, then `a`, whose `alias` resolves
            /// to the root ancestor `common`.
            fn new() -> Self {
                Self {
                    log: Vec::new(),
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
        impl Resolver for Chain {
            type Error = Payload;
            fn inspect(&mut self, name: Name<'_>) -> Result<Located, Payload> {
                let Name::Written {
                    file: File::Parent,
                    declaring,
                    written,
                } = name
                else {
                    panic!("unexpected inspection {name:?}")
                };
                if let Some(payload) =
                    self.effect(Effect::Resolve(declaring.into(), written.into()))
                {
                    return Err(payload);
                }
                Ok(Located {
                    identity: self
                        .aliases
                        .get(written)
                        .cloned()
                        .unwrap_or_else(|| written.into()),
                    display_path: format!("display/{written}"),
                })
            }
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Payload> {
                let Name::Located {
                    file: File::Parent,
                    source,
                } = name
                else {
                    panic!("unexpected capture {name:?}")
                };
                if let Some(payload) = self.effect(Effect::Read(source.identity.clone())) {
                    return Err(payload);
                }
                Ok(Captured::resource(source.identity.as_bytes().into(), true))
            }
            fn failure(&self, _: Name<'_>, _: &Payload) -> Option<resolver::Failure> {
                None
            }
            fn rebase(
                &mut self,
                layer: &Located,
                entry: &Located,
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
        /// The entry's parent layers, each decoded once by the identity its
        /// held bytes name.
        struct Layers(BTreeMap<String, Document>);
        impl Layers {
            fn new() -> Self {
                Self(
                    [
                        ("b", layer("b", Some("a"), false)),
                        ("a", layer("a", Some("alias"), false)),
                        ("common", layer("common", None, false)),
                    ]
                    .into_iter()
                    .map(|(name, document)| (name.into(), document))
                    .collect(),
                )
            }
        }
        impl decoder::Decoder for Layers {
            crate::support::ports::no_tables!();
            crate::support::ports::no_project_files!();
            type Raw = Document;
            type DocumentError = Never;
            fn raw(&mut self, identity: &str, bytes: &[u8]) -> Result<Document, Never> {
                assert_eq!(bytes, identity.as_bytes(), "the held bytes of {identity}");
                Ok(self
                    .0
                    .remove(identity)
                    .expect("each parent is decoded once"))
            }
            fn raw_document(raw: &Document) -> &Document {
                raw
            }
        }
        /// Why a 1.x chain did not resolve.
        enum Failure {
            Traversal(graph::Error<SourceFailure<Payload, Never>>),
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
            let graph::Traversal { chain: layers, .. } = graph::traverse_versioned(
                &schema,
                LanguageVersion::new(0),
                entry,
                layer("entry", Some("b"), true),
                None,
                chain,
                &mut Layers::new(),
            )
            .map_err(Failure::Traversal)?;
            // No layer copies columns, so no producer is read.
            let resolved =
                lifecycle::prepare_versioned(&schema, &layers, chain, |_, _, _, _, _| {
                    panic!("no layer copies columns")
                })
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
                    Err(Failure::Traversal(graph::Error::Host(SourceFailure::Host(payload))))
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
