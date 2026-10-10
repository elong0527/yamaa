#[allow(dead_code)]
#[path = "../../yamaa-core/tests/support/producer_graph.rs"]
mod support;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
use support::{candidate, document, environment, node};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    producer_graph::Node,
    project_environment::LockReference,
    project_function::Language,
    specification::{PreparedSpecification, SourceDeclaration},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::{
    dataset::{Dataset, Execution, ExecutionError},
    function_invocation::{Argument, FailureKind, HostError},
    producer_build::{self as build, CaptureError, CodecPort, Failure, ReportPort, StudyPort},
    project_activation::{self, ActivationPort},
    specification_output::{CompleteError, OutputReport},
    specification_run::{PortError, SourceDecoder},
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
            (Mode::CaseInterrupt, "beta", i64::MIN) => Err(HostError::Raised(self.payload(true))),
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
}
impl StudyPort for Study {
    type Error = &'static str;
    fn inspect(&mut self, node: &Node, source: &SourceDeclaration) -> Result<(), Self::Error> {
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
        maximum: usize,
    ) -> Result<Arc<[u8]>, Self::Error> {
        assert_eq!((node.identity(), source.name.as_str()), ("/p", "RAW"));
        self.trace.borrow_mut().push("capture:/p:RAW".into());
        if self.failure {
            return Err("original source refusal");
        }
        let bytes = b"ID,VALUE\n1,1\n";
        if bytes.len() > maximum {
            return Err("source byte limit");
        }
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
    type Error = &'static str;
    type Table = Table;
    fn decode(&mut self, source: &SourceDeclaration, bytes: &[u8]) -> Result<Table, Self::Error> {
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
    maximum: usize,
    failure: bool,
}
impl ReportPort for Report {
    fn select<C, D, T: TableAccess>(
        &mut self,
        node: &Node,
        _attempt: &yamaa_engine::specification_run::CapturedAttempt<C, D, T>,
        maximum: usize,
    ) -> Result<(), Self::Error> {
        self.trace
            .borrow_mut()
            .push(format!("report:{}", node.identity()));
        if self.failure {
            return Err("original report refusal");
        }
        if node.identity().len() > maximum {
            return Err("report byte limit");
        }
        self.node = node.identity().into();
        self.maximum = maximum;
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
    fn success(&mut self, _: &Execution, _: &[usize], _: &[u8]) -> Result<String, Self::Error> {
        assert!(self.node.len() <= self.maximum);
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
        maximum: usize,
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
        if bytes.len() > maximum {
            return Err("output byte limit");
        }
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
                maximum: 0,
                failure: false,
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
    yamaa_engine::producer_graph::check(
        "/root",
        &[node("/root", &root, &links), node("/p", &producer, &[])],
        environment(&["beta", "unused", "alpha"]),
        Default::default(),
    )
    .unwrap()
    .into_build()
}

#[test]
fn whole_union_activates_before_study_and_two_aliases_ingest_one_producer_artifact_arc() {
    let build = prepared();
    let mut ports = Ports::new();
    let mut attempt = Attempt::default();
    build.build_into(ports.borrow(), Default::default(), &mut attempt);
    assert!(attempt.outcome.is_ok());
    assert_eq!(ports.study.reads, 1);
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
    build.build_into(ports.borrow(), Default::default(), &mut first);
    build.build_into(ports.borrow(), Default::default(), &mut second);
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
    build.build_into(ports.borrow(), Default::default(), &mut second);
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
        build.build_into(ports.borrow(), Default::default(), &mut attempt);
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
                assert!(error.interrupt && Rc::ptr_eq(&error.original, &ports.activation.original));
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
        build.build_into(ports.borrow(), Default::default(), &mut attempt);
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
fn source_decode_report_and_codec_failures_stop_dependents_and_retain_exact_stage_evidence() {
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
        build.build_into(ports.borrow(), Default::default(), &mut attempt);
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
        assert!(!ports
            .activation
            .trace
            .borrow()
            .iter()
            .any(|s| s == "decode:FIRST"));
    }
}

#[test]
fn graph_quotas_precede_ports_or_keep_the_honest_node_prefix_and_count_alias_reingestion() {
    let build = prepared();
    for resource in ["nodes", "functions", "source", "output", "routing", "cells"] {
        let mut limits = build::Limits::default();
        match resource {
            "nodes" => limits.nodes = 1,
            "functions" => limits.activation.functions = 1,
            "source" => limits.source_bytes = b"ID,VALUE\n1,1\n".len() * 2,
            "output" => limits.output_bytes = b"ID,VALUE\n1,1\n".len(),
            "routing" => limits.routing_work = 0,
            _ => limits.source_cells = 0,
        }
        let mut ports = Ports::new();
        let mut attempt = Attempt::default();
        build.build_into(ports.borrow(), limits, &mut attempt);
        assert!(attempt.outcome.is_err());
        if matches!(resource, "nodes" | "functions") {
            assert!(ports.activation.trace.borrow().is_empty());
            assert!(attempt.nodes.is_empty());
        } else if resource == "cells" {
            assert_eq!(attempt.nodes.len(), 1);
            assert_eq!(ports.study.reads, 1);
            assert!(!ports
                .activation
                .trace
                .borrow()
                .iter()
                .any(|s| s == "invoke:beta:1"));
        } else {
            assert_eq!(attempt.nodes.len(), 2);
            assert_eq!(ports.study.reads, 1);
            assert!(attempt.nodes[0]
                .output
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .artifact()
                .is_some());
            if resource == "source" {
                assert_eq!(attempt.nodes[1].dataset.sources.len(), 2);
                assert!(attempt.nodes[1].dataset.sources[0].snapshot.is_some());
                assert!(attempt.nodes[1].dataset.sources[1].snapshot.is_none());
            }
        }
    }
}

#[test]
fn caller_owned_attempt_survives_original_host_unwind_with_incomplete_entered_node() {
    let build = prepared();
    let mut ports = Ports::new();
    ports.activation.mode = Mode::Unwind;
    let mut attempt = Attempt::default();
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build.build_into(ports.borrow(), Default::default(), &mut attempt)
    }))
    .unwrap_err();
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

impl yamaa_engine::producer_build::DecodePort for Decoder {
    fn decode_bounded(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        _contract: Option<&yamaa_core::producer_contract::Contract>,
        limits: yamaa_engine::producer_build::DecodeLimits,
    ) -> Result<Table, build::DecodeError<Self::Error>> {
        if limits.cells < 2 || bytes.len() > limits.storage_bytes || bytes.len() > limits.work_bytes
        {
            return Err(build::DecodeError::Codec("original bounded codec refusal"));
        }
        self.decode(source, bytes)
            .map_err(build::DecodeError::Codec)
    }
}

#[test]
fn producer_schema_mismatch_keeps_portable_diagnostics_before_consumer_cardinality_or_cell_observation(
) {
    let build = prepared();
    for schema_case in 1..=4 {
        let mut ports = Ports::new();
        ports.decoder.schema_case = schema_case;
        let mut attempt = Attempt::default();
        build.build_into(ports.borrow(), Default::default(), &mut attempt);
        assert!(matches!(attempt.outcome, Err(Failure::Node { node: 0 })));
        assert_eq!(ports.decoder.cardinality.get(), 0);
        assert_eq!(ports.decoder.cells.get(), 0);
        let Err(PortError::Run(yamaa_engine::specification_run::RunError::Sources(errors))) =
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
fn alias_cells_and_codec_storage_and_work_are_reserved_before_decoded_table_ownership() {
    let build = prepared();
    for mode in 0..4 {
        let mut ports = Ports::new();
        let mut limits = build::Limits::default();
        match mode {
            0 => limits.source_cells = 5,
            1 => limits.source_storage_bytes = 0,
            2 => limits.decode_work_bytes = 0,
            _ => limits.routing_work = 7,
        }
        let mut attempt = Attempt::default();
        build.build_into(ports.borrow(), limits, &mut attempt);
        assert!(attempt.outcome.is_err());
        let entered = attempt.nodes.last().unwrap();
        assert!(entered.dataset.sources.last().unwrap().table.is_none());
        if mode == 0 {
            assert_eq!(attempt.nodes.len(), 2);
            assert!(entered.dataset.sources[0].table.is_some());
            assert!(!ports
                .activation
                .trace
                .borrow()
                .iter()
                .any(|s| s == "decode:SECOND"));
        } else if mode == 3 {
            assert_eq!(attempt.nodes.len(), 2);
            assert!(entered.dataset.sources[0].snapshot.is_some());
            assert_eq!(entered.dataset.sources.len(), 1);
            assert!(entered.dataset.sources.iter().all(|s| s.table.is_none()));
            assert!(!ports
                .activation
                .trace
                .borrow()
                .iter()
                .any(|s| s == "decode:FIRST"));
            let Err(PortError::Run(yamaa_engine::specification_run::RunError::Sources(errors))) =
                &entered.dataset.result
            else {
                panic!("original bounded decoder refusal")
            };
            assert_eq!(errors.len(), 1);
            assert!(matches!(
                errors[0],
                (0, build::DecodeError::Limit("producer_routing_work"))
            ));
        } else {
            assert_eq!(attempt.nodes.len(), 1);
        }
    }
}
