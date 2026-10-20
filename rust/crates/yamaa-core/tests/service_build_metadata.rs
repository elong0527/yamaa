//! Service contracts.
#[allow(dead_code)]
mod support;
mod service {
    mod build_metadata {
        //! Independent original-entry and admission-order controls for the shared Build boundary.
        use std::rc::Rc;
        use yamaa_core::build::{self, MetadataAdmissionPort, SelectionFailure};

        struct Original(Rc<usize>);
        #[derive(Default)]
        struct Port {
            events: Vec<&'static str>,
            changed: Option<Original>,
        }
        impl MetadataAdmissionPort for Port {
            type Error = Original;

            fn verify_metadata(&mut self) -> Result<(), (String, Original)> {
                self.events.push("verify original snapshots");
                self.changed
                    .take()
                    .map_or(Ok(()), |error| Err(("original/spec.yaml".into(), error)))
            }
        }

        #[test]
        fn admitted_entry_verifies_original_snapshots_once_without_study_authority() {
            let mut port = Port::default();
            assert!(build::verify_entry(&mut port).is_ok());
            assert_eq!(port.events, ["verify original snapshots"]);
        }

        #[test]
        fn snapshot_refusal_moves_the_original_error() {
            let original = Rc::new(23);
            let mut port = Port {
                changed: Some(Original(Rc::clone(&original))),
                ..Default::default()
            };
            match build::verify_entry(&mut port) {
                Err(SelectionFailure::Snapshot { identity, error }) => {
                    assert_eq!(identity, "original/spec.yaml");
                    assert!(Rc::ptr_eq(&original, &error.0));
                }
                _ => panic!("retained changed snapshot"),
            }
            assert_eq!(port.events, ["verify original snapshots"]);
        }
    }
    mod build_ports {
        //! End-to-end shared request effects with the production metadata, activation,
        //! execution and held-output services and private native scripted ports.
        use crate::support::ports::*;
        use crate::support::project_call_compiler as support;
        use std::{convert::Infallible, rc::Rc, sync::Arc};
        use support::{call, specification, Tree};
        use yamaa_core::{
            application_observer::Scope,
            build::{self, MetadataAdmissionPort, PreparationPort, Prepared},
            domain_entry::{self, Ports, Request},
            function_invocation::{Argument, HostError},
            project_activation::{self, ActivationPort},
            project_domain::{self, CheckedOwnedProject},
            project_preparation as metadata,
            specification_output::{self as output, CompleteError, OutputReport, PreparedOutput},
            specification_run::CapturedAttempt,
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{ExecutionEnvironment, LockKind, LockReference},
            project_function::Language,
            schema::{Document, DocumentNode as N, SpecificationDocument},
            specification::PrepareError,
            table::{Column, TableSchema},
            value::{ColumnType, Value},
        };
        #[derive(Debug)]
        struct Opaque(Rc<()>);
        #[derive(Clone, Copy)]
        enum Scenario {
            Success,
            Environment,
            Unsupported,
            Conformance,
            Capture,
            Encode,
            Report,
        }
        #[derive(Debug)]
        enum Failure {
            Metadata(Box<metadata::Failure<Opaque, MetadataDocument, Opaque>>),
            Compile(PrepareError),
            Activation(project_activation::Failure<Opaque>),
            Output(CompleteError<&'static str, Opaque, Infallible>),
        }
        struct Entry {
            model: SpecificationDocument,
            environment: ExecutionEnvironment,
        }
        struct Activation {
            log: EventLog,
            fail: bool,
        }
        impl ActivationPort for Activation {
            type Handle = ();
            type Error = Opaque;
            fn verify_lock(
                &mut self,
                _: Language,
                _: &LockReference,
                _: &[ProjectFunctionIdentity],
            ) -> Result<(), Opaque> {
                self.log.record(Event::Observation("lock"));
                Ok(())
            }
            fn bind(
                &mut self,
                _: &ProjectFunctionIdentity,
                _: &LogicalSignature,
            ) -> Result<(), Opaque> {
                self.log.record(Event::Observation("binding"));
                Ok(())
            }
            fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<Opaque>> {
                self.log.record(Event::Call {
                    slot: 0,
                    arguments: vec![],
                });
                Ok(Value::Int(if self.fail { 8 } else { 7 }))
            }
        }
        struct Report {
            log: EventLog,
            fail: bool,
        }
        impl OutputReport for Report {
            type Error = &'static str;
            type Report = ();
            fn failure(&mut self) -> Result<(), Self::Error> {
                self.log.record(Event::Observation("failed report"));
                Ok(())
            }
            fn begin(&mut self, _: &yamaa_core::dataset::Execution) -> Result<(), Self::Error> {
                self.log.record(Event::Observation("output observations"));
                Ok(())
            }
            fn rejected(
                &mut self,
                _: &[yamaa_core::specification::OutputFinding],
            ) -> Result<(), Self::Error> {
                panic!("fixture output is admitted")
            }
            fn success(
                &mut self,
                _: &yamaa_core::dataset::Execution,
                _: &[usize],
                _: &[u8],
            ) -> Result<(), Self::Error> {
                self.log.record(Event::Observation("complete report"));
                if self.fail {
                    Err("original report refusal")
                } else {
                    Ok(())
                }
            }
        }
        fn document(tree: Tree) -> Document {
            tree.document()
        }
        /// The project links the Python environment file; an invalid link is a
        /// project scalar failure that precedes every capture.
        fn project(invalid: bool) -> Document {
            document(Tree::map(vec![(
                "environments",
                Tree::map(vec![(
                    "python",
                    if invalid {
                        Tree::boolean(true)
                    } else {
                        Tree::text("python/environment.yaml")
                    },
                )]),
            )]))
        }
        fn environment() -> Document {
            document(Tree::map(vec![
                ("lock", Tree::text("uv.lock")),
                (
                    "functions",
                    Tree::map(vec![(
                        "constant",
                        Tree::map(vec![
                            ("function", Tree::text("project.constant")),
                            ("description", Tree::text("A constant")),
                            ("params", Tree::List(vec![])),
                            ("returns", Tree::text("int")),
                            ("may_return_missing", Tree::Scalar(N::Boolean(false))),
                            ("comparison_decimals", Tree::Scalar(N::Integer("4".into()))),
                            (
                                "tests",
                                Tree::List(vec![Tree::map(vec![
                                    ("id", Tree::text("ordinary")),
                                    (
                                        "covers",
                                        Tree::List(vec![
                                            Tree::text("normal"),
                                            Tree::text("boundary"),
                                        ]),
                                    ),
                                    ("args", Tree::map(vec![])),
                                    ("result", Tree::Scalar(N::Integer("7".into()))),
                                ])]),
                            ),
                        ]),
                    )]),
                ),
            ]))
        }
        struct Application {
            scenario: Scenario,
            log: EventLog,
            source: ScriptedSource<Opaque>,
            decoder: ScriptedDecoder<Opaque, FixtureTable<Opaque>>,
            encoder: ScriptedEncoder<Opaque>,
            activation: Activation,
            report: Report,
        }
        impl Application {
            fn new(scenario: Scenario) -> Self {
                let log = EventLog::default();
                Self {
                    scenario,
                    source: ScriptedSource::new(log.clone()),
                    decoder: ScriptedDecoder::new(log.clone()),
                    encoder: ScriptedEncoder::new(log.clone()),
                    activation: Activation {
                        log: log.clone(),
                        fail: matches!(scenario, Scenario::Conformance),
                    },
                    report: Report {
                        log: log.clone(),
                        fail: matches!(scenario, Scenario::Report),
                    },
                    log,
                }
            }
            fn run(
                &mut self,
                checked: &CheckedOwnedProject,
            ) -> Result<PreparedOutput<()>, Failure> {
                let source = checked.compiled().source().clone();
                self.source.inspections.push_back(InspectionStep {
                    source: source.clone(),
                    outcome: Ok(()),
                });
                self.source.captures.push_back(CaptureStep {
                    source: source.clone(),
                    new_snapshots: 1,
                    outcome: if matches!(self.scenario, Scenario::Capture) {
                        Err(Opaque(Rc::new(())))
                    } else {
                        Ok(Arc::from(b"held study".as_slice()))
                    },
                });
                self.decoder.steps.push_back(DecodeStep {
                    source: source.clone(),
                    bytes: b"held study".to_vec(),
                    outcome: Ok(FixtureTable::new(
                        TableSchema::new(vec![Column {
                            name: "ID".into(),
                            kind: ColumnType::Int,
                        }])
                        .unwrap(),
                        vec![vec![Value::Int(1)]],
                        self.log.clone(),
                    )),
                });
                self.encoder.steps.push_back(EncodeStep {
                    projection: vec![0, 1],
                    outcome: if matches!(self.scenario, Scenario::Encode) {
                        Err(Opaque(Rc::new(())))
                    } else {
                        Ok(b"ID,VALUE\n1,7\n".to_vec())
                    },
                });
                let mut attempt = CapturedAttempt::new(&source);
                checked
                    .build_into(
                        &mut self.activation,
                        &mut self.source,
                        &mut self.decoder,
                        &mut attempt,
                        None,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    )
                    .map_err(Failure::Activation)?;
                let execution = attempt
                    .result
                    .as_ref()
                    .ok()
                    .and_then(|result| result.result.as_ref().ok());
                output::prepare(
                    checked.compiled(),
                    execution,
                    &mut self.report,
                    &mut self.encoder,
                )
                .map_err(Failure::Output)
            }
        }
        impl MetadataAdmissionPort for Application {
            type Error = Failure;

            fn verify_metadata(&mut self) -> Result<(), (String, Failure)> {
                self.log.record(Event::Observation("verify metadata"));
                Ok(())
            }
        }
        impl PreparationPort for Application {
            type Entry = Entry;
            type Plain = CheckedOwnedProject;
            type Project = CheckedOwnedProject;
            type Failure = Failure;
            fn capture(&mut self, request: Request<'_>) -> Result<Entry, Failure> {
                assert_eq!(request.environment, Some("project.yaml"));
                let source = metadata::Source {
                    identity: "/study/project.yaml".into(),
                    bytes: b"held project".to_vec(),
                };
                let linked = metadata::Source {
                    identity: "/study/python/environment.yaml".into(),
                    bytes: b"held environment".to_vec(),
                };
                let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(self.log.clone());
                decoder.steps.push_back(MetadataDecodeStep {
                    source: source.clone(),
                    target: metadata::DecodeTarget::Project,
                    outcome: Ok(project(matches!(self.scenario, Scenario::Environment))),
                });
                decoder.steps.push_back(MetadataDecodeStep {
                    source: linked.clone(),
                    target: metadata::DecodeTarget::Environment,
                    outcome: Ok(environment()),
                });
                let mut port = ScriptedMetadata::<Opaque>::new(self.log.clone());
                port.steps.push_back(MetadataCaptureStep {
                    kind: metadata::Kind::Environment,
                    declaring_source: source.identity.clone(),
                    written: "python/environment.yaml".into(),
                    outcome: Ok(metadata::Reply::Document(linked.clone())),
                });
                // The lock path resolves from the linked environment file.
                port.steps.push_back(MetadataCaptureStep {
                    kind: metadata::Kind::Lock,
                    declaring_source: linked.identity.clone(),
                    written: "uv.lock".into(),
                    outcome: Ok(metadata::Reply::Lock {
                        source: metadata::Source {
                            identity: "/study/python/uv.lock".into(),
                            bytes: b"lock".to_vec(),
                        },
                        kind: LockKind::Uv,
                    }),
                });
                let environment =
                    metadata::prepare(&mut decoder, source, Language::Python, &mut port)
                        .map_err(|error| Failure::Metadata(Box::new(error)))?
                        .environment
                        .into_execution();
                self.log.record(Event::Observation("entry"));
                let expression = if matches!(self.scenario, Scenario::Unsupported) {
                    Tree::map(vec![("future_operation", Tree::text("SRC.ID"))])
                } else {
                    call("constant", vec![])
                };
                let model = specification(vec![("VALUE", "int", expression)]);
                build::verify_entry(self).unwrap_or_else(|_| panic!("fixture metadata admission"));
                Ok(Entry { model, environment })
            }
            fn plain(&mut self, _: Entry) -> Result<CheckedOwnedProject, Failure> {
                panic!("explicit environment selects project")
            }
            fn project(&mut self, entry: Entry) -> Result<CheckedOwnedProject, Failure> {
                self.log.record(Event::Observation("compile"));
                project_domain::check_owned(&entry.model, entry.environment)
                    .map_err(|rejection| Failure::Compile(rejection.error))
            }
        }
        impl Ports for Application {
            type Plain = CheckedOwnedProject;
            type Project = CheckedOwnedProject;
            type CheckResult = ();
            type BuildResult = PreparedOutput<()>;
            type Failure = Failure;
            fn prepare(
                &mut self,
                request: Request<'_>,
            ) -> Result<Prepared<Self::Plain, Self::Project>, Failure> {
                build::prepare(request, self)
            }
            fn check_plain(&mut self, _: Self::Plain) -> Result<(), Failure> {
                unreachable!()
            }
            fn check_project(&mut self, _: Self::Project) -> Result<(), Failure> {
                self.log.record(Event::Observation("static check"));
                Ok(())
            }
            fn build_plain(
                &mut self,
                _: &Self::Plain,
                _: Scope<'_>,
            ) -> Result<PreparedOutput<()>, Failure> {
                unreachable!()
            }
            fn build_project(
                &mut self,
                checked: &Self::Project,
                _: Scope<'_>,
            ) -> Result<PreparedOutput<()>, Failure> {
                self.run(checked)
            }
        }
        fn request() -> Request<'static> {
            Request {
                specification: "spec.yaml",
                environment: Some("project.yaml"),
            }
        }
        #[test]
        fn complete_request_failures_never_publish_and_admission_precedes_study_capture() {
            for scenario in [
                Scenario::Environment,
                Scenario::Unsupported,
                Scenario::Conformance,
                Scenario::Capture,
                Scenario::Encode,
                Scenario::Report,
            ] {
                let mut ports = Application::new(scenario);
                let result = domain_entry::domain(request(), &mut ports, Scope::default());
                match (&scenario, &result) {
                    (Scenario::Environment, Err(Failure::Metadata(error))) => assert!(matches!(
                        error.as_ref(),
                        metadata::Failure::RootScalar { .. }
                    )),
                    (
                        Scenario::Unsupported,
                        Err(Failure::Compile(PrepareError::Unsupported(_))),
                    ) => (),
                    (
                        Scenario::Conformance,
                        Err(Failure::Activation(project_activation::Failure::Tests(_))),
                    ) => (),
                    (Scenario::Capture, Ok(result)) => assert!(result.artifact().is_none()),
                    (Scenario::Encode, Err(Failure::Output(CompleteError::Encode(error)))) => {
                        assert_eq!(Rc::strong_count(&error.0), 1)
                    }
                    (
                        Scenario::Report,
                        Err(Failure::Output(CompleteError::Report("original report refusal"))),
                    ) => (),
                    _ => panic!("wrong failure: {result:?}"),
                }
                if matches!(
                    scenario,
                    Scenario::Environment | Scenario::Unsupported | Scenario::Conformance
                ) {
                    assert!(!ports.log.events().iter().any(|event| matches!(
                        event,
                        Event::Inspect(_) | Event::Capture { .. } | Event::Decode { .. }
                    )));
                }
                let mut publisher = ScriptedPublisher::<Opaque>::new(ports.log.clone());
                if let Ok(result) = result {
                    assert!(result.save(&mut publisher).is_err());
                }
                assert!(!ports
                    .log
                    .events()
                    .iter()
                    .any(|event| matches!(event, Event::Publish { .. })));
            }
        }
        #[test]
        fn check_has_no_runtime_effects_and_successful_build_saves_only_the_held_bytes() {
            let mut ports = Application::new(Scenario::Success);
            domain_entry::check(request(), &mut ports).unwrap();
            assert!(!ports.log.events().iter().any(|event| matches!(
                event,
                Event::Call { .. }
                    | Event::Capture { .. }
                    | Event::Encode { .. }
                    | Event::Publish { .. }
                    | Event::Observation("lock" | "binding")
            )));
            ports.log.clear();
            let result = domain_entry::domain(request(), &mut ports, Scope::default()).unwrap();
            assert!(!ports
                .log
                .events()
                .iter()
                .any(|event| matches!(event, Event::Publish { .. })));
            let before = ports.log.events().len();
            let mut publisher = ScriptedPublisher::<Opaque>::new(ports.log.clone());
            for _ in 0..2 {
                publisher.steps.push_back(PublishStep {
                    path: "output.csv".into(),
                    bytes: b"ID,VALUE\n1,7\n".to_vec(),
                    outcome: Ok(()),
                });
            }
            result.save(&mut publisher).unwrap();
            result.save(&mut publisher).unwrap();
            assert_eq!(ports.log.events().len(), before + 2);
            assert!(ports.log.events()[before..]
                .iter()
                .all(|event| matches!(event, Event::Publish { .. })));
        }
    }
    mod define_service {
        //! Independently authored engine use-case controls; no native filesystem or host.
        use std::{cell::Cell, rc::Rc};
        use yamaa_core::define::{
            self, Application, Failure, Ports, PublicationState, SaveFailure,
        };
        use yamaa_core::{
            diagnostic::ConditionCode,
            project_terminology::Catalogue,
            schema::Document,
            table::{CellError, TableAccess, TableSchema, ValueRef},
        };

        #[derive(Debug)]
        struct Opaque(Box<u8>);
        impl Opaque {
            fn identity(&self) -> *const u8 {
                &*self.0
            }
        }
        struct Environment {
            document: Document,
            catalogue: Catalogue,
        }
        struct EmptyTable(TableSchema);
        impl TableAccess for EmptyTable {
            type Error = ();
            fn schema(&self) -> &TableSchema {
                &self.0
            }
            fn row_count(&self) -> usize {
                0
            }
            fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<()>> {
                Err(CellError::OutOfBounds { row, column })
            }
        }
        use crate::support::document::Tree;

        fn text(value: &'static str) -> Tree {
            Tree::text(value)
        }
        fn document(tree: Tree) -> Document {
            tree.document()
        }
        fn project(with_study: bool) -> Environment {
            let mut fields = vec![(
                "sdtm",
                Tree::map(vec![
                    (
                        "standard",
                        Tree::map(vec![
                            ("name", text("SDTMIG")),
                            ("version", text("3.4")),
                            ("status", text("Final")),
                        ]),
                    ),
                    ("specs", Tree::List(vec![])),
                    ("define", text("define.xml")),
                ]),
            )];
            if with_study {
                fields.push((
                    "study",
                    Tree::map(vec![
                        ("id", text("STUDY01")),
                        ("name", text("Study")),
                        ("description", text("Independent service fixture")),
                        ("protocol_name", text("Protocol")),
                    ]),
                ));
            }
            Environment {
                document: document(Tree::map(fields)),
                catalogue: Catalogue::admit(vec![]).unwrap(),
            }
        }
        struct Spy {
            environment: Option<Environment>,
            events: Vec<&'static str>,
            seconds: Option<u64>,
            environment_error: Option<Opaque>,
            verification_error: Option<Opaque>,
            publication_error: Option<Opaque>,
            publisher_error: Option<(usize, Opaque)>,
            publishers: usize,
            published: Vec<(String, Vec<u8>)>,
            clock_calls: Rc<Cell<usize>>,
        }
        impl Spy {
            fn new() -> Self {
                Self {
                    environment: Some(project(true)),
                    events: Vec::new(),
                    seconds: Some(0),
                    environment_error: None,
                    verification_error: None,
                    publication_error: None,
                    publisher_error: None,
                    publishers: 0,
                    published: Vec::new(),
                    clock_calls: Rc::new(Cell::new(0)),
                }
            }
        }
        impl Ports for Spy {
            type Environment = Environment;
            type Specification = Document;
            type Error = Opaque;
            type PublicationError = Opaque;
            type Bytes = Vec<u8>;
            type Parquet = EmptyTable;
            type ParquetError = ();
            type Target = String;
            type Publisher = String;
            fn utc_seconds(&mut self) -> Option<u64> {
                self.clock_calls.set(self.clock_calls.get() + 1);
                self.seconds
            }
            fn capture_environment(&mut self, written: &str) -> Result<Environment, Opaque> {
                assert_eq!(written, "project.yaml");
                self.events.push("capture_environment");
                if let Some(error) = self.environment_error.take() {
                    return Err(error);
                }
                Ok(self.environment.take().unwrap())
            }
            fn environment(environment: &Environment) -> define::EnvironmentView<'_> {
                define::EnvironmentView {
                    document: &environment.document,
                    catalogue: &environment.catalogue,
                    source: "/project/project.yaml",
                }
            }
            fn capture_specification(
                &mut self,
                _: &str,
                _: &str,
                _: &str,
            ) -> Result<Document, Opaque> {
                panic!("no spec selected")
            }
            fn specification(_: &Document) -> define::SpecificationView<'_> {
                panic!("no spec selected")
            }
            fn location(
                &self,
                path: &str,
                declaring: &str,
                written: &str,
            ) -> Result<String, Opaque> {
                assert_eq!(path, "sdtm.define");
                assert_eq!(declaring, "/project/project.yaml");
                assert_eq!(written, "define.xml");
                Ok("/project/define.xml".into())
            }
            fn transport_written(&self, _: &str, _: &str) -> String {
                panic!("no dataset selected")
            }
            fn same_location(&self, left: &str, right: &str) -> bool {
                left == right
            }
            fn package_reference(&self, _: &str, _: &str) -> Option<String> {
                panic!("no dataset selected")
            }
            fn define_name(&self, _: &str) -> String {
                panic!("no dataset selected")
            }
            fn verify_metadata(&mut self, path: &str, source: &str) -> Result<(), Opaque> {
                assert_eq!(path, "sdtm");
                assert_eq!(source, "/project/project.yaml");
                self.events.push("verify_metadata");
                self.verification_error.take().map_or(Ok(()), Err)
            }
            fn capture_published(&mut self, _: &str, _: &str, _: &str) -> Result<Vec<u8>, Opaque> {
                panic!("no dataset selected")
            }
            fn render_dataset_json<T: TableAccess>(
                &mut self,
                _: &yamaa_core::submission_json::DatasetJsonDocument,
                _: &T,
            ) -> Result<Vec<u8>, yamaa_core::submission_composition::Finding> {
                // The fixture declares no datasets, so composition never
                // reaches Dataset-JSON encoding.
                Ok(Vec::new())
            }
            fn render_xpt<T: TableAccess>(
                &mut self,
                _: &yamaa_core::submission_xpt::XptDocument,
                _: &T,
            ) -> Result<Vec<u8>, yamaa_core::submission_composition::Finding> {
                panic!("no dataset selected")
            }
            fn decode_parquet(&mut self, _: &[u8]) -> Result<EmptyTable, ()> {
                panic!("no codec selected")
            }
            fn target(&self, path: &str, declaring: &str, written: &str) -> Result<String, Opaque> {
                assert_eq!(
                    (path, declaring, written),
                    ("sdtm.define", "/project/project.yaml", "define.xml")
                );
                Ok("/project/define.xml".into())
            }
            fn publisher(&mut self, target: &String, written: &str) -> Result<String, Opaque> {
                assert_eq!(written, "define.xml");
                self.events.push("publisher");
                self.publishers += 1;
                if self
                    .publisher_error
                    .as_ref()
                    .is_some_and(|(call, _)| *call == self.publishers)
                {
                    return Err(self.publisher_error.take().unwrap().1);
                }
                Ok(target.clone())
            }
            fn same_target(&mut self, left: &String, right: &String) -> Result<bool, Opaque> {
                Ok(left == right)
            }
            fn publish(
                &mut self,
                publisher: &mut String,
                written: &str,
                bytes: &[u8],
            ) -> Result<(), Opaque> {
                assert_eq!(publisher, "/project/define.xml");
                self.events.push("publish");
                if let Some(error) = self.publication_error.take() {
                    return Err(error);
                }
                self.published.push((written.into(), bytes.into()));
                Ok(())
            }
        }
        fn prepare(spy: &mut Spy, standard: &str, created: Option<&str>) -> define::Attempt<Spy> {
            Application::prepare(
                spy,
                define::Request {
                    environment: "project.yaml",
                    standard,
                    created,
                    installed: "0.3.0",
                },
            )
        }
        #[test]
        fn standard_and_explicit_creation_admit_before_any_capture_or_clock() {
            for (standard, created, code) in [
                ("other", None, ConditionCode::DefineStandardName),
                (
                    "sdtm",
                    Some("2026-10-11T00:00:00.5"),
                    ConditionCode::DefineCreatedInvalid,
                ),
                (
                    "sdtm",
                    Some("2026-02-30T00:00:00"),
                    ConditionCode::DefineCreatedInvalid,
                ),
            ] {
                let mut spy = Spy::new();
                let mut attempt = prepare(&mut spy, standard, created);
                let Err(Failure::Metadata(findings)) = attempt.outcome else {
                    panic!("missing admission cause")
                };
                assert_eq!(findings.len(), 1);
                assert_eq!(findings[0].code, code);
                assert!(spy.events.is_empty());
                assert_eq!(spy.clock_calls.get(), 0);
                assert!(matches!(
                    attempt.application.save(&mut spy),
                    Err(SaveFailure::Refused)
                ));
                assert!(spy.events.is_empty());
            }
        }
        #[test]
        fn actual_capture_failure_moves_opaque_payload_once_and_refuses_save_without_retry() {
            let mut spy = Spy::new();
            let error = Opaque(Box::new(42));
            let identity = error.identity();
            spy.environment_error = Some(error);
            let mut attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
            let Err(Failure::Port(error)) = attempt.outcome else {
                panic!("original port error lost")
            };
            assert_eq!(error.identity(), identity);
            assert_eq!(spy.events, ["capture_environment"]);
            assert!(matches!(
                attempt.application.save(&mut spy),
                Err(SaveFailure::Refused)
            ));
            assert_eq!(spy.events, ["capture_environment"]);
            assert_eq!(attempt.application.artifacts().len(), 0);
        }
        #[test]
        fn complete_metadata_and_rendering_precede_publication_and_save_rewrites_held_bytes() {
            let mut spy = Spy::new();
            let mut attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
            assert!(attempt.outcome.is_ok());
            assert_eq!(
                spy.events,
                ["capture_environment", "verify_metadata", "publisher"]
            );
            assert_eq!(spy.clock_calls.get(), 0);
            assert!(spy.published.is_empty());
            let held = attempt.application.artifacts().next().unwrap();
            assert_eq!(held.0, "define.xml");
            assert_eq!(held.2, PublicationState::Ready);
            assert!(std::str::from_utf8(held.1)
                .unwrap()
                .contains("CreationDateTime=\"2026-10-11T00:00:00\""));
            let bytes = held.1.to_vec();
            assert!(attempt.application.save(&mut spy).ok().unwrap());
            assert_eq!(spy.published, [("define.xml".into(), bytes.clone())]);
            assert_eq!(
                attempt.application.artifacts().next().unwrap().2,
                PublicationState::Published
            );
            // A repeated save preflights again and rewrites the same held
            // bytes, so a deleted target is restored; nothing is recomposed.
            assert!(attempt.application.save(&mut spy).ok().unwrap());
            assert_eq!(
                spy.published,
                [
                    ("define.xml".into(), bytes.clone()),
                    ("define.xml".into(), bytes)
                ]
            );
            assert_eq!(spy.publishers, 3);
            assert_eq!(spy.clock_calls.get(), 0);
        }
        #[test]
        fn verification_failure_retains_original_payload_and_never_acquires_a_publisher() {
            let mut spy = Spy::new();
            let error = Opaque(Box::new(17));
            let identity = error.identity();
            spy.verification_error = Some(error);
            let attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
            let Err(Failure::Port(error)) = attempt.outcome else {
                panic!("original verification error lost")
            };
            assert_eq!(error.identity(), identity);
            assert_eq!(spy.events, ["capture_environment", "verify_metadata"]);
            assert_eq!(attempt.application.artifacts().len(), 0);
        }
        #[test]
        fn target_admission_during_prepare_and_save_retains_original_errors_and_never_writes() {
            for call in [1, 2] {
                let mut spy = Spy::new();
                let error = Opaque(Box::new(23));
                let identity = error.identity();
                spy.publisher_error = Some((call, error));
                let mut attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
                let failure = if call == 1 {
                    attempt.outcome.err().unwrap()
                } else {
                    assert!(attempt.outcome.is_ok());
                    let Err(SaveFailure::Failure(failure)) = attempt.application.save(&mut spy)
                    else {
                        panic!("missing save admission failure")
                    };
                    failure
                };
                let Failure::TargetAdmission { index, error } = failure else {
                    panic!("original target cause lost")
                };
                assert_eq!(index, 0);
                assert_eq!(error.identity(), identity);
                assert_eq!(
                    attempt.application.artifacts().next().unwrap().2,
                    PublicationState::Failed
                );
                assert!(spy.published.is_empty());
                let events = spy.events.clone();
                assert!(matches!(
                    attempt.application.save(&mut spy),
                    Err(SaveFailure::Refused)
                ));
                assert_eq!(spy.events, events);
            }
        }
        #[test]
        fn actual_publication_failure_keeps_admitted_bytes_and_refuses_further_calls() {
            let mut spy = Spy::new();
            let error = Opaque(Box::new(81));
            let identity = error.identity();
            spy.publication_error = Some(error);
            let mut attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
            assert!(attempt.outcome.is_ok());
            let held = attempt.application.artifacts().next().unwrap().1.to_vec();
            let Err(SaveFailure::Failure(Failure::Publication { index, error })) =
                attempt.application.save(&mut spy)
            else {
                panic!("original publish error lost")
            };
            assert_eq!(index, 0);
            assert_eq!(error.identity(), identity);
            assert_eq!(
                attempt.application.artifacts().next().unwrap(),
                ("define.xml", held.as_slice(), PublicationState::Failed)
            );
            let events = spy.events.clone();
            assert!(matches!(
                attempt.application.save(&mut spy),
                Err(SaveFailure::Refused)
            ));
            assert_eq!(spy.events, events);
        }
        #[test]
        fn missing_study_is_the_real_typed_owner_before_any_metadata_verification() {
            let mut spy = Spy::new();
            spy.environment = Some(project(false));
            let attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
            let Err(Failure::Metadata(findings)) = attempt.outcome else {
                panic!("missing actual cause")
            };
            assert_eq!(
                findings,
                [yamaa_core::submission_composition::Finding::cause(
                    ConditionCode::DefineRequiredDeclaration,
                    "study",
                    "study"
                )]
            );
            assert_eq!(findings[0].definition().requirement, Some("REQ-1337"));
            assert_eq!(spy.events, ["capture_environment"]);
        }
        #[test]
        fn a_section_listing_only_specs_reports_every_missing_define_declaration_together() {
            let mut spy = Spy::new();
            // A run admits this section; only Define composition needs more.
            spy.environment = Some(Environment {
                document: document(Tree::map(vec![(
                    "sdtm",
                    Tree::map(vec![("specs", Tree::List(vec![]))]),
                )])),
                catalogue: Catalogue::admit(vec![]).unwrap(),
            });
            let attempt = prepare(&mut spy, "sdtm", Some("2026-10-11T00:00:00"));
            let Err(Failure::Metadata(findings)) = attempt.outcome else {
                panic!("missing define declarations")
            };
            let cause = |path, detail| {
                yamaa_core::submission_composition::Finding::cause(
                    ConditionCode::DefineRequiredDeclaration,
                    path,
                    detail,
                )
            };
            assert_eq!(
                findings,
                [
                    cause("study", "study"),
                    cause("sdtm.standard", "standard"),
                    cause("sdtm.define", "define"),
                ]
            );
            assert!(findings
                .iter()
                .all(|finding| finding.definition().requirement == Some("REQ-1337")));
            assert_eq!(spy.events, ["capture_environment"]);
        }
        #[test]
        fn a_project_without_the_selected_section_names_that_section() {
            let mut spy = Spy::new();
            let attempt = prepare(&mut spy, "adam", Some("2026-10-11T00:00:00"));
            let Err(Failure::Metadata(findings)) = attempt.outcome else {
                panic!("missing selected section")
            };
            assert_eq!(
                findings,
                [yamaa_core::submission_composition::Finding::cause(
                    ConditionCode::DefineRequiredDeclaration,
                    "adam",
                    "adam"
                )]
            );
            assert_eq!(spy.events, ["capture_environment"]);
        }
        #[test]
        fn clock_is_observed_once_and_calendar_admission_is_owned_by_the_engine() {
            for (seconds, expected) in [
                (0, "1970-01-01T00:00:00"),
                (951782400, "2000-02-29T00:00:00"),
            ] {
                let mut spy = Spy::new();
                spy.seconds = Some(seconds);
                let attempt = prepare(&mut spy, "sdtm", None);
                assert!(attempt.outcome.is_ok());
                let held = attempt.application.artifacts().next().unwrap();
                assert!(std::str::from_utf8(held.1)
                    .unwrap()
                    .contains(&format!("CreationDateTime=\"{expected}\"")));
                assert_eq!(spy.clock_calls.get(), 1);
            }
            for (seconds, detail) in [
                (None, "UTC clock before epoch"),
                (Some(u64::MAX), "UTC clock outside supported calendar"),
            ] {
                let mut spy = Spy::new();
                spy.seconds = seconds;
                let attempt = prepare(&mut spy, "sdtm", None);
                let Err(Failure::Metadata(findings)) = attempt.outcome else {
                    panic!("missing clock refusal")
                };
                assert_eq!(
                    findings,
                    [yamaa_core::define_diagnostics::creation_refusal(detail)]
                );
                assert_eq!(spy.clock_calls.get(), 1);
                assert!(spy.events.is_empty());
            }
        }

        #[test]
        fn resource_observations_keep_original_normative_and_implementation_wire_owners() {
            use define::ResourceObservation as R;
            use yamaa_core::json::Value as C;

            for (path, observation, code, requirement) in [
                (
                    "datasets.DM.artifact",
                    R::Missing,
                    ConditionCode::ProducerContractMismatch,
                    "REQ-0535",
                ),
                (
                    "sdtm.define",
                    R::Missing,
                    ConditionCode::ResourceMissing,
                    "REQ-0785",
                ),
                (
                    "datasets.DM.artifact",
                    R::NotRegularFile,
                    ConditionCode::ResourceNotRegularFile,
                    "REQ-0785",
                ),
            ] {
                let finding = define::resource_diagnostic(path, "declared.csv", observation);
                assert_eq!(finding.code, code);
                assert_eq!(finding.definition().phase, "validation");
                assert_eq!(finding.definition().requirement, Some(requirement));
                assert_eq!(finding.spec_paths, [path]);
                assert_eq!(
                    finding.context,
                    [("path".into(), C::String("declared.csv".into()))].into()
                );
                assert_eq!(finding.source_span, None);
                assert_eq!(finding.operand_route, None);
            }
            for (observation, expected) in [
                (R::Unreadable, "resource_unreadable"),
                (R::InvalidRoot, "resource_root"),
                (R::InvalidBase, "resource_base"),
                (R::InvalidPath, "resource_path"),
                (R::InvalidLock, "invalid_packaging_lock"),
                (R::OutsideRoots, "resource_outside_roots"),
                (R::Symlink, "resource_link"),
            ] {
                let finding =
                    define::resource_diagnostic("sdtm.specs[0]", "declared.yaml", observation);
                assert_eq!(finding.code, ConditionCode::ApplicationRejected);
                assert_eq!(finding.definition().phase, "engine");
                assert_eq!(finding.definition().requirement, None);
                assert_eq!(finding.spec_paths, ["sdtm.specs[0]"]);
                assert_eq!(
                    finding.context,
                    [
                        ("path".into(), C::String("declared.yaml".into())),
                        ("stage".into(), C::String("define".into())),
                        ("code".into(), C::String(expected.into())),
                    ]
                    .into()
                );
            }
        }
        #[test]
        fn changed_resource_uses_the_authored_define_field_and_ingest_cause() {
            use yamaa_core::diagnostic::ConditionCode;
            use yamaa_core::json::Value as C;
            let finding = define::resource_diagnostic(
                "datasets.DM.artifact",
                "../declared.csv",
                define::ResourceObservation::Changed,
            );
            assert_eq!(finding.code, ConditionCode::ResourceContentChanged);
            assert_eq!(finding.definition().phase, "ingest");
            assert_eq!(
                finding.definition().condition,
                "resource_path_content_changed"
            );
            assert_eq!(finding.definition().requirement, Some("REQ-0788"));
            assert_eq!(finding.spec_paths, ["datasets.DM.artifact"]);
            assert_eq!(
                finding.context,
                [("path".into(), C::String("../declared.csv".into()))].into()
            );
        }
        #[test]
        fn publication_stage_projection_preserves_full_width_artifact_index_and_cause_text() {
            use define::PublicationFailure as P;
            use yamaa_core::json::Value as C;

            for (stage, code, phase, requirement) in [
                (
                    P::TargetAdmission(yamaa_core::application_issue::Fault::InvalidTarget),
                    ConditionCode::ApplicationRejected,
                    "engine",
                    None,
                ),
                (
                    P::Publication,
                    ConditionCode::OutputPublicationFailed,
                    "output",
                    Some("REQ-0764"),
                ),
            ] {
                let finding = define::publication_diagnostic(
                    usize::MAX,
                    stage,
                    "opaque original cause".into(),
                );
                assert_eq!(finding.code, code);
                assert_eq!(finding.definition().phase, phase);
                assert_eq!(finding.definition().requirement, requirement);
                assert_eq!(finding.spec_paths, ["define"]);
                let mut expected: yamaa_core::diagnostic::Context = [
                    ("artifact".into(), C::from(usize::MAX)),
                    ("cause".into(), C::String("opaque original cause".into())),
                ]
                .into();
                if let P::TargetAdmission(_) = stage {
                    // A refused target is an engine fault: its stage and closed code.
                    for (key, value) in [
                        ("stage", "publication_target_admission"),
                        ("code", "invalid_target"),
                    ] {
                        expected.insert(key.into(), C::String(value.into()));
                    }
                }
                assert_eq!(finding.context, expected);
                assert_eq!(finding.source_span, None);
                assert_eq!(finding.operand_route, None);
            }
        }

        #[test]
        fn fixed_physical_refusals_keep_the_exact_size_and_authored_define_field() {
            for (resource, limit) in [
                ("spec_bytes", 8_388_608usize),
                ("source_bytes", 10_737_418_240usize),
            ] {
                let finding = define::resource_diagnostic(
                    "sdtm.specs[0]",
                    "declared.yaml",
                    define::ResourceObservation::Limit(yamaa_core::limits::LimitExceeded {
                        resource,
                        limit,
                        actual: Some(limit + 1),
                    }),
                );
                assert_eq!(finding.code, ConditionCode::ApplicationRejected);
                assert_eq!(finding.spec_paths, ["sdtm.specs[0]"]);
                assert_eq!(
                    finding.context,
                    yamaa_core::json::Value::Object(
                        [
                            (
                                "path".into(),
                                yamaa_core::json::Value::from("declared.yaml")
                            ),
                            ("stage".into(), yamaa_core::json::Value::from("define")),
                            (
                                "code".into(),
                                yamaa_core::json::Value::from("resource_limit")
                            ),
                            ("resource".into(), yamaa_core::json::Value::from(resource)),
                            ("limit".into(), yamaa_core::json::Value::from(limit)),
                            ("actual".into(), yamaa_core::json::Value::from(limit + 1)),
                        ]
                        .into_iter()
                        .collect(),
                    )
                    .as_object()
                    .unwrap()
                    .clone()
                );
            }
        }
    }
    mod domain_entry {
        //! One request composition selects held metadata before any execution authority.
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{
            application_observer::Scope,
            build::{self, MetadataAdmissionPort, PreparationPort, Prepared},
            domain_entry::{self, Ports, Request},
        };

        struct Original(Rc<()>);
        struct Entry;
        #[derive(Default)]
        struct Fake {
            events: Vec<&'static str>,
            failure: Option<(&'static str, Original)>,
            phases: Option<Rc<RefCell<Vec<yamaa_core::application_observer::Phase>>>>,
        }
        impl Fake {
            fn enter(&mut self, event: &'static str) -> Result<(), Original> {
                if let Some(phases) = &self.phases {
                    use yamaa_core::application_observer::Phase;
                    let expected = if self
                        .events
                        .iter()
                        .any(|event| matches!(*event, "plain build" | "project build"))
                        || matches!(event, "plain build" | "project build")
                    {
                        Phase::Build
                    } else {
                        Phase::Preparation
                    };
                    assert_eq!(phases.borrow().last(), Some(&expected));
                }
                self.events.push(event);
                if self
                    .failure
                    .as_ref()
                    .is_some_and(|(stage, _)| *stage == event)
                {
                    return Err(self.failure.take().unwrap().1);
                }
                Ok(())
            }
            fn execute(&mut self, route: &'static str) -> Result<(), Original> {
                self.enter(route)?;
                self.enter("lock")?;
                self.enter("function tests")?;
                self.enter("study capture")?;
                self.enter("decode")?;
                self.enter("execute")?;
                self.enter("encode")?;
                self.enter("report")
            }
        }
        impl MetadataAdmissionPort for Fake {
            type Error = Original;

            fn verify_metadata(&mut self) -> Result<(), (String, Original)> {
                self.enter("snapshot verification")
                    .map_err(|e| ("spec.yaml".into(), e))
            }
        }
        impl PreparationPort for Fake {
            type Entry = Entry;
            type Plain = ();
            type Project = ();
            type Failure = Original;
            fn capture(&mut self, request: Request<'_>) -> Result<Entry, Original> {
                assert_eq!(request.specification, "spec.yaml");
                self.enter("root admission")?;
                if let Some(path) = request.environment {
                    assert_eq!(path, "environment.yaml");
                    self.enter("environment")?;
                }
                self.enter("entry")?;
                build::verify_entry(self).map_err(|error| match error {
                    build::SelectionFailure::Snapshot { error, .. } => error,
                })?;
                Ok(Entry)
            }
            fn plain(&mut self, _: Entry) -> Result<(), Original> {
                self.enter("plain compilation")
            }
            fn project(&mut self, _: Entry) -> Result<(), Original> {
                self.enter("project compilation")
            }
        }
        impl Ports for Fake {
            type Plain = ();
            type Project = ();
            type CheckResult = ();
            type BuildResult = ();
            type Failure = Original;
            fn prepare(&mut self, request: Request<'_>) -> Result<Prepared<(), ()>, Original> {
                build::prepare(request, self)
            }
            fn check_plain(&mut self, _: ()) -> Result<(), Original> {
                self.enter("plain check")
            }
            fn check_project(&mut self, _: ()) -> Result<(), Original> {
                self.enter("project check")
            }
            fn build_plain(&mut self, _: &(), _: Scope<'_>) -> Result<(), Original> {
                self.execute("plain build")
            }
            fn build_project(&mut self, _: &(), _: Scope<'_>) -> Result<(), Original> {
                self.execute("project build")
            }
        }
        fn request(environment: bool) -> Request<'static> {
            Request {
                specification: "spec.yaml",
                environment: environment.then_some("environment.yaml"),
            }
        }

        #[test]
        fn observed_request_brackets_actual_metadata_before_every_route_and_keeps_original_failure()
        {
            use yamaa_core::application_observer::{Event, Observer, Outcome, Phase, Scope};
            #[derive(Debug, PartialEq)]
            enum Observed {
                Entry(Phase),
                Exit(Phase, Outcome),
            }
            struct Stages {
                phases: Rc<RefCell<Vec<Phase>>>,
                events: RefCell<Vec<Observed>>,
            }
            impl Observer for Stages {
                fn observe(&self, event: Event) {
                    if let Event::Exit { phase, outcome } = event {
                        self.events
                            .borrow_mut()
                            .push(Observed::Exit(phase, outcome));
                        return;
                    }
                    let Event::Phase { phase } = event else {
                        panic!("this request spy invokes no real function port")
                    };
                    self.phases.borrow_mut().push(phase);
                    self.events.borrow_mut().push(Observed::Entry(phase));
                }
            }
            for environment in [false, true] {
                for failure in [None, Some("entry"), Some("study capture")] {
                    let marker = Rc::new(());
                    let phases = Rc::new(RefCell::new(vec![]));
                    let observer = Stages {
                        phases: Rc::clone(&phases),
                        events: RefCell::new(vec![]),
                    };
                    let mut port = Fake {
                        failure: failure.map(|stage| (stage, Original(Rc::clone(&marker)))),
                        phases: Some(Rc::clone(&phases)),
                        ..Default::default()
                    };
                    let result = domain_entry::domain(
                        request(environment),
                        &mut port,
                        Scope::new(&observer),
                    );
                    match failure {
                        None => assert!(result.is_ok()),
                        Some(stage) => {
                            let Err(error) = result else {
                                panic!("scripted original failure")
                            };
                            assert!(Rc::ptr_eq(&marker, &error.0));
                            assert_eq!(port.events.last(), Some(&stage));
                        }
                    }
                    let expected = if failure == Some("entry") {
                        vec![Phase::Preparation, Phase::Finished]
                    } else {
                        vec![Phase::Preparation, Phase::Build, Phase::Finished]
                    };
                    assert_eq!(*phases.borrow(), expected);
                    let events = observer.events.borrow();
                    let mut expected = vec![
                        Observed::Entry(Phase::Preparation),
                        Observed::Exit(
                            Phase::Preparation,
                            if failure == Some("entry") {
                                Outcome::Failed
                            } else {
                                Outcome::Succeeded
                            },
                        ),
                    ];
                    if failure != Some("entry") {
                        expected.extend([
                            Observed::Entry(Phase::Build),
                            Observed::Exit(
                                Phase::Build,
                                if failure.is_none() {
                                    Outcome::Succeeded
                                } else {
                                    Outcome::Failed
                                },
                            ),
                        ]);
                    }
                    expected.push(Observed::Entry(Phase::Finished));
                    assert_eq!(*events, expected);
                }
            }
        }
        #[test]
        fn both_entry_points_select_the_route_from_the_explicit_environment() {
            for (environment, compilation, check, build) in [
                (false, "plain compilation", "plain check", "plain build"),
                (
                    true,
                    "project compilation",
                    "project check",
                    "project build",
                ),
            ] {
                let mut port = Fake::default();
                assert!(domain_entry::check(request(environment), &mut port).is_ok());
                let mut expected = vec!["root admission"];
                if environment {
                    expected.push("environment");
                }
                expected.extend(["entry", "snapshot verification", compilation, check]);
                assert_eq!(port.events, expected);
                port.events.clear();
                assert!(
                    domain_entry::domain(request(environment), &mut port, Scope::default()).is_ok()
                );
                expected.pop();
                expected.extend([
                    build,
                    "lock",
                    "function tests",
                    "study capture",
                    "decode",
                    "execute",
                    "encode",
                    "report",
                ]);
                assert_eq!(port.events, expected);
            }
        }
        #[test]
        fn every_failure_retains_its_original_payload_and_stops_later_authority() {
            for stage in [
                "root admission",
                "environment",
                "entry",
                "snapshot verification",
                "project compilation",
                "lock",
                "function tests",
                "study capture",
                "decode",
                "execute",
                "encode",
                "report",
            ] {
                let marker = Rc::new(());
                let mut port = Fake {
                    failure: Some((stage, Original(Rc::clone(&marker)))),
                    ..Default::default()
                };
                let Err(error) = domain_entry::domain(request(true), &mut port, Scope::default())
                else {
                    panic!("scripted failure")
                };
                assert!(Rc::ptr_eq(&marker, &error.0));
                assert_eq!(port.events.last(), Some(&stage));
                if matches!(
                    stage,
                    "root admission"
                        | "environment"
                        | "entry"
                        | "snapshot verification"
                        | "project compilation"
                        | "lock"
                        | "function tests"
                ) {
                    assert!(!port.events.contains(&"study capture"));
                }
            }
        }
    }
    mod specification_odm {
        //! Engine source admission keeps held snapshots, typed coordinates and opaque effects.
        use std::{cell::RefCell, rc::Rc, sync::Arc};
        use yamaa_core::{
            dataset::ExecutionError,
            specification_run::{
                self as run, CapturedAttempt, PortError, RunError, SourceDecoder, SourceIssue,
                SpecificationSourcePort,
            },
        };
        use yamaa_core::{
            odm_source::FIELDS,
            schema::{Document, SpecificationDocument},
            specification::{PreparedSpecification, SourceDeclaration, SourceInterpretation},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::ColumnType,
        };

        use crate::support::document::Tree;

        fn prepared() -> PreparedSpecification {
            // The canonical document normalization admits: every shorthand is
            // expanded to its class.
            let column = |name, operation, item| {
                let field = if operation == "odm" {
                    "item"
                } else {
                    "variable"
                };
                Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text("str")),
                    (
                        "derivation",
                        Tree::map(vec![(
                            "value",
                            Tree::map(vec![(
                                operation,
                                Tree::map(vec![(field, Tree::text(item))]),
                            )]),
                        )]),
                    ),
                ])
            };
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("TEST")),
                ("base", Tree::text("SRC")),
                (
                    "input",
                    Tree::map(vec![
                        ("SRC", Tree::map(vec![("path", Tree::text("source.csv"))])),
                        ("LATER", Tree::map(vec![("path", Tree::text("later.csv"))])),
                    ]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(vec![
                        column("ID", "source", "SRC.SubjectKey"),
                        column("VAL", "odm", "SRC.IT.TEST.OID"),
                    ]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("result.csv")),
                        (
                            "columns",
                            Tree::List(vec![Tree::text("ID"), Tree::text("VAL")]),
                        ),
                    ]),
                ),
            ]);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            let document = Document::new(nodes, root).unwrap();
            let admitted = SpecificationDocument::admit(document).unwrap().unwrap();
            PreparedSpecification::prepare(&admitted).unwrap()
        }
        #[derive(Debug)]
        struct Original(Rc<()>);
        type Trace = Rc<RefCell<Vec<String>>>;
        struct Table {
            schema: TableSchema,
            trace: Trace,
            payload: Rc<()>,
            fail: Option<(usize, usize)>,
        }
        impl TableAccess for Table {
            type Error = Original;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                2
            }
            fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
                self.trace.borrow_mut().push(format!("cell:{row}:{column}"));
                if self.fail == Some((row, column)) {
                    return Err(CellError::Access(Original(Rc::clone(&self.payload))));
                }
                if column == 2 {
                    Ok(ValueRef::Missing)
                } else {
                    Ok(ValueRef::Str("present"))
                }
            }
        }
        struct Port {
            trace: Trace,
            reads: usize,
        }
        impl SpecificationSourcePort for Port {
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
                Ok(())
            }

            type Error = Original;
            fn capture_reads(&self) -> usize {
                self.reads
            }
            fn capture(
                &mut self,
                source: &SourceDeclaration,
                origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
            ) -> Result<Arc<[u8]>, Original> {
                *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                self.trace
                    .borrow_mut()
                    .push(format!("capture:{}", source.name));
                self.reads += 1;
                Ok(Arc::from(b"held".as_slice()))
            }
        }
        struct Decoder {
            trace: Trace,
            payload: Rc<()>,
            fail: Option<(usize, usize)>,
        }
        impl SourceDecoder for Decoder {
            fn continue_after(&self, _error: &Self::Error) -> bool {
                false
            }
            type Error = Original;
            type Table = Table;

            fn decode(
                &mut self,
                source: &SourceDeclaration,
                bytes: &[u8],
                interpretation: SourceInterpretation,
                _contract: Option<&yamaa_core::producer_contract::Contract>,
            ) -> Result<Table, Original> {
                assert_eq!(bytes, b"held");
                self.trace
                    .borrow_mut()
                    .push(format!("decode:{}", source.name));
                if source.name == "LATER" {
                    assert_eq!(interpretation, SourceInterpretation::Ordinary);
                    return Err(Original(Rc::clone(&self.payload)));
                }
                assert_eq!(interpretation, SourceInterpretation::Odm);
                Ok(Table {
                    schema: TableSchema::new(
                        FIELDS
                            .map(|name| Column {
                                name: name.into(),
                                kind: ColumnType::Str,
                            })
                            .to_vec(),
                    )
                    .unwrap(),
                    trace: Rc::clone(&self.trace),
                    payload: Rc::clone(&self.payload),
                    fail: self.fail,
                })
            }
        }
        fn fixtures(fail: Option<(usize, usize)>) -> (Port, Decoder) {
            let trace = Trace::default();
            (
                Port {
                    trace: Rc::clone(&trace),
                    reads: 0,
                },
                Decoder {
                    trace,
                    payload: Rc::new(()),
                    fail,
                },
            )
        }

        #[test]
        fn required_value_findings_keep_csv_coordinates_and_later_original_decoder_failure() {
            let prepared = prepared();
            let (mut port, mut decoder) = fixtures(None);
            let mut attempt = CapturedAttempt::new(prepared.source());
            run::execute_into(
                &prepared,
                &mut port,
                &mut decoder,
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            let Err(PortError::Run(RunError::AdmittedSources(errors))) = attempt.result else {
                panic!("admitted source failure")
            };
            assert_eq!(errors.len(), 2);
            assert_eq!(errors[0].0, 0);
            let SourceIssue::OdmValues(values) = &errors[0].1 else {
                panic!("required values")
            };
            assert_eq!(
                values,
                &[yamaa_core::odm_source::MissingValues {
                    field: "SubjectKey",
                    record: 2,
                    records: 2
                }]
            );
            let SourceIssue::Decode(Original(payload)) = &errors[1].1 else {
                panic!("original decoder failure")
            };
            assert_eq!(errors[1].0, 1);
            assert!(Rc::ptr_eq(payload, &decoder.payload));
            assert!(attempt.sources[0].snapshot.is_some() && attempt.sources[0].table.is_some());
            assert!(attempt.sources[1].snapshot.is_some() && attempt.sources[1].table.is_none());
            let mut expected = vec!["capture:SRC".to_string(), "decode:SRC".to_string()];
            for column in yamaa_core::odm_source::REQUIRED {
                for row in 0..2 {
                    expected.push(format!("cell:{row}:{column}"));
                }
            }
            expected.extend(["capture:LATER".into(), "decode:LATER".into()]);
            assert_eq!(*decoder.trace.borrow(), expected);
            assert_eq!(port.reads, 2);
        }

        #[test]
        fn original_identifier_access_failure_suppresses_partial_findings_and_later_captures() {
            let prepared = prepared();
            let (mut port, mut decoder) = fixtures(Some((1, 5)));
            let mut attempt = CapturedAttempt::new(prepared.source());
            run::execute_into(
                &prepared,
                &mut port,
                &mut decoder,
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            let execution = attempt.result.unwrap();
            assert!(execution.handler_counts.is_empty());
            assert!(execution.retained_verifications.is_empty());
            match *execution.result.unwrap_err() {
                ExecutionError::Cell {
                    path,
                    source_row: 1,
                    error: CellError::Access(Original(payload)),
                } => {
                    assert_eq!(path, "input.SRC.FormOID");
                    assert!(Rc::ptr_eq(&payload, &decoder.payload));
                }
                error => panic!("original payload lost: {error:?}"),
            }
            assert_eq!(port.reads, 1);
            assert_eq!(attempt.sources.len(), 1);
            assert!(attempt.sources[0].snapshot.is_some() && attempt.sources[0].table.is_some());
            let mut expected = vec!["capture:SRC".to_string(), "decode:SRC".to_string()];
            for column in [0, 1, 2, 3, 5] {
                for row in 0..2 {
                    expected.push(format!("cell:{row}:{column}"));
                }
            }
            assert_eq!(*decoder.trace.borrow(), expected);
        }
    }
}
