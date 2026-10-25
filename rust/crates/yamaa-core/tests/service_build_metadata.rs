//! Service contracts: build ports, Define-XML service, domain entry and
//! specification ODM. The Define service's resource and publication refusals
//! are cases of `fixtures/diagnostics/submission.json`; each test here proves
//! a seam the replay cannot reach, which its first comment names (#3798).
mod service {
    mod build_ports {
        //! End-to-end shared request effects with the production metadata, activation
        //! and execution services and private native scripted ports.
        use crate::support::ports::*;
        use crate::support::project_call_compiler as support;
        use std::cell::RefCell;
        use std::sync::Arc;
        use support::{call, specification, Tree};
        use yamaa_core::{
            application_observer::Scope,
            domain_entry::{self, Request},
            function_invocation::{Argument, HostError},
            host_functions::{HostFunctions, Lock},
            project_activation,
            project_domain::{self, CheckedOwnedProject},
            project_preparation as metadata,
            specification_run::CapturedAttempt,
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::ExecutionEnvironment,
            project_function::Language,
            schema::{Document, DocumentNode as N},
            specification::PrepareError,
            table::{Column, TableSchema},
            value::{ColumnType, Value},
        };
        #[derive(Debug)]
        struct Opaque;
        /// The rows one build executed.
        type Rows = Vec<Vec<Value>>;
        #[derive(Clone, Copy)]
        enum Scenario {
            Success,
            Environment,
            Unsupported,
            Conformance,
            Capture,
        }
        #[derive(Debug)]
        enum Failure {
            Metadata(Box<metadata::Failure<Opaque, MetadataDocument, Opaque>>),
            Compile(PrepareError),
            Activation(project_activation::Failure<Opaque>),
        }
        struct Entry {
            model: Document,
            environment: ExecutionEnvironment,
        }
        struct Activation {
            log: EventLog,
            fail: bool,
        }
        impl HostFunctions for Activation {
            type Handle = ();
            type Error = Opaque;
            fn verify_lock(
                &mut self,
                _: Language,
                _: Lock<'_>,
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
        fn document(tree: Tree) -> Document {
            tree.document()
        }
        /// The 1.0 project lists its function file and holds the Python lock
        /// (D8); an invalid lock path is a project scalar failure that
        /// precedes every capture.
        fn project(invalid: bool) -> Document {
            let lock = if invalid {
                Tree::boolean(true)
            } else {
                Tree::text("uv.lock")
            };
            document(Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("project")),
                (
                    "functions",
                    Tree::List(vec![Tree::text("functions/constant.yaml")]),
                ),
                (
                    "environments",
                    Tree::map(vec![("python", Tree::map(vec![("lock", lock)]))]),
                ),
            ]))
        }
        /// The function file, which names its function and its Python callable.
        fn function() -> Document {
            document(Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("function")),
                ("name", Tree::text("constant")),
                ("description", Tree::text("A constant")),
                ("params", Tree::List(vec![])),
                ("returns", Tree::text("int")),
                ("may_return_missing", Tree::Scalar(N::Boolean(false))),
                ("comparison_decimals", Tree::Scalar(N::Integer("4".into()))),
                (
                    "implementations",
                    Tree::map(vec![("python", Tree::text("project.constant"))]),
                ),
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
            ]))
        }
        struct Application {
            scenario: Scenario,
            log: EventLog,
            source: ScriptedSource<Opaque>,
            decoder: ScriptedDecoder<Opaque, FixtureTable<Opaque>>,
            activation: Activation,
        }
        impl Application {
            fn new(scenario: Scenario) -> Self {
                let log = EventLog::default();
                Self {
                    scenario,
                    source: ScriptedSource::new(log.clone()),
                    decoder: ScriptedDecoder::new(log.clone()),
                    activation: Activation {
                        log: log.clone(),
                        fail: matches!(scenario, Scenario::Conformance),
                    },
                    log,
                }
            }
            /// One run of `checked`: the rows it executed, or none when the
            /// study capture failed.
            fn run(&mut self, checked: &CheckedOwnedProject) -> Result<Option<Rows>, Failure> {
                let source = checked.compiled().source().clone();
                self.source.inspections.push_back(InspectionStep {
                    source: source.clone(),
                    outcome: Ok(()),
                });
                self.source.captures.push_back(CaptureStep {
                    source: source.clone(),
                    created: true,
                    outcome: if matches!(self.scenario, Scenario::Capture) {
                        Err(Opaque)
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
                let mut attempt = CapturedAttempt::new(&source);
                checked
                    .build_into(
                        (&mut self.activation, b""),
                        &mut self.source,
                        &mut self.decoder,
                        &mut attempt,
                        (
                            None,
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop,
                            ),
                        ),
                    )
                    .map_err(Failure::Activation)?;
                Ok(attempt
                    .result
                    .as_ref()
                    .ok()
                    .and_then(|result| result.result.as_ref().ok())
                    .map(|execution| execution.dataset.rows().to_vec()))
            }
        }
        impl Application {
            fn capture(&mut self, request: Request<'_>) -> Result<Entry, Failure> {
                assert_eq!(request.environment, Some("project.yaml"));
                let source = metadata::Source {
                    identity: "/study/project.yaml".into(),
                    bytes: b"held project".to_vec(),
                };
                let file = metadata::Source {
                    identity: "/study/functions/constant.yaml".into(),
                    bytes: b"held function".to_vec(),
                };
                let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(self.log.clone());
                decoder.steps.push_back(MetadataDecodeStep {
                    source: source.clone(),
                    target: metadata::DecodeTarget::Project,
                    outcome: Ok(project(matches!(self.scenario, Scenario::Environment))),
                });
                decoder.steps.push_back(MetadataDecodeStep {
                    source: file.clone(),
                    target: metadata::DecodeTarget::FunctionFile,
                    outcome: Ok(function()),
                });
                // The lock and function paths resolve from the project file (D8).
                let mut port = ScriptedMetadata::<Opaque>::new(self.log.clone());
                port.steps.push_back(MetadataCaptureStep {
                    kind: metadata::Kind::Lock,
                    declaring_source: source.identity.clone(),
                    written: "uv.lock".into(),
                    // The scripted decoder reads held bytes `uv` as a uv lock.
                    outcome: Ok(metadata::Source {
                        identity: "/study/uv.lock".into(),
                        bytes: b"uv".to_vec(),
                    }),
                });
                port.steps.push_back(MetadataCaptureStep {
                    kind: metadata::Kind::Function,
                    declaring_source: source.identity.clone(),
                    written: "functions/constant.yaml".into(),
                    outcome: Ok(file.clone()),
                });
                let environment =
                    metadata::prepare(&mut decoder, source, Some(Language::Python), &mut port)
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
                // The entry's metadata snapshots are verified before selection.
                self.log.record(Event::Observation("verify metadata"));
                Ok(Entry { model, environment })
            }
            fn compile(&mut self, entry: Entry) -> Result<CheckedOwnedProject, Failure> {
                self.log.record(Event::Observation("compile"));
                let tree = yamaa_core::front_end::v1::spec::lower(&entry.model, None).into_spec();
                project_domain::check_owned_tree_linked(
                    tree,
                    &entry.model,
                    entry.environment,
                    Default::default(),
                )
                .map_err(|rejection| Failure::Compile(rejection.error))
            }
            fn prepare(&mut self, request: Request<'_>) -> Result<CheckedOwnedProject, Failure> {
                let entry = self.capture(request)?;
                self.compile(entry)
            }
            fn check(&mut self, _: CheckedOwnedProject) -> Result<(), Failure> {
                self.log.record(Event::Observation("static check"));
                Ok(())
            }
            /// One build: compile, then run.
            fn build(&mut self) -> Result<Option<Rows>, Failure> {
                let ports = RefCell::new(self);
                domain_entry::domain(
                    Scope::default(),
                    || ports.borrow_mut().prepare(request()),
                    |checked| ports.borrow_mut().run(checked),
                )
            }
        }
        fn request() -> Request<'static> {
            Request {
                specification: "spec.yaml",
                environment: Some("project.yaml"),
            }
        }
        /// Seam: scripted build ports and the event log of what each was asked, which
        /// no reader scripts.
        #[test]
        fn complete_request_failures_execute_nothing_and_admission_precedes_study_capture() {
            for scenario in [
                Scenario::Environment,
                Scenario::Unsupported,
                Scenario::Conformance,
                Scenario::Capture,
            ] {
                let mut ports = Application::new(scenario);
                let result = ports.build();
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
                    (Scenario::Capture, Ok(rows)) => assert!(rows.is_none()),
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
            }
        }
        /// Seam: scripted build ports and the event log of what each was asked, which
        /// no reader scripts.
        #[test]
        fn check_has_no_runtime_effects_and_a_successful_build_executes_its_rows() {
            let mut ports = Application::new(Scenario::Success);
            let checked = ports.prepare(request()).unwrap();
            ports.check(checked).unwrap();
            assert!(!ports.log.events().iter().any(|event| matches!(
                event,
                Event::Call { .. } | Event::Capture { .. } | Event::Observation("lock" | "binding")
            )));
            ports.log.clear();
            let rows = ports.build().unwrap();
            assert_eq!(rows, Some(vec![vec![Value::Int(1), Value::Int(7)]]));
        }
    }
    mod define_service {
        //! The Define service's findings and creation time, which core keeps.
        //! The service itself, which composes, renders and admits a section's
        //! files, is the application layer's `file_define`.
        use std::cell::Cell;
        use yamaa_core::define;
        use yamaa_core::diagnostic::ConditionCode;

        /// Seam: the clock callback and how many times it is read.
        ///
        /// Without `created`, the clock is read once, and its reading is one
        /// whole-second civil time or a refusal (DEFINE-3).
        #[test]
        fn clock_is_observed_once_and_calendar_admission_is_owned_by_the_engine() {
            for (seconds, expected) in [
                (0, "1970-01-01T00:00:00"),
                (951782400, "2000-02-29T00:00:00"),
            ] {
                let calls = Cell::new(0);
                let created = define::created_time(None, || {
                    calls.set(calls.get() + 1);
                    Some(seconds)
                });
                assert_eq!(created.as_deref(), Ok(expected), "{seconds}");
                assert_eq!(calls.get(), 1);
            }
            // The refusal names the clock's reading, null when it gave none.
            for (seconds, detail, value) in [
                (
                    None,
                    "UTC clock before epoch",
                    yamaa_core::json::Value::Null,
                ),
                (
                    Some(u64::MAX),
                    "UTC clock outside supported calendar",
                    yamaa_core::json::Value::from(u64::MAX),
                ),
            ] {
                let calls = Cell::new(0);
                let refused = define::created_time(None, || {
                    calls.set(calls.get() + 1);
                    seconds
                });
                assert_eq!(
                    refused,
                    Err(yamaa_core::define_diagnostics::creation_refusal(
                        detail, value
                    )),
                    "{detail}"
                );
                assert_eq!(calls.get(), 1);
            }
        }

        /// Seam: a clock callback that must not be read.
        ///
        /// An explicit creation time is admitted, or refused, without reading
        /// the clock.
        #[test]
        fn explicit_creation_is_admitted_without_reading_the_clock() {
            let clock = || -> Option<u64> { panic!("an explicit creation time read the clock") };
            for created in ["2026-10-11T00:00:00.5", "2026-02-30T00:00:00"] {
                let Err(finding) = define::created_time(Some(created), clock) else {
                    panic!("{created} admitted")
                };
                assert_eq!(
                    finding.code,
                    ConditionCode::DefineCreatedInvalid,
                    "{created}"
                );
            }
            assert_eq!(
                define::created_time(Some("2026-10-11T00:00:00"), clock).as_deref(),
                Ok("2026-10-11T00:00:00")
            );
        }

        /// Seam: the `utc_time` kernel's value for each clock reading.
        ///
        /// The time a composition records for one clock reading, which a
        /// submission takes once for all its sections (D129).
        #[test]
        fn a_clock_reading_is_one_whole_second_civil_time_through_the_year_9999() {
            for (seconds, expected) in [
                (0, Some("1970-01-01T00:00:00")),
                (951782400, Some("2000-02-29T00:00:00")),
                (253402300799, Some("9999-12-31T23:59:59")),
                (253402300800, None),
                (u64::MAX, None),
            ] {
                assert_eq!(define::utc_time(seconds).as_deref(), expected, "{seconds}");
            }
        }
    }
    mod domain_entry {
        //! One request composition selects held metadata before any execution authority.
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{
            application_observer::Scope,
            domain_entry::{self, Request},
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
                    let expected = if self.events.contains(&"build") || event == "build" {
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
            fn execute(&mut self) -> Result<(), Original> {
                self.enter("build")?;
                self.enter("lock")?;
                self.enter("function tests")?;
                self.enter("study capture")?;
                self.enter("decode")?;
                self.enter("execute")?;
                self.enter("encode")?;
                self.enter("report")
            }
        }
        impl Fake {
            fn capture(&mut self, request: Request<'_>) -> Result<Entry, Original> {
                assert_eq!(request.specification, "spec.yaml");
                self.enter("root admission")?;
                if let Some(path) = request.environment {
                    assert_eq!(path, "environment.yaml");
                    self.enter("environment")?;
                }
                self.enter("entry")?;
                self.enter("snapshot verification")?;
                Ok(Entry)
            }
            /// One route with or without an environment: capture, then compile.
            fn prepare(&mut self, request: Request<'_>) -> Result<(), Original> {
                self.capture(request)?;
                self.enter("compilation")
            }
            fn check(&mut self) -> Result<(), Original> {
                self.enter("check")
            }
            /// One build: compile, then run, observed through `scope`.
            fn domain(&mut self, request: Request<'_>, scope: Scope<'_>) -> Result<(), Original> {
                let port = std::cell::RefCell::new(self);
                domain_entry::domain(
                    scope,
                    || port.borrow_mut().prepare(request),
                    |()| port.borrow_mut().execute(),
                )
            }
        }
        fn request(environment: bool) -> Request<'static> {
            Request {
                specification: "spec.yaml",
                environment: environment.then_some("environment.yaml"),
            }
        }

        /// Seam: the route-stage observer and the order of what it is told.
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
                    let result = port.domain(request(environment), Scope::new(&observer));
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
        /// Seam: scripted ports and the route each entry point takes through them.
        #[test]
        fn both_entry_points_take_one_route_with_or_without_an_environment() {
            for environment in [false, true] {
                let mut port = Fake::default();
                assert!(port.prepare(request(environment)).is_ok());
                assert!(port.check().is_ok());
                let mut expected = vec!["root admission"];
                if environment {
                    expected.push("environment");
                }
                expected.extend(["entry", "snapshot verification", "compilation", "check"]);
                assert_eq!(port.events, expected);
                port.events.clear();
                assert!(port.domain(request(environment), Scope::default()).is_ok());
                expected.pop();
                expected.extend([
                    "build",
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
        /// Seam: port doubles' original failure payloads and the later steps each
        /// failure stops.
        #[test]
        fn every_failure_retains_its_original_payload_and_stops_later_authority() {
            for stage in [
                "root admission",
                "environment",
                "entry",
                "snapshot verification",
                "compilation",
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
                let Err(error) = port.domain(request(true), Scope::default()) else {
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
                        | "compilation"
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
            decoder,
            resolver::{Captured, Failure, Located, Name, Resolver},
            specification_run::{self as run, CapturedAttempt, PortError, RunError, SourceIssue},
        };
        use yamaa_core::{
            odm_source::FIELDS,
            specification::{PreparedSpecification, SourceDeclaration, SourceInterpretation},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::ColumnType,
        };

        use crate::support::document::Tree;
        use crate::support::specification::prepare;

        fn prepared() -> PreparedSpecification {
            // The canonical document normalization admits: an ODM read is
            // expanded to its class, and a name is an expression's text.
            let column = |name, value| {
                Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text("str")),
                    ("derivation", Tree::map(vec![("value", value)])),
                ])
            };
            let odm = |item| Tree::map(vec![("odm", Tree::map(vec![("item", Tree::text(item))]))]);
            let tree = Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("spec")),
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
                        column("ID", Tree::text("SRC.SubjectKey")),
                        column("VAL", odm("SRC.IT.TEST.OID")),
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
            prepare(&tree.document()).unwrap()
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
        impl Resolver for Port {
            type Error = Original;
            fn inspect(&mut self, name: Name<'_>) -> Result<Located, Original> {
                let Name::Input(source) = name else {
                    panic!("unexpected request {name:?}")
                };
                Ok(Located {
                    identity: source.path.clone(),
                    display_path: source.path.clone(),
                })
            }
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Original> {
                let Name::Input(source) = name else {
                    panic!("unexpected request {name:?}")
                };
                self.trace
                    .borrow_mut()
                    .push(format!("capture:{}", source.name));
                self.reads += 1;
                Ok(Captured::resource(Arc::from(b"held".as_slice()), true))
            }
            fn failure(&self, _: Name<'_>, _: &Original) -> Option<Failure> {
                None
            }
            fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Original> {
                panic!("an input is never rebased")
            }
        }
        struct Decoder {
            trace: Trace,
            payload: Rc<()>,
            fail: Option<(usize, usize)>,
        }
        impl decoder::Decoder for Decoder {
            crate::support::ports::no_documents!();
            fn continue_after(&self, _error: &Self::Error) -> bool {
                false
            }
            type Error = Original;
            type Table = Table;

            fn table(
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

        /// Seam: a scripted resource port and decoder, and the original decoder
        /// failure the findings keep.
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

        /// Seam: a port double whose identifier access fails, and the findings and
        /// captures it suppresses.
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
