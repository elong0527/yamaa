#![cfg(any(unix, windows))]
use std::{
    fs,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use yamaa_adapters::{
    file_graph::FileGraph,
    file_producer_build::{Attempt, BoundaryFailure, CodecError, FileBuild, Limits},
    file_resources::{Error as ResourceError, Resources},
    project_source::{self, CapturePort, Kind, OwnedEnvironment, Reply, Request},
    shipped_schema,
    specification_source::Source,
};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    producer_graph::Node,
    project_environment::{LockKind, LockReference},
    project_function::Language,
    specification::OutputFinding,
    table::{TableAccess, ValueRef},
    value::Value,
};
use yamaa_engine::{
    dataset::{Execution, ExecutionError},
    function_invocation::{Argument, FailureKind, HostError},
    producer_build::{CaptureError, Failure, ReportPort},
    project_activation::{ActivationPort, Failure as ActivationFailure},
    specification_output::{CompleteError, OutputReport},
    specification_run::PortError,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-producer-build-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
    fn path(&self, written: &str) -> String {
        let path = self.0.join(written).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path
            .strip_prefix(r"\\?\")
            .unwrap_or(&path)
            .replace('\\', "/");
        path
    }
    fn write(&self, written: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(written);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn build(&self, language: Language) -> FileBuild {
        FileGraph::prepare(
            Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
            "consumer/root.yaml",
            environment(language),
            shipped_schema::capture().unwrap(),
            Default::default(),
        )
        .unwrap_or_else(|error| panic!("graph fixture: {:?}", error.error()))
        .into_build()
    }
    fn rounded(&self) {
        self.write("producer/layers/base.yaml", PRODUCER);
        self.write(
            "producer/p.yaml",
            "schema_version: '1.0'\nparents: [layers/base.yaml]\ndomain: PRODUCER\n",
        );
        self.write("consumer/root.yaml", CONSUMER);
        self.write("raw/raw.csv", b"ID,VALUE\n1,1.234\n2,2.345\n");
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
const PRODUCER: &str = "schema_version: '1.0'\ndomain: TEST\nbase: RAW\nkeys: [ID]\ninput: {RAW: {path: ../../raw/raw.csv, types: {ID: int, VALUE: float}}}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: float, label: Reported value, derivation: {function: {name: id_float, args: {x: RAW.VALUE}}}}\noutput: {path: ../../generated/producer.csv, columns: [ID, VALUE], decimals: 2}\n";
const CONSUMER: &str = "schema_version: '1.0'\ndomain: TEST\nbase: FIRST\nkeys: [ID]\ninput:\n  FIRST: {path: ../generated/producer.csv, schema: ../producer/p.yaml}\n  SECOND: {path: ../generated/producer.csv, schema: ../producer/./p.yaml}\nintermediates: [{id: SECOND_REFERENCE, dataset: SECOND, no_match: null}]\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: FIRST.ID}\n  - {name: FIRST_VALUE, type: float, label: First value, derivation: FIRST.VALUE}\n  - {name: SECOND_VALUE, type: float, label: Second value, derivation: SECOND_REFERENCE.VALUE}\n  - {name: VALUE, type: float, label: Reported value, derivation: {compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}}\noutput: {path: ../generated/root.csv, columns: [ID, VALUE], decimals: 2}\n";

#[test]
fn whole_graph_report_views_keep_original_document_and_plan_together() {
    use yamaa_adapters::specification_run_view::RunView;
    let study = Study::new();
    study.rounded();
    let build = study.build(Language::Python);
    let provenance = Arc::clone(build.provenance());
    assert_eq!(provenance.metadata().order(), [1, 0]);
    drop(build);
    fs::remove_dir_all(&study.0).unwrap();

    let views = provenance.report_views().collect::<Vec<_>>();
    assert_eq!(views.len(), 2);
    for (index, view) in views.iter().enumerate() {
        assert!(std::ptr::eq(
            view.document(),
            provenance.documents()[index].as_ref()
        ));
        assert_eq!(view.document().source().identity, view.node().identity());
        assert!(view.check_diagnostics().is_empty());
        assert_eq!(view.compiled().output_decimals(), Some("2"));
    }
    assert!(views[0].node().identity().ends_with("/consumer/root.yaml"));
    assert!(views[1].node().identity().ends_with("/producer/p.yaml"));
    assert_eq!(views[0].compiled().source().name, "FIRST");
    assert_eq!(views[1].compiled().source().name, "RAW");
    assert_eq!(views[0].document().parents().len(), 0);
    assert_eq!(views[1].document().parents().len(), 1);
    assert_eq!(
        views[1].document().parents()[0].source().bytes,
        PRODUCER.as_bytes()
    );
}

struct LockCapture {
    language: Language,
}
impl CapturePort for LockCapture {
    type Error = ();
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, ()> {
        assert_eq!(request.kind, Kind::Lock);
        Ok(Reply::Lock {
            source: Source {
                identity: "held/lock".into(),
                bytes: b"held original lock".to_vec(),
            },
            kind: if self.language == Language::Python {
                LockKind::Uv
            } else {
                LockKind::Renv
            },
        })
    }
}
fn environment(language: Language) -> OwnedEnvironment {
    let (name, call, lock) = if language == Language::Python {
        ("python", "program.id_float", "uv.lock")
    } else {
        ("r", "program::id_float", "renv.lock")
    };
    let text = format!("schema_version: '1.0'\nlanguage: {name}\nlock: {lock}\nfunctions:\n  id_float:\n    function: {call}\n    description: Float identity.\n    params: [{{name: x, type: float}}]\n    returns: float\n    tests:\n      - {{id: normal, covers: [normal, numeric-comparison], args: {{x: 7.0}}, result: 7.0}}\n      - {{id: boundary, covers: [boundary], args: {{x: 0.0}}, result: 0.0}}\n      - {{id: missing, covers: ['short-circuit-missing:x'], args: {{x: null}}, result: null}}\n");
    project_source::prepare(
        shipped_schema::capture_environment().unwrap(),
        Source {
            identity: "held/environment.yaml".into(),
            bytes: text.into_bytes(),
        },
        language,
        &mut LockCapture { language },
        Default::default(),
    )
    .unwrap_or_else(|error| match error {
        project_source::Failure::Rejected(error) => {
            panic!("environment fixture: {:?}", error.admission)
        }
        _ => panic!("environment fixture capture failed"),
    })
    .into_owned()
}
#[derive(Debug)]
struct Payload {
    original: Rc<()>,
    interrupt: bool,
}
#[derive(Clone, Copy)]
enum Mode {
    Pass,
    Lock,
    CaseInterrupt,
    LiveFailure,
    LiveInterrupt,
    Unwind,
    ChangeAfterActivation,
}
struct Activation {
    study: PathBuf,
    trace: Vec<String>,
    original: Rc<()>,
    unwind: Arc<()>,
    mode: Mode,
}
impl Activation {
    fn new(study: &Study, mode: Mode) -> Self {
        Self {
            study: study.0.clone(),
            trace: vec![],
            original: Rc::new(()),
            unwind: Arc::new(()),
            mode,
        }
    }
    fn payload(&self, interrupt: bool) -> Payload {
        Payload {
            original: Rc::clone(&self.original),
            interrupt,
        }
    }
}
impl ActivationPort for Activation {
    type Error = Payload;
    type Handle = ();
    fn verify_lock(
        &mut self,
        language: Language,
        lock: &LockReference,
        calls: &[ProjectFunctionIdentity],
    ) -> Result<(), Payload> {
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "id_float");
        assert_eq!(
            lock.kind,
            if language == Language::Python {
                LockKind::Uv
            } else {
                LockKind::Renv
            }
        );
        self.trace.push("lock".into());
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
    ) -> Result<(), Payload> {
        assert_eq!(call.name, "id_float");
        self.trace.push("bind".into());
        Ok(())
    }
    fn is_interrupt(&self, error: &Payload) -> bool {
        error.interrupt
    }
    fn invoke(&mut self, _: &(), args: &[Argument<'_>]) -> Result<Value, HostError<Payload>> {
        let [Argument {
            name: "x",
            value: ValueRef::Float(value),
        }] = args
        else {
            panic!("exact float argument")
        };
        self.trace.push(format!("invoke:{}", value.get()));
        if value.get() == 0.0 {
            if matches!(self.mode, Mode::CaseInterrupt) {
                return Err(HostError::Raised(self.payload(true)));
            }
            if matches!(self.mode, Mode::ChangeAfterActivation) {
                // Native study capture must happen after all conformance cases.
                fs::write(
                    self.study.join("raw/raw.csv"),
                    b"ID,VALUE\n1,1.234\n2,2.345\n",
                )
                .unwrap();
            }
        }
        if value.get() == 1.234 {
            match self.mode {
                Mode::LiveFailure => return Err(HostError::Raised(self.payload(false))),
                Mode::LiveInterrupt => return Err(HostError::Raised(self.payload(true))),
                Mode::Unwind => std::panic::panic_any(Arc::clone(&self.unwind)),
                _ => {}
            }
        }
        Ok(Value::Float(*value))
    }
}
#[derive(Default)]
struct Report {
    entered: Vec<String>,
    maximum: usize,
    fail: bool,
    final_failure: bool,
}
impl ReportPort for Report {
    fn select<C, D, T: TableAccess>(
        &mut self,
        node: &Node,
        _attempt: &yamaa_engine::specification_run::CapturedAttempt<C, D, T>,
        maximum: usize,
    ) -> Result<(), Self::Error> {
        self.entered.push(node.identity().into());
        self.maximum = maximum;
        if self.fail {
            Err("original report refusal")
        } else {
            Ok(())
        }
    }
}
impl OutputReport for Report {
    type Error = &'static str;
    type Report = String;
    fn failure(&mut self) -> Result<String, Self::Error> {
        Ok("failed".into())
    }
    fn begin(&mut self, _: &Execution) -> Result<(), Self::Error> {
        Ok(())
    }
    fn rejected(&mut self, _: &[OutputFinding]) -> Result<String, Self::Error> {
        Ok("rejected".into())
    }
    fn success(&mut self, _: &Execution, _: &[usize], _: &[u8]) -> Result<String, Self::Error> {
        if self.final_failure {
            Err("original report rendering refusal")
        } else if self.maximum < 8 {
            Err("original bounded report refusal")
        } else {
            Ok("accepted".into())
        }
    }
}
type NativeAttempt = Attempt<Payload, String, &'static str>;
fn artifact(attempt: &NativeAttempt, node: usize) -> &[u8] {
    attempt.graph.nodes[node]
        .output
        .as_ref()
        .ok()
        .unwrap()
        .as_ref()
        .unwrap()
        .artifact()
        .unwrap()
        .bytes()
}

fn passthrough(input: &str, base: &str, output: &str, kind: &str) -> String {
    format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - {{name: VALUE, type: {kind}, label: Value, derivation: {base}.VALUE}}\noutput: {{path: {output}, columns: [ID, VALUE]}}\n")
}

#[test]
fn native_parquet_round_trip_uses_held_artifact_and_a_functionless_graph_grants_no_activation() {
    let study = Study::new();
    study.write("raw/raw.csv", b"ID,VALUE\n1,13\n2,17\n");
    study.write(
        "producer/p.yaml",
        passthrough(
            "{RAW: {path: ../raw/raw.csv, types: {ID: int, VALUE: int}}}",
            "RAW",
            "../generated/producer.parquet",
            "int",
        ),
    );
    study.write("consumer/root.yaml", passthrough("{FIRST: {path: ../generated/producer.parquet, schema: ../producer/p.yaml}, SECOND: {path: ../generated/producer.parquet, schema: ../producer/./p.yaml}}", "FIRST", "../generated/root.csv", "int"));
    let mut build = study.build(Language::Python);
    let mut activation = Activation::new(&study, Mode::Lock);
    let mut report = Report::default();
    let attempt = build.build(&mut activation, &mut report, Limits::default());
    assert!(attempt.accepted());
    assert!(activation.trace.is_empty());
    assert_eq!(attempt.graph.activation, Default::default());
    assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,13\n2,17\n");
    let produced = artifact(&attempt, 0);
    assert!(produced.starts_with(b"PAR1") && produced.ends_with(b"PAR1"));
    for source in &attempt.graph.nodes[1].dataset.sources {
        assert_eq!(
            source.snapshot.as_ref().unwrap().as_ptr(),
            produced.as_ptr()
        );
        let table = source.table.as_ref().unwrap();
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(13));
        assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Int(17));
    }
    assert!(!study.0.join("generated").exists());
    let complete = build.build_reported(&mut activation, report_id(), Limits::default());
    assert!(complete.accepted());
    let value = yamaa_adapters::producer_report::attempt_report(&complete, report_id(), 16_777_216)
        .unwrap();
    assert_eq!(value["outcome"], "success");
    assert_eq!(
        value["activation"],
        serde_json::json!({"lock":"not_requested","bindings":[],"tests":[]})
    );
    assert_eq!(value["artifacts"][0]["profile"], "parquet");
    assert_eq!(
        value["artifacts"][0]["records"],
        serde_json::json!(["[\"ID\", \"VALUE\"]", "[1, 13]", "[2, 17]"])
    );
    assert_eq!(value["artifacts"][1]["content"], "ID,VALUE\n1,13\n2,17\n");
    let mut limits = Limits::default();
    limits.engine.source_storage_bytes = 1;
    let refused = build.build(&mut activation, &mut report, limits);
    assert!(!refused.accepted());
    assert!(refused.graph.nodes[0].dataset.sources[0].table.is_none());
}

#[test]
fn native_diamond_executes_each_canonical_node_once_and_keeps_shared_leaf_bytes() {
    let study = Study::new();
    study.rounded();
    for branch in ["left", "right"] {
        study.write(
            &format!("{branch}.yaml"),
            passthrough(
                "{LEAF: {path: generated/producer.csv, schema: producer/p.yaml}}",
                "LEAF",
                &format!("generated/{branch}.csv"),
                "float",
            ),
        );
    }
    let root = CONSUMER
        .replace(
            "../generated/producer.csv, schema: ../producer/p.yaml",
            "../generated/left.csv, schema: ../left.yaml",
        )
        .replace(
            "../generated/producer.csv, schema: ../producer/./p.yaml",
            "../generated/right.csv, schema: ../right.yaml",
        )
        .replace(
            "intermediates:",
            "  AGAIN: {path: ../generated/left.csv, schema: .././left.yaml}\nintermediates:",
        );
    study.write("consumer/root.yaml", root);
    let mut build = study.build(Language::Python);
    let mut activation = Activation::new(&study, Mode::Pass);
    let mut report = Report::default();
    let attempt = build.build(&mut activation, &mut report, Limits::default());
    assert!(attempt.accepted());
    assert_eq!(
        attempt
            .graph
            .nodes
            .iter()
            .map(|n| n.node)
            .collect::<Vec<_>>(),
        [3, 1, 2, 0]
    );
    assert_eq!(
        activation.trace,
        [
            "lock",
            "bind",
            "invoke:7",
            "invoke:0",
            "invoke:1.234",
            "invoke:2.345"
        ]
    );
    assert_eq!(
        report.entered,
        [
            study.path("producer/p.yaml"),
            study.path("left.yaml"),
            study.path("right.yaml"),
            study.path("consumer/root.yaml")
        ]
    );
    let left = attempt.graph.nodes[1].dataset.sources[0]
        .snapshot
        .as_ref()
        .unwrap();
    let right = attempt.graph.nodes[2].dataset.sources[0]
        .snapshot
        .as_ref()
        .unwrap();
    assert!(Arc::ptr_eq(left, right));
    assert_eq!(left.as_ptr(), artifact(&attempt, 0).as_ptr());
    assert_eq!(artifact(&attempt, 3), b"ID,VALUE\n1,2.46\n2,4.70\n");
    assert!(!study.0.join("generated").exists());
}

#[test]
fn native_rounded_bytes_are_reingested_twice_after_producer_only_activation_with_original_origins()
{
    for language in [Language::Python, Language::R] {
        let study = Study::new();
        study.rounded();
        let mut build = study.build(language);
        let mut activation = Activation::new(&study, Mode::ChangeAfterActivation);
        // A capture before the final activation case would observe the wrong data.
        study.write("raw/raw.csv", b"ID,VALUE\n1,99.99\n");
        let mut report = Report::default();
        let attempt = build.build(&mut activation, &mut report, Limits::default());
        assert!(attempt.accepted(), "{:?}", attempt.graph.outcome);
        assert_eq!(
            activation.trace,
            [
                "lock",
                "bind",
                "invoke:7",
                "invoke:0",
                "invoke:1.234",
                "invoke:2.345"
            ]
        );
        assert_eq!(
            attempt
                .graph
                .nodes
                .iter()
                .map(|n| n.node)
                .collect::<Vec<_>>(),
            [1, 0]
        );
        assert_eq!(artifact(&attempt, 0), b"ID,VALUE\n1,1.23\n2,2.35\n");
        assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
        let execution = attempt.graph.nodes[0]
            .dataset
            .result
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap();
        for (row, expected) in [1.234, 2.345].into_iter().enumerate() {
            let ValueRef::Float(value) = execution.dataset.cell(row, 1).unwrap() else {
                panic!("held unrounded producer float")
            };
            assert_eq!(value.get(), expected);
        }
        let consumer = &attempt.graph.nodes[1].dataset;
        assert_eq!(
            consumer
                .sources
                .iter()
                .map(|r| r.read.snapshots_created)
                .collect::<Vec<_>>(),
            [Some(0), Some(0)]
        );
        assert_eq!(consumer.sources.len(), 2);
        assert!(Arc::ptr_eq(
            consumer.sources[0].snapshot.as_ref().unwrap(),
            consumer.sources[1].snapshot.as_ref().unwrap()
        ));
        assert_eq!(
            &**consumer.sources[0].snapshot.as_ref().unwrap(),
            b"ID,VALUE\n1,1.23\n2,2.35\n"
        );
        assert_eq!(
            report.entered,
            [
                study.path("producer/p.yaml"),
                study.path("consumer/root.yaml")
            ]
        );
        let metadata = attempt.provenance.metadata();
        assert!(metadata.nodes()[0].activation_slots().is_empty());
        assert_eq!(metadata.nodes()[1].activation_slots(), [0]);
        assert_eq!(
            metadata.nodes()[0].producers()[0]
                .output_origin()
                .declaring_source(),
            study.path("producer/layers/base.yaml")
        );
        assert!(!study.0.join("generated").exists());
        drop(build);
        fs::remove_dir_all(&study.0).unwrap();
        assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
        assert_eq!(
            attempt.provenance.documents()[1].source().bytes,
            b"schema_version: '1.0'\nparents: [layers/base.yaml]\ndomain: PRODUCER\n"
        );
        assert!(attempt
            .resources
            .as_ref()
            .unwrap()
            .iter()
            .any(|(p, b)| p.ends_with("/raw/raw.csv") && &**b == b"ID,VALUE\n1,1.234\n2,2.345\n"));
    }
}

#[test]
fn fresh_builds_repeat_activation_and_detect_changed_study_after_activation_with_original_snapshot()
{
    let study = Study::new();
    study.rounded();
    let mut build = study.build(Language::Python);
    let mut activation = Activation::new(&study, Mode::Pass);
    let mut report = Report::default();
    let first = build.build(&mut activation, &mut report, Limits::default());
    let second = build.build(&mut activation, &mut report, Limits::default());
    assert!(first.accepted() && second.accepted());
    assert_eq!(
        first.graph.nodes[0].dataset.sources[0]
            .read
            .snapshots_created,
        Some(1)
    );
    assert_eq!(
        second.graph.nodes[0].dataset.sources[0]
            .read
            .snapshots_created,
        Some(0)
    );
    assert_eq!(activation.trace.len(), 12);
    study.write("raw/raw.csv", b"ID,VALUE\n1,9.99\n");
    activation.trace.clear();
    let changed = build.build(&mut activation, &mut report, Limits::default());
    assert!(!changed.accepted());
    assert!(changed.boundary.is_ok());
    assert_eq!(activation.trace, ["lock", "bind", "invoke:7", "invoke:0"]);
    assert_eq!(changed.graph.nodes.len(), 1);
    assert!(matches!(
        changed.graph.nodes[0].dataset.result,
        Err(PortError::Capture(CaptureError::External(
            ResourceError::Changed
        )))
    ));
    assert!(changed
        .resources
        .as_ref()
        .unwrap()
        .iter()
        .any(|(p, b)| p.ends_with("/raw/raw.csv") && &**b == b"ID,VALUE\n1,1.234\n2,2.345\n"));
    assert_eq!(artifact(&first, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
}

#[test]
fn native_changed_metadata_and_metadata_quota_refuse_before_host_or_study_authority() {
    let study = Study::new();
    study.rounded();
    let mut build = study.build(Language::Python);
    let mut activation = Activation::new(&study, Mode::Pass);
    let mut report = Report::default();
    let limits = Limits {
        resource_bytes: 0,
        ..Default::default()
    };
    let bounded = build.build(&mut activation, &mut report, limits);
    assert!(matches!(
        bounded.boundary,
        Err(BoundaryFailure::Resource { identity: None, .. })
    ));
    assert!(activation.trace.is_empty() && bounded.graph.nodes.is_empty());
    study.write("producer/layers/base.yaml", b"changed metadata");
    let changed = build.build(&mut activation, &mut report, Limits::default());
    assert!(
        matches!(&changed.boundary, Err(BoundaryFailure::Resource { identity: Some(p), error: ResourceError::Changed }) if p == &study.path("producer/layers/base.yaml"))
    );
    assert!(activation.trace.is_empty() && changed.graph.nodes.is_empty());
    assert!(changed
        .resources
        .as_ref()
        .unwrap()
        .iter()
        .any(|(p, b)| p.ends_with("/producer/layers/base.yaml") && &**b == PRODUCER.as_bytes()));
    assert!(!changed
        .resources
        .as_ref()
        .unwrap()
        .iter()
        .any(|(p, _)| p.ends_with(".csv")));
}

#[test]
fn native_activation_failures_interrupts_and_unwind_retain_owned_original_prefix_and_stop_dependents(
) {
    for mode in [
        Mode::Lock,
        Mode::CaseInterrupt,
        Mode::LiveFailure,
        Mode::LiveInterrupt,
        Mode::Unwind,
    ] {
        let study = Study::new();
        study.rounded();
        let mut build = study.build(Language::Python);
        let mut activation = Activation::new(&study, mode);
        let mut report = Report::default();
        let attempt = build.build(&mut activation, &mut report, Limits::default());
        assert!(!attempt.accepted());
        assert!(report.entered.is_empty());
        match mode {
            Mode::Lock => {
                assert!(attempt.graph.nodes.is_empty());
                assert!(
                    matches!(&attempt.graph.outcome, Err(Failure::Activation(ActivationFailure::Lock(e))) if Rc::ptr_eq(&e.original, &activation.original))
                );
            }
            Mode::CaseInterrupt => {
                assert!(attempt.graph.nodes.is_empty());
                assert_eq!(activation.trace.len(), 4);
            }
            Mode::Unwind => {
                let Err(BoundaryFailure::Unwind(payload)) = &attempt.boundary else {
                    panic!("original unwind")
                };
                assert!(Arc::ptr_eq(
                    payload.downcast_ref::<Arc<()>>().unwrap(),
                    &activation.unwind
                ));
                assert_eq!(attempt.graph.nodes.len(), 1);
                assert!(matches!(attempt.graph.outcome, Err(Failure::Incomplete)));
                assert_eq!(
                    attempt.graph.nodes[0]
                        .dataset
                        .sources
                        .iter()
                        .filter(|s| s.snapshot.is_some())
                        .count(),
                    1
                );
            }
            _ => {
                assert_eq!(attempt.graph.nodes.len(), 1);
                let execution = attempt.graph.nodes[0].dataset.result.as_ref().unwrap();
                let error = execution.result.as_ref().unwrap_err();
                let ExecutionError::ProjectFunction { error: failure, .. } = &**error else {
                    panic!("original live failure: {error:?}")
                };
                let FailureKind::CallFailed(payload) = &failure.kind else {
                    panic!("opaque original failure")
                };
                assert!(Rc::ptr_eq(&payload.original, &activation.original));
                assert_eq!(payload.interrupt, matches!(mode, Mode::LiveInterrupt));
            }
        }
        assert!(!study.0.join("generated").exists());
    }
}

#[test]
fn native_ingestion_verification_codec_report_and_retention_gates_keep_distinct_prefixes() {
    for mode in 0..6 {
        let study = Study::new();
        study.rounded();
        match mode {
            0 => study.write("raw/raw.csv", b"ID,VALUE\n1,not-a-float\n"),
            1 => study.write(
                "producer/layers/base.yaml",
                format!("{PRODUCER}verifications:\n  - row_count: {{min: 99}}\n"),
            ),
            _ => {}
        }
        let mut build = study.build(Language::Python);
        let mut activation = Activation::new(&study, Mode::Pass);
        let mut report = Report {
            fail: mode == 3,
            ..Default::default()
        };
        let mut limits = Limits::default();
        if mode == 2 {
            limits.engine.output_bytes = 5;
        }
        if mode == 4 {
            limits.evidence_aliases = 1;
            activation.mode = Mode::LiveFailure;
        }
        if mode == 5 {
            limits.engine.source_cells = 7;
        }
        let attempt = build.build(&mut activation, &mut report, limits);
        assert!(!attempt.accepted());
        assert!(!study.0.join("generated").exists());
        if mode == 5 {
            assert_eq!(attempt.graph.nodes.len(), 2);
            assert!(matches!(
                attempt.graph.nodes[1].dataset.result,
                Err(PortError::Run(_))
            ));
            assert_eq!(artifact(&attempt, 0), b"ID,VALUE\n1,1.23\n2,2.35\n");
        } else {
            assert_eq!(attempt.graph.nodes.len(), 1);
        }
        match mode {
            0 | 1 => {
                assert!(attempt.graph.nodes[0]
                    .dataset
                    .result
                    .as_ref()
                    .map_or(true, |e| e.result.is_err()));
                assert!(report.entered.is_empty());
            }
            2 => assert!(matches!(
                attempt.graph.nodes[0].output,
                Err(CompleteError::Encode(CodecError::Csv(_)))
            )),
            3 => assert!(matches!(
                attempt.graph.nodes[0].output,
                Err(CompleteError::Report("original report refusal"))
            )),
            4 => {
                let failure = attempt.resources.as_ref().unwrap_err();
                assert_eq!(failure.retained.len(), 1);
                assert!(attempt.graph.nodes[0]
                    .dataset
                    .result
                    .as_ref()
                    .unwrap()
                    .result
                    .is_err());
            }
            _ => {}
        }
    }
}

#[cfg(unix)]
#[test]
fn native_build_preserves_canonical_unix_root_with_literal_backslashes() {
    let outer = Study::new();
    let path = outer.0.join(r"native\root");
    fs::create_dir(&path).unwrap();
    let study = Study(fs::canonicalize(path).unwrap());
    study.rounded();
    let mut build = study.build(Language::Python);
    let attempt = build.build(
        &mut Activation::new(&study, Mode::Pass),
        &mut Report::default(),
        Limits::default(),
    );
    assert!(attempt.accepted());
    assert!(attempt
        .provenance
        .metadata()
        .nodes()
        .iter()
        .all(|n| n.identity().starts_with(&study.path(""))));
    assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
    assert!(!study.0.join("generated").exists());
}
#[test]
fn final_report_rendering_refusal_prevents_retaining_or_consuming_producer_bytes() {
    let study = Study::new();
    study.rounded();
    let mut build = study.build(Language::Python);
    let mut activation = Activation::new(&study, Mode::Pass);
    let mut report = Report {
        final_failure: true,
        ..Default::default()
    };
    let attempt = build.build(&mut activation, &mut report, Limits::default());
    assert!(!attempt.accepted());
    assert_eq!(attempt.graph.nodes.len(), 1);
    assert!(attempt.graph.nodes[0]
        .dataset
        .result
        .as_ref()
        .unwrap()
        .result
        .is_ok());
    assert!(matches!(
        attempt.graph.nodes[0].output,
        Err(CompleteError::Report("original report rendering refusal"))
    ));
    assert_eq!(report.entered.len(), 1);
    assert!(!study.0.join("generated").exists());
}

fn report_id() -> yamaa_adapters::specification_report::Identity<'static> {
    yamaa_adapters::specification_report::Identity {
        runtime: "python",
        runtime_version: "fixture-host",
        engine_version: "fixture-engine",
        example: "independent-rounded-producer",
        specification: "unused-entry-alias",
        base_directory: "unused-caller-base",
    }
}
fn truth_table(
    specification: &str,
    stage: &str,
    name: &str,
    columns: &[&str],
    rows: &[&[&str]],
) -> serde_json::Value {
    use serde_json::json;
    let kinds = columns
        .iter()
        .map(|&column| if column == "ID" { "int" } else { "float" })
        .collect::<Vec<_>>();
    json!({"specification":specification,"stage":stage,"name":name,"columns":columns,"types":kinds,
        "rows":rows.iter().map(|row| row.iter().zip(&kinds).map(|(value, kind)| json!({"type":kind,"value":value})).collect::<Vec<_>>()).collect::<Vec<_>>()})
}
#[test]
fn native_complete_reports_pin_independent_rounded_truth_and_union_after_all_owners_drop() {
    use serde_json::json;
    use yamaa_adapters::producer_report::attempt_report;
    let study = Study::new();
    study.rounded();
    let mut build = study.build(Language::Python);
    let mut host = Activation::new(&study, Mode::Pass);
    let first = build.build_reported(&mut host, report_id(), Limits::default());
    let second = build.build_reported(&mut host, report_id(), Limits::default());
    assert!(first.accepted() && second.accepted());
    assert_eq!(host.trace.len(), 12);
    let producer = study.path("producer/p.yaml");
    let consumer = study.path("consumer/root.yaml");
    let raw_rows: &[&[&str]] = &[&["1", "3ff3be76c8b43958"], &["2", "4002c28f5c28f5c3"]];
    let rounded_rows: &[&[&str]] = &[&["1", "3ff3ae147ae147ae"], &["2", "4002cccccccccccd"]];
    let derived_rows: &[&[&str]] = &[
        &[
            "1",
            "3ff3ae147ae147ae",
            "3ff3ae147ae147ae",
            "4003ae147ae147ae",
        ],
        &[
            "2",
            "4002cccccccccccd",
            "4002cccccccccccd",
            "4012cccccccccccd",
        ],
    ];
    let tables = json!([
        truth_table(&producer, "source", "RAW", &["ID", "VALUE"], raw_rows),
        truth_table(&producer, "derived", "output", &["ID", "VALUE"], raw_rows),
        truth_table(&consumer, "source", "FIRST", &["ID", "VALUE"], rounded_rows),
        truth_table(
            &consumer,
            "source",
            "SECOND",
            &["ID", "VALUE"],
            rounded_rows
        ),
        truth_table(
            &consumer,
            "derived",
            "output",
            &["ID", "FIRST_VALUE", "SECOND_VALUE", "VALUE"],
            derived_rows
        ),
    ]);
    let activation = json!({"lock":"verified","bindings":[{"function":"id_float","call":"program.id_float","source":"held/environment.yaml","outcome":"bound"}],
    "tests":[
        {"function":"id_float","call":"program.id_float","source":"held/environment.yaml","case":"normal","args":{"x":{"float":"401c000000000000"}},"expected":{"float":"401c000000000000"},"comparison_decimals":4,"outcome":"passed","invoked":true,"actual_retained":true,"actual":{"float":"401c000000000000"}},
        {"function":"id_float","call":"program.id_float","source":"held/environment.yaml","case":"boundary","args":{"x":{"float":"0000000000000000"}},"expected":{"float":"0000000000000000"},"comparison_decimals":4,"outcome":"passed","invoked":true,"actual_retained":true,"actual":{"float":"0000000000000000"}},
        {"function":"id_float","call":"program.id_float","source":"held/environment.yaml","case":"missing","args":{"x":{"missing":null}},"expected":{"missing":null},"comparison_decimals":4,"outcome":"passed","invoked":false,"actual_retained":true,"actual":{"missing":null}}
    ]});
    drop(build);
    drop(host);
    fs::remove_dir_all(&study.0).unwrap();
    for (attempt, created) in [(&first, 1), (&second, 0)] {
        let observed = attempt_report(attempt, report_id(), 16_777_216).unwrap();
        let expected = json!({"report_version":"0.3.0-draft","runtime":"python","backend":"rust","runtime_version":"fixture-host","engine_version":"fixture-engine","example":"independent-rounded-producer","entry":"consumer/root.yaml","outcome":"success","activation":activation,"environment":{"source":"held/environment.yaml","lock_source":"held/lock"},
            "nodes":[{"specification":producer,"outcome":"success","diagnostics":[],"unsupported":[],"handler_counts":[]},{"specification":consumer,"outcome":"success","diagnostics":[],"unsupported":[],"handler_counts":[]}],
            "artifacts":[
                {"name":"producer","profile":"csv","columns":["ID","VALUE"],"types":["int","float"],"row_count":2,"records":["ID,VALUE","1,1.23","2,2.35"],"byte_length":23,"content":"ID,VALUE\n1,1.23\n2,2.35\n","specification":producer},
                {"name":"root","profile":"csv","columns":["ID","VALUE"],"types":["int","float"],"row_count":2,"records":["ID,VALUE","1,2.46","2,4.70"],"byte_length":23,"content":"ID,VALUE\n1,2.46\n2,4.70\n","specification":consumer}],
            "tables":tables,"diagnostics":[],"unsupported":[],"handler_counts":[],"verifications":[],"callbacks":[],"error":null,
            "source_reads":[{"base_directory":study.path("producer"),"path":"../raw/raw.csv","outcome":"captured","condition":null,"snapshots_created":created,"specification":producer},
                {"base_directory":study.path("consumer"),"path":"../generated/producer.csv","outcome":"captured","condition":null,"snapshots_created":0,"specification":consumer},
                {"base_directory":study.path("consumer"),"path":"../generated/producer.csv","outcome":"captured","condition":null,"snapshots_created":0,"specification":consumer}]});
        assert_eq!(observed, expected);
        assert_eq!(
            attempt_report(attempt, report_id(), 16_777_216).unwrap(),
            expected
        );
        let bytes = attempt.graph.nodes[1]
            .output
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .artifact()
            .unwrap()
            .bytes();
        assert_eq!(bytes, b"ID,VALUE\n1,2.46\n2,4.70\n");
    }
}

#[test]
fn complete_failed_reports_keep_entered_producer_and_consumer_check_and_source_prefixes() {
    use serde_json::json;
    use yamaa_adapters::producer_report::attempt_report;
    for mode in 0..4 {
        let study = Study::new();
        study.rounded();
        match mode {
            0 => study.write(
                "producer/layers/base.yaml",
                format!("{PRODUCER}verifications:\n  - row_count: {{min: 99}}\n"),
            ),
            1 => study.write(
                "consumer/root.yaml",
                format!("{CONSUMER}verifications:\n  - row_count: {{min: 99}}\n"),
            ),
            2 => fs::remove_file(study.0.join("raw/raw.csv")).unwrap(),
            3 => study.write("raw/raw.csv", b"ID,VALUE\n1,bad\n"),
            _ => unreachable!(),
        }
        let mut build = study.build(Language::Python);
        let mut activation = Activation::new(&study, Mode::Pass);
        let attempt = build.build_reported(&mut activation, report_id(), Limits::default());
        assert!(!attempt.accepted());
        let before = activation.trace.clone();
        drop(build);
        fs::remove_dir_all(&study.0).unwrap();
        let value = attempt_report(&attempt, report_id(), 16_777_216).unwrap();
        assert_eq!(activation.trace, before);
        assert_eq!(value["outcome"], "failure");
        assert_eq!(
            value["nodes"].as_array().unwrap().len(),
            if mode == 1 { 2 } else { 1 }
        );
        assert_eq!(
            value["artifacts"].as_array().unwrap().len(),
            if mode == 1 { 1 } else { 0 }
        );
        assert_eq!(value["diagnostics"].as_array().unwrap().len(), 1);
        let finding = &value["diagnostics"][0];
        match mode {
            0 | 1 => {
                assert_eq!(finding["condition"], "row_count_failed");
                assert_eq!(finding["spec_paths"], json!(["verifications[0].row_count"]));
                assert_eq!(finding["context"]["count"], 2);
                assert_eq!(value["verifications"].as_array().unwrap().len(), 1);
            }
            2 => {
                assert_eq!(finding["condition"], "resource_path_missing");
                assert_eq!(finding["requirement"], "REQ-0785");
                assert_eq!(finding["context"]["path"], "../../raw/raw.csv");
                assert!(value["source_reads"].as_array().unwrap().is_empty());
            }
            3 => {
                assert_eq!(finding["condition"], "field_parse_failed");
                assert_eq!(finding["requirement"], "REQ-0536");
                assert_eq!(finding["context"]["value"], "bad");
                assert_eq!(value["source_reads"][0]["outcome"], "captured");
            }
            _ => unreachable!(),
        }
        assert_eq!(
            attempt_report(&attempt, report_id(), 16_777_216).unwrap(),
            value
        );
    }
}

#[test]
fn complete_projection_refusal_keeps_original_opaque_interrupt_unwind_and_actual_prefix() {
    use yamaa_adapters::producer_report::{attempt_report, ProjectionError};
    for mode in [
        Mode::Lock,
        Mode::CaseInterrupt,
        Mode::LiveFailure,
        Mode::LiveInterrupt,
        Mode::Unwind,
    ] {
        let study = Study::new();
        study.rounded();
        let mut build = study.build(Language::Python);
        let mut activation = Activation::new(&study, mode);
        let attempt = build.build_reported(&mut activation, report_id(), Limits::default());
        let original = Rc::clone(&activation.original);
        let unwind = Arc::clone(&activation.unwind);
        let before = activation.trace.clone();
        drop(build);
        fs::remove_dir_all(&study.0).unwrap();
        let Err(ProjectionError::Original {
            attempt: held,
            observations,
        }) = attempt_report(&attempt, report_id(), 16_777_216)
        else {
            panic!("original failure must accompany projection refusal");
        };
        assert!(std::ptr::eq(held, &attempt));
        assert_eq!(activation.trace, before);
        assert_eq!(observations["outcome"], "failure");
        assert!(observations["artifacts"].as_array().unwrap().is_empty());
        assert_eq!(
            observations["nodes"].as_array().unwrap().len(),
            attempt.graph.nodes.len()
        );
        match mode {
            Mode::Lock => assert!(
                matches!(&held.graph.outcome, Err(Failure::Activation(ActivationFailure::Lock(e))) if Rc::ptr_eq(&original, &e.original))
            ),
            Mode::Unwind => assert!(
                matches!(&held.boundary, Err(BoundaryFailure::Unwind(payload)) if Arc::ptr_eq(payload.downcast_ref::<Arc<()>>().unwrap(), &unwind))
            ),
            Mode::CaseInterrupt => {
                assert_eq!(
                    observations["activation"]["tests"]
                        .as_array()
                        .unwrap()
                        .len(),
                    2
                );
                assert_eq!(
                    observations["activation"]["tests"][1]["outcome"],
                    "interrupted"
                );
            }
            _ => {
                let error = held.graph.nodes[0]
                    .dataset
                    .result
                    .as_ref()
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap_err();
                assert!(
                    matches!(error.as_ref(), ExecutionError::ProjectFunction { error, .. } if matches!(&error.kind, FailureKind::CallFailed(e) if Rc::ptr_eq(&original, &e.original)))
                );
                assert_eq!(observations["source_reads"].as_array().unwrap().len(), 1);
                assert_eq!(observations["tables"].as_array().unwrap().len(), 1);
            }
        }
    }
}

#[test]
fn native_report_admission_stops_dependents_and_whole_report_limits_keep_earlier_artifacts() {
    use yamaa_adapters::{
        producer_report::{attempt_report, ProjectionError},
        specification_report::Error,
    };
    let study = Study::new();
    study.rounded();
    study.write(
        "consumer/root.yaml",
        format!("{CONSUMER}verifications:\n  - row_count: {{min: 1}}\n"),
    );
    let mut build = study.build(Language::Python);
    let mut activation = Activation::new(&study, Mode::Pass);
    let mut limits = Limits::default();
    limits.engine.report_bytes = 1;
    let first = build.build_reported(&mut activation, report_id(), limits);
    assert!(!first.accepted());
    assert_eq!(first.graph.nodes.len(), 1);
    assert!(matches!(
        first.graph.nodes[0].output,
        Err(CompleteError::Report(Error::OutputLimit))
    ));
    assert_eq!(activation.trace.len(), 6);
    assert_eq!(
        attempt_report(&first, report_id(), 16_777_216).unwrap()["error"]["code"],
        "output_limit"
    );
    limits.engine.report_bytes = 65536;
    let later = build.build_reported(&mut activation, report_id(), limits);
    assert!(!later.accepted());
    assert_eq!(later.graph.nodes.len(), 2);
    assert!(later.graph.nodes[0]
        .output
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .artifact()
        .is_some());
    assert!(matches!(
        later.graph.nodes[1].output,
        Err(CompleteError::Report(Error::OutputLimit))
    ));
    let value = attempt_report(&later, report_id(), 16_777_216).unwrap();
    assert_eq!(value["outcome"], "failure");
    assert_eq!(value["artifacts"].as_array().unwrap().len(), 1);
    let original = later.graph.nodes[0]
        .output
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .artifact()
        .unwrap();
    assert_eq!(original.bytes(), b"ID,VALUE\n1,1.23\n2,2.35\n");
    assert!(matches!(
        attempt_report(&later, report_id(), 1),
        Err(ProjectionError::Report(Error::OutputLimit))
    ));
    assert_eq!(original.bytes(), b"ID,VALUE\n1,1.23\n2,2.35\n");
    assert!(!study.0.join("generated").exists());
}

#[test]
fn report_context_pointer_and_table_count_admission_precede_table_authority_and_clear_old_selection(
) {
    use yamaa_adapters::{producer_report::NativeReport, specification_report::Error};
    use yamaa_engine::specification_run::{CapturedAttempt, CapturedSource};
    struct Bomb {
        schema: yamaa_core::table::TableSchema,
        reads: Rc<std::cell::Cell<usize>>,
    }
    impl TableAccess for Bomb {
        type Error = Payload;
        fn schema(&self) -> &yamaa_core::table::TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            usize::MAX
        }
        fn cell(
            &self,
            _: usize,
            _: usize,
        ) -> Result<ValueRef<'_>, yamaa_core::table::CellError<Payload>> {
            self.reads.set(self.reads.get() + 1);
            panic!("count admission must precede cell authority")
        }
    }
    let study = Study::new();
    study.rounded();
    let mut build = study.build(Language::Python);
    let unrelated = study.build(Language::Python);
    let mut attempt = build.build_reported(
        &mut Activation::new(&study, Mode::Pass),
        report_id(),
        Limits::default(),
    );
    assert!(attempt.accepted());
    let mut report = NativeReport::new(build.provenance(), report_id());
    let node = &build.provenance().metadata().nodes()[1];
    report
        .select(node, &attempt.graph.nodes[0].dataset, 16_777_216)
        .unwrap();
    assert!(matches!(
        report.select(
            &unrelated.provenance().metadata().nodes()[1],
            &attempt.graph.nodes[0].dataset,
            16_777_216
        ),
        Err(Error::InvalidObservation)
    ));
    assert!(matches!(report.failure(), Err(Error::InvalidObservation)));
    let mut entered = attempt.graph.nodes.remove(0);
    let source = entered.dataset.sources.remove(0);
    let reads = Rc::new(std::cell::Cell::new(0));
    let fake = CapturedAttempt::<(), (), Bomb> {
        sources: vec![CapturedSource {
            read: source.read,
            snapshot: source.snapshot,
            table: Some(Bomb {
                schema: source.table.unwrap().schema().clone(),
                reads: Rc::clone(&reads),
            }),
        }],
        result: Ok(entered.dataset.result.map_err(|_| ()).unwrap()),
    };
    assert!(matches!(
        report.select(node, &fake, 16_777_216),
        Err(Error::OutputLimit)
    ));
    assert_eq!(reads.get(), 0);
    assert!(matches!(report.failure(), Err(Error::InvalidObservation)));
}

#[test]
fn whole_report_rejects_contradictory_actual_activation_and_node_order_without_replay() {
    use yamaa_adapters::{
        producer_report::{attempt_report, ProjectionError},
        specification_report::Error,
    };
    let study = Study::new();
    study.rounded();
    let mut build = study.build(Language::Python);
    let mut host = Activation::new(&study, Mode::Pass);
    let mut attempt = build.build_reported(&mut host, report_id(), Limits::default());
    let before = host.trace.clone();
    attempt.graph.activation.tests[0].actual = Some(Value::float(99.0));
    assert!(matches!(
        attempt_report(&attempt, report_id(), 16_777_216),
        Err(ProjectionError::Report(Error::InvalidObservation))
    ));
    attempt.graph.activation.tests[0].actual = Some(Value::float(7.0));
    attempt.graph.nodes.swap(0, 1);
    assert!(matches!(
        attempt_report(&attempt, report_id(), 16_777_216),
        Err(ProjectionError::Report(Error::InvalidObservation))
    ));
    assert_eq!(host.trace, before);
}
