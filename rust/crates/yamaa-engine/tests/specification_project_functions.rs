//! Shared capture executes compiler-owned calls with already activated bindings.
#[path = "../../yamaa-core/tests/support/project_call_compiler.rs"]
mod support;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
use support::{call, function, specification, Tree};
use yamaa_core::{
    function_signature::{InvocationPlan, ProjectInvocationPlan},
    specification::{PrepareError, SourceDeclaration},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::{
    dataset::{self, ExecutionError, FunctionBindings},
    domain,
    function_invocation::{Argument, FailureKind, HostError},
    specification_run::{CapturedAttempt, Limits, SourceDecoder, SourcePort},
};
#[derive(Debug, PartialEq)]
struct Payload(Rc<()>);
type Trace = Rc<RefCell<Vec<&'static str>>>;
struct Port {
    trace: Trace,
    held: Arc<[u8]>,
    reads: usize,
    requested: bool,
}
impl SourcePort for Port {
    type Error = Payload;
    fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Payload> {
        assert_eq!(source.path, "input.csv");
        self.trace.borrow_mut().push("inspect");
        Ok(())
    }
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(&mut self, source: &SourceDeclaration, limit: usize) -> Result<Arc<[u8]>, Payload> {
        assert_eq!(source.path, "input.csv");
        assert_eq!(limit, 1024);
        self.trace.borrow_mut().push("capture");
        if !self.requested {
            self.reads += 1;
            self.requested = true;
        }
        Ok(Arc::clone(&self.held))
    }
}
struct Table {
    schema: TableSchema,
    cells: Rc<Cell<usize>>,
}
impl TableAccess for Table {
    type Error = Payload;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
        assert_eq!((row, column), (0, 0));
        self.cells.set(self.cells.get() + 1);
        Ok(ValueRef::Int(1))
    }
}
struct Decoder {
    trace: Trace,
    cells: Rc<Cell<usize>>,
}
impl SourceDecoder for Decoder {
    type Error = Payload;
    type Table = Table;
    fn decode(&mut self, source: &SourceDeclaration, bytes: &[u8]) -> Result<Table, Payload> {
        assert_eq!(source.path, "input.csv");
        assert_eq!(bytes, b"held input");
        self.trace.borrow_mut().push("decode");
        Ok(Table {
            schema: TableSchema::new(vec![Column {
                name: "ID".into(),
                kind: ColumnType::Int,
            }])
            .unwrap(),
            cells: Rc::clone(&self.cells),
        })
    }
}
struct Bindings {
    plan: Option<ProjectInvocationPlan>,
    trace: Trace,
    raised: Option<Rc<()>>,
}
impl FunctionBindings for Bindings {
    type Error = Payload;
    fn signature(&self, _: usize) -> Option<&InvocationPlan> {
        None
    }
    fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
        if slot == 0 {
            self.plan.as_ref()
        } else {
            None
        }
    }
    fn call(&mut self, slot: usize, args: &[Argument<'_>]) -> Result<Value, HostError<Payload>> {
        assert_eq!(slot, 0);
        let [Argument {
            name: "x",
            value: ValueRef::Int(value),
        }] = args
        else {
            panic!("exact compiled scalar argument")
        };
        self.trace.borrow_mut().push("call");
        if let Some(payload) = &self.raised {
            return Err(HostError::Raised(Payload(Rc::clone(payload))));
        }
        Ok(Value::Int(*value))
    }
}
fn limits() -> Limits {
    Limits {
        source_bytes: 1024,
        source_cells: 64,
        execution: dataset::Limits {
            source_rows: 4,
            output_rows: 4,
            output_cells: 16,
            key_cells: 8,
            work_cells: 64,
            scalar_text_bytes: 1024,
            output_text_bytes: 1024,
            identity_cells: 16,
            identity_text_bytes: 1024,
        },
    }
}
fn model(name: &str) -> yamaa_core::schema::SpecificationDocument {
    specification(vec![(
        "VALUE",
        "int",
        call(name, vec![("x", Tree::text("SRC.ID"))]),
    )])
}
fn ports(trace: &Trace, cells: &Rc<Cell<usize>>) -> (Port, Decoder) {
    (
        Port {
            trace: Rc::clone(trace),
            held: Arc::from(b"held input".as_slice()),
            reads: 0,
            requested: false,
        },
        Decoder {
            trace: Rc::clone(trace),
            cells: Rc::clone(cells),
        },
    )
}
#[test]
fn checked_calls_execute_through_shared_capture_and_repeat_on_held_snapshots() {
    let trace = Rc::new(RefCell::new(vec![]));
    let cells = Rc::new(Cell::new(0));
    let definition = function("id");
    let spec = model("id");
    assert!(matches!(
        domain::check(&spec),
        Err(PrepareError::Unsupported(_))
    ));
    let checked = domain::check_with_project(&spec, &[definition]).unwrap();
    assert!(trace.borrow().is_empty());
    assert!(checked.verification_declaration_diagnostics().is_empty());
    let mut bindings = Bindings {
        plan: Some(checked.compiled().project_calls().unwrap().plans()[0].clone()),
        trace: Rc::clone(&trace),
        raised: None,
    };
    let (mut port, mut decoder) = ports(&trace, &cells);
    for expected_reads in [Some(1), Some(0)] {
        let mut attempt = CapturedAttempt::new(checked.compiled().source());
        checked.build_with_functions_into(
            &mut port,
            &mut decoder,
            &mut bindings,
            limits(),
            &mut attempt,
        );
        assert_eq!(attempt.sources.len(), 1);
        assert_eq!(attempt.sources[0].read.snapshots_created, expected_reads);
        assert!(Arc::ptr_eq(
            attempt.sources[0].snapshot.as_ref().unwrap(),
            &port.held
        ));
        let execution = attempt.result.unwrap().result.unwrap();
        assert_eq!(
            execution.dataset.rows(),
            &[vec![Value::Int(1), Value::Int(1)]]
        );
    }
    assert_eq!(port.reads, 1);
    assert!(cells.get() > 0);
    assert_eq!(
        &*trace.borrow(),
        &["inspect", "capture", "decode", "call", "inspect", "capture", "decode", "call"]
    );
}
#[test]
fn unmatched_activation_signatures_fail_before_every_study_port_and_keep_static_failures_early() {
    let trace = Rc::new(RefCell::new(vec![]));
    let cells = Rc::new(Cell::new(0));
    assert!(matches!(
        domain::check_with_project(&model("unknown"), &[function("id")]),
        Err(PrepareError::Invalid(_))
    ));
    let checked = domain::check_with_project(&model("id"), &[function("id")]).unwrap();
    let (mut port, mut decoder) = ports(&trace, &cells);
    let mut bindings = Bindings {
        plan: None,
        trace: Rc::clone(&trace),
        raised: None,
    };
    let mut attempt = CapturedAttempt::new(checked.compiled().source());
    checked.build_with_functions_into(
        &mut port,
        &mut decoder,
        &mut bindings,
        limits(),
        &mut attempt,
    );
    assert!(attempt.sources.is_empty());
    let execution = attempt.result.unwrap();
    assert!(matches!(
        *execution.result.unwrap_err(),
        ExecutionError::FunctionBinding { slot: 0 }
    ));
    assert!(execution.handler_counts.is_empty() && execution.retained_verifications.is_empty());
    assert!(trace.borrow().is_empty());
    assert_eq!(cells.get(), 0);
    assert_eq!(port.reads, 0);
}
#[test]
fn callback_failures_preserve_original_payload_and_source_observations_without_retry() {
    let trace = Rc::new(RefCell::new(vec![]));
    let cells = Rc::new(Cell::new(0));
    let payload = Rc::new(());
    let checked = domain::check_with_project(&model("id"), &[function("id")]).unwrap();
    let mut bindings = Bindings {
        plan: Some(checked.compiled().project_calls().unwrap().plans()[0].clone()),
        trace: Rc::clone(&trace),
        raised: Some(Rc::clone(&payload)),
    };
    let (mut port, mut decoder) = ports(&trace, &cells);
    let mut attempt = CapturedAttempt::new(checked.compiled().source());
    checked.build_with_functions_into(
        &mut port,
        &mut decoder,
        &mut bindings,
        limits(),
        &mut attempt,
    );
    assert_eq!(attempt.sources.len(), 1);
    assert!(attempt.sources[0].table.is_some());
    assert!(Arc::ptr_eq(
        attempt.sources[0].snapshot.as_ref().unwrap(),
        &port.held
    ));
    let ExecutionError::ProjectFunction {
        path,
        error,
        identity,
    } = *attempt.result.unwrap().result.unwrap_err()
    else {
        panic!("owned versionless failure")
    };
    assert_eq!(path, "columns.VALUE.derivation.function");
    assert_eq!(error.identity.name, "id");
    assert!(identity.is_some());
    let FailureKind::CallFailed(Payload(original)) = error.kind else {
        panic!("original host payload")
    };
    assert!(Rc::ptr_eq(&original, &payload));
    assert_eq!(&*trace.borrow(), &["inspect", "capture", "decode", "call"]);
    assert_eq!(port.reads, 1);
}

mod project_usecase {
    use super::*;
    use yamaa_core::{
        function_signature::{LogicalSignature, ProjectFunctionIdentity},
        project_environment::{Draft, Environment, ExecutionEnvironment, LockKind, LockReference},
        project_function::{Definition, Language},
        schema::DocumentNode as N,
    };
    use yamaa_engine::{
        project_activation::{ActivationPort, Failure, TestFailure},
        project_domain,
    };
    struct Activation {
        trace: Trace,
        mode: u8,
        calls: Vec<i64>,
        payload: Rc<()>,
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
            assert_eq!(
                functions
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>(),
                ["id"]
            );
            self.trace.borrow_mut().push("lock");
            if self.mode == 1 {
                Err(Payload(Rc::clone(&self.payload)))
            } else {
                Ok(())
            }
        }
        fn bind(
            &mut self,
            identity: &ProjectFunctionIdentity,
            signature: &LogicalSignature,
        ) -> Result<(), Payload> {
            assert_eq!(identity.name, "id");
            assert_eq!(signature.parameters()[0].host_name, "x");
            self.trace.borrow_mut().push("bind");
            if self.mode == 2 {
                Err(Payload(Rc::clone(&self.payload)))
            } else {
                Ok(())
            }
        }
        fn is_interrupt(&self, error: &Payload) -> bool {
            self.mode == 4 && Rc::ptr_eq(&error.0, &self.payload)
        }
        fn invoke(
            &mut self,
            _: &(),
            arguments: &[Argument<'_>],
        ) -> Result<Value, HostError<Payload>> {
            let [Argument {
                name: "x",
                value: ValueRef::Int(value),
            }] = arguments
            else {
                panic!("exact ordered argument")
            };
            self.calls.push(*value);
            self.trace.borrow_mut().push(if *value == 1 {
                "call:live"
            } else {
                "call:case"
            });
            if self.mode == 4 {
                return Err(HostError::Raised(Payload(Rc::clone(&self.payload))));
            }
            Ok(Value::Int(if self.mode == 3 && *value == 7 {
                8
            } else {
                *value
            }))
        }
    }
    fn environment(functions: Option<Vec<Definition>>) -> ExecutionEnvironment {
        let present = functions.is_some();
        Environment::admit(
            Language::Python,
            Draft {
                language: present.then_some(Language::Python),
                lock: present.then_some(LockReference {
                    written: "../uv.lock".into(),
                    kind: LockKind::Uv,
                }),
                functions,
                codelists: vec![],
                has_study: false,
                submissions: vec![],
            },
        )
        .unwrap()
        .into_execution()
    }
    fn definitions() -> Vec<Definition> {
        vec![
            function("unused").definition().clone(),
            function("id").definition().clone(),
        ]
    }
    #[test]
    fn owned_environment_repeats_selected_activation_before_cached_study_capture() {
        let environment = environment(Some(definitions()));
        let trace = Rc::new(RefCell::new(vec![]));
        let cells = Rc::new(Cell::new(0));
        let checked = project_domain::check(&model("id"), &environment).unwrap();
        assert_eq!(checked.compiled().called_functions(), [1]);
        assert!(checked.check_diagnostics().is_empty());
        assert!(trace.borrow().is_empty());
        let mut activation = Activation {
            trace: Rc::clone(&trace),
            mode: 0,
            calls: vec![],
            payload: Rc::new(()),
        };
        let (mut port, mut decoder) = ports(&trace, &cells);
        let mut attempt = CapturedAttempt::new(checked.compiled().source());
        for created in [Some(1), Some(0)] {
            checked
                .build_into(
                    &mut activation,
                    &mut port,
                    &mut decoder,
                    limits(),
                    &mut attempt,
                )
                .unwrap();
            assert_eq!(attempt.sources[0].read.snapshots_created, created);
            assert_eq!(
                attempt
                    .result
                    .as_ref()
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap()
                    .dataset
                    .rows(),
                &[vec![Value::Int(1), Value::Int(1)]]
            );
        }
        let once = [
            "lock",
            "bind",
            "call:case",
            "call:case",
            "inspect",
            "capture",
            "decode",
            "call:live",
        ];
        assert_eq!(
            &*trace.borrow(),
            &once.into_iter().chain(once).collect::<Vec<_>>()
        );
        assert_eq!(activation.calls, [7, i64::MIN, 1, 7, i64::MIN, 1]);
        assert_eq!(port.reads, 1);
        activation.mode = 1;
        trace.borrow_mut().clear();
        assert!(matches!(
            checked.build_into(
                &mut activation,
                &mut port,
                &mut decoder,
                limits(),
                &mut attempt
            ),
            Err(Failure::Lock(_))
        ));
        assert!(attempt.sources.is_empty());
        assert!(matches!(
            attempt.result,
            Err(yamaa_engine::specification_run::PortError::Incomplete)
        ));
        assert_eq!(&*trace.borrow(), &["lock"]);
        assert_eq!(port.reads, 1);
    }
    #[test]
    fn lock_binding_case_and_original_interrupt_failures_never_enter_study_ports() {
        let environment = environment(Some(definitions()));
        let checked = project_domain::check(&model("id"), &environment).unwrap();
        for mode in 1..=4 {
            let trace = Rc::new(RefCell::new(vec![]));
            let cells = Rc::new(Cell::new(0));
            let payload = Rc::new(());
            let mut activation = Activation {
                trace: Rc::clone(&trace),
                mode,
                calls: vec![],
                payload: Rc::clone(&payload),
            };
            let (mut port, mut decoder) = ports(&trace, &cells);
            let mut attempt = CapturedAttempt::new(checked.compiled().source());
            let failure = checked
                .build_into(
                    &mut activation,
                    &mut port,
                    &mut decoder,
                    limits(),
                    &mut attempt,
                )
                .unwrap_err();
            match (mode, failure) {
                (1, Failure::Lock(Payload(original))) => assert!(Rc::ptr_eq(&original, &payload)),
                (2, Failure::Bindings(findings)) => {
                    assert_eq!(findings.len(), 1);
                    assert_eq!(findings[0].function, 0);
                    assert!(Rc::ptr_eq(&findings[0].error.0, &payload));
                }
                (3, Failure::Tests(findings)) => {
                    assert_eq!(
                        findings,
                        [TestFailure::Result {
                            function: 0,
                            case: 0,
                            actual: Value::Int(8),
                            expected: Value::Int(7)
                        }]
                    );
                    assert_eq!(activation.calls, [7, i64::MIN]);
                }
                (
                    4,
                    Failure::InterruptedInvocation {
                        function: 0,
                        case: 0,
                        error,
                    },
                ) => {
                    let FailureKind::CallFailed(Payload(original)) = error.kind else {
                        panic!("original interruption")
                    };
                    assert!(Rc::ptr_eq(&original, &payload));
                    assert_eq!(activation.calls, [7]);
                }
                _ => panic!("typed pre-study failure"),
            }
            assert!(attempt.sources.is_empty());
            assert_eq!(port.reads, 0);
            assert_eq!(cells.get(), 0);
            assert!(!trace.borrow().iter().any(|stage| [
                "inspect",
                "capture",
                "decode",
                "call:live"
            ]
            .contains(stage)));
        }
    }
    #[test]
    fn empty_selection_runs_study_without_lock_or_code_effects_for_absent_and_present_functions() {
        for functions in [None, Some(vec![]), Some(definitions())] {
            let environment = environment(functions);
            let trace = Rc::new(RefCell::new(vec![]));
            let cells = Rc::new(Cell::new(0));
            let spec = specification(vec![(
                "VALUE",
                "int",
                Tree::map(vec![("literal", Tree::Scalar(N::Integer("3".into())))]),
            )]);
            let checked = project_domain::check(&spec, &environment).unwrap();
            let mut activation = Activation {
                trace: Rc::clone(&trace),
                mode: 1,
                calls: vec![],
                payload: Rc::new(()),
            };
            let (mut port, mut decoder) = ports(&trace, &cells);
            let mut attempt = CapturedAttempt::new(checked.compiled().source());
            checked
                .build_into(
                    &mut activation,
                    &mut port,
                    &mut decoder,
                    limits(),
                    &mut attempt,
                )
                .unwrap();
            assert_eq!(
                attempt.result.unwrap().result.unwrap().dataset.rows(),
                &[vec![Value::Int(1), Value::Int(3)]]
            );
            assert_eq!(&*trace.borrow(), &["inspect", "capture", "decode"]);
            assert!(activation.calls.is_empty());
        }
    }
}
