//! Project contracts.
#[allow(dead_code)]
mod support;
mod project {
    mod project_function_execution {
        //! Shared capture executes compiler-owned calls with already activated bindings.
        use crate::support::project_call_compiler as support;
        use std::{
            cell::{Cell, RefCell},
            rc::Rc,
            sync::Arc,
        };
        use support::{call, function, specification, Tree};
        use yamaa_core::{
            dataset::ExecutionError,
            domain,
            function_invocation::{Argument, FailureKind, FunctionPort, HostError},
            specification_run::{CapturedAttempt, SourceDecoder, SpecificationSourcePort},
        };
        use yamaa_core::{
            function_signature::{InvocationPlan, ProjectInvocationPlan},
            specification::{PrepareError, SourceDeclaration},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };
        #[derive(Debug, PartialEq)]
        struct Payload(Rc<()>);
        type Trace = Rc<RefCell<Vec<&'static str>>>;
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum Stage {
            Phase(yamaa_core::application_observer::Phase),
            Call(yamaa_core::application_observer::CallStage),
            Exit(
                yamaa_core::application_observer::Phase,
                yamaa_core::application_observer::Outcome,
            ),
            Return(
                yamaa_core::application_observer::CallStage,
                yamaa_core::application_observer::Outcome,
            ),
            Refusal(
                yamaa_core::application_observer::Phase,
                yamaa_core::application_observer::Refusal,
            ),
        }
        #[derive(Default)]
        struct Stages(RefCell<Vec<Stage>>);
        impl yamaa_core::application_observer::Observer for Stages {
            fn observe(&self, event: yamaa_core::application_observer::Event) {
                use yamaa_core::application_observer::Event;
                self.0.borrow_mut().push(match event {
                    Event::Phase { phase } => Stage::Phase(phase),
                    Event::FunctionCall { stage } => Stage::Call(stage),
                    Event::Exit { phase, outcome } => Stage::Exit(phase, outcome),
                    Event::FunctionReturn { stage, outcome } => Stage::Return(stage, outcome),
                    Event::Refusal { phase, refusal } => Stage::Refusal(phase, refusal),
                });
            }
        }
        struct Port {
            trace: Trace,
            held: Arc<[u8]>,
            reads: usize,
            requested: bool,
        }
        impl SpecificationSourcePort for Port {
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }

            type Error = Payload;
            fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Payload> {
                assert_eq!(source.path, "input.csv");
                self.trace.borrow_mut().push("inspect");
                Ok(())
            }
            fn capture_reads(&self) -> usize {
                self.reads
            }
            fn capture(
                &mut self,
                source: &SourceDeclaration,
                origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
            ) -> Result<Arc<[u8]>, Payload> {
                assert_eq!(source.path, "input.csv");
                *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
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
            fn continue_after(&self, _error: &Self::Error) -> bool {
                false
            }

            type Error = Payload;
            type Table = Table;
            fn decode(
                &mut self,
                source: &SourceDeclaration,
                bytes: &[u8],
                _interpretation: yamaa_core::specification::SourceInterpretation,
                _contract: Option<&yamaa_core::producer_contract::Contract>,
            ) -> Result<Table, Payload> {
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
            fn call(
                &mut self,
                slot: usize,
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Payload>> {
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
                findings
                    .iter()
                    .any(|finding| finding.diagnostic().definition().condition
                        == "unknown_project_function"),
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
                checked.build_into(
                    &mut port,
                    &mut decoder,
                    &mut bindings,
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
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
        fn observer_counts_actual_calls_and_preserves_signature_short_circuit_and_original_failure()
        {
            use yamaa_core::application_observer::{CallStage, Outcome, Phase, Scope};
            use yamaa_core::dataset::ExecutionPhase;
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
                checked.build_into(
                    &mut source,
                    &mut decoder,
                    &mut functions,
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
                            Stage::Exit(
                                Phase::Dataset(ExecutionPhase::Derivation),
                                Outcome::Failed
                            ),
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
        fn unmatched_activation_signatures_fail_before_every_study_port_and_keep_static_failures_early(
        ) {
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
            checked.build_into(
                &mut port,
                &mut decoder,
                &mut bindings,
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
            );
            assert!(attempt.sources.is_empty());
            let execution = attempt.result.unwrap();
            assert!(matches!(
                *execution.result.unwrap_err(),
                ExecutionError::FunctionBinding { slot: 0 }
            ));
            assert!(
                execution.handler_counts.is_empty() && execution.retained_verifications.is_empty()
            );
            assert!(trace.borrow().is_empty());
            assert_eq!(cells.get(), 0);
            assert_eq!(port.reads, 0);
        }

        #[test]
        fn nested_function_signature_is_admitted_before_any_table_access() {
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
            let integer =
                |value: &str| Tree::Scalar(yamaa_core::schema::DocumentNode::Integer(value.into()));
            let spec = specification(vec![(
                "VALUE",
                "int",
                Tree::map(vec![(
                    "case",
                    Tree::List(vec![
                        Tree::map(vec![
                            ("when", Tree::text("ID > 0")),
                            ("then", call("id", vec![("x", integer("7"))])),
                        ]),
                        Tree::map(vec![(
                            "otherwise",
                            Tree::map(vec![("literal", integer("0"))]),
                        )]),
                    ]),
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
            let attempt = plan.execute(&Untouched, &[], &mut bindings, None);
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::FunctionBinding { slot: 0 }
            ));
            assert!(attempt.handler_counts.is_empty() && attempt.retained_verifications.is_empty());
            assert!(trace.borrow().is_empty());
        }
        #[test]
        fn compiled_first_available_preserves_operand_cell_failure_without_fallback() {
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
                fn cell(
                    &self,
                    row: usize,
                    column: usize,
                ) -> Result<ValueRef<'_>, CellError<Payload>> {
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
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
            checked.build_into(
                &mut port,
                &mut decoder,
                &mut bindings,
                &mut attempt,
                yamaa_core::application_observer::Scope::new(
                    &yamaa_core::application_observer::Noop,
                ),
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
                project_environment::{
                    Draft, Environment, ExecutionEnvironment, LockKind, LockReference,
                },
                project_function::{Definition, Language},
                schema::DocumentNode as N,
            };
            use yamaa_core::{
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
                        if (self.mode == 3 && *value == 7) || (self.mode == 5 && *value == i64::MIN)
                        {
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
                let rejected =
                    project_domain::check_owned(&model("unknown"), environment).unwrap_err();
                assert!(matches!(rejected.error, PrepareError::Invalid(_)));
                assert_eq!(rejected.environment.functions().as_ptr(), functions);
                assert_eq!(
                    rejected.environment.functions()[1]
                        .definition()
                        .tests
                        .as_ptr(),
                    cases
                );
                let checked =
                    project_domain::check_owned(&model("id"), rejected.environment).unwrap();
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
                            &mut attempt,
                            None,
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop,
                            ),
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
                        &mut attempt,
                        None,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop
                        )
                    ),
                    Err(Failure::Lock(_))
                ));
                assert!(attempt.sources.is_empty());
                assert!(matches!(
                    attempt.result,
                    Err(yamaa_core::specification_run::PortError::Incomplete)
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
                            &mut attempt,
                            None,
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop,
                            ),
                        )
                        .unwrap_err();
                    match (mode, failure) {
                        (1, Failure::Lock(Payload(original))) => {
                            assert!(Rc::ptr_eq(&original, &payload))
                        }
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
            fn owned_observed_build_retains_activation_after_execution_failure_and_resets_on_retry()
            {
                use yamaa_core::project_activation::{LockObservation, Observations, TestOutcome};
                let checked =
                    project_domain::check_owned(&model("id"), environment(Some(definitions())))
                        .unwrap();
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
                        .build_into(
                            &mut activation,
                            &mut port,
                            &mut decoder,
                            &mut attempt,
                            Some(&mut observations),
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop,
                            ),
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
                    checked.build_into(
                        &mut activation,
                        &mut port,
                        &mut decoder,
                        &mut attempt,
                        Some(&mut observations),
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop
                        )
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
                    Err(yamaa_core::specification_run::PortError::Incomplete)
                ));
                assert_eq!(port.reads, 1);
            }
            #[test]
            fn borrowed_observed_build_keeps_earlier_passes_after_late_case_or_original_host_failure(
            ) {
                use yamaa_core::project_activation::{Observations, TestOutcome};
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
                    let Err(Failure::Tests(failures)) = checked.build_into(
                        &mut activation,
                        &mut port,
                        &mut decoder,
                        &mut attempt,
                        Some(&mut observations),
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
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
            fn owning_application_observer_attributes_actual_activation_and_preserves_empty_selection(
            ) {
                use yamaa_core::application_observer::{CallStage, Phase, Scope};
                use yamaa_core::dataset::ExecutionPhase;
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
                        project_domain::check_owned(&spec, environment(Some(definitions())))
                            .unwrap();
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
                        .build_into(
                            &mut activation,
                            &mut source,
                            &mut decoder,
                            &mut attempt,
                            Some(&mut activation_observations),
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
            fn empty_selection_runs_study_without_lock_or_code_effects_for_absent_and_present_functions(
            ) {
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
                            &mut attempt,
                            None,
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop,
                            ),
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
    }
    mod project_function_result {
        use yamaa_core::{
            project_function_result::results_match,
            temporal::{Date, Precision},
            value::Value,
        };
        #[test]
        fn ties_and_near_ties_go_away_from_zero_with_the_one_rounding_interval() {
            for (actual, expected, decimals, matches) in [
                (0.5, 1.0, 0, true),
                (-0.5, -1.0, 0, true),
                (0.49, -0.49, 0, true),
                (0.5, -0.5, 0, false),
                (1.23445, 1.2345, 4, true),
                (-1.23445, -1.2345, 4, true),
                (1.23444, 1.2345, 4, false),
                // The exact binary64 2.675 is 1.8e-14 below the scaled tie,
                // inside REQ-0418's 2^-26 interval, so its copy is 2.68.
                (2.675, 2.67, 2, false),
                (2.675, 2.68, 2, true),
                (-2.675, -2.68, 2, true),
                (2.685, 2.69, 2, true),
                // 1.0e-8 below the scaled tie is inside the interval; 1.0e-7 is not.
                (1.0049999999, 1.01, 2, true),
                (1.004999999, 1.00, 2, true),
                (1.004999999, 1.01, 2, false),
            ] {
                assert_eq!(
                    results_match(&Value::float(actual), &Value::float(expected), decimals),
                    matches,
                    "{actual} {expected} {decimals}"
                );
            }
        }
        #[test]
        fn minimum_subnormal_and_huge_precision_are_bounded_and_do_not_coalesce() {
            let smallest = Value::float(f64::from_bits(1));
            let next = Value::float(f64::from_bits(2));
            assert!(results_match(&smallest, &Value::float(0.0), 323));
            assert!(!results_match(&smallest, &Value::float(0.0), 324));
            for digits in [324, 340, i64::MAX] {
                assert!(results_match(&smallest, &smallest, digits));
                assert!(!results_match(&smallest, &next, digits));
                assert!(!results_match(
                    &Value::float(f64::MAX),
                    &Value::float(f64::from_bits(f64::MAX.to_bits() - 1)),
                    digits
                ));
            }
        }
        #[test]
        fn comparison_never_mutates_runtime_bits_or_normalizes_text() {
            let actual = Value::float(1.23445);
            let negative_zero = Value::float(-0.0);
            assert!(results_match(&actual, &Value::float(1.2345), 4));
            let Value::Float(value) = actual else {
                unreachable!()
            };
            assert_eq!(value.get().to_bits(), 1.23445f64.to_bits());
            assert!(results_match(&negative_zero, &Value::float(0.0), 4));
            let Value::Float(value) = negative_zero else {
                unreachable!()
            };
            assert_eq!(value.get().to_bits(), 0x8000_0000_0000_0000);
            assert!(!results_match(
                &Value::Str("e\u{301}".into()),
                &Value::Str("\u{e9}".into()),
                4
            ));
            assert!(results_match(
                &Value::Str("a\0b".into()),
                &Value::Str("a\0b".into()),
                4
            ));
        }
        #[test]
        fn all_other_values_use_exact_type_and_missing_equality() {
            for digits in [0, 4, i64::MAX] {
                assert!(results_match(&Value::Missing, &Value::Missing, digits));
                assert!(!results_match(&Value::Missing, &Value::Int(0), digits));
                assert!(!results_match(&Value::Int(1), &Value::float(1.0), digits));
                assert!(results_match(
                    &Value::Int(i64::MAX),
                    &Value::Int(i64::MAX),
                    digits
                ));
                let full = Value::Date(Date::new(2024, 1, 1, Precision::Day).unwrap());
                let collected = Value::Date(Date::new(2024, 1, 1, Precision::Year).unwrap());
                assert!(results_match(&full, &collected, digits));
            }
            assert!(!results_match(&Value::Int(1), &Value::Int(1), -1));
        }
    }
    mod project_lock_diagnostics {
        //! Lock reporting only borrows a completed native activation rejection.
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{
            diagnostic::{
                lock_diagnostics, LockDiagnosticError, LockFinding, LockOrigins, LockReason,
            },
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{LockKind, LockReference},
            project_function::{Case, Definition, Function, Language, Parameter},
            value::{ColumnType, Value, ValueType},
        };
        use yamaa_core::{
            function_invocation::{Argument, HostError},
            project_activation::{activate, ActivationPort, Failure},
        };

        // No Clone, Debug or Send implementation exists for this original host payload.
        struct Opaque {
            marker: Rc<u8>,
            findings: Vec<LockFinding>,
        }
        struct Port {
            original: Option<Opaque>,
            effects: Rc<RefCell<Vec<&'static str>>>,
        }
        impl ActivationPort for Port {
            type Handle = ();
            type Error = Opaque;
            fn verify_lock(
                &mut self,
                language: Language,
                lock: &LockReference,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Opaque> {
                assert_eq!(language, Language::Python);
                assert_eq!(lock.written, "../uv.lock");
                assert_eq!(functions.len(), 1);
                assert_eq!(functions[0].call, "program.identity");
                self.effects.borrow_mut().push("metadata/lock");
                Err(self.original.take().expect("single lock effect"))
            }
            fn bind(
                &mut self,
                _: &ProjectFunctionIdentity,
                _: &LogicalSignature,
            ) -> Result<(), Opaque> {
                panic!("lock rejection forbids provider binding")
            }
            fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<Opaque>> {
                panic!("lock rejection forbids invocation")
            }
        }
        fn function() -> Function {
            Function::admit(
                Language::Python,
                Definition {
                    name: "identity".into(),
                    function: "program.identity".into(),
                    description: "Identity integer.".into(),
                    params: vec![Parameter {
                        name: "x".into(),
                        kind: ValueType::Int,
                        required: true,
                        default: None,
                        accepts_missing: false,
                    }],
                    returns: ColumnType::Int,
                    may_return_missing: false,
                    comparison_decimals: 4,
                    tests: vec![
                        Case {
                            id: "normal".into(),
                            covers: vec!["normal".into()],
                            args: vec![("x".into(), Value::Int(7))],
                            result: Value::Int(7),
                        },
                        Case {
                            id: "boundary".into(),
                            covers: vec!["boundary".into()],
                            args: vec![("x".into(), Value::Int(i64::MIN))],
                            result: Value::Int(i64::MIN),
                        },
                        Case {
                            id: "missing".into(),
                            covers: vec!["short-circuit-missing:x".into()],
                            args: vec![("x".into(), Value::Missing)],
                            result: Value::Missing,
                        },
                    ],
                },
            )
            .unwrap()
        }
        #[test]
        fn non_clone_non_debug_non_send_lock_error_retains_identity_without_provider_retry() {
            let original = Opaque {
                marker: Rc::new(73),
                findings: vec![
                    LockFinding {
                        package: "program".into(),
                        reason: LockReason::VersionMismatch,
                        expected: vec!["1.0".into()],
                        actual: Some("1.0.0".into()),
                    },
                    LockFinding {
                        package: "yamaa".into(),
                        reason: LockReason::PackageNotInstalled,
                        expected: vec!["0.3.0".into()],
                        actual: None,
                    },
                ],
            };
            let address = Rc::as_ptr(&original.marker);
            let expected_address = original.findings[0].expected[0].as_ptr();
            let effects = Rc::new(RefCell::new(vec![]));
            let mut port = Port {
                original: Some(original),
                effects: Rc::clone(&effects),
            };
            let functions = [function()];
            let lock = LockReference {
                written: "../uv.lock".into(),
                kind: LockKind::Uv,
            };
            let Err(failure) = activate(Language::Python, &lock, &functions, &mut port) else {
                panic!("original lock failure")
            };
            let Failure::Lock(original) = &failure else {
                panic!("exact native failure kind")
            };
            let source = "study/environment.yaml";
            let captured = "held/uv.lock";
            let origins = LockOrigins {
                source,
                lock_source: captured,
                written: &lock.written,
            };
            let first = lock_diagnostics(Some(origins), &original.findings).unwrap();
            assert_eq!(first.len(), 2);
            assert_eq!(first[0].definition().requirement, Some("REQ-0697"));
            assert_eq!(
                lock_diagnostics(None, &original.findings),
                Err(LockDiagnosticError::InvalidContext)
            );
            assert_eq!(
                lock_diagnostics(Some(origins), &original.findings).unwrap(),
                first
            );
            assert_eq!(Rc::as_ptr(&original.marker), address);
            assert_eq!(Rc::strong_count(&original.marker), 1);
            assert_eq!(original.findings[0].expected[0].as_ptr(), expected_address);
            assert_eq!(*effects.borrow(), ["metadata/lock"]);
            assert!(port.original.is_none());
        }
    }
    mod project_terminology {
        use yamaa_core::{
            project_terminology::{
                validate, BindingFinding, Catalogue, Codelist, DataType, External, Fault, Finding,
                Item, Source, Standard,
            },
            value::{ColumnType, Value},
        };
        fn item(value: Value) -> Item {
            Item {
                value,
                decode: None,
                rank: None,
                alias: None,
                extended: false,
            }
        }
        fn list() -> Codelist {
            Codelist {
                id: "SEX".into(),
                name: "Sex".into(),
                data_type: DataType::Text,
                extensible: false,
                alias: None,
                format_name: None,
                items: Some(vec![
                    item(Value::Str("F".into())),
                    item(Value::Str("M".into())),
                ]),
                external: None,
            }
        }
        fn source(list: Codelist) -> Source {
            Source {
                standard: None,
                codelists: vec![list],
            }
        }
        fn published(mut source: Source) -> Source {
            source.standard = Some(Standard {
                name: "CDISC/NCI".into(),
                publishing_set: "SDTM".into(),
                version: "2023-12-15".into(),
            });
            source
        }

        #[test]
        fn global_duplicates_across_sources_have_no_override() {
            let first = source(list());
            let second = published(first.clone());
            assert_eq!(
                validate(&[first, second]),
                vec![
                    Finding {
                        source: 1,
                        codelist: 0,
                        fault: Fault::DuplicateId
                    },
                    Finding {
                        source: 1,
                        codelist: 0,
                        fault: Fault::DuplicateName
                    }
                ]
            );
        }
        #[test]
        fn complete_independent_item_findings_survive_invalid_shape() {
            let mut list = list();
            list.external = Some(External {
                dictionary: "dictionary".into(),
                version: "1".into(),
                href: None,
            });
            list.items = Some(vec![
                Item {
                    decode: Some("a".into()),
                    rank: Some(1),
                    extended: true,
                    ..item(Value::Int(1))
                },
                item(Value::Int(1)),
            ]);
            assert_eq!(
                validate(&[source(list)])
                    .iter()
                    .map(|f| &f.fault)
                    .collect::<Vec<_>>(),
                vec![
                    &Fault::InvalidShape,
                    &Fault::InvalidValue { item: 0 },
                    &Fault::ExtensionNotAdmitted { item: 0 },
                    &Fault::InvalidValue { item: 1 },
                    &Fault::DuplicateValue { item: 1 },
                    &Fault::PartialDecode,
                    &Fault::PartialRank
                ]
            );
        }
        #[test]
        fn extensions_require_both_a_published_source_and_an_extensible_list() {
            for extensible in [false, true] {
                for standard in [false, true] {
                    let mut list = list();
                    list.extensible = extensible;
                    list.items.as_mut().unwrap()[0].extended = true;
                    let mut source = source(list);
                    if standard {
                        source = published(source);
                    }
                    let f = validate(&[source]);
                    assert_eq!(f.is_empty(), extensible && standard);
                    if !f.is_empty() {
                        assert_eq!(f[0].fault, Fault::ExtensionNotAdmitted { item: 0 });
                    }
                }
            }
        }
        #[test]
        fn extended_items_carry_no_published_alias() {
            // A sponsor addition has no published code: the published item
            // keeps its alias and the extended one may declare none.
            let mut list = list();
            list.extensible = true;
            list.alias = Some("C66731".into());
            let items = list.items.as_mut().unwrap();
            items[0].alias = Some("C16576".into());
            items[1].extended = true;
            assert!(validate(&[published(source(list.clone()))]).is_empty());
            list.items.as_mut().unwrap()[1].alias = Some("C20197".into());
            assert_eq!(
                validate(&[published(source(list))])
                    .iter()
                    .map(|f| &f.fault)
                    .collect::<Vec<_>>(),
                vec![&Fault::ExtendedAlias { item: 1 }]
            );
        }
        #[test]
        fn numeric_equality_is_exact_at_i64_and_binary64_boundaries() {
            let mut list = list();
            list.data_type = DataType::Float;
            list.items = Some(vec![
                item(Value::Int(9_007_199_254_740_993)),
                item(Value::float(9_007_199_254_740_992.0)),
                item(Value::Int(9_007_199_254_740_992)),
                item(Value::float(-0.0)),
                item(Value::Int(0)),
                item(Value::Int(i64::MAX)),
                item(Value::float(9_223_372_036_854_775_808.0)),
            ]);
            let f = validate(&[source(list)]);
            assert_eq!(
                f.iter().map(|f| &f.fault).collect::<Vec<_>>(),
                vec![
                    &Fault::DuplicateValue { item: 2 },
                    &Fault::DuplicateValue { item: 4 }
                ]
            );
        }
        #[test]
        fn text_keeps_nul_unicode_normalization_and_authored_order() {
            let mut list = list();
            list.items = Some(vec![
                item(Value::Str("e\u{301}".into())),
                item(Value::Str("\u{e9}".into())),
                item(Value::Str("\0".into())),
                item(Value::Str("".into())),
            ]);
            let expected = source(list);
            let catalogue = Catalogue::admit(vec![expected.clone()]).unwrap();
            assert_eq!(catalogue.sources(), &[expected]);
            assert_eq!(catalogue.enforced_items("SEX").unwrap().len(), 4);
        }
        #[test]
        fn binding_checks_exact_column_type_and_rejects_restated_item_sets() {
            let catalogue = Catalogue::admit(vec![source(list())]).unwrap();
            assert_eq!(
                catalogue.bind("unknown", ColumnType::Str, false),
                vec![BindingFinding::UnknownCodelist]
            );
            assert_eq!(
                catalogue.bind("SEX", ColumnType::Int, true),
                vec![
                    BindingFinding::TypeMismatch {
                        expected: ColumnType::Str,
                        actual: ColumnType::Int
                    },
                    BindingFinding::RedundantAllowedValues
                ]
            );
            assert!(catalogue.bind("SEX", ColumnType::Str, false).is_empty());
            // An extensible list of items is enforced like any other list; only
            // an external dictionary leaves values and verifications unchecked.
            let mut extensible = list();
            extensible.extensible = true;
            let catalogue = Catalogue::admit(vec![source(extensible)]).unwrap();
            assert_eq!(
                catalogue.bind("SEX", ColumnType::Str, true),
                vec![BindingFinding::RedundantAllowedValues]
            );
            assert_eq!(catalogue.enforced_items("SEX").unwrap().len(), 2);
            let mut external = list();
            external.items = None;
            external.external = Some(External {
                dictionary: "d".into(),
                version: "1".into(),
                href: Some("relative/path".into()),
            });
            let catalogue = Catalogue::admit(vec![source(external)]).unwrap();
            assert!(catalogue.bind("SEX", ColumnType::Str, true).is_empty());
            assert_eq!(catalogue.enforced_items("SEX"), None);
        }
        #[test]
        fn invalid_shapes_and_missing_boolean_fractional_item_types_are_rejected() {
            let mut shape = list();
            shape.items = None;
            assert_eq!(validate(&[source(shape)])[0].fault, Fault::InvalidShape);
            for (kind, value) in [
                (DataType::Text, Value::Missing),
                (DataType::Integer, Value::Bool(true)),
                (DataType::Integer, Value::float(1.5)),
                (DataType::Float, Value::Str("1".into())),
            ] {
                let mut list = list();
                list.data_type = kind;
                list.items = Some(vec![item(value)]);
                assert_eq!(
                    validate(&[source(list)])[0].fault,
                    Fault::InvalidValue { item: 0 }
                );
            }
        }
    }
    mod specification_codelist_execution {
        use crate::support::project_codelist_compiler as support;
        use std::{cell::RefCell, convert::Infallible};
        use support::{environment, schema, spec};
        use yamaa_core::{dataset::ExecutionError, project_domain};
        use yamaa_core::{
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::Value,
        };
        struct Table {
            schema: TableSchema,
            reads: RefCell<Vec<(usize, usize)>>,
            bad: bool,
        }
        impl TableAccess for Table {
            type Error = Infallible;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                2
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Infallible>> {
                self.reads.borrow_mut().push((row, column));
                Ok(match column {
                    0 => ValueRef::Int(i64::MAX - row as i64),
                    1 if row == 0 => ValueRef::Str("F"),
                    1 if self.bad => ValueRef::Str("é\0🙂"),
                    1 => ValueRef::Missing,
                    2 => {
                        assert!(!self.bad, "later cell read after a failed CT checkpoint");
                        ValueRef::Int(9)
                    }
                    _ => panic!("bad coordinate"),
                })
            }
        }

        #[test]
        fn owned_checked_environment_executes_fixed_codelists_before_later_cells_in_both_plan_forms(
        ) {
            for rows in [false, true] {
                let checked = project_domain::check_owned(
                    &spec("SEX", "str", None, rows, None),
                    environment(false),
                )
                .unwrap();
                assert!(checked.compiled().called_functions().is_empty());
                let table = Table {
                    schema: schema(),
                    reads: RefCell::new(vec![]),
                    bad: true,
                };
                let plan = checked.compiled().bind(&table.schema).unwrap();
                let attempt = plan.execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                let ExecutionError::VerificationFailures(records) = *attempt.result.unwrap_err()
                else {
                    panic!("fixed-list failure")
                };
                assert_eq!(records.len(), 1);
                let record = &records[0];
                assert_eq!(
                    (
                        record.definition().requirement.unwrap(),
                        record.path.as_str(),
                        record.evaluated_count,
                        record.failed_count
                    ),
                    ("REQ-0957", "columns.CODE.submission.codelist", 2, 1)
                );
                assert_eq!(record.offending_rows[0].values, [Value::Int(i64::MAX - 1)]);
                assert_eq!(
                    record.codelist.as_ref().unwrap().values,
                    [Value::Str("é\0🙂".into())]
                );
                assert!(table.reads.borrow().iter().all(|&(_, column)| column < 2));
            }
        }
        #[test]
        fn missing_cells_pass_repeated_compiled_builds_and_keep_complete_check_observations() {
            let checked = project_domain::check_owned(
                &spec("SEX", "str", None, false, None),
                environment(false),
            )
            .unwrap();
            let table = Table {
                schema: schema(),
                reads: RefCell::new(vec![]),
                bad: false,
            };
            for _ in 0..2 {
                let result = checked
                    .compiled()
                    .bind(&table.schema)
                    .unwrap()
                    .execute(
                        &table,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap();
                assert_eq!(
                    result.dataset.rows(),
                    &[
                        vec![Value::Int(i64::MAX), Value::Str("F".into()), Value::Int(9)],
                        vec![Value::Int(i64::MAX - 1), Value::Missing, Value::Int(9)]
                    ]
                );
                assert_eq!(result.verifications.len(), 1);
                let record = &result.verifications[0];
                assert_eq!((record.evaluated_count, record.failed_count), (2, 0));
                assert_eq!(record.codelist.as_ref().unwrap().id, "SEX");
                assert!(record.codelist.as_ref().unwrap().values.is_empty());
            }
        }
    }
    mod specification_project_codelists {
        use crate::support::project_codelist_compiler as support;
        use support::{environment, schema, spec};
        use yamaa_core::{
            dataset::Check,
            specification::{PrepareError, PreparedSpecification},
            value::Value,
        };
        #[test]
        fn fixed_column_checkpoint_is_compiled_for_ordinary_and_row_plans_without_activation() {
            for rows in [false, true] {
                let doc = spec("SEX", "str", None, rows, None);
                let env = environment(false);
                let prepared = PreparedSpecification::prepare_with_environment(&doc, &env).unwrap();
                assert!(prepared.called_functions().is_empty());
                assert!(prepared.verification_declaration_diagnostics().is_empty());
                assert_eq!(
                    prepared.verification_target("columns.CODE.submission.codelist"),
                    Some("CODE")
                );
                assert_eq!(
                    prepared.verification_identity("columns.CODE.submission.codelist"),
                    None
                );
                let plan = prepared.bind(&schema()).unwrap();
                assert_eq!(plan.column_verifications().len(), 1);
                assert_eq!(plan.column_verifications()[0].column, 1);
                assert!(
                    matches!(&plan.column_verifications()[0].checks[0].check,Check::Codelist {id,values} if id=="SEX" && values==&[Value::Str("F".into())])
                );
            }
        }
        #[test]
        fn compiler_projects_unknown_type_and_redundant_sets_as_static_preflight_causes() {
            for (doc, condition, requirement, path) in [
                (
                    spec("UNKNOWN", "str", None, false, None),
                    "unknown_codelist",
                    "REQ-0953",
                    "columns.CODE.submission.codelist",
                ),
                (
                    spec("SEX", "int", None, false, None),
                    "codelist_type_mismatch",
                    "REQ-0954",
                    "columns.CODE.submission.codelist",
                ),
                (
                    spec("SEX", "str", Some("F"), false, None),
                    "redundant_declaration",
                    "REQ-0955",
                    "columns.CODE.verifications[0].allowed_values",
                ),
            ] {
                let Err(PrepareError::Invalid(findings)) =
                    PreparedSpecification::prepare_with_environment(&doc, &environment(false))
                else {
                    panic!("static rejection")
                };
                assert_eq!(findings.len(), 1);
                let d = findings[0].diagnostic();
                assert_eq!(
                    (
                        d.definition().phase,
                        d.definition().condition,
                        d.definition().requirement
                    ),
                    ("validation", condition, Some(requirement))
                );
                assert_eq!(d.spec_paths, [path]);
            }
        }
        #[test]
        fn extensible_catalogue_is_enforced_and_states_the_value_set_once() {
            let prepared = PreparedSpecification::prepare_with_environment(
                &spec("SEX", "str", None, false, None),
                &environment(true),
            )
            .unwrap();
            let plan = prepared.bind(&schema()).unwrap();
            assert_eq!(plan.column_verifications()[0].checks.len(), 1);
            assert!(
                matches!(&plan.column_verifications()[0].checks[0].check,Check::Codelist {id,values} if id=="SEX" && values==&[Value::Str("F".into())])
            );
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_environment(
                    &spec("SEX", "str", Some("M"), false, None),
                    &environment(true),
                )
            else {
                panic!("allowed values beside a listing codelist")
            };
            assert_eq!(findings.len(), 1);
            let d = findings[0].diagnostic();
            assert_eq!(d.definition().condition, "redundant_declaration");
            assert_eq!(
                d.spec_paths,
                ["columns.CODE.verifications[0].allowed_values"]
            );
            assert_eq!(
                d.context["stated_by"],
                yamaa_core::json::Value::String("columns.CODE.submission.codelist".into())
            );
        }
        #[test]
        fn submission_claims_are_admitted_and_reserved_metadata_has_a_registered_cause() {
            for rows in [false, true] {
                assert!(
                    PreparedSpecification::prepare(&spec("SEX", "str", None, rows, None)).is_ok()
                );
                assert!(PreparedSpecification::prepare_with_environment(
                    &spec("SEX", "str", None, rows, Some("role")),
                    &environment(false)
                )
                .is_ok());
                let Err(PrepareError::Invalid(findings)) =
                    PreparedSpecification::prepare_with_environment(
                        &spec("SEX", "str", None, rows, Some("metadata")),
                        &environment(false),
                    )
                else {
                    panic!("reserved column metadata")
                };
                let diagnostics = findings
                    .iter()
                    .map(|finding| finding.diagnostic())
                    .collect::<Vec<_>>();
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(
                    diagnostics[0].code,
                    yamaa_core::diagnostic::ConditionCode::SubmissionReservedMetadata
                );
                assert_eq!(
                    diagnostics[0].spec_paths,
                    ["columns.CODE.metadata.codelist"]
                );
            }
        }
        #[test]
        fn root_metadata_rejects_the_governed_has_no_data_key() {
            let doc = spec("SEX", "str", None, false, Some("root_metadata"));
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_environment(&doc, &environment(false))
            else {
                panic!("root metadata.has_no_data must be rejected");
            };
            let diagnostics = findings
                .iter()
                .map(|finding| finding.diagnostic())
                .collect::<Vec<_>>();
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(
                diagnostics[0].code,
                yamaa_core::diagnostic::ConditionCode::SubmissionReservedMetadata
            );
            assert_eq!(diagnostics[0].spec_paths, ["metadata.has_no_data"]);
        }
    }
    mod specification_project_functions {
        use crate::support::id_source_schema as source;
        use crate::support::project_call_compiler as support;
        use support::{call, function, specification, Tree};
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{
            bound_expression::Read,
            dataset::{Expression, FunctionInput},
            schema::DocumentNode as N,
            specification::{
                BindError, BindFinding, FunctionCause, PreflightFinding, PrepareError,
                PreparedSpecification,
            },
            table::{Column, TableSchema},
            value::ColumnType,
        };
        #[test]
        fn absent_environment_and_empty_function_catalogue_retain_identical_static_nested_call_causes(
        ) {
            let spec = specification(vec![(
                "A",
                "int",
                Tree::map(vec![(
                    "case",
                    Tree::List(vec![Tree::map(vec![
                        ("when", Tree::text("ID > 0")),
                        ("then", call("unknown", vec![("x", Tree::text("ID"))])),
                    ])]),
                )]),
            )]);
            let findings = |result| {
                let Err(PrepareError::Invalid(findings)) = result else {
                    panic!("static function admission")
                };
                findings
                    .into_iter()
                    .map(|finding| {
                        let PreflightFinding::ProjectFunction(finding) = finding else {
                            panic!("function cause")
                        };
                        finding.diagnostic()
                    })
                    .collect::<Vec<_>>()
            };
            let absent = findings(PreparedSpecification::prepare(&spec));
            let empty = findings(PreparedSpecification::prepare_with_project(&spec, &[]));
            assert_eq!(absent, empty);
            assert_eq!(absent.len(), 1);
            assert_eq!(absent[0].definition().condition, "unknown_project_function");
            assert_eq!(
                absent[0].spec_paths,
                ["columns.A.derivation.case[0].then.function.name"]
            );
        }

        #[test]
        fn compiler_selects_environment_slots_and_lowers_output_argument_dependencies() {
            let spec = specification(vec![
                ("A", "int", call("beta", vec![("x", Tree::text("ID"))])),
                ("B", "int", call("alpha", vec![("x", Tree::text("A"))])),
            ]);
            let prepared = PreparedSpecification::prepare_with_project(
                &spec,
                &[function("alpha"), function("beta"), function("unused")],
            )
            .unwrap();
            assert_eq!(prepared.called_functions(), [0, 1]);
            let plan = prepared.bind(&source()).unwrap();
            assert_eq!(
                plan.columns().iter().map(|a| a.column).collect::<Vec<_>>(),
                [1, 2]
            );
            for (assignment, slot, column) in
                [(&plan.columns()[0], 1, 0), (&plan.columns()[1], 0, 1)]
            {
                let Expression::ProjectFunction(bound) = &assignment.expression else {
                    panic!("versionless function expression")
                };
                assert_eq!(bound.slot(), slot);
                assert_eq!(
                    bound.arguments()[0].input,
                    FunctionInput::Read(Read::Column(column))
                );
            }
        }

        #[test]
        fn complete_static_faults_are_projected_in_authored_call_order_without_source_binding() {
            let spec = specification(vec![
                (
                    "A",
                    "int",
                    call("unknown", vec![("x", Tree::Scalar(N::Integer("7".into())))]),
                ),
                (
                    "B",
                    "int",
                    call("id", vec![("x", Tree::Scalar(N::Float(7.0)))]),
                ),
                (
                    "C",
                    "int",
                    call(
                        "id",
                        vec![("x", Tree::Scalar(N::Integer("9223372036854775808".into())))],
                    ),
                ),
            ]);
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")])
            else {
                panic!("complete static findings")
            };
            assert_eq!(findings.len(), 3);
            let diagnostics = findings.iter().map(|f| f.diagnostic()).collect::<Vec<_>>();
            assert_eq!(
                diagnostics
                    .iter()
                    .map(|d| d.spec_paths[0].as_str())
                    .collect::<Vec<_>>(),
                [
                    "columns.A.derivation.function.name",
                    "columns.B.derivation.function.args.x",
                    "columns.C.derivation.function.args.x",
                ]
            );
            for (d, condition, requirement) in [
                (&diagnostics[0], "unknown_project_function", "REQ-0698"),
                (&diagnostics[1], "invalid_function_argument", "REQ-0700"),
                (&diagnostics[2], "invalid_function_argument", "REQ-0700"),
            ] {
                assert_eq!(
                    (
                        d.definition().phase,
                        d.definition().condition,
                        d.definition().requirement
                    ),
                    ("validation", condition, Some(requirement))
                );
            }
            assert_eq!(
                diagnostics[1].context["expected_type"],
                JsonValue::String("int".into())
            );
            assert_eq!(
                diagnostics[1].context["actual_type"],
                JsonValue::String("float".into())
            );
            assert_eq!(
                diagnostics[2].context["cause"],
                JsonValue::String("integer_range".into())
            );
        }

        #[test]
        fn declared_output_reference_types_and_unknown_names_fail_during_prepare() {
            let spec = specification(vec![
                (
                    "F",
                    "float",
                    Tree::map(vec![("literal", Tree::Scalar(N::Float(1.0)))]),
                ),
                ("A", "int", call("id", vec![("x", Tree::text("F"))])),
                (
                    "B",
                    "int",
                    call("id", vec![("x", Tree::text("UNDECLARED"))]),
                ),
            ]);
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")])
            else {
                panic!("static reference findings")
            };
            assert_eq!(findings.len(), 2);
            assert_eq!(
                findings[0].diagnostic().spec_paths,
                ["columns.A.derivation.function.args.x"]
            );
            assert_eq!(
                findings[0].diagnostic().definition().condition,
                "invalid_function_argument"
            );
            assert_eq!(
                findings[1].diagnostic().definition().condition,
                "unknown_field"
            );
            assert_eq!(
                findings[1].diagnostic().context["identifier"],
                JsonValue::String("UNDECLARED".into())
            );
        }

        #[test]
        fn stored_field_kinds_bind_exactly_and_nonkey_arguments_use_collection() {
            let spec = specification(vec![(
                "A",
                "int",
                call("id", vec![("x", Tree::text("SRC.ID"))]),
            )]);
            let prepared =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
            let plan = prepared.bind(&source()).unwrap();
            let Expression::ProjectFunction(bound) = &plan.columns()[0].expression else {
                panic!("function")
            };
            assert_eq!(
                bound.arguments()[0].input,
                FunctionInput::Collect {
                    column: 0,
                    identifier: "SRC.ID".into()
                }
            );
            let wrong = TableSchema::new(vec![Column {
                name: "ID".into(),
                kind: ColumnType::Float,
            }])
            .unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&wrong) else {
                panic!("exact argument type")
            };
            let finding = findings
                .iter()
                .find_map(|f| match f {
                    BindFinding::ProjectFunction(f) => Some(f),
                    _ => None,
                })
                .unwrap();
            assert!(matches!(finding.cause, FunctionCause::ArgumentType { .. }));
            assert_eq!(
                finding.diagnostic().definition().requirement,
                Some("REQ-0700")
            );
            assert_eq!(
                finding.diagnostic().spec_paths,
                ["columns.A.derivation.function.args.x"]
            );
        }

        #[test]
        fn missing_stored_fields_and_unknown_relations_keep_authored_argument_diagnostics() {
            let spec = specification(vec![
                ("A", "int", call("id", vec![("x", Tree::text("SRC.TYPO"))])),
                ("B", "int", call("id", vec![("x", Tree::text("OTHER.ID"))])),
            ]);
            let prepared =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("complete source-reference findings")
            };
            assert_eq!(findings.len(), 2);
            for (finding, column, name) in [
                (&findings[0], "A", "SRC.TYPO"),
                (&findings[1], "B", "OTHER.ID"),
            ] {
                let diagnostics = finding
                    .diagnostics(prepared.source())
                    .expect("authored diagnostic");
                assert_eq!(diagnostics.len(), 1);
                let diagnostic = &diagnostics[0];
                assert_eq!(diagnostic.definition().condition, "unknown_field");
                assert_eq!(diagnostic.definition().requirement, Some("REQ-0103"));
                assert_eq!(
                    diagnostic.spec_paths,
                    [format!("columns.{column}.derivation.function.args.x")]
                );
                assert_eq!(
                    diagnostic.context["identifier"],
                    JsonValue::String(name.into())
                );
                assert_eq!(diagnostic.context.len(), 1);
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
            }
        }

        #[test]
        fn calls_keep_dependency_cycles_and_absent_catalogue_static_causes() {
            let spec = specification(vec![
                ("A", "int", call("id", vec![("x", Tree::text("B"))])),
                ("B", "int", call("id", vec![("x", Tree::text("A"))])),
            ]);
            let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(&spec) else {
                panic!("absent function catalogue retains the unknown-call cause")
            };
            assert_eq!(findings.len(), 2);
            for finding in findings {
                let PreflightFinding::ProjectFunction(finding) = finding else {
                    panic!("function cause")
                };
                assert_eq!(finding.cause, FunctionCause::UnknownFunction);
            }
            let prepared =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
            let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
                panic!("dependency findings")
            };
            assert!(findings
                .iter()
                .any(|f| matches!(f, BindFinding::Dependencies { .. })));
        }

        #[test]
        fn retired_fields_and_malformed_literals_are_complete_static_findings() {
            let retired = Tree::map(vec![(
                "function",
                Tree::map(vec![
                    ("name", Tree::text("id")),
                    ("contract_version", Tree::text("1")),
                    (
                        "args",
                        Tree::map(vec![(
                            "x",
                            Tree::map(vec![("literal", Tree::Scalar(N::Integer("7".into())))]),
                        )]),
                    ),
                ]),
            )]);
            let spec = specification(vec![("A", "int", retired)]);
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")])
            else {
                panic!("closed calls")
            };
            assert_eq!(findings.len(), 2);
            assert!(findings
                .iter()
                .all(|f| matches!(f, PreflightFinding::ProjectFunction(_))));
        }

        #[test]
        fn unknown_function_is_retained_alongside_an_invalid_scalar_in_the_same_call() {
            let spec = specification(vec![(
                "VALUE",
                "int",
                call(
                    "unknown",
                    vec![("x", Tree::Scalar(N::Integer("9223372036854775808".into())))],
                ),
            )]);
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare_with_project(&spec, &[function("id")])
            else {
                panic!("independent name and scalar findings")
            };
            assert_eq!(findings.len(), 2);
            assert_eq!(
                findings[0].diagnostic().definition().condition,
                "unknown_project_function"
            );
            assert_eq!(
                findings[0].diagnostic().spec_paths,
                ["columns.VALUE.derivation.function.name"]
            );
            assert_eq!(
                findings[1].diagnostic().context["cause"],
                JsonValue::String("integer_range".into())
            );
        }
    }
}
