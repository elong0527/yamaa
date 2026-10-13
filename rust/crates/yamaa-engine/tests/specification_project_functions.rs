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
    dataset::{self, ExecutionError, FunctionPort},
    domain,
    function_invocation::{Argument, FailureKind, HostError},
    specification_run::{CapturedAttempt, Limits, SourceDecoder, SourcePort},
};
#[derive(Debug, PartialEq)]
struct Payload(Rc<()>);
type Trace = Rc<RefCell<Vec<&'static str>>>;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Stage {
    Phase(yamaa_engine::application_observer::Phase),
    Call(yamaa_engine::application_observer::CallStage),
    Exit(
        yamaa_engine::application_observer::Phase,
        yamaa_engine::application_observer::Outcome,
    ),
    Return(
        yamaa_engine::application_observer::CallStage,
        yamaa_engine::application_observer::Outcome,
    ),
}
#[derive(Default)]
struct Stages(RefCell<Vec<Stage>>);
impl yamaa_engine::application_observer::Observer for Stages {
    fn observe(&self, event: yamaa_engine::application_observer::Event<'_>) {
        use yamaa_engine::application_observer::Event;
        self.0.borrow_mut().push(match event {
            Event::Phase { node, phase } => {
                assert_eq!(node, None);
                Stage::Phase(phase)
            }
            Event::FunctionCall { node, stage } => {
                assert_eq!(node, None);
                Stage::Call(stage)
            }
            Event::Exit {
                node,
                phase,
                outcome,
            } => {
                assert_eq!(node, None);
                Stage::Exit(phase, outcome)
            }
            Event::FunctionReturn {
                node,
                stage,
                outcome,
            } => {
                assert_eq!(node, None);
                Stage::Return(stage, outcome)
            }
        });
    }
}
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
impl FunctionPort for Bindings {
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
    let Err(PrepareError::Invalid(findings)) = domain::check(&spec) else {
        panic!("the called function needs its admitted environment");
    };
    assert!(
        findings.iter().any(
            |finding| finding.diagnostic().definition().condition == "unknown_project_function"
        ),
        "{findings:?}"
    );
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
fn observer_counts_actual_calls_and_preserves_signature_short_circuit_and_original_failure() {
    use yamaa_engine::application_observer::{CallStage, Outcome, Phase, Scope};
    use yamaa_engine::dataset::ExecutionPhase;
    for mode in ["normal", "missing", "signature", "raised"] {
        let spec = specification(vec![(
            "VALUE",
            "int",
            call(
                "id",
                vec![(
                    "x",
                    if mode == "missing" {
                        Tree::Scalar(yamaa_core::schema::DocumentNode::Null)
                    } else {
                        Tree::text("SRC.ID")
                    },
                )],
            ),
        )]);
        let checked = domain::check_with_project(&spec, &[function("id")]).unwrap();
        let trace = Rc::new(RefCell::new(vec![]));
        let cells = Rc::new(Cell::new(0));
        let marker = Rc::new(());
        let mut functions = Bindings {
            plan: (mode != "signature")
                .then(|| checked.compiled().project_calls().unwrap().plans()[0].clone()),
            trace: Rc::clone(&trace),
            raised: (mode == "raised").then(|| Rc::clone(&marker)),
        };
        let (mut source, mut decoder) = ports(&trace, &cells);
        let mut attempt = CapturedAttempt::new(checked.compiled().source());
        let observer = Stages::default();
        checked.build_with_observer_into(
            &mut source,
            &mut decoder,
            &mut functions,
            limits(),
            &mut attempt,
            Scope::new(&observer),
        );
        let all_stages = observer.0.borrow();
        let stages = all_stages
            .iter()
            .filter(|event| matches!(event, Stage::Phase(_) | Stage::Call(_)))
            .cloned()
            .collect::<Vec<_>>();
        let observed_calls = stages
            .iter()
            .filter(|event| matches!(event, Stage::Call(_)))
            .count();
        assert_eq!(
            observed_calls,
            trace
                .borrow()
                .iter()
                .filter(|event| **event == "call")
                .count()
        );
        assert_eq!(
            observed_calls,
            usize::from(matches!(mode, "normal" | "raised"))
        );
        if mode == "signature" {
            assert_eq!(stages, [Stage::Phase(Phase::Input)]);
            assert_eq!(
                *all_stages,
                [
                    Stage::Phase(Phase::Input),
                    Stage::Exit(Phase::Input, Outcome::Failed)
                ]
            );
            assert!(trace.borrow().is_empty());
            assert_eq!(source.reads, 0);
            assert!(matches!(
                *attempt.result.unwrap().result.unwrap_err(),
                ExecutionError::FunctionBinding { slot: 0 }
            ));
            continue;
        }
        assert_eq!(
            &stages[..3],
            [
                Stage::Phase(Phase::Input),
                Stage::Phase(Phase::Dataset(ExecutionPhase::Admission)),
                Stage::Phase(Phase::Dataset(ExecutionPhase::Derivation))
            ]
        );
        if mode == "raised" {
            assert!(all_stages.contains(&Stage::Return(
                CallStage::Dataset(ExecutionPhase::Derivation),
                Outcome::Failed
            )));
            assert_eq!(
                &all_stages[all_stages.len() - 2..],
                [
                    Stage::Exit(Phase::Dataset(ExecutionPhase::Derivation), Outcome::Failed),
                    Stage::Phase(Phase::Dataset(ExecutionPhase::Finished))
                ]
            );
            assert_eq!(
                stages[3],
                Stage::Call(CallStage::Dataset(ExecutionPhase::Derivation))
            );
            assert_eq!(
                stages.last(),
                Some(&Stage::Phase(Phase::Dataset(ExecutionPhase::Finished)))
            );
            assert!(!stages.iter().any(|event| matches!(
                event,
                Stage::Phase(Phase::Dataset(
                    ExecutionPhase::OutputKeys | ExecutionPhase::Verification
                ))
            )));
            let error = attempt.result.unwrap().result.unwrap_err();
            let ExecutionError::ProjectFunction { error, .. } = *error else {
                panic!("original function failure")
            };
            let FailureKind::CallFailed(Payload(original)) = error.kind else {
                panic!("original payload")
            };
            assert!(Rc::ptr_eq(&marker, &original));
        } else {
            assert_eq!(
                all_stages
                    .iter()
                    .filter_map(|event| if let Stage::Exit(phase, outcome) = event {
                        Some((*phase, *outcome))
                    } else {
                        None
                    })
                    .collect::<Vec<_>>(),
                [
                    (Phase::Input, Outcome::Succeeded),
                    (
                        Phase::Dataset(ExecutionPhase::Admission),
                        Outcome::Succeeded
                    ),
                    (
                        Phase::Dataset(ExecutionPhase::Derivation),
                        Outcome::Succeeded
                    ),
                    (
                        Phase::Dataset(ExecutionPhase::OutputKeys),
                        Outcome::Succeeded
                    ),
                    (
                        Phase::Dataset(ExecutionPhase::Verification),
                        Outcome::Succeeded
                    ),
                ]
            );
            assert_eq!(
                attempt.result.unwrap().result.unwrap().dataset.rows(),
                &[vec![
                    Value::Int(1),
                    if mode == "missing" {
                        Value::Missing
                    } else {
                        Value::Int(1)
                    }
                ]]
            );
            assert_eq!(
                &stages[stages.len() - 3..],
                [
                    Stage::Phase(Phase::Dataset(ExecutionPhase::OutputKeys)),
                    Stage::Phase(Phase::Dataset(ExecutionPhase::Verification)),
                    Stage::Phase(Phase::Dataset(ExecutionPhase::Finished))
                ]
            );
        }
    }
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
fn concat_function_signature_is_admitted_before_any_table_access() {
    use yamaa_engine::dataset::DatasetExecution;
    struct Untouched;
    impl TableAccess for Untouched {
        type Error = Payload;
        fn schema(&self) -> &TableSchema {
            panic!("signature admission must precede schema access")
        }
        fn row_count(&self) -> usize {
            panic!("signature admission must precede row access")
        }
        fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
            panic!("signature admission must precede cell access")
        }
    }
    let spec = specification(vec![(
        "VALUE",
        "str",
        Tree::map(vec![(
            "str_concat",
            Tree::map(vec![(
                "sources",
                Tree::List(vec![call(
                    "id",
                    vec![(
                        "x",
                        Tree::Scalar(yamaa_core::schema::DocumentNode::Integer("7".into())),
                    )],
                )]),
            )]),
        )]),
    )]);
    let checked = domain::check_with_project(&spec, &[function("id")]).unwrap();
    let source = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    let plan = checked.compiled().bind(&source).unwrap();
    let trace = Rc::new(RefCell::new(vec![]));
    let mut bindings = Bindings {
        plan: None,
        trace: Rc::clone(&trace),
        raised: None,
    };
    let attempt =
        plan.execute_observed_functions(&Untouched, &[], &mut bindings, limits().execution);
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::FunctionBinding { slot: 0 }
    ));
    assert!(attempt.handler_counts.is_empty() && attempt.retained_verifications.is_empty());
    assert!(trace.borrow().is_empty());
}
#[test]
fn compiled_first_available_preserves_operand_cell_failure_without_fallback() {
    use yamaa_engine::dataset::DatasetExecution;
    struct FailingTable {
        schema: TableSchema,
        payload: Rc<()>,
        reads: RefCell<Vec<usize>>,
    }
    impl TableAccess for FailingTable {
        type Error = Payload;
        fn schema(&self) -> &TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            1
        }
        fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
            assert_eq!(row, 0);
            self.reads.borrow_mut().push(column);
            match column {
                0 => Ok(ValueRef::Int(1)),
                1 => Err(CellError::Access(Payload(Rc::clone(&self.payload)))),
                2 => panic!("a failing operand must not read the fallback"),
                _ => panic!("unexpected column"),
            }
        }
    }
    let spec = specification(vec![(
        "VALUE",
        "int",
        Tree::map(vec![(
            "first_available",
            Tree::map(vec![(
                "sources",
                Tree::List(vec![
                    Tree::map(vec![("variable", Tree::text("SRC.A"))]),
                    Tree::map(vec![("variable", Tree::text("SRC.B"))]),
                ]),
            )]),
        )]),
    )]);
    let payload = Rc::new(());
    let source = FailingTable {
        schema: TableSchema::new(
            ["ID", "A", "B"]
                .into_iter()
                .map(|name| Column {
                    name: name.into(),
                    kind: ColumnType::Int,
                })
                .collect(),
        )
        .unwrap(),
        payload: Rc::clone(&payload),
        reads: RefCell::new(vec![]),
    };
    let checked = domain::check(&spec).unwrap();
    let plan = checked.compiled().bind(&source.schema).unwrap();
    let attempt = plan.execute_observed(&source, limits().execution);
    assert!(attempt.handler_counts.is_empty() && attempt.retained_verifications.is_empty());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(Payload(original)),
    } = *attempt.result.unwrap_err()
    else {
        panic!("original failing cell access")
    };
    assert_eq!(path, "columns.VALUE.derivation.first_available.sources[0]");
    assert_eq!(source_row, 0);
    assert!(Rc::ptr_eq(&original, &payload));
    assert_eq!(
        source
            .reads
            .borrow()
            .iter()
            .filter(|&&column| column == 1)
            .count(),
        1
    );
    assert!(!source.reads.borrow().contains(&2));
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
            if (self.mode == 6 && *value == i64::MIN) || (self.mode == 8 && *value == 1) {
                return Err(HostError::Raised(Payload(Rc::clone(&self.payload))));
            }
            Ok(Value::Int(
                if (self.mode == 3 && *value == 7) || (self.mode == 5 && *value == i64::MIN) {
                    8
                } else {
                    *value
                },
            ))
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
    fn owning_check_preserves_definitions_and_cases_on_rejection_and_success() {
        let environment = environment(Some(definitions()));
        let functions = environment.functions().as_ptr();
        let cases = environment.functions()[1].definition().tests.as_ptr();
        let rejected = project_domain::check_owned(&model("unknown"), environment).unwrap_err();
        assert!(matches!(rejected.error, PrepareError::Invalid(_)));
        assert_eq!(rejected.environment.functions().as_ptr(), functions);
        assert_eq!(
            rejected.environment.functions()[1]
                .definition()
                .tests
                .as_ptr(),
            cases
        );
        let checked = project_domain::check_owned(&model("id"), rejected.environment).unwrap();
        assert_eq!(checked.environment().functions().as_ptr(), functions);
        assert_eq!(
            checked.environment().functions()[1]
                .definition()
                .tests
                .as_ptr(),
            cases
        );
        assert_eq!(checked.compiled().called_functions(), [1]);
        assert!(checked.check_diagnostics().is_empty());
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
    fn owned_observed_build_retains_activation_after_execution_failure_and_resets_on_retry() {
        use yamaa_engine::project_activation::{LockObservation, Observations, TestOutcome};
        let checked =
            project_domain::check_owned(&model("id"), environment(Some(definitions()))).unwrap();
        let trace = Rc::new(RefCell::new(vec![]));
        let cells = Rc::new(Cell::new(0));
        let payload = Rc::new(());
        let mut activation = Activation {
            trace: Rc::clone(&trace),
            mode: 0,
            calls: vec![],
            payload: Rc::clone(&payload),
        };
        let (mut port, mut decoder) = ports(&trace, &cells);
        let mut attempt = CapturedAttempt::new(checked.compiled().source());
        let mut observations = Observations::default();
        for mode in [0, 8] {
            activation.mode = mode;
            checked
                .build_observed_into(
                    &mut activation,
                    &mut port,
                    &mut decoder,
                    limits(),
                    &mut attempt,
                    &mut observations,
                )
                .unwrap();
            assert_eq!(observations.lock, LockObservation::Verified);
            assert_eq!(observations.bindings.len(), 1);
            assert_eq!(
                observations
                    .tests
                    .iter()
                    .map(|t| (t.function, t.case, t.outcome, t.invoked, t.actual.as_ref()))
                    .collect::<Vec<_>>(),
                [
                    (0, 0, TestOutcome::Passed, true, Some(&Value::Int(7))),
                    (0, 1, TestOutcome::Passed, true, Some(&Value::Int(i64::MIN))),
                    (0, 2, TestOutcome::Passed, false, Some(&Value::Missing))
                ]
            );
            let execution = attempt.result.as_ref().unwrap();
            if mode == 0 {
                assert_eq!(
                    execution.result.as_ref().unwrap().dataset.rows(),
                    &[vec![Value::Int(1), Value::Int(1)]]
                );
            } else {
                let ExecutionError::ProjectFunction { error, .. } =
                    execution.result.as_ref().unwrap_err().as_ref()
                else {
                    panic!("retained invocation failure");
                };
                let FailureKind::CallFailed(Payload(original)) = &error.kind else {
                    panic!("original host payload");
                };
                assert!(Rc::ptr_eq(original, &payload));
                assert_eq!(attempt.sources.len(), 1);
                assert_eq!(
                    attempt.sources[0].snapshot.as_deref(),
                    Some(b"held input".as_slice())
                );
            }
        }
        assert_eq!(activation.calls, [7, i64::MIN, 1, 7, i64::MIN, 1]);
        assert_eq!(port.reads, 1);
        activation.mode = 1;
        assert!(matches!(
            checked.build_observed_into(
                &mut activation,
                &mut port,
                &mut decoder,
                limits(),
                &mut attempt,
                &mut observations
            ),
            Err(Failure::Lock(_))
        ));
        assert_eq!(
            observations,
            Observations {
                lock: LockObservation::Rejected,
                ..Observations::default()
            }
        );
        assert!(attempt.sources.is_empty());
        assert!(matches!(
            attempt.result,
            Err(yamaa_engine::specification_run::PortError::Incomplete)
        ));
        assert_eq!(port.reads, 1);
    }
    #[test]
    fn borrowed_observed_build_keeps_earlier_passes_after_late_case_or_original_host_failure() {
        use yamaa_engine::project_activation::{Observations, TestOutcome};
        let environment = environment(Some(definitions()));
        let checked = project_domain::check(&model("id"), &environment).unwrap();
        for mode in [5, 6] {
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
            let mut observations = Observations::default();
            let Err(Failure::Tests(failures)) = checked.build_observed_into(
                &mut activation,
                &mut port,
                &mut decoder,
                limits(),
                &mut attempt,
                &mut observations,
            ) else {
                panic!("late case failure");
            };
            assert_eq!(failures.len(), 1);
            if mode == 6 {
                let TestFailure::Invocation { case: 1, error, .. } = &failures[0] else {
                    panic!("original invocation failure");
                };
                let FailureKind::CallFailed(Payload(original)) = &error.kind else {
                    panic!("original host payload");
                };
                assert!(Rc::ptr_eq(original, &payload));
            }
            assert_eq!(observations.tests[0].outcome, TestOutcome::Passed);
            assert_eq!(observations.tests[0].actual, Some(Value::Int(7)));
            assert_eq!(
                observations.tests[1].outcome,
                if mode == 5 {
                    TestOutcome::ResultMismatch
                } else {
                    TestOutcome::InvocationFailed
                }
            );
            assert_eq!(observations.tests[2].outcome, TestOutcome::Passed);
            assert!(!observations.tests[2].invoked);
            assert_eq!(activation.calls, [7, i64::MIN]);
            assert_eq!(
                &*trace.borrow(),
                &["lock", "bind", "call:case", "call:case"]
            );
            assert!(attempt.sources.is_empty());
            assert_eq!(port.reads, 0);
            assert_eq!(cells.get(), 0);
        }
    }
    #[test]
    fn owning_application_observer_attributes_actual_activation_and_preserves_empty_selection() {
        use yamaa_engine::application_observer::{CallStage, Phase, Scope};
        use yamaa_engine::dataset::ExecutionPhase;
        for called in [false, true] {
            let spec = if called {
                model("id")
            } else {
                specification(vec![(
                    "VALUE",
                    "int",
                    Tree::map(vec![("literal", Tree::Scalar(N::Integer("3".into())))]),
                )])
            };
            let checked =
                project_domain::check_owned(&spec, environment(Some(definitions()))).unwrap();
            let trace = Rc::new(RefCell::new(vec![]));
            let cells = Rc::new(Cell::new(0));
            let mut activation = Activation {
                trace: Rc::clone(&trace),
                mode: if called { 0 } else { 1 },
                calls: vec![],
                payload: Rc::new(()),
            };
            let (mut source, mut decoder) = ports(&trace, &cells);
            let mut attempt = CapturedAttempt::new(checked.compiled().source());
            let mut activation_observations = Default::default();
            let observer = Stages::default();
            checked
                .build_with_observer_into(
                    &mut activation,
                    &mut source,
                    &mut decoder,
                    limits(),
                    (&mut attempt, &mut activation_observations),
                    Scope::new(&observer),
                )
                .unwrap();
            assert!(attempt.result.unwrap().result.is_ok());
            let stages = observer.0.borrow();
            assert_eq!(
                stages
                    .iter()
                    .filter(|event| **event == Stage::Call(CallStage::Activation))
                    .count(),
                if called { 2 } else { 0 }
            );
            assert_eq!(
                stages
                    .iter()
                    .filter(|event| **event
                        == Stage::Call(CallStage::Dataset(ExecutionPhase::Derivation)))
                    .count(),
                usize::from(called)
            );
            assert_eq!(activation.calls.len(), if called { 3 } else { 0 });
            assert_eq!(
                stages.first(),
                Some(&Stage::Phase(if called {
                    Phase::Activation
                } else {
                    Phase::Input
                }))
            );
            assert_eq!(
                activation_observations.tests.len(),
                if called { 3 } else { 0 }
            );
            assert_eq!(
                activation_observations
                    .tests
                    .iter()
                    .filter(|case| case.invoked)
                    .count(),
                if called { 2 } else { 0 }
            );
            assert_eq!(source.reads, 1);
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
