//! Project contracts: the seams of project function execution, function
//! results, lock diagnostics, terminology, codelist execution and a
//! specification's project codelists and functions that the diagnostic
//! replay cannot reach, each named in its test's first comment: scripted
//! resolver, decoder, table and host-function ports with their effect traces
//! and observations, the result comparison kernel, the terminology
//! validator's internal findings, the resolved plan and executed rows. What
//! a coded specification refuses is
//! `tests/fixtures/diagnostics/codelist_binding.json`'s, and what a
//! specification's project calls refuse is `projects.json`'s (#3798).
mod project {
    mod project_function_execution {
        //! Shared capture executes compiler-owned calls with already activated bindings.
        use crate::support::project_call_compiler as support;
        use crate::support::specification::{check, check_owned};
        use std::{
            cell::{Cell, RefCell},
            rc::Rc,
            sync::Arc,
        };
        use support::{
            call, environment as with_functions, function, specification, typed_specification, Tree,
        };
        use yamaa_core::{
            dataset::ExecutionError,
            decoder,
            function_invocation::{Argument, FailureKind, FunctionPort, HostError},
            resolver::{Captured, Failure, Located, Name, Resolver},
            specification_run::CapturedAttempt,
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
        impl Resolver for Port {
            type Error = Payload;
            fn inspect(&mut self, name: Name<'_>) -> Result<Located, Payload> {
                let Name::Input(source) = name else {
                    panic!("unexpected request {name:?}")
                };
                assert_eq!(source.path, "input.csv");
                self.trace.borrow_mut().push("inspect");
                Ok(Located {
                    identity: source.path.clone(),
                    display_path: source.path.clone(),
                })
            }
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Payload> {
                let Name::Input(source) = name else {
                    panic!("unexpected request {name:?}")
                };
                assert_eq!(source.path, "input.csv");
                self.trace.borrow_mut().push("capture");
                // The first capture takes the snapshot; later ones reuse it.
                let created = !self.requested;
                if created {
                    self.reads += 1;
                    self.requested = true;
                }
                Ok(Captured::resource(Arc::clone(&self.held), created))
            }
            fn failure(&self, _: Name<'_>, _: &Payload) -> Option<Failure> {
                None
            }
            fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Payload> {
                panic!("an input is never rebased")
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
        impl decoder::Decoder for Decoder {
            crate::support::ports::no_documents!();
            fn continue_after(&self, _error: &Self::Error) -> bool {
                false
            }

            type Error = Payload;
            type Table = Table;
            fn table(
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

        fn model(name: &str) -> yamaa_core::schema::Document {
            typed_specification(vec![(
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
        /// A seam the replay cannot reach: the effect trace of a scripted
        /// resolver, decoder and function port, and the held snapshot two
        /// builds share.
        #[test]
        fn checked_calls_execute_through_shared_capture_and_repeat_on_held_snapshots() {
            let trace = Rc::new(RefCell::new(vec![]));
            let cells = Rc::new(Cell::new(0));
            let spec = model("id");
            // Without its project, the function a 1.0 specification calls
            // binds nothing (CT-13).
            let Err(PrepareError::Invalid(findings)) = check(&spec, None) else {
                panic!("the called function needs its admitted environment");
            };
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.diagnostic().definition().condition
                        == "project_binding_missing"),
                "{findings:?}"
            );
            let checked = check(&spec, Some(&with_functions(&["id"]))).unwrap();
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
        /// A seam the replay cannot reach: the application observer's events
        /// around each call through scripted ports, and the original failure a
        /// call raises.
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
                let checked = check(&spec, Some(&with_functions(&["id"]))).unwrap();
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

        /// A seam the replay cannot reach: a function port without the call's
        /// signature fails the execution before a scripted resolver, decoder or
        /// table is touched.
        #[test]
        fn unmatched_activation_signatures_fail_before_every_study_port_and_keep_static_failures_early(
        ) {
            let trace = Rc::new(RefCell::new(vec![]));
            let cells = Rc::new(Cell::new(0));
            assert!(matches!(
                check(&model("unknown"), Some(&with_functions(&["id"]))),
                Err(PrepareError::Invalid(_))
            ));
            let checked = check(&model("id"), Some(&with_functions(&["id"]))).unwrap();
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

        /// A seam the replay cannot reach: a nested call's signature is
        /// admitted before any access to a scripted table.
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
                        Tree::map(vec![("otherwise", integer("0"))]),
                    ]),
                )]),
            )]);
            let checked = check(&spec, Some(&with_functions(&["id"]))).unwrap();
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
            let attempt = plan.execute(&Untouched, &[], &mut bindings, None, &|| false);
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::FunctionBinding { slot: 0 }
            ));
            assert!(attempt.handler_counts.is_empty() && attempt.retained_verifications.is_empty());
            assert!(trace.borrow().is_empty());
        }
        /// A seam the replay cannot reach: a scripted table's failing cell
        /// read, its original payload, and the handler counts an execution
        /// returns.
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
                        Tree::List(vec![Tree::text("SRC.A"), Tree::text("SRC.B")]),
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
            let checked = check(&spec, None).unwrap();
            let plan = checked.compiled().bind(&source.schema).unwrap();
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
                &|| false,
            );
            // The selection's default substitution is registered at zero; the
            // failing operand read is never answered by it.
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [("columns.VALUE.derivation.first_available.missing", 0)]
            );
            assert!(attempt.retained_verifications.is_empty());
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
            // The failing operand is read; the later one never is.
            assert!(source.reads.borrow().contains(&1));
            assert!(!source.reads.borrow().contains(&2));
        }

        /// A seam the replay cannot reach: a scripted function port's original
        /// failure payload and the source observations of the build it fails.
        #[test]
        fn callback_failures_preserve_original_payload_and_source_observations_without_retry() {
            let trace = Rc::new(RefCell::new(vec![]));
            let cells = Rc::new(Cell::new(0));
            let payload = Rc::new(());
            let checked = check(&model("id"), Some(&with_functions(&["id"]))).unwrap();
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
                ast,
                function_signature::{LogicalSignature, ProjectFunctionIdentity},
                project_environment::{
                    Draft, Environment, ExecutionEnvironment, LockKind, LockReference,
                },
                project_function::Language,
                schema::DocumentNode as N,
            };
            use yamaa_core::{
                host_functions::{HostFunctions, Lock},
                project_activation::{Failure, TestFailure},
            };
            struct Activation {
                trace: Trace,
                mode: u8,
                calls: Vec<i64>,
                payload: Rc<()>,
            }
            impl HostFunctions for Activation {
                type Handle = ();
                type Error = Payload;
                fn verify_lock(
                    &mut self,
                    language: Language,
                    lock: Lock<'_>,
                    functions: &[ProjectFunctionIdentity],
                ) -> Result<(), Payload> {
                    assert_eq!(language, Language::Python);
                    assert_eq!(lock.reference.written, "../uv.lock");
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
            fn environment(functions: Option<Vec<ast::Function>>) -> ExecutionEnvironment {
                let present = functions.is_some();
                Environment::admit(
                    Some(Language::Python),
                    Draft {
                        language: present.then_some(Language::Python),
                        lock: present.then_some(LockReference {
                            written: "../uv.lock".into(),
                            kind: LockKind::Uv,
                        }),
                        functions,
                        codelists: vec![],
                    },
                )
                .unwrap()
                .into_execution()
            }
            fn definitions() -> Vec<ast::Function> {
                vec![
                    function("unused").definition().clone(),
                    function("id").definition().clone(),
                ]
            }
            /// A seam the replay cannot reach: the owned environment's
            /// definitions and cases keep their allocations through a rejected
            /// and an accepted check.
            #[test]
            fn owning_check_preserves_definitions_and_cases_on_rejection_and_success() {
                let environment = environment(Some(definitions()));
                let functions = environment.functions().as_ptr();
                let cases = environment.functions()[1].definition().tests.as_ptr();
                let rejected = check_owned(&model("unknown"), environment).unwrap_err();
                assert!(matches!(rejected.error, PrepareError::Invalid(_)));
                assert_eq!(rejected.environment.functions().as_ptr(), functions);
                assert_eq!(
                    rejected.environment.functions()[1]
                        .definition()
                        .tests
                        .as_ptr(),
                    cases
                );
                let checked = check_owned(&model("id"), rejected.environment).unwrap();
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
            /// A seam the replay cannot reach: a scripted host-function port's
            /// activation effects, repeated before each cached study capture.
            #[test]
            fn owned_environment_repeats_selected_activation_before_cached_study_capture() {
                let environment = environment(Some(definitions()));
                let trace = Rc::new(RefCell::new(vec![]));
                let cells = Rc::new(Cell::new(0));
                let checked = check_owned(&model("id"), environment.clone()).unwrap();
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
                            (&mut activation, b""),
                            &mut port,
                            &mut decoder,
                            &mut attempt,
                            (
                                None,
                                yamaa_core::application_observer::Scope::new(
                                    &yamaa_core::application_observer::Noop,
                                ),
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
                        (&mut activation, b""),
                        &mut port,
                        &mut decoder,
                        &mut attempt,
                        (
                            None,
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop
                            )
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
            /// A seam the replay cannot reach: the lock, binding, case and
            /// interrupt failures of a scripted host-function port, and the
            /// study ports they never enter.
            #[test]
            fn lock_binding_case_and_original_interrupt_failures_never_enter_study_ports() {
                let environment = environment(Some(definitions()));
                let checked = check_owned(&model("id"), environment.clone()).unwrap();
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
                            (&mut activation, b""),
                            &mut port,
                            &mut decoder,
                            &mut attempt,
                            (
                                None,
                                yamaa_core::application_observer::Scope::new(
                                    &yamaa_core::application_observer::Noop,
                                ),
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
            /// A seam the replay cannot reach: the activation observations an
            /// owned build keeps after an execution failure and resets on
            /// retry.
            #[test]
            fn owned_observed_build_retains_activation_after_execution_failure_and_resets_on_retry()
            {
                use yamaa_core::project_activation::{LockObservation, Observations, TestOutcome};
                let checked = check_owned(&model("id"), environment(Some(definitions()))).unwrap();
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
                            (&mut activation, b""),
                            &mut port,
                            &mut decoder,
                            &mut attempt,
                            (
                                Some(&mut observations),
                                yamaa_core::application_observer::Scope::new(
                                    &yamaa_core::application_observer::Noop,
                                ),
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
                        (&mut activation, b""),
                        &mut port,
                        &mut decoder,
                        &mut attempt,
                        (
                            Some(&mut observations),
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop
                            )
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
            /// A seam the replay cannot reach: the passing case observations a
            /// borrowed build keeps beside a late case failure or the host's
            /// original failure.
            #[test]
            fn borrowed_observed_build_keeps_earlier_passes_after_late_case_or_original_host_failure(
            ) {
                use yamaa_core::project_activation::{Observations, TestOutcome};
                let environment = environment(Some(definitions()));
                let checked = check_owned(&model("id"), environment.clone()).unwrap();
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
                        (&mut activation, b""),
                        &mut port,
                        &mut decoder,
                        &mut attempt,
                        (
                            Some(&mut observations),
                            yamaa_core::application_observer::Scope::new(
                                &yamaa_core::application_observer::Noop,
                            ),
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
            /// A seam the replay cannot reach: the application observer's
            /// activation events, and none for an empty selection.
            #[test]
            fn owning_application_observer_attributes_actual_activation_and_preserves_empty_selection(
            ) {
                use yamaa_core::application_observer::{CallStage, Phase, Scope};
                use yamaa_core::dataset::ExecutionPhase;
                for called in [false, true] {
                    let spec = if called {
                        model("id")
                    } else {
                        specification(vec![("VALUE", "int", Tree::Scalar(N::Integer("3".into())))])
                    };
                    let checked = check_owned(&spec, environment(Some(definitions()))).unwrap();
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
                            (&mut activation, b""),
                            &mut source,
                            &mut decoder,
                            &mut attempt,
                            (Some(&mut activation_observations), Scope::new(&observer)),
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
            /// A seam the replay cannot reach: the effect trace and rows of a
            /// build that calls no function, with no lock or code effect.
            #[test]
            fn empty_selection_runs_study_without_lock_or_code_effects_for_absent_and_present_functions(
            ) {
                for functions in [None, Some(vec![]), Some(definitions())] {
                    let environment = environment(functions);
                    let trace = Rc::new(RefCell::new(vec![]));
                    let cells = Rc::new(Cell::new(0));
                    let spec =
                        specification(vec![("VALUE", "int", Tree::Scalar(N::Integer("3".into())))]);
                    let checked = check_owned(&spec, environment.clone()).unwrap();
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
                            (&mut activation, b""),
                            &mut port,
                            &mut decoder,
                            &mut attempt,
                            (
                                None,
                                yamaa_core::application_observer::Scope::new(
                                    &yamaa_core::application_observer::Noop,
                                ),
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
        /// A seam the replay cannot reach: the result comparison kernel,
        /// `results_match`, on binary64 ties and near ties.
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
                // inside FORMULA-16's 2^-26 interval, so its copy is 2.68.
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
        /// A seam the replay cannot reach: `results_match` at the subnormal and
        /// precision extremes.
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
        /// A seam the replay cannot reach: `results_match` leaves the bits and
        /// text of the values it compares as they are.
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
        /// A seam the replay cannot reach: `results_match` over every other
        /// value type and missing.
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
        use crate::support::roots::{Case, Definition, Parameter};
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{
            diagnostic::{
                lock_diagnostics, LockDiagnosticError, LockFinding, LockOrigins, LockReason,
            },
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{LockKind, LockReference},
            project_function::{Function, Language},
            value::{ColumnType, Value, ValueType},
        };
        use yamaa_core::{
            function_invocation::{Argument, HostError},
            host_functions::{HostFunctions, Lock},
            project_activation::{activate, Failure},
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
        impl HostFunctions for Port {
            type Handle = ();
            type Error = Opaque;
            fn verify_lock(
                &mut self,
                language: Language,
                lock: Lock<'_>,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Opaque> {
                assert_eq!(language, Language::Python);
                assert_eq!(lock.reference.written, "../uv.lock");
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
                }
                .root(Language::Python),
            )
            .unwrap()
        }
        /// A seam the replay cannot reach: a scripted lock port's opaque
        /// original error keeps its identity, and the provider is never
        /// retried.
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
            let held = Lock {
                reference: &lock,
                bytes: b"",
            };
            let Err(failure) = activate(Language::Python, held, &functions, &mut port) else {
                panic!("original lock failure")
            };
            let Failure::Lock(original) = &failure else {
                panic!("exact native failure kind")
            };
            let source = "study/environment.yaml";
            let captured = "held/uv.lock";
            let origins = LockOrigins {
                source,
                entry: "study/project.yaml",
                field: "lock",
                lock_source: captured,
                written: &lock.written,
            };
            let first = lock_diagnostics(Some(origins), &original.findings).unwrap();
            assert_eq!(first.len(), 2);
            assert_eq!(first[0].definition().rule, Some("FUNC-3"));
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
        use crate::support::roots::{Codelist, CodelistType, External, Item, Source, Standard};
        use yamaa_core::{
            ast::Codelists,
            project_terminology::{validate, BindingFinding, Catalogue, Fault, Finding},
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
                data_type: CodelistType::Text,
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
        fn source(list: Codelist) -> Codelists {
            Source {
                standard: None,
                codelists: vec![list],
            }
            .root()
        }
        fn published(mut source: Codelists) -> Codelists {
            source.standard = Some(
                Standard {
                    name: "CDISC/NCI".into(),
                    publishing_set: "SDTM".into(),
                    version: "2023-12-15".into(),
                }
                .root(),
            );
            source
        }

        /// A seam the replay cannot reach: the terminology validator's internal
        /// findings, each a source, a codelist and a `Fault`.
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
        /// A seam the replay cannot reach: every internal `Fault` the
        /// terminology validator keeps beside an invalid shape, by item.
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
        /// A seam the replay cannot reach: the terminology validator's internal
        /// `Fault` for an extended item, over each combination of a standard
        /// and extensibility.
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
        /// A seam the replay cannot reach: the terminology validator's internal
        /// `Fault` for an extended item's alias.
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
        /// A seam the replay cannot reach: the terminology validator's internal
        /// finding for a format name's dollar sign, for each data type.
        #[test]
        fn format_names_begin_with_a_dollar_sign_exactly_for_text_codelists() {
            // A text codelist names a character format, so `$` leads its
            // format name; a numeric codelist names a numeric format, whose
            // name never begins with `$`.
            let named = |data_type, format: Option<&str>| {
                let mut list = list();
                list.data_type = data_type;
                if data_type != CodelistType::Text {
                    list.items = Some(vec![item(Value::Int(1)), item(Value::Int(2))]);
                }
                list.format_name = format.map(String::from);
                validate(&[source(list)])
            };
            let refused = vec![Finding {
                source: 0,
                codelist: 0,
                fault: Fault::FormatNamePrefix,
            }];
            assert!(named(CodelistType::Text, None).is_empty());
            assert!(named(CodelistType::Text, Some("$SEX")).is_empty());
            for format in ["SEX", "S$EX", " $SEX"] {
                assert_eq!(named(CodelistType::Text, Some(format)), refused);
            }
            for data_type in [CodelistType::Integer, CodelistType::Float] {
                assert!(named(data_type, None).is_empty());
                for format in ["NY", "N$Y", " $NY"] {
                    assert!(named(data_type, Some(format)).is_empty(), "{format}");
                }
                for format in ["$NY", "$"] {
                    assert_eq!(named(data_type, Some(format)), refused, "{format}");
                }
            }
        }
        /// A seam the replay cannot reach: the terminology validator's internal
        /// duplicate `Fault`s at the i64 and binary64 boundaries.
        #[test]
        fn numeric_equality_is_exact_at_i64_and_binary64_boundaries() {
            let mut list = list();
            list.data_type = CodelistType::Float;
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
        /// A seam the replay cannot reach: an admitted catalogue's sources and
        /// enforced items, values and not issues.
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
        /// A seam the replay cannot reach: the catalogue's internal
        /// `BindingFinding`s and the items it enforces.
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
        /// A seam the replay cannot reach: the terminology validator's internal
        /// `Fault` for each item value a data type refuses.
        #[test]
        fn invalid_shapes_and_missing_boolean_fractional_item_types_are_rejected() {
            let mut shape = list();
            shape.items = None;
            assert_eq!(validate(&[source(shape)])[0].fault, Fault::InvalidShape);
            for (kind, value) in [
                (CodelistType::Text, Value::Missing),
                (CodelistType::Integer, Value::Bool(true)),
                (CodelistType::Integer, Value::float(1.5)),
                (CodelistType::Float, Value::Str("1".into())),
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
        use crate::support::specification::check_owned;
        use std::{cell::RefCell, convert::Infallible};
        use support::{environment, schema, spec};
        use yamaa_core::dataset::ExecutionError;
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

        /// A seam the replay cannot reach: the verification records an
        /// execution returns and the cells of a scripted table it read.
        #[test]
        fn owned_checked_environment_executes_fixed_codelists_before_later_cells_in_both_plan_forms(
        ) {
            for rows in [false, true] {
                let checked =
                    check_owned(&spec("SEX", "str", None, rows, None), environment(false)).unwrap();
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
                    &|| false,
                );
                let ExecutionError::VerificationFailures(records) = *attempt.result.unwrap_err()
                else {
                    panic!("fixed-list failure")
                };
                assert_eq!(records.len(), 1);
                let record = &records[0];
                assert_eq!(
                    (
                        record.definition().rule.unwrap(),
                        record.path.as_str(),
                        record.evaluated_count,
                        record.failed_count
                    ),
                    ("CT-7", "columns.CODE.submission.codelist", 2, 1)
                );
                assert_eq!(record.offending_rows[0].values, [Value::Int(i64::MAX - 1)]);
                assert_eq!(
                    record.codelist.as_ref().unwrap().values,
                    [Value::Str("é\0🙂".into())]
                );
                assert!(table.reads.borrow().iter().all(|&(_, column)| column < 2));
            }
        }
        /// A seam the replay cannot reach: the rows and check observations two
        /// executions of one plan return.
        #[test]
        fn missing_cells_pass_repeated_compiled_builds_and_keep_complete_check_observations() {
            let checked =
                check_owned(&spec("SEX", "str", None, false, None), environment(false)).unwrap();
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
                        &|| false,
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
        use crate::support::resolved;
        use crate::support::specification::prepare_in;
        use support::{environment, schema, spec};
        use yamaa_core::{dataset::Check, value::Value};
        /// A seam the replay cannot reach: the verification target and
        /// identity a preparation returns, and the codelist check the bound
        /// plan holds in either plan form. What a coded specification refuses
        /// is `codelist_binding.json`'s.
        #[test]
        fn fixed_column_checkpoint_is_compiled_for_ordinary_and_row_plans_without_activation() {
            for rows in [false, true] {
                let doc = spec("SEX", "str", None, rows, None);
                let env = environment(false);
                let prepared = prepare_in(&doc, &env).unwrap();
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
                let groups = resolved::column_verifications(&plan);
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0].0, 1);
                assert!(
                    matches!(groups[0].1[0],Check::Codelist {id,values} if id=="SEX" && values==&[Value::Str("F".into())])
                );
            }
        }
        /// A seam the replay cannot reach: the one codelist check of its
        /// listed values that an extensible catalogue binds, a value of the
        /// bound plan. That an allowed values verification beside it states the
        /// value set again is
        /// `codelist_binding.json::an_extensible_catalogue_states_the_value_set_once`.
        #[test]
        fn extensible_catalogue_is_enforced_as_its_listed_values() {
            let prepared =
                prepare_in(&spec("SEX", "str", None, false, None), &environment(true)).unwrap();
            let plan = prepared.bind(&schema()).unwrap();
            let groups = resolved::column_verifications(&plan);
            assert_eq!(groups[0].1.len(), 1);
            assert!(
                matches!(groups[0].1[0],Check::Codelist {id,values} if id=="SEX" && values==&[Value::Str("F".into())])
            );
        }
    }
    mod specification_project_functions {
        use crate::support::id_source_schema as source;
        use crate::support::project_call_compiler as support;
        use crate::support::resolved;
        use crate::support::specification::prepare_in;
        use support::{call, environment as with_functions, specification, Tree};
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{
            bound_expression::Read,
            dataset::{resolved::Op, FunctionInput},
            schema::DocumentNode as N,
            specification::PrepareError,
        };
        /// A seam the replay cannot reach: the functions the compiled
        /// specification calls and the callable the environment selects,
        /// values and not issues.
        ///
        /// A project's function files are language-neutral and read by every
        /// engine, so one that claims no host language compiles a call of a
        /// function with no callable for any host and misses no implementation
        /// (FUNC-1); a run of the call is refused instead (D110).
        #[test]
        fn an_engine_without_a_host_language_compiles_a_call_and_misses_no_implementation() {
            use crate::support::project_call_compiler::definition;
            use yamaa_core::{
                project_environment::{Draft, Environment},
                project_function::Language,
            };
            let spec = specification(vec![(
                "VALUE",
                "int",
                call("id", vec![("x", Tree::Scalar(N::Integer("7".into())))]),
            )]);
            let neutral = Environment::admit(
                None,
                Draft {
                    language: None,
                    lock: None,
                    functions: Some(vec![definition("id").file(&[(Language::R, "project::id")])]),
                    codelists: vec![],
                },
            )
            .unwrap()
            .into_execution();
            let compiled =
                prepare_in(&spec, &neutral).expect("a call compiles without a host language");
            assert_eq!(compiled.called_functions(), [0]);
            assert_eq!(neutral.functions()[0].call(), "");
        }

        /// A seam the replay cannot reach: the environment slot each call binds
        /// and the column its argument reads, in the resolved plan.
        #[test]
        fn compiler_selects_environment_slots_and_lowers_output_argument_dependencies() {
            let spec = specification(vec![
                ("A", "int", call("beta", vec![("x", Tree::text("ID"))])),
                ("B", "int", call("alpha", vec![("x", Tree::text("A"))])),
            ]);
            let prepared =
                prepare_in(&spec, &with_functions(&["alpha", "beta", "unused"])).unwrap();
            assert_eq!(prepared.called_functions(), [0, 1]);
            let plan = prepared.bind(&source()).unwrap();
            let columns = resolved::columns(&plan);
            assert_eq!(
                columns
                    .iter()
                    .map(|binding| binding.column.index())
                    .collect::<Vec<_>>(),
                [1, 2]
            );
            for (binding, slot, column) in [(columns[0], 1, 0), (columns[1], 0, 1)] {
                let Op::ProjectFunction(bound) = resolved::op(&plan, binding.expr) else {
                    panic!("versionless function expression")
                };
                assert_eq!(bound.slot(), slot);
                assert_eq!(
                    bound.arguments()[0].input,
                    FunctionInput::Read(Read::Column(column))
                );
            }
        }

        /// A seam the replay cannot reach: the resolved plan collects a stored
        /// field argument that is not a key. That a stored field of another
        /// type than its parameter is refused at binding is `projects.json`'s.
        #[test]
        fn nonkey_stored_field_arguments_use_collection() {
            let spec = specification(vec![(
                "A",
                "int",
                call("id", vec![("x", Tree::text("SRC.ID"))]),
            )]);
            let prepared = prepare_in(&spec, &with_functions(&["id"])).unwrap();
            let plan = prepared.bind(&source()).unwrap();
            let Op::ProjectFunction(bound) = resolved::op(&plan, resolved::columns(&plan)[0].expr)
            else {
                panic!("function")
            };
            assert_eq!(
                bound.arguments()[0].input,
                FunctionInput::Collect {
                    column: 0,
                    identifier: "SRC.ID".into()
                }
            );
        }

        /// A seam the case form cannot state: the table compared the first
        /// finding's condition and path and the second's `cause` alone. A
        /// case's `compares` applies to every row, so this single-call test
        /// keeps its mixed-field assertions. The separate three-call case,
        /// `projects.json::complete_static_faults_are_projected_in_authored_call_order_without_source_binding`,
        /// compares complete contexts, including this out-of-range cause.
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
            let Err(PrepareError::Invalid(findings)) = prepare_in(&spec, &with_functions(&["id"]))
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
