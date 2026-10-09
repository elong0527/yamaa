#[path = "support/project_schemas.rs"]
mod schemas;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use yamaa_adapters::{
    project_run::{BoundaryFailure, PreparedRun},
    project_source::{self, CapturePort, Kind, OwnedEnvironment, Reply, Request},
    specification_source::{PreparedDocument, Source},
};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{LockKind, LockReference},
    project_function::Language,
    specification::SourceDeclaration,
    table::{TableAccess, ValueRef},
    value::Value,
};
use yamaa_engine::{
    dataset::ExecutionError,
    function_invocation::{Argument, FailureKind, HostError},
    project_activation::{ActivationPort, Failure, LockObservation, TestOutcome},
    specification_run::{PortError, SourcePort},
};

fn source(identity: &str, bytes: &[u8]) -> Source {
    Source {
        identity: identity.into(),
        bytes: bytes.to_vec(),
    }
}
struct EnvironmentCapture;
impl CapturePort for EnvironmentCapture {
    type Error = std::convert::Infallible;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
        assert_eq!(request.kind, Kind::Lock);
        assert_eq!(request.written, "../uv.lock");
        Ok(Reply::Lock {
            source: source("held/uv.lock", b"version = 1\n"),
            kind: LockKind::Uv,
        })
    }
}
fn environment() -> OwnedEnvironment {
    let schema = schemas::environment_schema();
    project_source::prepare(
        schema,
        source(
            "study/environment.yaml",
            br#"schema_version: '1.0'
language: python
lock: ../uv.lock
functions:
  unused:
    function: project.unused
    description: Uncalled definition.
    params: []
    returns: int
    tests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]
  id:
    function: project.id
    description: Integer identity.
    params: [{name: x, type: int}]
    returns: int
    tests:
      - {id: ordinary, covers: [normal], args: {x: 7}, result: 7}
      - {id: boundary, covers: [boundary], args: {x: -9223372036854775808}, result: -9223372036854775808}
      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}
"#,
        ),
        Language::Python,
        &mut EnvironmentCapture,
        Default::default(),
    )
    .unwrap()
    .into_owned()
}
fn document(function: Option<&str>) -> PreparedDocument {
    let schema = schemas::domain_schema();
    let derivation = function.map_or_else(
        || "SRC.ID".into(),
        |name| format!("{{function: {{name: {name}, args: {{x: SRC.ID}}}}}}"),
    );
    let bytes = format!(
        "schema_version: '1.0'\ndomain: TEST\ninput: {{SRC: {{path: input.csv, types: {{ID: int}}}}}}\nkeys: [ID]\ncolumns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n  - {{name: VALUE, type: int, derivation: {derivation}}}\noutput: {{path: output.csv, columns: [ID, VALUE]}}\n"
    );
    schema
        .prepare_standalone(source("study/domain.yaml", bytes.as_bytes()))
        .unwrap()
}

#[derive(Debug, PartialEq)]
struct Payload {
    original: Rc<()>,
    interrupt: bool,
}
type Trace = Rc<RefCell<Vec<&'static str>>>;
#[derive(Clone, Copy)]
enum Mode {
    Pass,
    LockFailure,
    BindFailure,
    CaseFailure,
    CaseInterrupt,
    CaseUnwind,
    LiveFailure,
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
    type Handle = ();
    type Error = Payload;
    fn verify_lock(
        &mut self,
        language: Language,
        lock: &LockReference,
        functions: &[ProjectFunctionIdentity],
    ) -> Result<(), Payload> {
        assert_eq!(language, Language::Python);
        assert_eq!(lock.written, "../uv.lock");
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "id");
        self.trace.borrow_mut().push("lock");
        if matches!(self.mode, Mode::LockFailure) {
            Err(self.payload(false))
        } else {
            Ok(())
        }
    }
    fn bind(
        &mut self,
        identity: &ProjectFunctionIdentity,
        signature: &LogicalSignature,
    ) -> Result<(), Payload> {
        assert_eq!(identity.call, "project.id");
        assert_eq!(signature.parameters()[0].name, "x");
        self.trace.borrow_mut().push("bind");
        if matches!(self.mode, Mode::BindFailure) {
            Err(self.payload(false))
        } else {
            Ok(())
        }
    }
    fn is_interrupt(&self, error: &Payload) -> bool {
        error.interrupt
    }
    fn invoke(&mut self, _: &(), args: &[Argument<'_>]) -> Result<Value, HostError<Payload>> {
        let [Argument {
            name: "x",
            value: ValueRef::Int(value),
        }] = args
        else {
            panic!("an exact integer argument; missing cases are engine-short-circuited")
        };
        self.trace
            .borrow_mut()
            .push(if *value == 1 { "live" } else { "case" });
        match (self.mode, *value) {
            (Mode::CaseFailure, i64::MIN) | (Mode::LiveFailure, 1) => {
                Err(HostError::Raised(self.payload(false)))
            }
            (Mode::CaseInterrupt, i64::MIN) => Err(HostError::Raised(self.payload(true))),
            (Mode::CaseUnwind, i64::MIN) => std::panic::panic_any(Arc::clone(&self.unwind)),
            _ => Ok(Value::Int(*value)),
        }
    }
}
struct Study {
    trace: Trace,
    bytes: Arc<[u8]>,
    captured: bool,
    inspections: usize,
}
impl SourcePort for Study {
    type Error = std::convert::Infallible;
    fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Self::Error> {
        assert_eq!(source.path, "input.csv");
        self.trace.borrow_mut().push("inspect");
        self.inspections += 1;
        Ok(())
    }
    fn capture_reads(&self) -> usize {
        usize::from(self.captured)
    }
    fn capture(&mut self, _: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, Self::Error> {
        assert!(self.bytes.len() <= maximum);
        self.trace.borrow_mut().push("capture");
        self.captured = true;
        Ok(Arc::clone(&self.bytes))
    }
}
fn ports() -> (Activation, Study) {
    let trace = Rc::new(RefCell::new(vec![]));
    (
        Activation {
            trace: Rc::clone(&trace),
            original: Rc::new(()),
            unwind: Arc::new(()),
            mode: Mode::Pass,
        },
        Study {
            trace,
            bytes: Arc::from(b"ID\n1\n".as_slice()),
            captured: false,
            inspections: 0,
        },
    )
}

#[test]
fn prepared_and_rejected_runs_retain_original_documents_lock_and_metadata_allocations() {
    for name in ["id", "unknown"] {
        let document = document(Some(name));
        let environment = environment();
        let document_bytes = document.source().bytes.as_ptr();
        let root_bytes = environment.root().source().bytes.as_ptr();
        let lock_bytes = environment.lock().unwrap().source.bytes.as_ptr();
        let definition = environment.environment().functions()[1]
            .definition()
            .tests
            .as_ptr();
        let prepared = PreparedRun::prepare(document, environment);
        let (document, captured, environment) = match &prepared {
            Ok(run) => {
                assert!(run.check_diagnostics().is_empty());
                (
                    run.document(),
                    run.captured_environment(),
                    run.environment(),
                )
            }
            Err(run) => (&run.document, &run.captured, &run.rejection.environment),
        };
        assert_eq!(prepared.is_ok(), name == "id");
        assert_eq!(document.source().bytes.as_ptr(), document_bytes);
        assert_eq!(captured.root().source().bytes.as_ptr(), root_bytes);
        assert_eq!(captured.lock().unwrap().source.bytes.as_ptr(), lock_bytes);
        assert_eq!(
            environment.functions()[1].definition().tests.as_ptr(),
            definition
        );
        assert_eq!(captured.origins().functions, ["unused", "id"]);
    }
}

#[test]
fn every_build_reactivates_only_called_definitions_on_exact_cached_study_bytes() {
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    let (mut activation, mut study) = ports();
    assert_eq!(run.compiled().called_functions(), [1]);
    assert!(activation.trace.borrow().is_empty());
    for reads in [Some(1), Some(0)] {
        let attempt = run.execute_with_ports(&mut activation, &mut study);
        assert!(attempt.boundary.is_ok());
        assert_eq!(attempt.activation.lock, LockObservation::Verified);
        assert_eq!(attempt.activation.tests.len(), 3);
        assert!(attempt
            .activation
            .tests
            .iter()
            .all(|case| case.outcome == TestOutcome::Passed));
        assert!(!attempt.activation.tests[2].invoked);
        assert_eq!(attempt.activation.tests[2].actual, Some(Value::Missing));
        let trace = activation.trace.borrow().clone();
        let observed =
            yamaa_adapters::project_activation_observations::activation(&run, &attempt.activation)
                .unwrap();
        assert_eq!(
            observed["tests"][1]["args"]["x"],
            serde_json::json!({"int":"-9223372036854775808"})
        );
        assert_eq!(
            observed["tests"][1]["actual"],
            serde_json::json!({"int":"-9223372036854775808"})
        );
        assert_eq!(
            observed["tests"][2]["actual"],
            serde_json::json!({"missing":null})
        );
        assert_eq!(observed["tests"][2]["actual_retained"], true);
        assert_eq!(observed["tests"][2]["invoked"], false);
        assert_eq!(observed["tests"][0]["source"], "study/environment.yaml");
        assert_eq!(observed["tests"][0]["function"], "id");
        assert_eq!(observed["tests"][0]["call"], "project.id");
        assert_eq!(*activation.trace.borrow(), trace);
        assert!(yamaa_adapters::project_attempt::execution(&attempt).is_some());
        let captured = &attempt.dataset.sources[0];
        assert_eq!(captured.read.snapshots_created, reads);
        assert!(Arc::ptr_eq(
            captured.snapshot.as_ref().unwrap(),
            &study.bytes
        ));
        assert_eq!(
            captured.table.as_ref().unwrap().cell(0, 0).unwrap(),
            ValueRef::Int(1)
        );
        assert_eq!(
            attempt
                .dataset
                .result
                .unwrap()
                .result
                .unwrap()
                .dataset
                .rows(),
            &[vec![Value::Int(1), Value::Int(1)]]
        );
    }
    let once = ["lock", "bind", "case", "case", "inspect", "capture", "live"];
    assert_eq!(
        &*activation.trace.borrow(),
        &once.into_iter().chain(once).collect::<Vec<_>>()
    );
    assert_eq!(study.capture_reads(), 1);
}

#[test]
fn rejected_activation_preserves_original_payloads_and_has_zero_study_authority() {
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    for mode in [
        Mode::LockFailure,
        Mode::BindFailure,
        Mode::CaseFailure,
        Mode::CaseInterrupt,
    ] {
        let (mut activation, mut study) = ports();
        activation.mode = mode;
        let attempt = run.execute_with_ports(&mut activation, &mut study);
        let Err(BoundaryFailure::Activation(error)) = &attempt.boundary else {
            panic!("activation failure")
        };
        let payload = match error {
            Failure::Lock(payload) => payload,
            Failure::Bindings(failures) => &failures[0].error,
            Failure::Tests(failures) => {
                let yamaa_engine::project_activation::TestFailure::Invocation { error, .. } =
                    &failures[0]
                else {
                    panic!("invocation failure")
                };
                let FailureKind::CallFailed(payload) = &error.kind else {
                    panic!("original case error")
                };
                payload
            }
            Failure::InterruptedInvocation { error, .. } => {
                let FailureKind::CallFailed(payload) = &error.kind else {
                    panic!("original interrupt")
                };
                payload
            }
            _ => panic!("exact ordinary or interrupt stage"),
        };
        let mut visited = 0;
        yamaa_adapters::project_attempt::visit_host_failures(&attempt, |_, original| {
            assert!(std::ptr::eq(original, payload));
            visited += 1;
        });
        assert_eq!(visited, 1);
        assert!(yamaa_adapters::project_attempt::execution(&attempt).is_none());
        assert!(Rc::ptr_eq(&payload.original, &activation.original));
        assert_eq!(payload.interrupt, matches!(mode, Mode::CaseInterrupt));
        assert!(attempt.dataset.sources.is_empty());
        assert!(matches!(attempt.dataset.result, Err(PortError::Incomplete)));
        assert_eq!(study.inspections, 0);
        assert_eq!(study.capture_reads(), 0);
        if matches!(mode, Mode::CaseFailure | Mode::CaseInterrupt) {
            assert_eq!(attempt.activation.tests[0].actual, Some(Value::Int(7)));
            assert_eq!(attempt.activation.tests[0].outcome, TestOutcome::Passed);
        }
    }
}

#[test]
fn unwound_activation_keeps_original_boundary_payload_and_completed_case_evidence() {
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    let (mut activation, mut study) = ports();
    activation.mode = Mode::CaseUnwind;
    let attempt = run.execute_with_ports(&mut activation, &mut study);
    let Err(BoundaryFailure::Unwind(payload)) = &attempt.boundary else {
        panic!("original unwind")
    };
    assert!(Arc::ptr_eq(
        payload.downcast_ref::<Arc<()>>().unwrap(),
        &activation.unwind
    ));
    assert_eq!(attempt.activation.tests[0].actual, Some(Value::Int(7)));
    assert_eq!(attempt.activation.tests[0].outcome, TestOutcome::Passed);
    assert_eq!(attempt.activation.tests[1].outcome, TestOutcome::Attempted);
    assert!(attempt.activation.tests[1].invoked);
    let observed =
        yamaa_adapters::project_activation_observations::activation(&run, &attempt.activation)
            .unwrap();
    assert_eq!(observed["tests"].as_array().unwrap().len(), 2);
    assert_eq!(
        observed["tests"][0]["actual"],
        serde_json::json!({"int":"7"})
    );
    assert_eq!(observed["tests"][1]["outcome"], "attempted");
    assert_eq!(observed["tests"][1]["actual_retained"], false);
    assert!(observed["tests"][1]["actual"].is_null());
    assert!(attempt.dataset.sources.is_empty());
    assert_eq!(study.inspections, 0);
    activation.mode = Mode::Pass;
    let next = run.execute_with_ports(&mut activation, &mut study);
    assert!(next.boundary.is_ok());
    assert_eq!(next.activation.tests.len(), 3);
    assert!(next.dataset.result.unwrap().result.is_ok());
    // The held failed attempt remains intact while a fresh attempt succeeds.
    assert_eq!(attempt.activation.tests[1].outcome, TestOutcome::Attempted);
}

#[test]
fn live_host_failure_retains_exact_study_snapshot_and_typed_activation_success() {
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    let (mut activation, mut study) = ports();
    activation.mode = Mode::LiveFailure;
    let attempt = run.execute_with_ports(&mut activation, &mut study);
    assert!(attempt.boundary.is_ok());
    assert!(attempt
        .activation
        .tests
        .iter()
        .all(|case| case.outcome == TestOutcome::Passed));
    let ExecutionError::ProjectFunction {
        path,
        error,
        identity,
    } = attempt
        .dataset
        .result
        .as_ref()
        .unwrap()
        .result
        .as_ref()
        .unwrap_err()
        .as_ref()
    else {
        panic!("retained versionless live error")
    };
    assert_eq!(path, "columns.VALUE.derivation.function");
    assert_eq!(identity.as_ref().unwrap().values, [Value::Int(1)]);
    let FailureKind::CallFailed(payload) = &error.kind else {
        panic!("original payload")
    };
    assert!(Rc::ptr_eq(&payload.original, &activation.original));
    assert!(Arc::ptr_eq(
        attempt.dataset.sources[0].snapshot.as_ref().unwrap(),
        &study.bytes
    ));
    let mut visited = 0;
    yamaa_adapters::project_attempt::visit_host_failures(&attempt, |stage, original| {
        assert_eq!(stage, "derivation");
        assert!(std::ptr::eq(original, payload));
        visited += 1;
    });
    assert_eq!(visited, 1);
    assert!(yamaa_adapters::project_attempt::execution(&attempt).is_none());
    assert_eq!(study.capture_reads(), 1);
    assert_eq!(
        &*activation.trace.borrow(),
        &["lock", "bind", "case", "case", "inspect", "capture", "live"]
    );
}

#[test]
fn unused_environment_does_not_request_any_host_activation() {
    let run = PreparedRun::prepare(document(None), environment()).unwrap();
    let (mut activation, mut study) = ports();
    activation.mode = Mode::LockFailure;
    let attempt = run.execute_with_ports(&mut activation, &mut study);
    assert!(attempt.boundary.is_ok());
    assert_eq!(attempt.activation.lock, LockObservation::NotRequested);
    assert!(attempt.activation.bindings.is_empty());
    assert!(attempt.activation.tests.is_empty());
    assert!(attempt.dataset.result.unwrap().result.is_ok());
    assert_eq!(&*activation.trace.borrow(), &["inspect", "capture"]);
}

#[test]
fn activation_projection_refuses_partial_budget_and_forged_case_order_without_effects() {
    use yamaa_adapters::project_activation_observations::{
        activation, activation_with_limit, Error,
    };
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    let (mut host, mut study) = ports();
    let mut attempt = run.execute_with_ports(&mut host, &mut study);
    let trace = host.trace.borrow().clone();
    let reads = study.capture_reads();
    assert_eq!(
        activation_with_limit(&run, &attempt.activation, 1),
        Err(Error::Limit)
    );
    assert!(activation(&run, &attempt.activation).is_ok());
    attempt.activation.tests.swap(0, 1);
    assert_eq!(
        activation(&run, &attempt.activation),
        Err(Error::InvalidObservation)
    );
    assert_eq!(*host.trace.borrow(), trace);
    assert_eq!(study.capture_reads(), reads);
}
