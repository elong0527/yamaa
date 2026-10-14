//! End-to-end shared request effects with the production metadata, activation,
//! execution and held-output services and opt-in native scripted ports.
#![cfg(feature = "test-support")]
#[allow(dead_code)]
#[path = "../../yamaa-core/tests/support/project_call_compiler.rs"]
mod support;
use std::{convert::Infallible, rc::Rc, sync::Arc};
use support::{call, specification, Tree};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{ExecutionEnvironment, LockKind, LockReference},
    project_function::Language,
    schema::{Document, DocumentNode as N, SpecificationDocument},
    specification::PrepareError,
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};
use yamaa_engine::{
    build::{self, EntrySelection, MetadataAdmissionPort, PreparationPort, Prepared},
    domain_entry::{self, Ports, Request},
    function_invocation::{Argument, HostError},
    project_activation::{self, ActivationPort},
    project_domain::{self, CheckedOwnedProject},
    project_preparation as metadata,
    specification_output::{self as output, CompleteError, OutputReport, PreparedOutput},
    specification_run::{CapturedAttempt, Limits},
    test_support::*,
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
    selection: EntrySelection,
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
    fn bind(&mut self, _: &ProjectFunctionIdentity, _: &LogicalSignature) -> Result<(), Opaque> {
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
    fn begin(&mut self, _: &yamaa_engine::dataset::Execution) -> Result<(), Self::Error> {
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
        _: &yamaa_engine::dataset::Execution,
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
    fn append(tree: Tree, nodes: &mut Vec<N>) -> usize {
        let node = match tree {
            Tree::Scalar(node) => node,
            Tree::List(items) => N::Sequence(items.into_iter().map(|v| append(v, nodes)).collect()),
            Tree::Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (append(Tree::text(&k), nodes), append(v, nodes)))
                    .collect(),
            ),
        };
        let i = nodes.len();
        nodes.push(node);
        i
    }
    let mut nodes = vec![];
    let root = append(tree, &mut nodes);
    Document::new(nodes, root, Default::default()).unwrap()
}
fn environment(invalid: bool) -> Document {
    document(Tree::map(vec![
        (
            "language",
            Tree::text(if invalid { "invalid" } else { "python" }),
        ),
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
                                Tree::List(vec![Tree::text("normal"), Tree::text("boundary")]),
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
    fn run(&mut self, checked: CheckedOwnedProject) -> Result<PreparedOutput<()>, Failure> {
        let source = checked.compiled().source().clone();
        self.source.inspections.push_back(InspectionStep {
            source: source.clone(),
            outcome: Ok(()),
        });
        self.source.captures.push_back(CaptureStep {
            source: source.clone(),
            byte_limit: 1024,
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
            byte_limit: 1024,
            outcome: if matches!(self.scenario, Scenario::Encode) {
                Err(Opaque(Rc::new(())))
            } else {
                Ok(b"ID,VALUE\n1,7\n".to_vec())
            },
        });
        let mut attempt = CapturedAttempt::new(&source);
        let limits = Limits {
            source_bytes: 1024,
            source_cells: 64,
            execution: yamaa_engine::dataset::Limits {
                source_rows: 8,
                output_rows: 8,
                output_cells: 64,
                key_cells: 64,
                work_cells: 4096,
                scalar_text_bytes: 4096,
                output_text_bytes: 4096,
                identity_cells: 64,
                identity_text_bytes: 4096,
            },
        };
        checked
            .build_into(
                &mut self.activation,
                &mut self.source,
                &mut self.decoder,
                limits,
                &mut attempt,
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
            1024,
            &mut self.report,
            &mut self.encoder,
        )
        .map_err(Failure::Output)
    }
}
impl MetadataAdmissionPort for Application {
    type Error = Failure;
    fn charge_work(&mut self, _: usize) -> Result<(), Failure> {
        self.log.record(Event::Observation("admit original arena"));
        Ok(())
    }
    fn verify_metadata(&mut self) -> Result<(), (String, Failure)> {
        self.log.record(Event::Observation("verify metadata"));
        Ok(())
    }
}
impl PreparationPort for Application {
    type Entry = Entry;
    type Plain = CheckedOwnedProject;
    type Project = CheckedOwnedProject;
    type Producer = CheckedOwnedProject;
    type Failure = Failure;
    fn capture(&mut self, request: Request<'_>) -> Result<Entry, Failure> {
        assert_eq!(request.environment, Some("env.yaml"));
        let source = metadata::Source {
            identity: "/study/env.yaml".into(),
            bytes: b"held environment".to_vec(),
        };
        let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(self.log.clone());
        decoder.steps.push_back(MetadataDecodeStep {
            source: source.clone(),
            target: metadata::DecodeTarget::Environment,
            outcome: Ok(environment(matches!(self.scenario, Scenario::Environment))),
        });
        let mut port = ScriptedMetadata::<Opaque>::new(self.log.clone());
        port.steps.push_back(MetadataCaptureStep {
            kind: metadata::Kind::Lock,
            declaring_source: source.identity.clone(),
            written: "uv.lock".into(),
            remaining_bytes: 1024 - source.bytes.len(),
            remaining_identity_bytes: 1024 - source.identity.len(),
            outcome: Ok(metadata::Reply::Lock {
                source: metadata::Source {
                    identity: "/study/uv.lock".into(),
                    bytes: b"lock".to_vec(),
                },
                kind: LockKind::Uv,
            }),
        });
        let limits = metadata::Limits {
            bytes: 1024,
            identity_bytes: 1024,
            ..Default::default()
        };
        let environment =
            metadata::prepare(&mut decoder, source, Language::Python, &mut port, limits)
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
        let selection = build::select_entry(
            build::EntryView {
                original: model.document(),
                model_nodes: model.document().nodes().len(),
            },
            1024,
            self,
        )
        .unwrap_or_else(|_| panic!("fixture metadata admission"));
        Ok(Entry {
            selection,
            model,
            environment,
        })
    }
    fn selection(entry: &Entry) -> &EntrySelection {
        &entry.selection
    }
    fn plain(&mut self, _: Entry) -> Result<CheckedOwnedProject, Failure> {
        panic!("explicit environment selects project")
    }
    fn project(&mut self, entry: Entry) -> Result<CheckedOwnedProject, Failure> {
        self.log.record(Event::Observation("compile"));
        project_domain::check_owned(&entry.model, entry.environment)
            .map_err(|rejection| Failure::Compile(rejection.error))
    }
    fn producer(&mut self, _: Entry) -> Result<CheckedOwnedProject, Failure> {
        panic!("ordinary fixture selects project")
    }
}
impl Ports for Application {
    type Plain = CheckedOwnedProject;
    type Project = CheckedOwnedProject;
    type Producer = CheckedOwnedProject;
    type CheckResult = ();
    type BuildResult = PreparedOutput<()>;
    type Failure = Failure;
    fn prepare(
        &mut self,
        request: Request<'_>,
    ) -> Result<Prepared<Self::Plain, Self::Project, Self::Producer>, Failure> {
        build::prepare(request, self)
    }
    fn check_plain(&mut self, _: Self::Plain) -> Result<(), Failure> {
        unreachable!()
    }
    fn check_project(&mut self, _: Self::Project) -> Result<(), Failure> {
        self.log.record(Event::Observation("static check"));
        Ok(())
    }
    fn check_producer(&mut self, _: Self::Producer) -> Result<(), Failure> {
        unreachable!()
    }
    fn build_plain(&mut self, _: Self::Plain) -> Result<PreparedOutput<()>, Failure> {
        unreachable!()
    }
    fn build_project(&mut self, checked: Self::Project) -> Result<PreparedOutput<()>, Failure> {
        self.run(checked)
    }
    fn build_producer(&mut self, _: Self::Producer) -> Result<PreparedOutput<()>, Failure> {
        unreachable!()
    }
}
fn request() -> Request<'static> {
    Request {
        specification: "spec.yaml",
        environment: Some("env.yaml"),
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
        let result = domain_entry::domain(request(), &mut ports);
        match (&scenario, &result) {
            (Scenario::Environment, Err(Failure::Metadata(error))) => assert!(matches!(
                error.as_ref(),
                metadata::Failure::RootScalar { .. }
            )),
            (Scenario::Unsupported, Err(Failure::Compile(PrepareError::Unsupported(_)))) => (),
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
    let result = domain_entry::domain(request(), &mut ports).unwrap();
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
