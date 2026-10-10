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
    CaseMismatch,
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
            (Mode::CaseMismatch, 7) => Ok(Value::Int(8)),
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

fn report_identity() -> yamaa_adapters::specification_report::Identity<'static> {
    yamaa_adapters::specification_report::Identity {
        runtime: "test",
        runtime_version: "test-1",
        engine_version: "0.1.0",
        example: "owned-project",
        specification: "study/domain.yaml",
        base_directory: "study",
    }
}
#[derive(Default)]
struct Publication {
    calls: Vec<(String, Vec<u8>)>,
    reject: bool,
}
impl yamaa_engine::specification_output::ArtifactPort for Publication {
    type Error = Rc<()>;
    fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        self.calls.push((path.into(), bytes.into()));
        if self.reject {
            Err(Rc::new(()))
        } else {
            Ok(())
        }
    }
}

#[test]
fn complete_project_output_and_save_borrow_the_exact_attempt_without_reactivation() {
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    let (mut activation, mut study) = ports();
    let attempt = run.execute_with_ports(&mut activation, &mut study);
    let trace = activation.trace.borrow().clone();
    let snapshot = attempt.dataset.sources[0]
        .snapshot
        .as_ref()
        .unwrap()
        .as_ptr();
    let result =
        yamaa_adapters::project_report::build_result(&run, &attempt, report_identity(), &[])
            .unwrap();
    let report = result.observations();
    assert_eq!(report["outcome"], "success");
    assert_eq!(report["artifacts"], serde_json::json!([]));
    assert_eq!(report["diagnostics"], serde_json::json!([]));
    assert_eq!(
        report["source_reads"],
        serde_json::json!([{"base_directory":"study","path":"input.csv","outcome":"captured","condition":null,"snapshots_created":1}])
    );
    assert_eq!(
        report["tables"],
        serde_json::json!([
            {"specification":"study/domain.yaml","stage":"source","name":"SRC","columns":["ID"],"types":["int"],"rows":[[{"type":"int","value":"1"}]]},
            {"specification":"study/domain.yaml","stage":"derived","name":"output","columns":["ID","VALUE"],"types":["int","int"],"rows":[[{"type":"int","value":"1"},{"type":"int","value":"1"}]]}
        ])
    );
    assert_eq!(report["activation"]["lock"], "verified");
    assert_eq!(report["activation"]["tests"].as_array().unwrap().len(), 3);
    assert_eq!(
        report["activation"]["tests"][2]["actual"],
        serde_json::json!({"missing":null})
    );
    assert!(result.output().is_some());
    assert!(result.issues().is_empty());
    let mut publisher = Publication {
        reject: true,
        ..Default::default()
    };
    assert!(matches!(
        result.save(&mut publisher),
        Err(yamaa_engine::specification_output::SaveError::Publish(_))
    ));
    publisher.reject = false;
    let saved = result.save(&mut publisher).unwrap();
    assert_eq!(
        publisher.calls,
        [
            ("output.csv".into(), b"ID,VALUE\n1,1\n".to_vec()),
            ("output.csv".into(), b"ID,VALUE\n1,1\n".to_vec())
        ]
    );
    assert_eq!(saved["artifacts"][0]["content"], "ID,VALUE\n1,1\n");
    assert_eq!(saved["artifacts"][0]["row_count"], 1);
    assert_eq!(result.observations(), report);
    assert_eq!(*activation.trace.borrow(), trace);
    assert_eq!(study.capture_reads(), 1);
    assert_eq!(
        attempt.dataset.sources[0]
            .snapshot
            .as_ref()
            .unwrap()
            .as_ptr(),
        snapshot
    );
}

#[test]
fn project_reports_keep_opaque_failures_and_interrupts_original_and_failed_saves_inert() {
    use yamaa_adapters::{
        project_function_diagnostics::HostDetails,
        project_report::{Classified, Error},
    };
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    for mode in [
        Mode::BindFailure,
        Mode::CaseFailure,
        Mode::LiveFailure,
        Mode::CaseInterrupt,
        Mode::CaseUnwind,
    ] {
        let (mut activation, mut study) = ports();
        activation.mode = mode;
        let attempt = run.execute_with_ports(&mut activation, &mut study);
        let trace = activation.trace.borrow().clone();
        let unclassified =
            yamaa_adapters::project_report::build_result(&run, &attempt, report_identity(), &[]);
        if matches!(mode, Mode::CaseInterrupt | Mode::CaseUnwind) {
            let Error::Boundary(held) = unclassified.unwrap_err() else {
                panic!("original boundary")
            };
            assert!(std::ptr::eq(held, attempt.boundary.as_ref().unwrap_err()));
        } else {
            let Error::Opaque(held) = unclassified.unwrap_err() else {
                panic!("unclassified original payload")
            };
            assert!(Rc::ptr_eq(&held.original, &activation.original));
            let facts = [Classified::Host {
                failure: held,
                details: HostDetails::Exception {
                    class: "OriginalFailure",
                    message: "é failure",
                    truncated: false,
                },
            }];
            let result = yamaa_adapters::project_report::build_result(
                &run,
                &attempt,
                report_identity(),
                &facts,
            )
            .unwrap();
            let observed = result.observations();
            assert_eq!(observed["outcome"], "failure");
            assert_eq!(observed["artifacts"], serde_json::json!([]));
            assert_eq!(result.output(), None);
            assert_eq!(result.issues().len(), 1);
            let expected = if matches!(mode, Mode::BindFailure) {
                "project_environment_invalid"
            } else if matches!(mode, Mode::CaseFailure) {
                "function_conformance_failed"
            } else {
                "function_call_failed"
            };
            assert_eq!(result.issues()[0].condition, expected);
            if matches!(mode, Mode::LiveFailure) {
                assert_eq!(observed["source_reads"].as_array().unwrap().len(), 1);
                assert_eq!(observed["activation"]["tests"].as_array().unwrap().len(), 3);
            } else {
                assert_eq!(observed["source_reads"], serde_json::json!([]));
            }
            let mut publisher = Publication::default();
            assert!(matches!(
                result.save(&mut publisher),
                Err(yamaa_engine::specification_output::SaveError::FailedBuild)
            ));
            assert!(publisher.calls.is_empty());
            assert!(Rc::ptr_eq(&held.original, &activation.original));
        }
        assert_eq!(*activation.trace.borrow(), trace);
        assert_eq!(
            study.capture_reads(),
            usize::from(matches!(mode, Mode::LiveFailure))
        );
    }
}

#[test]
fn report_classifications_cannot_target_a_different_failure_or_supply_duplicates() {
    use yamaa_adapters::{
        project_function_diagnostics::HostDetails,
        project_report::{Classified, Error},
    };
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    let (mut activation, mut study) = ports();
    activation.mode = Mode::BindFailure;
    let attempt = run.execute_with_ports(&mut activation, &mut study);
    let Err(BoundaryFailure::Activation(Failure::Bindings(failures))) = &attempt.boundary else {
        panic!("binding failure")
    };
    let original = &failures[0].error;
    let other = Payload {
        original: Rc::clone(&original.original),
        interrupt: false,
    };
    let details = HostDetails::Exception {
        class: "Failure",
        message: "same text",
        truncated: false,
    };
    for facts in [
        vec![Classified::Host {
            failure: &other,
            details,
        }],
        vec![
            Classified::Host {
                failure: original,
                details,
            },
            Classified::Host {
                failure: original,
                details,
            },
        ],
    ] {
        assert!(matches!(
            yamaa_adapters::project_report::build_result(&run, &attempt, report_identity(), &facts),
            Err(Error::Report(
                yamaa_adapters::specification_report::Error::InvalidObservation
            ))
        ));
    }
    assert_eq!(study.capture_reads(), 0);
    assert_eq!(*activation.trace.borrow(), ["lock", "bind"]);
}

#[test]
fn complete_reports_refuse_missing_or_contradictory_activation_success() {
    use yamaa_adapters::{project_report::Error, specification_report};
    use yamaa_engine::project_activation::BindingOutcome;
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    for change in 0..5 {
        let (mut activation, mut study) = ports();
        let mut attempt = run.execute_with_ports(&mut activation, &mut study);
        let trace = activation.trace.borrow().clone();
        match change {
            0 => {
                attempt.activation.tests.pop();
            }
            1 => attempt.activation.tests[0].actual = None,
            2 => attempt.activation.tests[0].actual = Some(Value::Int(99)),
            3 => attempt.activation.lock = LockObservation::Rejected,
            _ => attempt.activation.bindings[0].outcome = BindingOutcome::Rejected,
        }
        assert!(matches!(
            yamaa_adapters::project_report::build_result(&run, &attempt, report_identity(), &[]),
            Err(Error::Report(
                specification_report::Error::InvalidObservation
            ))
        ));
        assert_eq!(*activation.trace.borrow(), trace);
        assert_eq!(study.capture_reads(), 1);
        assert!(attempt.dataset.result.as_ref().unwrap().result.is_ok());
    }
}

#[test]
fn lock_and_actual_conformance_mismatches_keep_complete_zero_read_reports() {
    use yamaa_adapters::{
        project_lock::{Finding, Reason},
        project_report::{Classified, Error},
    };
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    let (mut activation, mut study) = ports();
    activation.mode = Mode::LockFailure;
    let attempt = run.execute_with_ports(&mut activation, &mut study);
    let Err(BoundaryFailure::Activation(Failure::Lock(original))) = &attempt.boundary else {
        panic!("original lock payload")
    };
    let findings = [Finding {
        package: "project".into(),
        reason: Reason::DistributionNotIdentified,
        expected: vec![],
        actual: None,
    }];
    let result = yamaa_adapters::project_report::build_result(
        &run,
        &attempt,
        report_identity(),
        &[Classified::Lock {
            failure: original,
            findings: &findings,
        }],
    )
    .unwrap();
    assert_eq!(result.issues()[0].condition, "project_environment_invalid");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result.issues()[0].context).unwrap(),
        serde_json::json!({"source":"study/environment.yaml","entry":"study/environment.yaml","lock_source":"held/uv.lock","lock":"../uv.lock","package":"project","reason":"distribution_not_identified","expected":[],"actual":null})
    );
    assert_eq!(result.observations()["source_reads"], serde_json::json!([]));
    assert_eq!(result.observations()["activation"]["lock"], "rejected");
    assert_eq!(*activation.trace.borrow(), ["lock"]);
    assert_eq!(study.capture_reads(), 0);
    assert!(Rc::ptr_eq(&original.original, &activation.original));
    assert!(matches!(
        yamaa_adapters::project_report::build_result(
            &run,
            &attempt,
            report_identity(),
            &[Classified::Lock {
                failure: original,
                findings: &[]
            }]
        ),
        Err(Error::Report(_))
    ));
    let (mut activation, mut study) = ports();
    activation.mode = Mode::CaseMismatch;
    let attempt = run.execute_with_ports(&mut activation, &mut study);
    let result =
        yamaa_adapters::project_report::build_result(&run, &attempt, report_identity(), &[])
            .unwrap();
    assert_eq!(result.issues()[0].condition, "function_conformance_failed");
    assert_eq!(result.issues()[0].spec_paths, ["functions.id.tests[0]"]);
    let context: serde_json::Value = serde_json::from_str(&result.issues()[0].context).unwrap();
    assert_eq!(context["expected"], 7);
    assert_eq!(context["actual"], 8);
    assert_eq!(
        result.observations()["activation"]["tests"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        result.observations()["activation"]["tests"][1]["outcome"],
        "passed"
    );
    assert_eq!(result.output(), None);
    let mut publisher = Publication::default();
    assert!(matches!(
        result.save(&mut publisher),
        Err(yamaa_engine::specification_output::SaveError::FailedBuild)
    ));
    assert!(publisher.calls.is_empty());
    assert_eq!(study.capture_reads(), 0);
    assert_eq!(*activation.trace.borrow(), ["lock", "bind", "case", "case"]);
}

#[test]
fn zero_sized_host_failures_remain_opaque_and_never_accept_address_classification() {
    use yamaa_adapters::{
        project_function_diagnostics::HostDetails,
        project_report::{self, Classified, Error},
        specification_report,
    };
    struct EmptyFailure;
    struct RejectLock;
    impl ActivationPort for RejectLock {
        type Handle = ();
        type Error = EmptyFailure;
        fn verify_lock(
            &mut self,
            _: Language,
            _: &LockReference,
            _: &[ProjectFunctionIdentity],
        ) -> Result<(), EmptyFailure> {
            Err(EmptyFailure)
        }
        fn bind(
            &mut self,
            _: &ProjectFunctionIdentity,
            _: &LogicalSignature,
        ) -> Result<(), EmptyFailure> {
            panic!("rejected lock grants no binding authority")
        }
        fn is_interrupt(&self, _: &EmptyFailure) -> bool {
            false
        }
        fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<EmptyFailure>> {
            panic!("rejected lock grants no invocation authority")
        }
    }
    let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
    let (_, mut study) = ports();
    let attempt = run.execute_with_ports(&mut RejectLock, &mut study);
    let Err(BoundaryFailure::Activation(Failure::Lock(original))) = &attempt.boundary else {
        panic!("held original zero-sized lock failure")
    };
    assert!(matches!(
        project_report::build_result(&run, &attempt, report_identity(), &[]),
        Err(Error::Opaque(_))
    ));
    let foreign = EmptyFailure;
    for facts in [
        vec![Classified::Lock {
            failure: original,
            findings: &[],
        }],
        vec![Classified::Lock {
            failure: &foreign,
            findings: &[],
        }],
        vec![Classified::Host {
            failure: original,
            details: HostDetails::Exception {
                class: "Error",
                message: "foreign facts",
                truncated: false,
            },
        }],
    ] {
        assert!(matches!(
            project_report::build_result(&run, &attempt, report_identity(), &facts),
            Err(Error::Report(
                specification_report::Error::InvalidObservation
            ))
        ));
    }
    assert!(matches!(
        attempt.boundary,
        Err(BoundaryFailure::Activation(Failure::Lock(_)))
    ));
    assert_eq!(study.capture_reads(), 0);
}
