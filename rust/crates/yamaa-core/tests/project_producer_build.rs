//! Project contracts.
#[allow(dead_code)]
mod support;
mod project {
    mod producer_build {
        use crate::support::producer_graph as support;
        use std::{
            cell::{Cell, RefCell},
            rc::Rc,
            sync::Arc,
        };
        use support::{candidate, document, environment, node};
        use yamaa_core::{
            dataset::{Dataset, Execution, ExecutionError},
            function_invocation::{Argument, FailureKind, HostError},
            producer_build::{
                self as build, CaptureError, CodecPort, Failure, ReportPort, StudyPort,
            },
            project_activation::{self, ActivationPort},
            specification_output::{CompleteError, OutputReport},
            specification_run::{PortError, SourceDecoder},
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            producer_graph::Node,
            project_environment::LockReference,
            project_function::Language,
            specification::{PreparedSpecification, SourceDeclaration},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };
        type Trace = Rc<RefCell<Vec<String>>>;
        #[derive(Debug)]
        struct Payload {
            original: Rc<()>,
            interrupt: bool,
        }
        #[derive(Clone, Copy)]
        enum Mode {
            Pass,
            Lock,
            Binding,
            CaseInterrupt,
            Producer,
            Consumer,
            Unwind,
        }
        struct Activation {
            trace: Trace,
            original: Rc<()>,
            unwind: Arc<()>,
            mode: Mode,
        }
        impl Activation {
            fn payload(&self, interrupt: bool) -> Payload {
                Payload {
                    original: Rc::clone(&self.original),
                    interrupt,
                }
            }
        }
        impl ActivationPort for Activation {
            type Error = Payload;
            type Handle = String;
            fn verify_lock(
                &mut self,
                _: Language,
                lock: &LockReference,
                calls: &[ProjectFunctionIdentity],
            ) -> Result<(), Payload> {
                assert_eq!(lock.written, "uv.lock");
                assert_eq!(
                    calls.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
                    ["beta", "alpha"]
                );
                self.trace.borrow_mut().push("lock".into());
                if matches!(self.mode, Mode::Lock) {
                    Err(self.payload(false))
                } else {
                    Ok(())
                }
            }
            fn bind(
                &mut self,
                call: &ProjectFunctionIdentity,
                _: &LogicalSignature,
            ) -> Result<String, Payload> {
                self.trace.borrow_mut().push(format!("bind:{}", call.name));
                if matches!(self.mode, Mode::Binding) {
                    Err(self.payload(false))
                } else {
                    Ok(call.name.clone())
                }
            }
            fn is_interrupt(&self, error: &Payload) -> bool {
                error.interrupt
            }
            fn invoke(
                &mut self,
                name: &String,
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Payload>> {
                let [Argument {
                    name: "x",
                    value: ValueRef::Int(value),
                }] = args
                else {
                    panic!("exact integer argument")
                };
                self.trace
                    .borrow_mut()
                    .push(format!("invoke:{name}:{value}"));
                match (self.mode, name.as_str(), *value) {
                    (Mode::CaseInterrupt, "beta", i64::MIN) => {
                        Err(HostError::Raised(self.payload(true)))
                    }
                    (Mode::Producer, "beta", 1) | (Mode::Consumer, "alpha", 1) => {
                        Err(HostError::Raised(self.payload(false)))
                    }
                    (Mode::Unwind, "beta", 1) => std::panic::panic_any(Arc::clone(&self.unwind)),
                    _ => Ok(Value::Int(*value)),
                }
            }
        }
        struct Study {
            trace: Trace,
            reads: usize,
            failure: bool,
            unwind: Option<Arc<()>>,
        }
        impl StudyPort for Study {
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            type Error = &'static str;
            fn inspect(
                &mut self,
                node: &Node,
                source: &SourceDeclaration,
            ) -> Result<(), Self::Error> {
                assert_eq!((node.identity(), source.name.as_str()), ("/p", "RAW"));
                self.trace.borrow_mut().push("inspect:/p:RAW".into());
                Ok(())
            }
            fn capture_reads(&self) -> usize {
                self.reads
            }
            fn capture(
                &mut self,
                node: &Node,
                source: &SourceDeclaration,
            ) -> Result<Arc<[u8]>, Self::Error> {
                assert_eq!((node.identity(), source.name.as_str()), ("/p", "RAW"));
                self.trace.borrow_mut().push("capture:/p:RAW".into());
                if let Some(payload) = &self.unwind {
                    std::panic::panic_any(Arc::clone(payload));
                }
                if self.failure {
                    return Err("original source refusal");
                }
                let bytes = b"ID,VALUE\n1,1\n";

                self.reads += 1;
                Ok(Arc::from(bytes.as_slice()))
            }
        }
        struct Table {
            cells: Rc<Cell<usize>>,
            cardinality: Rc<Cell<usize>>,
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
        }
        impl TableAccess for Table {
            type Error = Payload;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.cardinality.set(self.cardinality.get() + 1);
                self.rows.len()
            }
            fn cell(&self, row: usize, col: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
                self.cells.set(self.cells.get() + 1);
                Ok(ValueRef::from(&self.rows[row][col]))
            }
        }
        struct Decoder {
            schema_case: usize,
            cells: Rc<Cell<usize>>,
            cardinality: Rc<Cell<usize>>,
            trace: Trace,
            failure: bool,
        }
        impl SourceDecoder for Decoder {
            fn continue_after(&self, _error: &Self::Error) -> bool {
                false
            }

            type Error = &'static str;
            type Table = Table;
            fn decode(
                &mut self,
                source: &SourceDeclaration,
                bytes: &[u8],
                _interpretation: yamaa_core::specification::SourceInterpretation,
                _contract: Option<&yamaa_core::producer_contract::Contract>,
            ) -> Result<Table, Self::Error> {
                self.trace
                    .borrow_mut()
                    .push(format!("decode:{}", source.name));
                if self.failure {
                    return Err("original decode refusal");
                }
                assert_eq!(bytes, b"ID,VALUE\n1,1\n");
                let mut columns = ["ID", "VALUE"]
                    .map(|name| Column {
                        name: name.into(),
                        kind: ColumnType::Int,
                    })
                    .to_vec();
                let mut row = vec![Value::Int(1), Value::Int(1)];
                if source.name != "RAW" {
                    self.cells.set(0);
                    self.cardinality.set(0);
                    match self.schema_case {
                        1 => columns.swap(0, 1),
                        2 => {
                            columns.push(Column {
                                name: "EXTRA".into(),
                                kind: ColumnType::Int,
                            });
                            row.push(Value::Int(1));
                        }
                        3 => {
                            columns.pop();
                            row.pop();
                        }
                        4 => columns[1].kind = ColumnType::Str,
                        _ => {}
                    }
                }
                Ok(Table {
                    cells: Rc::clone(&self.cells),
                    cardinality: Rc::clone(&self.cardinality),
                    schema: TableSchema::new(columns).unwrap(),
                    rows: vec![row],
                })
            }
        }
        struct Report {
            trace: Trace,
            node: String,
            failure: bool,
            diagnostics: bool,
            diagnostic_failure: bool,
            diagnostic_unwind: Option<Arc<()>>,
        }
        impl ReportPort for Report {
            fn prepare_diagnostics<C, D, T: TableAccess>(
                &mut self,
                node: &Node,
                _attempt: &yamaa_core::specification_run::CapturedAttempt<C, D, T>,
            ) -> Result<(), Self::Error> {
                if !self.diagnostics {
                    return Ok(());
                }
                self.trace
                    .borrow_mut()
                    .push(format!("diagnostics:{}", node.identity()));
                if let Some(payload) = &self.diagnostic_unwind {
                    std::panic::panic_any(Arc::clone(payload));
                }
                if self.diagnostic_failure {
                    return Err("original diagnostic refusal");
                }
                Ok(())
            }
            fn select<C, D, T: TableAccess>(
                &mut self,
                node: &Node,
                _attempt: &yamaa_core::specification_run::CapturedAttempt<C, D, T>,
            ) -> Result<(), Self::Error> {
                self.trace
                    .borrow_mut()
                    .push(format!("report:{}", node.identity()));
                if self.failure {
                    return Err("original report refusal");
                }

                self.node = node.identity().into();
                Ok(())
            }
        }
        impl OutputReport for Report {
            type Error = &'static str;
            type Report = String;
            fn failure(&mut self) -> Result<String, Self::Error> {
                Ok(self.node.clone())
            }
            fn begin(&mut self, _: &Execution) -> Result<(), Self::Error> {
                self.trace.borrow_mut().push("begin".into());
                Ok(())
            }
            fn rejected(
                &mut self,
                _: &[yamaa_core::specification::OutputFinding],
            ) -> Result<String, Self::Error> {
                Ok(self.node.clone())
            }
            fn success(
                &mut self,
                _: &Execution,
                _: &[usize],
                _: &[u8],
            ) -> Result<String, Self::Error> {
                self.trace.borrow_mut().push("success".into());
                Ok(self.node.clone())
            }
        }
        struct Codec {
            trace: Trace,
            failure: bool,
        }
        impl CodecPort for Codec {
            type Error = &'static str;
            fn encode(
                &mut self,
                plan: &PreparedSpecification,
                dataset: &Dataset,
                projection: &[usize],
            ) -> Result<Vec<u8>, Self::Error> {
                self.trace
                    .borrow_mut()
                    .push(format!("encode:{}", plan.source().name));
                if self.failure {
                    return Err("original codec refusal");
                }
                assert_eq!(projection, [0, 1]);
                assert_eq!(dataset.cell(0, 0).unwrap(), ValueRef::Int(1));
                assert_eq!(dataset.cell(0, 1).unwrap(), ValueRef::Int(1));
                let bytes = b"ID,VALUE\n1,1\n";

                Ok(bytes.to_vec())
            }
        }
        struct Ports {
            activation: Activation,
            study: Study,
            decoder: Decoder,
            report: Report,
            codec: Codec,
        }
        impl Ports {
            fn new() -> Self {
                let trace = Rc::new(RefCell::new(vec![]));
                Self {
                    activation: Activation {
                        trace: Rc::clone(&trace),
                        original: Rc::new(()),
                        unwind: Arc::new(()),
                        mode: Mode::Pass,
                    },
                    study: Study {
                        trace: Rc::clone(&trace),
                        reads: 0,
                        failure: false,
                        unwind: None,
                    },
                    decoder: Decoder {
                        schema_case: 0,
                        cells: Rc::new(Cell::new(0)),
                        cardinality: Rc::new(Cell::new(0)),
                        trace: Rc::clone(&trace),
                        failure: false,
                    },
                    report: Report {
                        trace: Rc::clone(&trace),
                        node: String::new(),
                        failure: false,
                        diagnostics: false,
                        diagnostic_failure: false,
                        diagnostic_unwind: None,
                    },
                    codec: Codec {
                        trace,
                        failure: false,
                    },
                }
            }
            fn borrow(&mut self) -> build::Ports<'_, Activation, Study, Decoder, Report, Codec> {
                build::Ports {
                    activation: &mut self.activation,
                    study: &mut self.study,
                    decoder: &mut self.decoder,
                    report: &mut self.report,
                    codec: &mut self.codec,
                }
            }
        }
        type Attempt = build::BuildAttempt<Activation, Study, Decoder, Report, Codec>;

        mod application_observations {
            use super::*;
            use yamaa_core::{
                application_observer::{CallStage, Event, Observer, Outcome, Phase, Scope},
                dataset::ExecutionPhase,
            };
            type NodeCoordinate = Option<(usize, String)>;
            #[derive(Clone, Debug, PartialEq, Eq)]
            enum Seen {
                Phase(NodeCoordinate, Phase),
                Call(NodeCoordinate, CallStage),
                Exit(NodeCoordinate, Phase, Outcome),
                Return(NodeCoordinate, CallStage, Outcome),
                Refusal(
                    NodeCoordinate,
                    Phase,
                    yamaa_core::application_observer::Refusal,
                ),
            }
            struct Recorder {
                trace: Trace,
                events: RefCell<Vec<Seen>>,
            }
            impl Observer for Recorder {
                fn observe(&self, event: Event<'_>) {
                    let (node, seen, marker) = match event {
                        Event::Phase { node, phase } => {
                            (node, Seen::Phase(None, phase), "observe:phase")
                        }
                        Event::FunctionCall { node, stage } => {
                            (node, Seen::Call(None, stage), "observe:call")
                        }
                        Event::Exit {
                            node,
                            phase,
                            outcome,
                        } => (node, Seen::Exit(None, phase, outcome), "observe:exit"),
                        Event::FunctionReturn {
                            node,
                            stage,
                            outcome,
                        } => (node, Seen::Return(None, stage, outcome), "observe:return"),
                        Event::Refusal {
                            node,
                            phase,
                            refusal,
                        } => (node, Seen::Refusal(None, phase, refusal), "observe:refusal"),
                    };
                    let node = node.map(|node| (node.index, node.identity.to_owned()));
                    let seen = match seen {
                        Seen::Phase(_, phase) => Seen::Phase(node, phase),
                        Seen::Call(_, stage) => Seen::Call(node, stage),
                        Seen::Exit(_, phase, outcome) => Seen::Exit(node, phase, outcome),
                        Seen::Return(_, stage, outcome) => Seen::Return(node, stage, outcome),
                        Seen::Refusal(_, phase, refusal) => Seen::Refusal(node, phase, refusal),
                    };
                    self.events.borrow_mut().push(seen);
                    self.trace.borrow_mut().push(marker.into());
                }
            }
            fn recorder(ports: &Ports) -> Recorder {
                Recorder {
                    trace: Rc::clone(&ports.activation.trace),
                    events: RefCell::new(vec![]),
                }
            }
            fn calls(events: &[Seen]) -> Vec<(NodeCoordinate, CallStage)> {
                events
                    .iter()
                    .filter_map(|event| match event {
                        Seen::Call(node, stage) => Some((node.clone(), *stage)),
                        _ => None,
                    })
                    .collect()
            }
            fn assert_immediately_before_actual_calls(ports: &Ports) {
                let trace = ports.activation.trace.borrow();
                let invoked = trace
                    .iter()
                    .enumerate()
                    .filter(|(_, event)| event.starts_with("invoke:"));
                for (index, _) in invoked {
                    assert_eq!(trace[index - 1], "observe:call");
                }
                assert_eq!(
                    trace
                        .iter()
                        .filter(|event| *event == "observe:call")
                        .count(),
                    trace
                        .iter()
                        .filter(|event| event.starts_with("invoke:"))
                        .count()
                );
            }
            #[test]
            fn actual_node_exits_and_calls_keep_their_original_observer_order() {
                let mut ports = Ports::new();
                let observer = recorder(&ports);
                let mut attempt = Attempt::default();
                let result =
                    prepared().build_into(ports.borrow(), &mut attempt, Scope::new(&observer));
                assert!(result.is_ok());
                assert!(attempt.outcome.is_ok());
                assert_immediately_before_actual_calls(&ports);
                let events = observer.events.borrow();
                assert_eq!(
                    calls(&events),
                    [
                        (None, CallStage::Activation),
                        (None, CallStage::Activation),
                        (None, CallStage::Activation),
                        (None, CallStage::Activation),
                        (
                            Some((1, "/p".into())),
                            CallStage::Dataset(ExecutionPhase::Derivation)
                        ),
                        (
                            Some((0, "/root".into())),
                            CallStage::Dataset(ExecutionPhase::Derivation)
                        )
                    ]
                );
                for node in [(1, "/p"), (0, "/root")] {
                    let identity = Some((node.0, node.1.into()));
                    assert!(events.contains(&Seen::Exit(
                        identity.clone(),
                        Phase::Input,
                        Outcome::Succeeded
                    )));
                    assert!(events.contains(&Seen::Exit(
                        identity.clone(),
                        Phase::Dataset(ExecutionPhase::Admission),
                        Outcome::Succeeded
                    )));
                    assert!(events.contains(&Seen::Exit(
                        identity,
                        Phase::Dataset(ExecutionPhase::Derivation),
                        Outcome::Succeeded
                    )));
                }
                let root = Some((0, "/root".into()));

                assert!(!events.iter().any(|e| matches!(e, Seen::Refusal(..))));
                assert!(events.contains(&Seen::Exit(
                    root,
                    Phase::Dataset(ExecutionPhase::Verification),
                    Outcome::Succeeded
                )));
            }

            #[test]
            fn observer_preserves_alias_graph_truth_and_counts_only_real_activation_and_node_calls()
            {
                let build = prepared();
                let mut ordinary = Ports::new();
                let mut plain = Attempt::default();
                let expected = build
                    .build_into(
                        ordinary.borrow(),
                        &mut plain,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    )
                    .unwrap();
                let mut ports = Ports::new();
                let observer = recorder(&ports);
                let mut attempt = Attempt::default();
                let held = build
                    .build_into(ports.borrow(), &mut attempt, Scope::new(&observer))
                    .unwrap();
                let real_trace = ports
                    .activation
                    .trace
                    .borrow()
                    .iter()
                    .filter(|event| !event.starts_with("observe:"))
                    .cloned()
                    .collect::<Vec<_>>();
                assert_eq!(real_trace, *ordinary.activation.trace.borrow());
                assert_eq!(ports.study.reads, ordinary.study.reads);
                assert_eq!(ports.decoder.cells.get(), ordinary.decoder.cells.get());
                assert_eq!(attempt.activation, plain.activation);
                assert_eq!(
                    held.artifacts()
                        .iter()
                        .map(|artifact| (artifact.node(), artifact.path(), artifact.bytes()))
                        .collect::<Vec<_>>(),
                    expected
                        .artifacts()
                        .iter()
                        .map(|artifact| (artifact.node(), artifact.path(), artifact.bytes()))
                        .collect::<Vec<_>>()
                );
                let events = observer.events.borrow();
                assert_eq!(
                    calls(&events),
                    [
                        (None, CallStage::Activation),
                        (None, CallStage::Activation),
                        (None, CallStage::Activation),
                        (None, CallStage::Activation),
                        (
                            Some((1, "/p".into())),
                            CallStage::Dataset(ExecutionPhase::Derivation)
                        ),
                        (
                            Some((0, "/root".into())),
                            CallStage::Dataset(ExecutionPhase::Derivation)
                        ),
                    ]
                );
                // Each function's third authored case short-circuits missingness without
                // crossing the port; aliases still execute their one producer once.
                assert_eq!(attempt.activation.tests.len(), 6);
                assert_eq!(
                    attempt
                        .activation
                        .tests
                        .iter()
                        .filter(|case| case.invoked)
                        .count(),
                    4
                );
                let phases = events
                    .iter()
                    .filter_map(|event| match event {
                        Seen::Phase(node, phase) => Some((node.clone(), *phase)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let mut expected_phases = vec![(None, Phase::Activation)];
                for (index, identity) in [(1, "/p"), (0, "/root")] {
                    expected_phases.push((Some((index, identity.into())), Phase::Input));
                    for phase in [
                        ExecutionPhase::Admission,
                        ExecutionPhase::Derivation,
                        ExecutionPhase::OutputKeys,
                        ExecutionPhase::Verification,
                        ExecutionPhase::Finished,
                    ] {
                        expected_phases
                            .push((Some((index, identity.into())), Phase::Dataset(phase)));
                    }
                    expected_phases.push((Some((index, identity.into())), Phase::Output));
                }
                assert_eq!(phases, expected_phases);
                assert_eq!(
                    events
                        .iter()
                        .filter_map(|event| if let Seen::Return(node, stage, outcome) = event {
                            Some((node.clone(), *stage, *outcome))
                        } else {
                            None
                        })
                        .collect::<Vec<_>>(),
                    calls(&events)
                        .into_iter()
                        .map(|(node, stage)| (node, stage, Outcome::Succeeded))
                        .collect::<Vec<_>>()
                );
                assert!(events
                    .iter()
                    .filter_map(|event| if let Seen::Exit(_, _, outcome) = event {
                        Some(outcome)
                    } else {
                        None
                    })
                    .all(|outcome| *outcome == Outcome::Succeeded));
                assert_immediately_before_actual_calls(&ports);
            }
            #[test]
            fn observer_keeps_failed_activation_live_callback_and_unwind_prefixes() {
                for mode in [
                    Mode::Lock,
                    Mode::CaseInterrupt,
                    Mode::Producer,
                    Mode::Unwind,
                ] {
                    let mut ports = Ports::new();
                    ports.activation.mode = mode;
                    let observer = recorder(&ports);
                    let mut attempt = Attempt::default();
                    let build = prepared();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        build.build_into(ports.borrow(), &mut attempt, Scope::new(&observer))
                    }));
                    assert_immediately_before_actual_calls(&ports);
                    let events = observer.events.borrow();
                    let calls = calls(&events);
                    match mode {
                        Mode::Lock => {
                            assert!(result.unwrap().is_err());
                            assert!(calls.is_empty());
                            assert_eq!(
                                *events,
                                [
                                    Seen::Phase(None, Phase::Activation),
                                    Seen::Exit(None, Phase::Activation, Outcome::Failed)
                                ]
                            );
                            assert!(attempt.nodes.is_empty());
                        }
                        Mode::CaseInterrupt => {
                            assert!(result.unwrap().is_err());
                            assert_eq!(
                                calls,
                                [(None, CallStage::Activation), (None, CallStage::Activation)]
                            );
                            assert!(attempt.nodes.is_empty());
                            assert_eq!(ports.study.reads, 0);
                            assert_eq!(
                                events.last(),
                                Some(&Seen::Exit(None, Phase::Activation, Outcome::Failed))
                            );
                            assert!(events.contains(&Seen::Return(
                                None,
                                CallStage::Activation,
                                Outcome::Failed
                            )));
                        }
                        Mode::Producer => {
                            assert!(result.unwrap().is_err());
                            assert_eq!(calls.len(), 5);
                            assert!(events.contains(&Seen::Return(
                                Some((1, "/p".into())),
                                CallStage::Dataset(ExecutionPhase::Derivation),
                                Outcome::Failed
                            )));
                            assert!(events.contains(&Seen::Exit(
                                Some((1, "/p".into())),
                                Phase::Dataset(ExecutionPhase::Derivation),
                                Outcome::Failed
                            )));
                            assert_eq!(
                                calls.last(),
                                Some(&(
                                    Some((1, "/p".into())),
                                    CallStage::Dataset(ExecutionPhase::Derivation)
                                ))
                            );
                            let error = attempt.nodes[0]
                                .dataset
                                .result
                                .as_ref()
                                .unwrap()
                                .result
                                .as_ref()
                                .unwrap_err();
                            let ExecutionError::ProjectFunction { error, .. } = error.as_ref()
                            else {
                                panic!("original callback error")
                            };
                            let FailureKind::CallFailed(error) = &error.kind else {
                                panic!("original payload")
                            };
                            assert!(Rc::ptr_eq(&error.original, &ports.activation.original));
                            assert!(!events
                                .iter()
                                .any(|event| matches!(event, Seen::Phase(Some((0, _)), _))));
                        }
                        Mode::Unwind => {
                            let Err(payload) = result else {
                                panic!("original invocation unwind")
                            };
                            assert!(Arc::ptr_eq(
                                payload.downcast_ref::<Arc<()>>().unwrap(),
                                &ports.activation.unwind
                            ));
                            assert_eq!(calls.len(), 5);
                            assert_eq!(
                                events
                                    .iter()
                                    .filter(|event| matches!(event, Seen::Return(..)))
                                    .count(),
                                4
                            );
                            assert_eq!(
                                events.last(),
                                Some(&Seen::Call(
                                    Some((1, "/p".into())),
                                    CallStage::Dataset(ExecutionPhase::Derivation)
                                ))
                            );
                            assert!(matches!(
                                attempt.nodes[0].dataset.result,
                                Err(PortError::Incomplete)
                            ));
                        }
                        _ => unreachable!(),
                    }
                }
            }

            #[test]
            fn source_and_output_exits_use_actual_refusals_and_leave_unwound_output_unmatched() {
                for failure in ["source", "report", "unwind"] {
                    let mut ports = Ports::new();
                    ports.report.diagnostics = true;
                    let original = Arc::new(());
                    match failure {
                        "source" => ports.study.failure = true,
                        "report" => ports.report.diagnostic_failure = true,
                        "unwind" => ports.report.diagnostic_unwind = Some(Arc::clone(&original)),
                        _ => unreachable!(),
                    }
                    let observer = recorder(&ports);
                    let mut attempt = Attempt::default();
                    let build = prepared();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        build.build_into(ports.borrow(), &mut attempt, Scope::new(&observer))
                    }));
                    let events = observer.events.borrow();
                    match failure {
                        "source" => {
                            assert!(result.unwrap().is_err());
                            assert!(events.contains(&Seen::Exit(
                                Some((1, "/p".into())),
                                Phase::Input,
                                Outcome::Failed
                            )));
                            assert!(!events
                                .iter()
                                .any(|event| matches!(event, Seen::Phase(_, Phase::Dataset(_)))));
                            assert!(matches!(
                                attempt.nodes[0].dataset.result,
                                Err(PortError::Capture(CaptureError::External(
                                    "original source refusal"
                                )))
                            ));
                        }
                        "report" => {
                            assert!(result.unwrap().is_err());
                            assert_eq!(
                                events.last(),
                                Some(&Seen::Exit(
                                    Some((1, "/p".into())),
                                    Phase::Output,
                                    Outcome::Failed
                                ))
                            );
                            assert!(matches!(
                                attempt.nodes[0].output,
                                Err(CompleteError::Report("original diagnostic refusal"))
                            ));
                        }
                        "unwind" => {
                            let Err(payload) = result else {
                                panic!("original report unwind")
                            };
                            assert!(Arc::ptr_eq(
                                payload.downcast_ref::<Arc<()>>().unwrap(),
                                &original
                            ));
                            assert_eq!(
                                events.last(),
                                Some(&Seen::Phase(Some((1, "/p".into())), Phase::Output))
                            );
                            assert!(!events
                                .iter()
                                .any(|event| matches!(event, Seen::Exit(_, Phase::Output, _))));
                        }
                        _ => unreachable!(),
                    }
                    assert_immediately_before_actual_calls(&ports);
                }
            }
        }
        fn prepared() -> build::PreparedBuild {
            let root = document(
                &[("FIRST", Some("/p")), ("SECOND", Some("/p"))],
                Some("alpha"),
            );
            let producer = document(&[("RAW", None)], Some("beta"));
            let links = [
                candidate("/root", "FIRST", "/p", &producer),
                candidate("/root", "SECOND", "/p", &producer),
            ];
            yamaa_core::producer_graph::prepare(
                "/root",
                &[node("/root", &root, &links), node("/p", &producer, &[])],
                environment(&["beta", "unused", "alpha"]),
            )
            .unwrap()
            .into_build()
        }

        fn publication_diamond() -> build::PreparedBuild {
            let leaf = document(&[("RAW", None)], Some("beta"));
            let left = document(&[("FIRST", Some("/p"))], None);
            let right = document(&[("FIRST", Some("/p"))], None);
            let root = document(
                &[("FIRST", Some("/left")), ("SECOND", Some("/right"))],
                Some("alpha"),
            );
            let roots = [
                candidate("/root", "FIRST", "/left", &left),
                candidate("/root", "SECOND", "/right", &right),
            ];
            let lefts = [candidate("/left", "FIRST", "/p", &leaf)];
            let rights = [candidate("/right", "FIRST", "/p", &leaf)];
            yamaa_core::producer_graph::prepare(
                "/root",
                &[
                    node("/root", &root, &roots),
                    node("/left", &left, &lefts),
                    node("/right", &right, &rights),
                    node("/p", &leaf, &[]),
                ],
                environment(&["beta", "unused", "alpha"]),
            )
            .unwrap()
            .into_build()
        }

        #[test]
        fn diagnostic_preparation_precedes_primary_encoding_and_dependent_entry() {
            let mut ports = Ports::new();
            ports.report.diagnostics = true;
            let mut attempt = Attempt::default();
            assert!(prepared()
                .build_into(
                    ports.borrow(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop
                    )
                )
                .is_ok());
            assert!(attempt.outcome.is_ok());
            assert_eq!(ports.study.reads, 1);
            let events = ports.activation.trace.borrow();
            let at = |event: &str| events.iter().position(|e| e == event).unwrap();
            assert!(at("diagnostics:/p") < at("encode:RAW"));
            assert!(at("encode:RAW") < at("decode:FIRST"));
            assert!(at("decode:SECOND") < at("diagnostics:/root"));
            assert!(at("diagnostics:/root") < at("encode:FIRST"));
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.starts_with("diagnostics:"))
                    .count(),
                2
            );
        }

        #[test]
        fn diagnostic_refusal_stops_primary_encoding_and_every_dependent_source() {
            let mut ports = Ports::new();
            ports.report.diagnostics = true;
            ports.report.diagnostic_failure = true;
            let mut attempt = Attempt::default();
            assert!(prepared()
                .build_into(
                    ports.borrow(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop
                    )
                )
                .is_err());
            assert_eq!(attempt.nodes.len(), 1);
            assert_eq!(attempt.nodes[0].node, 1);
            assert!(attempt.nodes[0]
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .is_ok());
            assert!(matches!(
                attempt.nodes[0].output,
                Err(CompleteError::Report("original diagnostic refusal"))
            ));
            assert!(!ports
                .activation
                .trace
                .borrow()
                .iter()
                .any(|e| e.starts_with("encode:") || e == "decode:FIRST" || e == "decode:SECOND"));
        }

        #[test]
        fn failed_execution_diagnostic_refusal_retains_the_original_live_payload() {
            let mut ports = Ports::new();
            ports.activation.mode = Mode::Producer;
            ports.report.diagnostics = true;
            ports.report.diagnostic_failure = true;
            let mut attempt = Attempt::default();
            let _ = prepared().build_into(
                ports.borrow(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert_eq!(attempt.nodes.len(), 1);
            let entered = &attempt.nodes[0];
            let error = entered
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap_err();
            let ExecutionError::ProjectFunction { error, .. } = error.as_ref() else {
                panic!("original execution error")
            };
            let FailureKind::CallFailed(error) = &error.kind else {
                panic!("original live error")
            };
            assert!(Rc::ptr_eq(&error.original, &ports.activation.original));
            assert!(matches!(
                entered.output,
                Err(CompleteError::Report("original diagnostic refusal"))
            ));
            let events = ports.activation.trace.borrow();
            assert!(events.iter().any(|e| e == "diagnostics:/p"));
            assert!(!events.iter().any(|e| e.starts_with("encode:")));
            assert_eq!(events.iter().filter(|e| *e == "invoke:beta:1").count(), 1);
        }

        #[test]
        fn diagnostic_unwind_preserves_entered_execution_and_original_payload() {
            let mut ports = Ports::new();
            ports.report.diagnostics = true;
            let original = Arc::new(());
            ports.report.diagnostic_unwind = Some(Arc::clone(&original));
            let mut attempt = Attempt::default();
            let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = prepared().build_into(
                    ports.borrow(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
            }))
            .unwrap_err();
            assert!(Arc::ptr_eq(
                error.downcast_ref::<Arc<()>>().unwrap(),
                &original
            ));
            assert_eq!(attempt.nodes.len(), 1);
            assert!(attempt.nodes[0]
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .is_ok());
            assert!(matches!(attempt.nodes[0].output, Ok(None)));
            assert!(!ports
                .activation
                .trace
                .borrow()
                .iter()
                .any(|e| e.starts_with("encode:")));
        }

        struct Publication {
            calls: Vec<String>,
            writes: Vec<usize>,
            failure: Option<usize>,
            interrupt: bool,
            unwind: Option<usize>,
            original: Rc<()>,
            payload: Arc<()>,
            preflight_failure: bool,
        }
        impl Publication {
            fn new() -> Self {
                Self {
                    calls: vec![],
                    writes: vec![],
                    failure: None,
                    interrupt: false,
                    unwind: None,
                    original: Rc::new(()),
                    payload: Arc::new(()),
                    preflight_failure: false,
                }
            }
        }
        impl yamaa_core::producer_publication::PublicationPort for Publication {
            type Error = Payload;
            fn preflight(
                &mut self,
                artifacts: &[yamaa_core::producer_publication::Artifact],
            ) -> Result<(), Payload> {
                self.calls.push("preflight".into());
                assert!(artifacts.iter().all(|a| a.bytes() == b"ID,VALUE\n1,1\n"));
                if self.preflight_failure {
                    Err(Payload {
                        original: Rc::clone(&self.original),
                        interrupt: self.interrupt,
                    })
                } else {
                    Ok(())
                }
            }
            fn publish(
                &mut self,
                artifact: &yamaa_core::producer_publication::Artifact,
            ) -> Result<(), Payload> {
                let node = artifact.node();
                self.calls.push(format!("publish:{node}"));
                assert_eq!(artifact.path(), "produced.csv");
                assert_eq!(artifact.bytes(), b"ID,VALUE\n1,1\n");
                if self.unwind == Some(node) {
                    std::panic::panic_any(Arc::clone(&self.payload));
                }
                if self.failure == Some(node) {
                    return Err(Payload {
                        original: Rc::clone(&self.original),
                        interrupt: self.interrupt,
                    });
                }
                self.writes.push(node);
                Ok(())
            }
        }
        #[test]
        fn actual_build_seals_aliases_and_diamond_order_without_any_build_publication_or_save_replay(
        ) {
            for (build, order) in [
                (prepared(), vec![1, 0]),
                (publication_diamond(), vec![3, 1, 2, 0]),
            ] {
                let mut ports = Ports::new();
                let mut evidence = Attempt::default();
                let held = build
                    .build_into(
                        ports.borrow(),
                        &mut evidence,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    )
                    .unwrap();
                assert_eq!(ports.study.reads, 1);
                let build_events = ports.activation.trace.borrow().clone();
                assert!(!build_events.iter().any(|e| e.starts_with("publish")));
                assert_eq!(
                    held.artifacts()
                        .iter()
                        .map(|a| a.node())
                        .collect::<Vec<_>>(),
                    order
                );
                // Mutable caller evidence is not publication authority and cannot change
                // any privately held request, even after the prepared owner drops.
                evidence.nodes.clear();
                drop(build);
                let mut publisher = Publication::new();
                let mut saved = Default::default();
                for _ in 0..2 {
                    held.save_into(&mut publisher, &mut saved);
                    assert!(saved.outcome.is_ok());
                    assert_eq!(
                        saved
                            .entered
                            .iter()
                            .map(|n| (n.node, n.published))
                            .collect::<Vec<_>>(),
                        order.iter().map(|&n| (n, true)).collect::<Vec<_>>()
                    );
                }
                assert_eq!(publisher.writes, order.repeat(2));
                assert_eq!(*ports.activation.trace.borrow(), build_events);
            }
        }
        #[test]
        fn every_save_failure_and_interrupt_keeps_original_and_actual_prefix_and_prior_writes() {
            use yamaa_core::producer_publication::Failure as SaveFailure;
            for failed in [3, 1, 0] {
                for interrupt in [false, true] {
                    let mut ports = Ports::new();
                    let mut evidence = Attempt::default();
                    let held = publication_diamond()
                        .build_into(
                            ports.borrow(),
                            &mut evidence,
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop,
                            ),
                        )
                        .unwrap();
                    let build_events = ports.activation.trace.borrow().clone();
                    let mut publisher = Publication::new();
                    publisher.failure = Some(failed);
                    publisher.interrupt = interrupt;
                    let mut saved = Default::default();
                    held.save_into(&mut publisher, &mut saved);
                    let order = [3, 1, 2, 0];
                    let position = order.iter().position(|&n| n == failed).unwrap();
                    assert_eq!(
                        saved
                            .entered
                            .iter()
                            .map(|n| (n.node, n.published))
                            .collect::<Vec<_>>(),
                        order[..=position]
                            .iter()
                            .map(|&n| (n, n != failed))
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(publisher.writes, order[..position]);
                    assert!(
                        matches!(&saved.outcome,Err(SaveFailure::Publish {node,error}) if *node==failed && Rc::ptr_eq(&error.original,&publisher.original) && error.interrupt==interrupt)
                    );
                    publisher.failure = None;
                    held.save_into(&mut publisher, &mut saved);
                    assert!(saved.outcome.is_ok());
                    assert_eq!(*ports.activation.trace.borrow(), build_events);
                    assert!(evidence.outcome.is_ok() && evidence.nodes.len() == 4);
                }
            }
        }
        #[test]
        fn save_unwind_and_whole_set_preflight_refusal_preserve_the_actual_evidence_without_replay()
        {
            use yamaa_core::producer_publication::Failure as SaveFailure;
            let mut ports = Ports::new();
            let mut evidence = Attempt::default();
            let held = publication_diamond()
                .build_into(
                    ports.borrow(),
                    &mut evidence,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                )
                .unwrap();
            let mut publisher = Publication::new();
            publisher.unwind = Some(2);
            let mut saved = Default::default();
            let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                held.save_into(&mut publisher, &mut saved)
            }))
            .unwrap_err();
            assert!(Arc::ptr_eq(
                payload.downcast_ref::<Arc<()>>().unwrap(),
                &publisher.payload
            ));
            assert_eq!(
                saved
                    .entered
                    .iter()
                    .map(|n| (n.node, n.published))
                    .collect::<Vec<_>>(),
                [(3, true), (1, true), (2, false)]
            );
            assert!(matches!(saved.outcome, Err(SaveFailure::Incomplete)));
            assert_eq!(publisher.writes, [3, 1]);
            publisher.calls.clear();
            publisher.preflight_failure = true;
            held.save_into(&mut publisher, &mut saved);
            assert!(saved.entered.is_empty());
            assert_eq!(publisher.calls, ["preflight"]);
            assert!(
                matches!(&saved.outcome,Err(SaveFailure::Preflight(e)) if Rc::ptr_eq(&e.original,&publisher.original))
            );
        }
        #[test]
        fn failed_actual_build_discards_forged_caller_success_before_publication() {
            let build = publication_diamond();
            let mut ports = Ports::new();
            let mut evidence = Attempt::default();
            // Forged caller success is discarded on actual failed activation.
            ports.activation.mode = Mode::Lock;
            evidence.outcome = Ok(());
            assert!(build
                .build_into(
                    ports.borrow(),
                    &mut evidence,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop
                    )
                )
                .is_err());
            assert!(evidence.nodes.is_empty());
            assert!(matches!(evidence.outcome, Err(Failure::Activation(_))));
        }

        #[test]
        fn whole_union_activates_before_study_and_two_aliases_ingest_one_producer_artifact_arc() {
            let build = prepared();
            let mut ports = Ports::new();
            let mut attempt = Attempt::default();
            let _ = build.build_into(
                ports.borrow(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert!(attempt.outcome.is_ok());
            assert_eq!(ports.study.reads, 1);
            assert_eq!(
                attempt.nodes[0].dataset.sources[0].read.origin,
                Some(yamaa_core::specification_run::SourceOrigin::Resource)
            );
            assert_eq!(
                attempt.nodes.iter().map(|n| n.node).collect::<Vec<_>>(),
                [1, 0]
            );
            let artifact = attempt.nodes[0]
                .output
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .artifact()
                .unwrap();
            for source in &attempt.nodes[1].dataset.sources {
                let snapshot = source.snapshot.as_ref().unwrap();
                assert_eq!(snapshot.as_ptr(), artifact.bytes().as_ptr());
                assert_eq!(source.read.snapshots_created, Some(0));
                assert_eq!(
                    source.read.origin,
                    Some(yamaa_core::specification_run::SourceOrigin::Generated)
                );
            }
            assert_eq!(
                *ports.activation.trace.borrow(),
                [
                    "lock",
                    "bind:beta",
                    "bind:alpha",
                    "invoke:beta:7",
                    "invoke:beta:-9223372036854775808",
                    "invoke:alpha:7",
                    "invoke:alpha:-9223372036854775808",
                    "inspect:/p:RAW",
                    "capture:/p:RAW",
                    "decode:RAW",
                    "invoke:beta:1",
                    "report:/p",
                    "begin",
                    "encode:RAW",
                    "success",
                    "decode:FIRST",
                    "decode:SECOND",
                    "invoke:alpha:1",
                    "report:/root",
                    "begin",
                    "encode:FIRST",
                    "success"
                ]
            );
        }

        #[test]
        fn each_build_repeats_every_gate_and_old_attempt_evidence_stays_independent() {
            let build = prepared();
            let mut ports = Ports::new();
            let mut first = Attempt::default();
            let mut second = Attempt::default();
            let _ = build.build_into(
                ports.borrow(),
                &mut first,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            let _ = build.build_into(
                ports.borrow(),
                &mut second,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert!(first.outcome.is_ok() && second.outcome.is_ok());
            assert_eq!(ports.study.reads, 2);
            assert_eq!(
                ports
                    .activation
                    .trace
                    .borrow()
                    .iter()
                    .filter(|s| *s == "lock")
                    .count(),
                2
            );
            ports.activation.mode = Mode::Lock;
            let _ = build.build_into(
                ports.borrow(),
                &mut second,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert!(second.nodes.is_empty());
            assert_eq!(first.nodes.len(), 2);
            assert!(matches!(
                second.outcome,
                Err(Failure::Activation(project_activation::Failure::Lock(_)))
            ));
            assert_eq!(ports.study.reads, 2);
        }

        #[test]
        fn lock_binding_and_case_interrupts_keep_original_failure_and_zero_study_authority() {
            let build = prepared();
            for mode in [Mode::Lock, Mode::Binding, Mode::CaseInterrupt] {
                let mut ports = Ports::new();
                ports.activation.mode = mode;
                let mut attempt = Attempt::default();
                let _ = build.build_into(
                    ports.borrow(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                assert!(attempt.nodes.is_empty());
                assert_eq!(ports.study.reads, 0);
                assert!(!ports
                    .activation
                    .trace
                    .borrow()
                    .iter()
                    .any(|s| s.starts_with("inspect:")));
                match attempt.outcome.unwrap_err() {
                    Failure::Activation(project_activation::Failure::Lock(error)) => {
                        assert!(Rc::ptr_eq(&error.original, &ports.activation.original))
                    }
                    Failure::Activation(project_activation::Failure::Bindings(errors)) => {
                        assert_eq!(errors.len(), 2);
                        assert!(errors
                            .iter()
                            .all(|e| Rc::ptr_eq(&e.error.original, &ports.activation.original)));
                        assert!(!ports
                            .activation
                            .trace
                            .borrow()
                            .iter()
                            .any(|s| s.starts_with("invoke:")));
                    }
                    Failure::Activation(project_activation::Failure::InterruptedInvocation {
                        error,
                        ..
                    }) => {
                        let FailureKind::CallFailed(error) = error.kind else {
                            panic!("original interruption")
                        };
                        assert!(
                            error.interrupt
                                && Rc::ptr_eq(&error.original, &ports.activation.original)
                        );
                        assert!(!ports
                            .activation
                            .trace
                            .borrow()
                            .iter()
                            .any(|s| s.starts_with("invoke:alpha:")));
                    }
                    _ => panic!("original activation failure"),
                }
            }
        }

        #[test]
        fn producer_and_consumer_failures_keep_entered_prefix_and_original_payload_without_retry() {
            let build = prepared();
            for (mode, nodes, name) in [(Mode::Producer, 1, "beta"), (Mode::Consumer, 2, "alpha")] {
                let mut ports = Ports::new();
                ports.activation.mode = mode;
                let mut attempt = Attempt::default();
                let _ = build.build_into(
                    ports.borrow(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                assert!(matches!(attempt.outcome, Err(Failure::Node { .. })));
                assert_eq!(attempt.nodes.len(), nodes);
                let last = attempt.nodes.last().unwrap();
                assert!(matches!(last.output, Ok(None)));
                let error = last
                    .dataset
                    .result
                    .as_ref()
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap_err();
                let ExecutionError::ProjectFunction { error, .. } = error.as_ref() else {
                    panic!("original live error")
                };
                let FailureKind::CallFailed(error) = &error.kind else {
                    panic!("opaque failure")
                };
                assert!(Rc::ptr_eq(&error.original, &ports.activation.original));
                assert_eq!(
                    ports
                        .activation
                        .trace
                        .borrow()
                        .iter()
                        .filter(|s| *s == &format!("invoke:{name}:1"))
                        .count(),
                    1
                );
                assert_eq!(ports.study.reads, 1);
                if nodes == 2 {
                    assert!(attempt.nodes[0]
                        .output
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .artifact()
                        .is_some());
                }
            }
        }

        #[test]
        fn source_decode_report_and_codec_failures_stop_dependents_and_retain_exact_stage_evidence()
        {
            let build = prepared();
            for stage in ["source", "decode", "report", "codec"] {
                let mut ports = Ports::new();
                match stage {
                    "source" => ports.study.failure = true,
                    "decode" => ports.decoder.failure = true,
                    "report" => ports.report.failure = true,
                    _ => ports.codec.failure = true,
                }
                let mut attempt = Attempt::default();
                let _ = build.build_into(
                    ports.borrow(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                assert_eq!(attempt.nodes.len(), 1);
                let node = &attempt.nodes[0];
                assert!(matches!(attempt.outcome, Err(Failure::Node { node: 1 })));
                match stage {
                    "source" => assert!(matches!(
                        node.dataset.result,
                        Err(PortError::Capture(CaptureError::External(
                            "original source refusal"
                        )))
                    )),
                    "decode" => {
                        assert!(node.dataset.result.is_err());
                        assert_eq!(
                            node.dataset.sources[0].snapshot.as_deref().unwrap(),
                            b"ID,VALUE\n1,1\n"
                        );
                    }
                    "report" => assert!(matches!(
                        node.output,
                        Err(CompleteError::Report("original report refusal"))
                    )),
                    _ => assert!(matches!(
                        node.output,
                        Err(CompleteError::Encode("original codec refusal"))
                    )),
                }
                assert_eq!(
                    node.dataset.sources[0].read.origin,
                    Some(yamaa_core::specification_run::SourceOrigin::Resource)
                );
                assert!(!ports
                    .activation
                    .trace
                    .borrow()
                    .iter()
                    .any(|s| s == "decode:FIRST"));
            }
        }

        #[test]
        fn study_capture_unwind_retains_resource_entry_without_inventing_a_completed_snapshot() {
            let build = prepared();
            let mut ports = Ports::new();
            let payload = Arc::new(());
            ports.study.unwind = Some(Arc::clone(&payload));
            let mut attempt = Attempt::default();
            let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = build.build_into(
                    ports.borrow(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
            }))
            .unwrap_err();
            assert!(Arc::ptr_eq(
                error.downcast_ref::<Arc<()>>().unwrap(),
                &payload
            ));
            assert!(matches!(attempt.outcome, Err(Failure::Incomplete)));
            assert_eq!(attempt.nodes.len(), 1);
            let source = &attempt.nodes[0].dataset.sources[0];
            assert_eq!(
                source.read.origin,
                Some(yamaa_core::specification_run::SourceOrigin::Resource)
            );
            assert_eq!(source.read.snapshots_created, None);
            assert!(!source.read.captured && source.snapshot.is_none() && source.table.is_none());
            assert_eq!(ports.study.reads, 0);
        }

        #[test]
        fn caller_owned_attempt_survives_original_host_unwind_with_incomplete_entered_node() {
            let build = prepared();
            let mut ports = Ports::new();
            ports.activation.mode = Mode::Unwind;
            let mut attempt = Attempt::default();
            let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                build.build_into(
                    ports.borrow(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                )
            }))
            .err()
            .expect("the original callback panic must propagate");
            assert!(Arc::ptr_eq(
                error.downcast_ref::<Arc<()>>().unwrap(),
                &ports.activation.unwind
            ));
            assert!(matches!(attempt.outcome, Err(Failure::Incomplete)));
            assert_eq!(attempt.nodes.len(), 1);
            assert_eq!(attempt.nodes[0].dataset.sources.len(), 1);
            assert!(attempt.nodes[0].dataset.sources[0].snapshot.is_some());
            assert_eq!(attempt.activation.bindings.len(), 2);
        }

        #[test]
        fn producer_schema_mismatch_keeps_portable_diagnostics_before_consumer_cardinality_or_cell_observation(
        ) {
            let build = prepared();
            for schema_case in 1..=4 {
                let mut ports = Ports::new();
                ports.decoder.schema_case = schema_case;
                let mut attempt = Attempt::default();
                let _ = build.build_into(
                    ports.borrow(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                assert!(matches!(attempt.outcome, Err(Failure::Node { node: 0 })));
                assert_eq!(ports.decoder.cardinality.get(), 0);
                assert_eq!(ports.decoder.cells.get(), 0);
                let Err(PortError::Run(yamaa_core::specification_run::RunError::Sources(errors))) =
                    &attempt.nodes[1].dataset.result
                else {
                    panic!("retained producer metadata refusals")
                };
                assert_eq!(errors.len(), 2);
                for (index, error) in errors {
                    let build::DecodeError::Metadata(diagnostic) = error else {
                        panic!("portable producer metadata mismatch")
                    };
                    assert_eq!(
                        diagnostic.code,
                        yamaa_core::diagnostic::ConditionCode::ProducerContractMismatch
                    );
                    let name = ["FIRST", "SECOND"][*index];
                    assert_eq!(
                        diagnostic.spec_paths,
                        [format!("input.{name}.schema"), format!("input.{name}.path")]
                    );
                }
                assert!(!ports
                    .activation
                    .trace
                    .borrow()
                    .iter()
                    .any(|s| s == "invoke:alpha:1"));
                assert!(attempt.nodes[1]
                    .dataset
                    .sources
                    .iter()
                    .all(|s| s.table.is_none() && s.snapshot.is_some()));
            }
        }

        #[test]
        fn aliases_reingest_the_held_producer_without_repeating_study_authority() {
            let prepared = prepared();
            let mut ports = Ports::new();
            let mut attempt = Attempt::default();
            let _ = prepared.build_into(
                ports.borrow(),
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert!(attempt.outcome.is_ok());
            assert_eq!(ports.study.reads, 1);
            assert_eq!(attempt.nodes.len(), 2);
            let trace = ports.activation.trace.borrow();
            assert_eq!(
                &trace[..7],
                [
                    "lock",
                    "bind:beta",
                    "bind:alpha",
                    "invoke:beta:7",
                    "invoke:beta:-9223372036854775808",
                    "invoke:alpha:7",
                    "invoke:alpha:-9223372036854775808"
                ]
            );
            assert!(attempt.nodes[0]
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .is_ok());
            assert!(attempt.nodes[0]
                .output
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .artifact()
                .is_some());
            for source in &attempt.nodes[1].dataset.sources {
                assert_eq!(source.read.snapshots_created, Some(0));
            }
            assert!(trace.iter().any(|event| event == "invoke:beta:1"));
            assert!(trace.iter().any(|event| event == "invoke:alpha:1"));
            assert!(attempt.nodes[1]
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .is_ok());
            assert!(trace.iter().any(|event| event == "encode:FIRST"));
        }

        #[test]
        fn owned_columns_and_serialized_projection_keep_independent_shapes_across_nodes() {
            use yamaa_core::schema::{Document, DocumentNode as N, SpecificationDocument};
            fn append(nodes: &mut Vec<N>, node: N) -> usize {
                let index = nodes.len();
                nodes.push(node);
                index
            }
            fn text(nodes: &mut Vec<N>, value: &str) -> usize {
                append(nodes, N::Text(value.into()))
            }
            fn mapping(nodes: &mut Vec<N>, fields: Vec<(&str, usize)>) -> usize {
                let fields = fields
                    .into_iter()
                    .map(|(name, value)| (text(nodes, name), value))
                    .collect();
                append(nodes, N::Mapping(fields))
            }
            fn literal(nodes: &mut Vec<N>, value: i64) -> usize {
                let value = append(nodes, N::Integer(value.to_string()));
                let expression = mapping(nodes, vec![("literal", value)]);
                mapping(nodes, vec![("value", expression)])
            }
            fn ordered(nodes: &[N], index: usize, output: &mut Vec<N>) -> usize {
                let node = match &nodes[index] {
                    N::Sequence(items) => N::Sequence(
                        items
                            .iter()
                            .map(|&item| ordered(nodes, item, output))
                            .collect(),
                    ),
                    N::Mapping(fields) => N::Mapping(
                        fields
                            .iter()
                            .map(|&(name, value)| {
                                (ordered(nodes, name, output), ordered(nodes, value, output))
                            })
                            .collect(),
                    ),
                    node => node.clone(),
                };
                append(output, node)
            }
            struct RootCodec(Codec);
            impl CodecPort for RootCodec {
                type Error = &'static str;
                fn encode(
                    &mut self,
                    plan: &PreparedSpecification,
                    dataset: &Dataset,
                    projection: &[usize],
                ) -> Result<Vec<u8>, Self::Error> {
                    if plan.source().name == "RAW" {
                        return self.0.encode(plan, dataset, projection);
                    }
                    assert_eq!(projection, [0, 1, 2]);
                    assert_eq!(dataset.row_count(), 2);
                    for row in 0..2 {
                        assert_eq!(dataset.cell(row, 0).unwrap(), ValueRef::Int(1));
                        assert_eq!(dataset.cell(row, 1).unwrap(), ValueRef::Int(1));
                        assert_eq!(dataset.cell(row, 2).unwrap(), ValueRef::Int(row as i64 + 1));
                    }
                    self.0.trace.borrow_mut().push("encode:FIRST".into());
                    let bytes = b"ID,VALUE,ROW\n1,1,1\n1,1,2\n";
                    Ok(bytes.to_vec())
                }
            }
            let original = document(
                &[("FIRST", Some("/p")), ("SECOND", Some("/p"))],
                Some("alpha"),
            );
            let document = original.document();
            let mut nodes = document.nodes().to_vec();
            let columns = document.field(document.root(), "columns").unwrap();
            for name in ["ROW", "EXTRA"] {
                let column_name = text(&mut nodes, name);
                let kind = text(&mut nodes, "int");
                let label = text(&mut nodes, name);
                let derivation = literal(&mut nodes, 0);
                let column = mapping(
                    &mut nodes,
                    vec![
                        ("name", column_name),
                        ("type", kind),
                        ("label", label),
                        ("derivation", derivation),
                    ],
                );
                let N::Sequence(columns) = &mut nodes[columns] else {
                    panic!("declared columns");
                };
                columns.push(column);
            }
            let row_key = text(&mut nodes, "ROW");
            let keys = document.field(document.root(), "keys").unwrap();
            let N::Sequence(keys) = &mut nodes[keys] else {
                panic!("declared keys");
            };
            keys.push(row_key);
            let output = document.field(document.root(), "output").unwrap();
            let projection = document.field(output, "columns").unwrap();
            let row_projection = text(&mut nodes, "ROW");
            let N::Sequence(projection) = &mut nodes[projection] else {
                panic!("declared output projection");
            };
            projection.push(row_projection);
            let rows = (1..=2)
                .map(|value| {
                    let id = text(&mut nodes, &format!("row-{value}"));
                    let dataset = text(&mut nodes, "FIRST");
                    let row = literal(&mut nodes, value);
                    let derivations = mapping(&mut nodes, vec![("ROW", row)]);
                    mapping(
                        &mut nodes,
                        vec![
                            ("id", id),
                            ("dataset", dataset),
                            ("derivations", derivations),
                        ],
                    )
                })
                .collect();
            let rows = append(&mut nodes, N::Sequence(rows));
            let rows_name = text(&mut nodes, "rows");
            let N::Mapping(root) = &mut nodes[document.root()] else {
                panic!("specification mapping");
            };
            root.push((rows_name, rows));
            let mut ordered_nodes = Vec::new();
            let root = ordered(&nodes, document.root(), &mut ordered_nodes);
            let root = SpecificationDocument::admit(Document::new(ordered_nodes, root).unwrap())
                .unwrap()
                .unwrap();
            let producer = support::document(&[("RAW", None)], Some("beta"));
            let links = [
                candidate("/root", "FIRST", "/p", &producer),
                candidate("/root", "SECOND", "/p", &producer),
            ];
            let prepared = yamaa_core::producer_graph::prepare(
                "/root",
                &[node("/root", &root, &links), node("/p", &producer, &[])],
                environment(&["beta", "unused", "alpha"]),
            )
            .unwrap()
            .into_build();
            assert_eq!(
                prepared
                    .report_contexts()
                    .map(|(node, plan)| (node.identity(), plan.output().columns().len()))
                    .collect::<Vec<_>>(),
                [("/root", 4), ("/p", 2)]
            );
            // Runtime columns and the serialized projection remain distinct.

            let mut ports = Ports::new();
            let mut codec = RootCodec(Codec {
                trace: Rc::clone(&ports.codec.trace),
                failure: false,
            });
            let mut attempt =
                build::BuildAttempt::<Activation, Study, Decoder, Report, RootCodec>::default();
            let _ = prepared.build_into(
                build::Ports {
                    activation: &mut ports.activation,
                    study: &mut ports.study,
                    decoder: &mut ports.decoder,
                    report: &mut ports.report,
                    codec: &mut codec,
                },
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert!(
                attempt.outcome.is_ok(),
                "outcome={:?}, dataset={:?}",
                attempt.outcome,
                attempt.nodes.last().map(|node| node
                    .dataset
                    .result
                    .as_ref()
                    .map(|run| &run.result))
            );
            assert_eq!(ports.study.reads, 1);
            assert_eq!(attempt.nodes.len(), 2);
            let producer = &attempt.nodes[0];
            assert_eq!(producer.node, 1);
            let execution = producer
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap();
            assert_eq!(execution.dataset.row_count(), 1);
            assert_eq!(execution.dataset.schema().columns().len(), 2);
            assert_eq!(execution.dataset.cell(0, 1).unwrap(), ValueRef::Int(1));
            assert_eq!(
                producer
                    .output
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .artifact()
                    .unwrap()
                    .bytes(),
                b"ID,VALUE\n1,1\n"
            );
            let root = &attempt.nodes[1];
            assert_eq!(root.node, 0);
            assert!(root.dataset.sources.iter().all(|source| {
                source.read.captured
                    && source.read.snapshots_created == Some(0)
                    && source.snapshot.is_some()
                    && source.table.is_some()
            }));
            let trace = ports.activation.trace.borrow();
            assert_eq!(
                trace
                    .iter()
                    .filter(|event| *event == "invoke:beta:1")
                    .count(),
                1
            );
            assert_eq!(
                trace
                    .iter()
                    .filter(|event| *event == "invoke:alpha:1")
                    .count(),
                2
            );
            assert_eq!(
                trace.iter().filter(|event| *event == "encode:RAW").count(),
                1
            );

            let execution = root
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap();
            assert_eq!(execution.dataset.row_count(), 2);
            assert_eq!(execution.dataset.schema().columns().len(), 4);
            assert_eq!(execution.dataset.cell(0, 2).unwrap(), ValueRef::Int(1));
            assert_eq!(execution.dataset.cell(1, 2).unwrap(), ValueRef::Int(2));
            assert!(root
                .output
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .artifact()
                .is_some());
            assert_eq!(
                trace
                    .iter()
                    .filter(|event| *event == "encode:FIRST")
                    .count(),
                1
            );
        }

        struct PoolDecoder {
            inner: Decoder,
            raw_rows: usize,
            consumer_rows: usize,
        }
        impl SourceDecoder for PoolDecoder {
            fn continue_after(&self, _error: &Self::Error) -> bool {
                false
            }

            type Error = &'static str;
            type Table = Table;
            fn decode(
                &mut self,
                source: &SourceDeclaration,
                bytes: &[u8],
                interpretation: yamaa_core::specification::SourceInterpretation,
                contract: Option<&yamaa_core::producer_contract::Contract>,
            ) -> Result<Table, Self::Error> {
                let mut table = self.inner.decode(source, bytes, interpretation, contract)?;
                let rows = if source.name == "RAW" {
                    self.raw_rows
                } else {
                    self.consumer_rows
                };
                table.rows.resize(rows, vec![Value::Int(1), Value::Int(1)]);
                Ok(table)
            }
        }

        fn uneven_prepared() -> build::PreparedBuild {
            let producer = support::grouped_document(&[("RAW", None)]);
            let root = document(&[("FIRST", Some("/p"))], None);
            let links = [candidate("/root", "FIRST", "/p", &producer)];
            yamaa_core::producer_graph::prepare(
                "/root",
                &[node("/root", &root, &links), node("/p", &producer, &[])],
                environment(&[]),
            )
            .unwrap()
            .into_build()
        }
        #[test]
        fn uneven_graph_tables_keep_independent_row_counts_and_one_study_capture() {
            let mut ports = Ports::new();
            let mut decoder = PoolDecoder {
                inner: ports.decoder,
                raw_rows: 7,
                consumer_rows: 1,
            };
            let mut attempt =
                build::BuildAttempt::<Activation, Study, PoolDecoder, Report, Codec>::default();
            let _ = uneven_prepared().build_into(
                build::Ports {
                    activation: &mut ports.activation,
                    study: &mut ports.study,
                    decoder: &mut decoder,
                    report: &mut ports.report,
                    codec: &mut ports.codec,
                },
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert!(attempt.outcome.is_ok(), "{:?}", attempt.outcome);
            assert_eq!(attempt.nodes.len(), 2);
            assert_eq!(
                attempt.nodes[0].dataset.sources[0]
                    .table
                    .as_ref()
                    .unwrap()
                    .rows
                    .len(),
                7
            );
            assert_eq!(
                attempt.nodes[1].dataset.sources[0]
                    .table
                    .as_ref()
                    .unwrap()
                    .rows
                    .len(),
                1
            );
            assert_eq!(ports.study.reads, 1);
        }

        struct PrefixStudy(usize);
        impl StudyPort for PrefixStudy {
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            type Error = &'static str;
            fn capture_reads(&self) -> usize {
                self.0
            }
            fn inspect(&mut self, _: &Node, _: &SourceDeclaration) -> Result<(), Self::Error> {
                Ok(())
            }
            fn capture(
                &mut self,
                _: &Node,
                _: &SourceDeclaration,
            ) -> Result<Arc<[u8]>, Self::Error> {
                self.0 += 1;
                Ok(Arc::from(b"ID,VALUE\n1,1\n".as_slice()))
            }
        }
        struct PrefixDecoder {
            inner: Decoder,
            unwind: Option<Arc<()>>,
            entered: Vec<String>,
        }
        impl SourceDecoder for PrefixDecoder {
            type Error = &'static str;
            type Table = Table;
            fn continue_after(&self, error: &Self::Error) -> bool {
                *error == "original semantic decode failure"
            }
            fn decode(
                &mut self,
                source: &SourceDeclaration,
                bytes: &[u8],
                interpretation: yamaa_core::specification::SourceInterpretation,
                contract: Option<&yamaa_core::producer_contract::Contract>,
            ) -> Result<Table, Self::Error> {
                self.entered.push(source.name.clone());
                if source.name == "RAW" {
                    if let Some(payload) = &self.unwind {
                        std::panic::panic_any(Arc::clone(payload));
                    }
                    return Err("original semantic decode failure");
                }
                self.inner.decode(source, bytes, interpretation, contract)
            }
        }

        #[test]
        fn native_semantic_decode_failures_keep_later_independent_sources_and_original_unwind() {
            let root = document(&[("RAW", None), ("NEXT", None)], None);
            let prepared = yamaa_core::producer_graph::prepare(
                "/p",
                &[node("/p", &root, &[])],
                environment(&[]),
            )
            .unwrap()
            .into_build();
            for unwind in [false, true] {
                let mut ports = Ports::new();
                let payload = Arc::new(());
                let mut decoder = PrefixDecoder {
                    inner: ports.decoder,
                    unwind: unwind.then(|| Arc::clone(&payload)),
                    entered: Vec::new(),
                };
                let mut study = PrefixStudy(0);
                let mut attempt = build::BuildAttempt::<
                    Activation,
                    PrefixStudy,
                    PrefixDecoder,
                    Report,
                    Codec,
                >::default();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = prepared.build_into(
                        build::Ports {
                            activation: &mut ports.activation,
                            study: &mut study,
                            decoder: &mut decoder,
                            report: &mut ports.report,
                            codec: &mut ports.codec,
                        },
                        &mut attempt,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    );
                }));
                assert_eq!(attempt.nodes.len(), 1);
                if unwind {
                    let held = result.unwrap_err();
                    assert!(Arc::ptr_eq(
                        held.downcast_ref::<Arc<()>>().unwrap(),
                        &payload
                    ));
                    assert_eq!(decoder.entered.len(), 1);
                    assert!(attempt.nodes[0].dataset.result.is_err());
                    assert!(attempt.nodes[0].dataset.sources[0].table.is_none());
                } else {
                    result.unwrap();
                    assert_eq!(decoder.entered.len(), 2);
                    assert_eq!(decoder.entered, ["RAW", "NEXT"]);
                    assert!(attempt.nodes[0].dataset.sources[1].table.is_some());
                    let Err(PortError::Run(yamaa_core::specification_run::RunError::Sources(
                        errors,
                    ))) = &attempt.nodes[0].dataset.result
                    else {
                        panic!("original source failure survives later decode");
                    };
                    assert!(matches!(
                        errors.first(),
                        Some((
                            0,
                            build::DecodeError::Codec("original semantic decode failure")
                        ))
                    ));
                    assert_eq!(errors.len(), 1);
                }
                assert!(!ports
                    .activation
                    .trace
                    .borrow()
                    .iter()
                    .any(|event| event.starts_with("encode:")));
            }
        }
    }
    mod producer_contract {
        use yamaa_core::{
            producer_contract::{prepare, Cause, Error},
            schema::{Document, SpecificationDocument},
            value::ColumnType,
        };

        use crate::support::document::Tree;

        fn document(
            columns: &[(&str, &str, Option<&str>)],
            output: &[&str],
        ) -> SpecificationDocument {
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("PRODUCER")),
                (
                    "input",
                    Tree::map(vec![(
                        "SRC",
                        Tree::map(vec![("path", Tree::text("input.csv"))]),
                    )]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(
                        columns
                            .iter()
                            .map(|(name, kind, label)| {
                                let mut fields = vec![
                                    ("name", Tree::text(name)),
                                    ("type", Tree::text(kind)),
                                    (
                                        "derivation",
                                        Tree::map(vec![(
                                            "value",
                                            Tree::map(vec![(
                                                "source",
                                                Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                                            )]),
                                        )]),
                                    ),
                                ];
                                if let Some(label) = label {
                                    fields.push(("label", Tree::text(label)));
                                }
                                Tree::map(fields)
                            })
                            .collect(),
                    ),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("produced.csv")),
                        (
                            "columns",
                            Tree::List(output.iter().copied().map(Tree::text).collect()),
                        ),
                    ]),
                ),
            ]);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            SpecificationDocument::admit(Document::new(nodes, root).unwrap())
                .unwrap()
                .unwrap()
        }
        const FIELDS: &[(&str, &str, Option<&str>)] = &[
            ("VALUE", "float", Some("Reported value")),
            ("ID", "int", Some("Identifier")),
            ("INTERNAL", "str", None),
        ];

        #[test]
        fn producer_width_admits_1023_and_1024_and_refuses_each_1025_boundary() {
            let names: Vec<_> = (0..1_025).map(|i| format!("C{i:04}")).collect();
            let selected: Vec<_> = names.iter().map(String::as_str).collect();
            let columns: Vec<_> = selected
                .iter()
                .map(|&name| (name, "str", Some(name)))
                .collect();
            for width in [1_023, 1_024] {
                let contract = prepare(&document(&columns[..width], &selected[..width])).unwrap();
                assert_eq!(contract.fields().len(), width);
                assert!(contract.matches_header(&selected[..width]));
            }
            assert_eq!(
                prepare(&document(&columns, &selected[..1_024])),
                Err(Error::Limit(yamaa_core::limits::LimitExceeded {
                    resource: "columns",
                    limit: 1024,
                    actual: Some(1025)
                }))
            );
            assert_eq!(
                prepare(&document(&columns[..1_024], &selected)),
                Err(Error::Limit(yamaa_core::limits::LimitExceeded {
                    resource: "columns",
                    limit: 1024,
                    actual: Some(1025)
                }))
            );
        }

        #[test]
        fn producer_projection_is_the_stored_order_and_internal_columns_are_excluded() {
            let contract = prepare(&document(FIELDS, &["ID", "VALUE"])).unwrap();
            assert_eq!(contract.path(), "produced.csv");
            assert_eq!(
                contract
                    .fields()
                    .iter()
                    .map(|f| (f.name.as_str(), f.kind, f.label.as_str()))
                    .collect::<Vec<_>>(),
                vec![
                    ("ID", ColumnType::Int, "Identifier"),
                    ("VALUE", ColumnType::Float, "Reported value")
                ]
            );
            assert!(contract.matches_header(&["ID", "VALUE"]));
            for actual in [
                vec!["VALUE", "ID"],
                vec!["ID"],
                vec!["ID", "VALUE", "OTHER"],
                vec!["ID", "ID"],
                vec![],
            ] {
                assert!(!contract.matches_header(&actual));
            }
            assert!(contract
                .matches_typed_fields(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Float)]));
            assert!(!contract
                .matches_typed_fields(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Str)]));
            assert!(!contract
                .matches_typed_fields(&[("VALUE", ColumnType::Float), ("ID", ColumnType::Int)]));
        }

        #[test]
        fn producer_projection_rejects_empty_duplicate_and_undeclared_fields_in_order() {
            assert_eq!(
                prepare(&document(FIELDS, &[])),
                Err(Error::Invalid(vec![Cause::Empty]))
            );
            assert_eq!(
                prepare(&document(FIELDS, &["ID", "ID", "OTHER"])),
                Err(Error::Invalid(vec![
                    Cause::Duplicate {
                        position: 1,
                        field: "ID".into()
                    },
                    Cause::Declaration {
                        position: 2,
                        field: "OTHER".into(),
                        count: 0
                    }
                ]))
            );
        }

        #[test]
        fn producer_requires_one_typed_declaration_and_a_nonblank_label_per_stored_field() {
            let duplicates = [
                ("ID", "int", Some("Identifier")),
                ("ID", "str", Some("Other")),
            ];
            assert_eq!(
                prepare(&document(&duplicates, &["ID"])),
                Err(Error::Invalid(vec![Cause::Declaration {
                    position: 0,
                    field: "ID".into(),
                    count: 2
                }]))
            );
            for label in [None, Some(""), Some(" \t\n")] {
                assert_eq!(
                    prepare(&document(&[("ID", "int", label)], &["ID"])),
                    Err(Error::Invalid(vec![Cause::Label { field: "ID".into() }]))
                );
            }
        }

        #[test]
        fn every_declared_logical_type_survives_without_value_inference() {
            let fields = [
                ("ID", "int", Some("ID")),
                ("TEXT", "str", Some("Text")),
                ("VALUE", "float", Some("Value")),
                ("DATE", "date", Some("Date")),
                ("TIME", "datetime", Some("Time")),
            ];
            let names = ["TIME", "DATE", "VALUE", "TEXT", "ID"];
            let contract = prepare(&document(&fields, &names)).unwrap();
            assert_eq!(
                contract.fields().iter().map(|f| f.kind).collect::<Vec<_>>(),
                vec![
                    ColumnType::DateTime,
                    ColumnType::Date,
                    ColumnType::Float,
                    ColumnType::Str,
                    ColumnType::Int
                ]
            );
        }
    }
    mod producer_graph {
        use crate::support::producer_graph as support;
        use support::{candidate, document, environment, node};
        use yamaa_core::{
            producer_graph::{prepare, Error},
            specification::PrepareError,
        };

        #[test]
        fn diamond_and_shared_aliases_retain_authored_order_but_compile_each_identity_once() {
            let root = document(
                &[
                    ("RIGHT", Some("/right")),
                    ("LEFT", Some("/left")),
                    ("AGAIN", Some("/left")),
                ],
                None,
            );
            let left = document(&[("LEAF", Some("/leaf"))], None);
            let right = document(&[("LEAF", Some("/leaf"))], None);
            let leaf = document(&[("RAW", None)], None);
            let links = [
                candidate("/root", "AGAIN", "/left", &left),
                candidate("/root", "LEFT", "/left", &left),
                candidate("/root", "RIGHT", "/right", &right),
            ];
            let l = [candidate("/left", "LEAF", "/leaf", &leaf)];
            let r = [candidate("/right", "LEAF", "/leaf", &leaf)];
            let graph = prepare(
                "/root",
                &[
                    node("/root", &root, &links),
                    node("/left", &left, &l),
                    node("/leaf", &leaf, &[]),
                    node("/right", &right, &r),
                ],
                environment(&[]),
            )
            .unwrap();
            assert_eq!(graph.root(), 0);
            assert_eq!(graph.order(), [2, 3, 1, 0]);
            assert_eq!(graph.nodes().len(), 4);
            assert_eq!(graph.nodes()[0].dependencies(), [3, 1, 1]);
            assert_eq!(
                graph.nodes()[0]
                    .producers()
                    .iter()
                    .map(|p| p.dataset())
                    .collect::<Vec<_>>(),
                ["RIGHT", "LEFT", "AGAIN"]
            );
            assert!(graph.called_functions().is_empty());
            assert!(matches!(
                yamaa_core::specification::PreparedSpecification::prepare(&root),
                Err(PrepareError::Unsupported(_))
            ));
        }

        #[test]
        fn local_slots_project_into_unique_graph_selection_in_environment_order() {
            let root = document(&[("PROD", Some("/producer"))], Some("alpha"));
            let producer = document(&[("RAW", None)], Some("beta"));
            let links = [candidate("/root", "PROD", "/producer", &producer)];
            let env = environment(&["unused", "beta", "alpha"]);
            let cases = env.functions()[1].definition().tests.as_ptr();
            let graph = prepare(
                "/root",
                &[
                    node("/root", &root, &links),
                    node("/producer", &producer, &[]),
                ],
                env,
            )
            .unwrap();
            assert_eq!(graph.order(), [1, 0]);
            assert_eq!(graph.called_functions(), [1, 2]);
            assert_eq!(graph.nodes()[0].called_functions(), [2]);
            assert_eq!(graph.nodes()[0].activation_slots(), [1]);
            assert_eq!(graph.nodes()[1].called_functions(), [1]);
            assert_eq!(graph.nodes()[1].activation_slots(), [0]);
            assert_eq!(
                graph.environment().functions()[1]
                    .definition()
                    .tests
                    .as_ptr(),
                cases
            );
            for n in graph.nodes() {
                for (&local, &slot) in n.called_functions().iter().zip(n.activation_slots()) {
                    assert_eq!(local, graph.called_functions()[slot]);
                }
            }
        }

        #[test]
        fn recursive_and_self_cycles_retain_closed_identity_route_without_compiling_calls() {
            use serde_json::Value as V;

            let a = document(&[("B", Some("/b"))], Some("unknown"));
            let b = document(&[("C", Some("/c"))], None);
            let c = document(&[("A", Some("/a"))], None);
            let aa = [candidate("/a", "B", "/b", &b)];
            let bb = [candidate("/b", "C", "/c", &c)];
            let cc = [candidate("/c", "A", "/a", &a)];
            let Err(Error::Cycle {
                node: 2,
                diagnostic,
            }) = prepare(
                "/a",
                &[
                    node("/a", &a, &aa),
                    node("/b", &b, &bb),
                    node("/c", &c, &cc),
                ],
                environment(&[]),
            )
            else {
                panic!("cycle before unknown call")
            };
            assert_eq!(
                (
                    diagnostic.definition().condition,
                    diagnostic.definition().requirement
                ),
                ("producer_workflow_cycle", Some("REQ-0534"))
            );
            assert_eq!(diagnostic.spec_paths, ["input.A.schema"]);
            assert_eq!(
                diagnostic.context["cycle"],
                V::Array(
                    ["/a", "/b", "/c", "/a"]
                        .into_iter()
                        .map(|s| V::String(s.into()))
                        .collect()
                )
            );
            let own = document(&[("SELF", Some("/self"))], None);
            let links = [candidate("/self", "SELF", "/self", &own)];
            assert!(matches!(
                prepare("/self", &[node("/self", &own, &links)], environment(&[])),
                Err(Error::Cycle { .. })
            ));
        }

        #[test]
        fn closure_rejects_missing_duplicate_unreachable_or_replaced_node() {
            let root = document(&[("P", Some("/p"))], None);
            let p = document(&[("RAW", None)], None);
            let replaced = document(&[("RAW", None)], Some("unknown"));
            let links = [candidate("/root", "P", "/p", &p)];
            for (nodes, reason) in [
                (vec![node("/root", &root, &links)], "missing_producer_node"),
                (
                    vec![
                        node("/root", &root, &links),
                        node("/p", &p, &[]),
                        node("/p", &p, &[]),
                    ],
                    "duplicate_or_empty_identity",
                ),
                (
                    vec![
                        node("/root", &root, &links),
                        node("/p", &p, &[]),
                        node("/extra", &p, &[]),
                    ],
                    "unreachable_node",
                ),
                (
                    vec![node("/root", &root, &links), node("/p", &replaced, &[])],
                    "contradictory_producer_document",
                ),
            ] {
                assert!(
                    matches!(prepare("/root", &nodes, environment(&[])), Err(Error::Boundary { reason: actual, .. }) if actual == reason)
                );
            }
            assert!(matches!(
                prepare("/missing", &[node("/p", &p, &[])], environment(&[])),
                Err(Error::Boundary {
                    reason: "missing_root",
                    ..
                })
            ));
        }

        #[test]
        fn invalid_producer_and_consumer_calls_cannot_be_hidden_by_successful_graph_metadata() {
            for bad_producer in [true, false] {
                let root = document(
                    &[("P", Some("/p"))],
                    if bad_producer { None } else { Some("unknown") },
                );
                let p = document(
                    &[("RAW", None)],
                    if bad_producer { Some("unknown") } else { None },
                );
                let links = [candidate("/root", "P", "/p", &p)];
                let Err(Error::Compilation {
                    node: index,
                    error: PrepareError::Invalid(findings),
                }) = prepare(
                    "/root",
                    &[node("/root", &root, &links), node("/p", &p, &[])],
                    environment(&["id"]),
                )
                else {
                    panic!("complete static call checking")
                };
                assert_eq!(index, usize::from(bad_producer));
                assert_eq!(
                    findings[0].diagnostic().definition().condition,
                    "unknown_project_function"
                );
            }
        }

        #[test]
        fn shared_function_has_one_graph_slot_and_shared_output_metadata_cannot_diverge() {
            let root = document(
                &[("LEFT", Some("/left")), ("RIGHT", Some("/right"))],
                Some("id"),
            );
            let left = document(&[("LEAF", Some("/leaf"))], Some("id"));
            let right = document(&[("LEAF", Some("/leaf"))], Some("id"));
            let leaf = document(&[("RAW", None)], Some("id"));
            let links = [
                candidate("/root", "LEFT", "/left", &left),
                candidate("/root", "RIGHT", "/right", &right),
            ];
            let l = [candidate("/left", "LEAF", "/leaf", &leaf)];
            let mut r = [candidate("/right", "LEAF", "/leaf", &leaf)];
            let graph = prepare(
                "/root",
                &[
                    node("/root", &root, &links),
                    node("/left", &left, &l),
                    node("/right", &right, &r),
                    node("/leaf", &leaf, &[]),
                ],
                environment(&["unused", "id"]),
            )
            .unwrap();
            assert_eq!(graph.called_functions(), [1]);
            for node in graph.nodes() {
                assert_eq!(node.activation_slots(), [0]);
            }
            r[0].source_identity = "/different.csv";
            r[0].output_identity = "/different.csv";
            assert!(matches!(
                prepare(
                    "/root",
                    &[
                        node("/root", &root, &links),
                        node("/left", &left, &l),
                        node("/right", &right, &r),
                        node("/leaf", &leaf, &[])
                    ],
                    environment(&["id"])
                ),
                Err(Error::Boundary {
                    reason: "contradictory_producer_metadata",
                    ..
                })
            ));
        }

        #[test]
        fn long_callable_metadata_is_retained_without_an_accounting_pool() {
            use yamaa_core::{
                project_environment::{Draft, Environment},
                project_function::Language,
            };
            let mut definition = environment(&["id"]).functions()[0].definition().clone();
            definition.function = format!("project.{}", "a".repeat(8000));
            let env = Environment::admit(
                Language::Python,
                Draft {
                    language: Some(Language::Python),
                    lock: environment(&[]).lock().cloned(),
                    functions: Some(vec![definition]),
                    codelists: vec![],
                    has_study: false,
                    submissions: vec![],
                },
            )
            .unwrap()
            .into_execution();
            let root = document(&[("P", Some("/p"))], Some("id"));
            let p = document(&[("RAW", None)], None);
            let links = [candidate("/root", "P", "/p", &p)];
            let checked = prepare(
                "/root",
                &[node("/root", &root, &links), node("/p", &p, &[])],
                env,
            )
            .unwrap();
            assert_eq!(
                checked.environment().functions()[0].definition().function,
                format!("project.{}", "a".repeat(8000))
            );
        }
    }
}
