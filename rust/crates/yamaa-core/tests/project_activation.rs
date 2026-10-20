//! Project contracts.
#[allow(dead_code)]
mod support;
mod project {
    mod project_activation {
        use crate::support::roots::{Case, Definition, Parameter};
        use yamaa_core::{
            function_invocation::{Argument, HostError},
            project_activation::{
                activate, ActivatedFunction, ActivationPort, Failure, TestFailure,
            },
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{LockKind, LockReference},
            project_function::{Function, Language},
            table::ValueRef,
            value::{ColumnType, Value, ValueType},
        };
        #[derive(Debug, PartialEq)]
        struct Payload(&'static str);
        #[derive(Debug)]
        struct Handle(String);
        struct Port {
            trace: Vec<String>,
            lock_failure: bool,
            bind_failure: Option<String>,
            mismatch: bool,
            raised: bool,
            interrupt: bool,
            text_result: Option<String>,
        }
        impl Port {
            fn new() -> Self {
                Self {
                    trace: vec![],
                    lock_failure: false,
                    bind_failure: None,
                    mismatch: false,
                    raised: false,
                    interrupt: false,
                    text_result: None,
                }
            }
        }
        impl ActivationPort for Port {
            type Handle = Handle;
            type Error = Payload;
            fn verify_lock(
                &mut self,
                language: Language,
                lock: &LockReference,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Payload> {
                assert_eq!(language, Language::Python);
                assert_eq!(lock.written, "../uv.lock");
                assert!(functions
                    .iter()
                    .all(|f| f.call == format!("program.{}", f.name)));
                self.trace.push("lock:yamaa+called-packages".into());
                if self.lock_failure {
                    Err(Payload("version-mismatch"))
                } else {
                    Ok(())
                }
            }
            fn bind(
                &mut self,
                identity: &ProjectFunctionIdentity,
                signature: &LogicalSignature,
            ) -> Result<Handle, Payload> {
                self.trace.push(format!("bind:{}", identity.name));
                assert_eq!(signature.parameters()[0].host_name, "x");
                if self.bind_failure.as_ref() == Some(&identity.name)
                    || self.bind_failure.as_deref() == Some("*")
                {
                    Err(Payload("bad-signature"))
                } else {
                    Ok(Handle(identity.name.clone()))
                }
            }
            fn is_interrupt(&self, error: &Payload) -> bool {
                error.0 == "user interrupt"
            }
            fn invoke(
                &mut self,
                handle: &Handle,
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Payload>> {
                let [Argument {
                    name: "x",
                    value: ValueRef::Int(value),
                }] = args
                else {
                    panic!("exact ordered signature required");
                };
                self.trace.push(format!("call:{}:{value}", handle.0));
                if self.interrupt {
                    return Err(HostError::Raised(Payload("user interrupt")));
                }
                if self.raised {
                    return Err(HostError::Raised(Payload("opaque-host-failure")));
                }
                if let Some(text) = &self.text_result {
                    return Ok(Value::Str(text.clone()));
                }
                Ok(Value::Int(if self.mismatch { 8 } else { *value }))
            }
        }
        fn function(name: &str) -> Function {
            Function::admit(
                Language::Python,
                Definition {
                    name: name.into(),
                    function: format!("program.{name}"),
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
                            id: "missing-x".into(),
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
        fn lock() -> LockReference {
            LockReference {
                written: "../uv.lock".into(),
                kind: LockKind::Uv,
            }
        }

        #[test]
        fn absent_packaging_lock_still_binds_and_runs_every_conformance_case() {
            use yamaa_core::project_activation::{
                activate_references_observed, LockObservation, Observations,
            };
            let function = function("identity");
            let mut port = Port::new();
            port.lock_failure = true;
            let mut observations = Observations::default();
            let activated = activate_references_observed(
                Language::Python,
                None,
                &[&function],
                &mut port,
                &mut observations,
            )
            .unwrap();
            assert_eq!(activated.len(), 1);
            assert_eq!(activated[0].cases.len(), function.definition().tests.len());
            assert_eq!(observations.lock, LockObservation::NotRequested);
            assert_eq!(port.trace.first().unwrap(), "bind:identity");
            assert_eq!(observations.tests.len(), function.definition().tests.len());
            assert!(!port.trace.iter().any(|entry| entry.starts_with("lock:")));

            port.trace.clear();
            port.bind_failure = Some("identity".into());
            assert!(matches!(
                activate_references_observed(
                    Language::Python,
                    None,
                    &[&function],
                    &mut port,
                    &mut observations,
                ),
                Err(Failure::Bindings(_))
            ));
            assert_eq!(port.trace, ["bind:identity"]);
            assert_eq!(observations.lock, LockObservation::NotRequested);
            assert!(observations.tests.is_empty());

            port.bind_failure = None;
            port.mismatch = true;
            assert!(matches!(
                activate_references_observed(
                    Language::Python,
                    None,
                    &[&function],
                    &mut port,
                    &mut observations,
                ),
                Err(Failure::Tests(_))
            ));
        }

        fn build(
            functions: &[Function],
            port: &mut Port,
        ) -> Result<Vec<ActivatedFunction<Handle>>, Failure<Payload>> {
            let bindings = activate(Language::Python, &lock(), functions, port)?;
            port.trace.push("study.read".into());
            Ok(bindings)
        }
        #[test]
        fn every_successful_build_repeats_lock_binding_and_all_cases_before_study() {
            let functions = [function("identity")];
            let mut port = Port::new();
            for _ in 0..2 {
                let bindings = build(&functions, &mut port).unwrap();
                assert_eq!(
                    bindings[0]
                        .cases
                        .iter()
                        .map(|case| (case.id.as_str(), case.invoked))
                        .collect::<Vec<_>>(),
                    vec![("normal", true), ("boundary", true), ("missing-x", false)]
                );
                assert_eq!(bindings[0].cases[1].actual, Value::Int(i64::MIN));
            }
            let one = vec![
                "lock:yamaa+called-packages",
                "bind:identity",
                "call:identity:7",
                "call:identity:-9223372036854775808",
                "study.read",
            ];
            assert_eq!(
                port.trace,
                one.iter()
                    .chain(&one)
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>()
            );
        }
        #[test]
        fn lock_and_binding_failures_precede_code_tests_and_all_study_reads() {
            let functions = [function("first"), function("second")];
            let mut port = Port::new();
            port.lock_failure = true;
            assert!(matches!(
                build(&functions, &mut port),
                Err(Failure::Lock(Payload("version-mismatch")))
            ));
            assert_eq!(port.trace, vec!["lock:yamaa+called-packages"]);
            let mut port = Port::new();
            port.bind_failure = Some("second".into());
            let Err(Failure::Bindings(failures)) = build(&functions, &mut port) else {
                panic!("ordered binding failures before tests");
            };
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].function, 1);
            assert_eq!(failures[0].error, Payload("bad-signature"));
            assert_eq!(
                port.trace,
                vec!["lock:yamaa+called-packages", "bind:first", "bind:second"]
            );
        }
        #[test]
        fn all_mismatches_and_original_opaque_failures_collect_before_study() {
            let functions = [function("identity")];
            let mut port = Port::new();
            port.mismatch = true;
            let Err(Failure::Tests(failures)) = build(&functions, &mut port) else {
                panic!("all independent conformance failures");
            };
            assert_eq!(
                failures,
                vec![
                    TestFailure::Result {
                        function: 0,
                        case: 0,
                        actual: Value::Int(8),
                        expected: Value::Int(7)
                    },
                    TestFailure::Result {
                        function: 0,
                        case: 1,
                        actual: Value::Int(8),
                        expected: Value::Int(i64::MIN)
                    },
                ]
            );
            assert_eq!(
                port.trace,
                vec![
                    "lock:yamaa+called-packages",
                    "bind:identity",
                    "call:identity:7",
                    "call:identity:-9223372036854775808"
                ]
            );
            let mut port = Port::new();
            port.raised = true;
            let Err(Failure::Tests(failures)) = build(&functions, &mut port) else {
                panic!("retain every opaque host error");
            };
            assert_eq!(failures.len(), 2);
            for (case, failure) in failures.into_iter().enumerate() {
                let TestFailure::Invocation {
                    function: 0,
                    case: actual_case,
                    error,
                } = failure
                else {
                    panic!("original invocation facts");
                };
                assert_eq!(actual_case, case);
                assert_eq!(
                    error.kind,
                    yamaa_core::function_invocation::FailureKind::CallFailed(Payload(
                        "opaque-host-failure"
                    ))
                );
            }
            assert!(!port.trace.iter().any(|event| event == "study.read"));
        }
        #[test]
        fn every_called_function_runs_every_case_even_after_ordinary_test_failure() {
            let functions = [function("first"), function("second")];
            let mut port = Port::new();
            port.mismatch = true;
            for _ in 0..2 {
                let Err(Failure::Tests(failures)) = build(&functions, &mut port) else {
                    panic!("failed activation");
                };
                assert_eq!(
                    failures
                        .into_iter()
                        .map(|failure| match failure {
                            TestFailure::Result { function, case, .. } => (function, case),
                            _ => panic!("exact result mismatch"),
                        })
                        .collect::<Vec<_>>(),
                    vec![(0, 0), (0, 1), (1, 0), (1, 1)]
                );
            }
            let one = vec![
                "lock:yamaa+called-packages",
                "bind:first",
                "bind:second",
                "call:first:7",
                "call:first:-9223372036854775808",
                "call:second:7",
                "call:second:-9223372036854775808",
            ];
            assert_eq!(
                port.trace,
                one.iter()
                    .chain(&one)
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>()
            );
        }
        #[test]
        fn all_binding_failures_collect_without_any_test_or_study_invocation() {
            let functions = [function("first"), function("second"), function("third")];
            let mut port = Port::new();
            port.bind_failure = Some("*".into());
            let Err(Failure::Bindings(failures)) = build(&functions, &mut port) else {
                panic!("complete binding failures");
            };
            assert_eq!(
                failures
                    .iter()
                    .map(|failure| failure.function)
                    .collect::<Vec<_>>(),
                vec![0, 1, 2]
            );
            assert!(failures
                .iter()
                .all(|failure| failure.error == Payload("bad-signature")));
            assert_eq!(
                port.trace,
                vec![
                    "lock:yamaa+called-packages",
                    "bind:first",
                    "bind:second",
                    "bind:third"
                ]
            );
        }
        #[test]
        fn user_interrupt_preserves_original_payload_and_stops_remaining_cases_and_functions() {
            let functions = [function("first"), function("second")];
            let mut port = Port::new();
            port.interrupt = true;
            let Err(Failure::InterruptedInvocation {
                function: 0,
                case: 0,
                error,
            }) = build(&functions, &mut port)
            else {
                panic!("immediate original interrupt");
            };
            assert_eq!(
                error.kind,
                yamaa_core::function_invocation::FailureKind::CallFailed(Payload("user interrupt"))
            );
            assert_eq!(
                port.trace,
                vec![
                    "lock:yamaa+called-packages",
                    "bind:first",
                    "bind:second",
                    "call:first:7"
                ]
            );
        }
        #[test]
        fn no_calls_do_no_activation_work_and_duplicate_selection_has_no_effects() {
            let mut port = Port::new();
            assert!(activate(Language::Python, &lock(), &[], &mut port)
                .unwrap()
                .is_empty());
            assert!(port.trace.is_empty());
            let functions = [function("duplicate"), function("duplicate")];
            assert!(matches!(
                activate(Language::Python, &lock(), &functions, &mut port),
                Err(Failure::DuplicateSelection { function: 1 })
            ));
            assert!(port.trace.is_empty());
        }

        #[test]
        fn language_and_lock_kind_cannot_escape_static_admission_before_any_port() {
            let functions = [function("identity")];
            let mut port = Port::new();
            let wrong_lock = LockReference {
                written: "../renv.lock".into(),
                kind: LockKind::Renv,
            };
            assert!(matches!(
                activate(Language::Python, &wrong_lock, &functions, &mut port),
                Err(Failure::LockKindMismatch {
                    language: Language::Python,
                    actual: LockKind::Renv
                })
            ));
            let mut definition = function("ridentity").definition().clone();
            definition.implementations = vec![yamaa_core::ast::Implementation {
                language: Language::R,
                callable: crate::support::roots::text("program::ridentity"),
            }];
            let rfunction = Function::admit(Language::R, definition).unwrap();
            assert_eq!(rfunction.language(), Language::R);
            assert!(matches!(
                activate(Language::Python, &lock(), &[rfunction], &mut port),
                Err(Failure::FunctionLanguageMismatch {
                    function: 0,
                    declared: Language::R,
                    selected: Language::Python
                })
            ));
            assert!(port.trace.is_empty());
        }

        #[test]
        fn borrowed_compiler_selection_activates_only_held_definitions_and_repeats_cases() {
            use yamaa_core::project_activation::activate_references;
            let environment = [
                function("first"),
                function("second"),
                function("third"),
                function("fourth"),
            ];
            let selected = [&environment[1], &environment[3]];
            let mut port = Port::new();
            port.bind_failure = Some("first".into());
            for _ in 0..2 {
                let activated =
                    activate_references(Language::Python, Some(&lock()), &selected, &mut port)
                        .unwrap();
                assert_eq!(
                    activated
                        .iter()
                        .map(|f| f.plan.identity().name.as_str())
                        .collect::<Vec<_>>(),
                    ["second", "fourth"]
                );
                assert!(activated.iter().all(|f| f.cases.len() == 3));
                assert_eq!(activated[0].cases[0].actual, Value::Int(7));
                assert_eq!(activated[1].cases[1].actual, Value::Int(i64::MIN));
                assert_eq!(activated[1].cases[2].actual, Value::Missing);
            }
            let once = [
                "lock:yamaa+called-packages",
                "bind:second",
                "bind:fourth",
                "call:second:7",
                "call:second:-9223372036854775808",
                "call:fourth:7",
                "call:fourth:-9223372036854775808",
            ];
            assert_eq!(
                port.trace,
                once.into_iter()
                    .chain(once)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            );
            port.trace.clear();
            port.bind_failure = Some("second".into());
            let error = activate_references(Language::Python, Some(&lock()), &selected, &mut port)
                .unwrap_err();
            assert_eq!(
                error,
                Failure::Bindings(vec![yamaa_core::project_activation::BindingFailure {
                    function: 0,
                    error: Payload("bad-signature")
                }])
            );
            assert_eq!(
                port.trace,
                ["lock:yamaa+called-packages", "bind:second", "bind:fourth"]
            );
        }

        #[test]
        fn borrowed_selection_keeps_empty_and_duplicate_gates_before_ports() {
            use yamaa_core::project_activation::activate_references;
            let environment = [function("first"), function("second")];
            let mut port = Port::new();
            port.lock_failure = true;
            assert!(
                activate_references(Language::Python, Some(&lock()), &[], &mut port)
                    .unwrap()
                    .is_empty()
            );
            assert!(port.trace.is_empty());
            assert_eq!(
                activate_references(
                    Language::Python,
                    Some(&lock()),
                    &[&environment[0], &environment[0]],
                    &mut port
                )
                .unwrap_err(),
                Failure::DuplicateSelection { function: 1 }
            );
            assert!(port.trace.is_empty());
        }

        mod observed {
            use super::*;
            use yamaa_core::project_activation::{
                activate_references_observed, BindingObservation, BindingOutcome, LockObservation,
                Observations, TestObservation, TestOutcome,
            };
            struct ObservedPort {
                inner: Port,
                late_failure: u8,
                panic_stage: &'static str,
            }
            impl ObservedPort {
                fn new() -> Self {
                    Self {
                        inner: Port::new(),
                        late_failure: 0,
                        panic_stage: "",
                    }
                }
            }
            impl ActivationPort for ObservedPort {
                type Handle = Handle;
                type Error = Payload;
                fn verify_lock(
                    &mut self,
                    language: Language,
                    lock: &LockReference,
                    functions: &[ProjectFunctionIdentity],
                ) -> Result<(), Payload> {
                    assert_ne!(self.panic_stage, "lock", "original lock panic");
                    if self.late_failure == 4 {
                        return Err(Payload("user interrupt"));
                    }
                    self.inner.verify_lock(language, lock, functions)
                }
                fn bind(
                    &mut self,
                    identity: &ProjectFunctionIdentity,
                    signature: &LogicalSignature,
                ) -> Result<Handle, Payload> {
                    if identity.name == "second" {
                        assert_ne!(self.panic_stage, "bind", "original binding panic");
                        if self.late_failure == 3 {
                            return Err(Payload("user interrupt"));
                        }
                    }
                    self.inner.bind(identity, signature)
                }
                fn is_interrupt(&self, error: &Payload) -> bool {
                    self.inner.is_interrupt(error)
                }
                fn invoke(
                    &mut self,
                    handle: &Handle,
                    args: &[Argument<'_>],
                ) -> Result<Value, HostError<Payload>> {
                    if handle.0 == "second" {
                        assert_ne!(self.panic_stage, "test", "original callback panic");
                        if self.late_failure == 2 {
                            return Err(HostError::Raised(Payload("user interrupt")));
                        }
                        if self.late_failure == 1 && args[0].value == ValueRef::Int(i64::MIN) {
                            return Ok(Value::Int(8));
                        }
                    }
                    self.inner.invoke(handle, args)
                }
            }
            fn run(
                functions: &[Function],
                port: &mut ObservedPort,
                observations: &mut Observations,
            ) -> Result<Vec<ActivatedFunction<Handle>>, Failure<Payload>> {
                activate_references_observed(
                    Language::Python,
                    Some(&lock()),
                    &functions.iter().collect::<Vec<_>>(),
                    port,
                    observations,
                )
            }
            #[test]
            fn later_case_failure_keeps_prior_successes_and_all_short_circuit_observations() {
                let functions = [function("first"), function("second")];
                let mut port = ObservedPort::new();
                port.late_failure = 1;
                let mut observations = Observations::default();
                assert_eq!(
                    run(&functions, &mut port, &mut observations).unwrap_err(),
                    Failure::Tests(vec![TestFailure::Result {
                        function: 1,
                        case: 1,
                        actual: Value::Int(8),
                        expected: Value::Int(i64::MIN)
                    }])
                );
                assert_eq!(observations.lock, LockObservation::Verified);
                assert_eq!(
                    observations.bindings,
                    vec![
                        BindingObservation {
                            function: 0,
                            outcome: BindingOutcome::Bound
                        },
                        BindingObservation {
                            function: 1,
                            outcome: BindingOutcome::Bound
                        }
                    ]
                );
                let expected = [
                    (0, 0, TestOutcome::Passed, true, Value::Int(7)),
                    (0, 1, TestOutcome::Passed, true, Value::Int(i64::MIN)),
                    (0, 2, TestOutcome::Passed, false, Value::Missing),
                    (1, 0, TestOutcome::Passed, true, Value::Int(7)),
                    (1, 1, TestOutcome::ResultMismatch, true, Value::Int(8)),
                    (1, 2, TestOutcome::Passed, false, Value::Missing),
                ];
                assert_eq!(
                    observations.tests,
                    expected
                        .into_iter()
                        .map(
                            |(function, case, outcome, invoked, actual)| TestObservation {
                                function,
                                case,
                                outcome,
                                invoked,
                                actual: Some(actual)
                            }
                        )
                        .collect::<Vec<_>>()
                );
                assert_eq!(port.inner.trace.len(), 6); // lock, two bindings, three successful delegations
            }
            #[test]
            fn binding_failures_keep_successful_bindings_without_inventing_test_records() {
                let functions = [function("first"), function("second"), function("third")];
                let mut port = ObservedPort::new();
                port.inner.bind_failure = Some("second".into());
                let mut observations = Observations::default();
                assert!(matches!(
                    run(&functions, &mut port, &mut observations),
                    Err(Failure::Bindings(_))
                ));
                assert_eq!(
                    observations
                        .bindings
                        .iter()
                        .map(|b| (b.function, b.outcome))
                        .collect::<Vec<_>>(),
                    [
                        (0, BindingOutcome::Bound),
                        (1, BindingOutcome::Rejected),
                        (2, BindingOutcome::Bound)
                    ]
                );
                assert!(observations.tests.is_empty());
                assert_eq!(port.inner.trace.len(), 4);
            }
            #[test]
            fn original_interrupt_keeps_completed_cases_and_stops_future_observations() {
                let functions = [function("first"), function("second")];
                let mut port = ObservedPort::new();
                port.late_failure = 2;
                let mut observations = Observations::default();
                let Err(Failure::InterruptedInvocation {
                    function: 1,
                    case: 0,
                    error,
                }) = run(&functions, &mut port, &mut observations)
                else {
                    panic!("original interrupt");
                };
                assert_eq!(
                    error.kind,
                    yamaa_core::function_invocation::FailureKind::CallFailed(Payload(
                        "user interrupt"
                    ))
                );
                assert_eq!(observations.tests.len(), 4);
                assert_eq!(
                    observations.tests[3],
                    TestObservation {
                        function: 1,
                        case: 0,
                        outcome: TestOutcome::Interrupted,
                        invoked: true,
                        actual: None
                    }
                );
                assert!(observations.tests[..3]
                    .iter()
                    .all(|case| case.outcome == TestOutcome::Passed));
            }
            #[test]
            fn ordinary_opaque_invocation_failures_collect_with_zero_invented_results() {
                let functions = [function("first")];
                let mut port = ObservedPort::new();
                port.inner.raised = true;
                let mut observations = Observations::default();
                assert!(matches!(
                    run(&functions, &mut port, &mut observations),
                    Err(Failure::Tests(_))
                ));
                assert_eq!(
                    observations
                        .tests
                        .iter()
                        .map(|t| (t.outcome, t.invoked, t.actual.as_ref()))
                        .collect::<Vec<_>>(),
                    [
                        (TestOutcome::InvocationFailed, true, None),
                        (TestOutcome::InvocationFailed, true, None),
                        (TestOutcome::Passed, false, Some(&Value::Missing))
                    ]
                );
            }
            #[test]
            fn lock_and_binding_interrupts_keep_original_errors_and_only_attempted_bindings() {
                let functions = [function("first"), function("second"), function("third")];
                for mode in [3, 4] {
                    let mut port = ObservedPort::new();
                    port.late_failure = mode;
                    let mut observations = Observations::default();
                    let failure = run(&functions, &mut port, &mut observations).unwrap_err();
                    if mode == 4 {
                        assert_eq!(failure, Failure::Lock(Payload("user interrupt")));
                        assert_eq!(observations.lock, LockObservation::Interrupted);
                        assert!(observations.bindings.is_empty());
                    } else {
                        assert_eq!(
                            failure,
                            Failure::InterruptedBinding {
                                function: 1,
                                error: Payload("user interrupt")
                            }
                        );
                        assert_eq!(
                            observations.bindings,
                            [
                                BindingObservation {
                                    function: 0,
                                    outcome: BindingOutcome::Bound
                                },
                                BindingObservation {
                                    function: 1,
                                    outcome: BindingOutcome::Interrupted
                                }
                            ]
                        );
                    }
                    assert!(observations.tests.is_empty());
                }
            }
            #[test]
            fn boundary_unwind_keeps_attempted_stage_and_actual_callback_entry() {
                use std::panic::{catch_unwind, AssertUnwindSafe};
                let functions = [function("first"), function("second")];
                for stage in ["lock", "bind", "test"] {
                    let mut port = ObservedPort::new();
                    port.panic_stage = stage;
                    let mut observations = Observations::default();
                    assert!(catch_unwind(AssertUnwindSafe(|| run(
                        &functions,
                        &mut port,
                        &mut observations
                    )))
                    .is_err());
                    match stage {
                        "lock" => {
                            assert_eq!(observations.lock, LockObservation::Attempted);
                            assert!(observations.bindings.is_empty());
                            assert!(observations.tests.is_empty());
                        }
                        "bind" => {
                            assert_eq!(observations.lock, LockObservation::Verified);
                            assert_eq!(observations.bindings[1].outcome, BindingOutcome::Attempted);
                            assert!(observations.tests.is_empty());
                        }
                        "test" => {
                            assert_eq!(observations.tests.len(), 4);
                            assert_eq!(
                                observations.tests[3],
                                TestObservation {
                                    function: 1,
                                    case: 0,
                                    outcome: TestOutcome::Attempted,
                                    invoked: true,
                                    actual: None
                                }
                            );
                        }
                        _ => unreachable!(),
                    }
                }
            }

            #[test]
            fn repeated_lock_refusal_and_empty_selection_clear_previous_evidence() {
                let functions = [function("first")];
                let mut port = ObservedPort::new();
                let mut observations = Observations::default();
                run(&functions, &mut port, &mut observations).unwrap();
                port.inner.lock_failure = true;
                assert!(matches!(
                    run(&functions, &mut port, &mut observations),
                    Err(Failure::Lock(_))
                ));
                assert_eq!(
                    observations,
                    Observations {
                        lock: LockObservation::Rejected,
                        ..Observations::default()
                    }
                );
                let before = port.inner.trace.len();
                assert!(run(&[], &mut port, &mut observations).unwrap().is_empty());
                assert_eq!(observations, Observations::default());
                assert_eq!(port.inner.trace.len(), before);
            }
        }
    }
    mod project_call_binding {
        use crate::support::roots::{Case, Definition, Parameter};
        use yamaa_core::{
            bound_expression::Read,
            dataset::FunctionInput,
            project_call_binding::{bind, Context, Error, Kind, SourceMode},
            project_call_document::{Argument, Call, Input},
            project_calls::{LocatedCall, ProjectCalls},
            project_function::{Function, Language},
            reference_binding::{Catalog, Dataset, Diagnostic, Field},
            reference_scope::{Finding as ScopeFinding, Phase, Reach, Scope},
            value::{ColumnType, Value, ValueType},
        };
        fn prepared(input: Input) -> ProjectCalls {
            let function = Function::admit(
                Language::Python,
                Definition {
                    name: "id".into(),
                    function: "project.identity".into(),
                    description: "Integer identity.".into(),
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
            .unwrap();
            ProjectCalls::admit(
                &[function],
                vec![LocatedCall {
                    path: "columns.OUT.function".into(),
                    call: Call {
                        name: "id".into(),
                        name_node: 1,
                        node: 2,
                        arguments: vec![Argument {
                            name: "x".into(),
                            node: 3,
                            input,
                        }],
                    },
                }],
            )
            .unwrap()
        }
        fn catalog(kind: ColumnType) -> Catalog {
            Catalog::compile(
                &[Field {
                    name: "OUT",
                    column_type: ColumnType::Int,
                }],
                &[Dataset {
                    name: "SRC",
                    fields: &[
                        Field {
                            name: "ID",
                            column_type: ColumnType::Int,
                        },
                        Field {
                            name: "X",
                            column_type: kind,
                        },
                    ],
                }],
            )
            .unwrap()
        }
        fn column_context(mode: SourceMode) -> Context<'static> {
            Context {
                source_dataset: 0,
                source_mode: mode,
                available_outputs: None,
                scope: Scope {
                    drivers: &["SRC"],
                    current_driver: true,
                    reach: Reach::Relation,
                    joined: false,
                    phase: Phase::Column { groups: &[] },
                },
            }
        }

        #[test]
        fn stored_arguments_collect_for_key_grain_values_and_read_for_records() {
            let calls = prepared(Input::Reference("SRC.X".into()));
            let catalog = catalog(ColumnType::Int);
            let collected = bind(&calls, 0, &catalog, column_context(SourceMode::Collect)).unwrap();
            assert!(collected.dependencies.is_empty());
            assert_eq!(collected.function.slot(), 0);
            assert_eq!(
                collected.function.signature().identity().call,
                "project.identity"
            );
            assert_eq!(
                collected.function.arguments()[0].input,
                FunctionInput::Collect {
                    column: 1,
                    identifier: "SRC.X".into()
                }
            );
            let record = bind(&calls, 0, &catalog, column_context(SourceMode::Record)).unwrap();
            assert_eq!(
                record.function.arguments()[0].input,
                FunctionInput::Read(Read::Source(1))
            );
        }

        #[test]
        fn bare_output_arguments_create_dependencies_and_respect_row_phase_availability() {
            let calls = prepared(Input::Reference("OUT".into()));
            let catalog = catalog(ColumnType::Int);
            let bound = bind(&calls, 0, &catalog, column_context(SourceMode::Collect)).unwrap();
            assert_eq!(bound.dependencies, [0]);
            assert_eq!(
                bound.function.arguments()[0].input,
                FunctionInput::Read(Read::Column(0))
            );
            let context = Context {
                available_outputs: Some(&[]),
                ..column_context(SourceMode::Record)
            };
            let Err(Error::Findings(findings)) = bind(&calls, 0, &catalog, context) else {
                panic!("phase finding")
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings[0].kind,
                Kind::Output(Diagnostic::PhaseBoundary { column: 0 })
            );
        }

        #[test]
        fn grouping_fault_keeps_independent_exact_type_fault_for_the_same_argument() {
            let calls = prepared(Input::Reference("SRC.X".into()));
            let catalog = catalog(ColumnType::Float);
            let context = Context {
                source_dataset: 0,
                source_mode: SourceMode::Record,
                available_outputs: Some(&[]),
                scope: Scope {
                    drivers: &["SRC"],
                    current_driver: true,
                    reach: Reach::Scalar,
                    joined: false,
                    phase: Phase::Row {
                        group_by: Some(&["SRC.ID"]),
                    },
                },
            };
            let Err(Error::Findings(findings)) = bind(&calls, 0, &catalog, context) else {
                panic!("complete independent findings")
            };
            assert_eq!(
                findings.iter().map(|f| f.argument).collect::<Vec<_>>(),
                [0, 0]
            );
            assert_eq!(
                findings.iter().map(|f| f.kind.clone()).collect::<Vec<_>>(),
                [
                    Kind::Scope(ScopeFinding::RowGroup),
                    Kind::ArgumentType {
                        expected: ValueType::Int,
                        actual: ValueType::Float
                    },
                ]
            );
        }

        #[test]
        fn unresolved_bare_and_wrong_driver_references_keep_existing_diagnostic_priority() {
            let catalog = catalog(ColumnType::Float);
            let calls = prepared(Input::Reference("X".into()));
            let Err(Error::Findings(findings)) =
                bind(&calls, 0, &catalog, column_context(SourceMode::Collect))
            else {
                panic!("bare reference finding")
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings[0].kind,
                Kind::Output(Diagnostic::UnresolvableName { dataset: 0 })
            );
            let calls = prepared(Input::Reference("OTHER.X".into()));
            let Err(Error::Findings(findings)) =
                bind(&calls, 0, &catalog, column_context(SourceMode::Collect))
            else {
                panic!("driver finding")
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].kind, Kind::Scope(ScopeFinding::DriverMismatch));
        }

        #[test]
        fn missing_and_full_width_literals_need_no_reference_bindings() {
            let empty = Catalog::compile(&[], &[]).unwrap();
            for value in [Value::Missing, Value::Int(i64::MIN), Value::Int(i64::MAX)] {
                let calls = prepared(Input::Literal(value.clone()));
                let bound = bind(&calls, 0, &empty, column_context(SourceMode::Record)).unwrap();
                assert_eq!(
                    bound.function.arguments()[0].input,
                    FunctionInput::Literal(value)
                );
                assert!(bound.dependencies.is_empty());
            }
            assert!(matches!(
                bind(
                    &prepared(Input::Literal(Value::Int(1))),
                    1,
                    &empty,
                    column_context(SourceMode::Record)
                ),
                Err(Error::InvalidCallIndex)
            ));
        }
    }
    mod project_call_document {
        use crate::support::roots::{Case, Definition, Parameter};
        use std::collections::BTreeMap;
        use yamaa_core::{
            project_call_document::{decode, BindingFinding, Input},
            project_function::{CallFinding, Function, Language},
            project_function_document::{Finding, Kind},
            schema::{Document, DocumentNode as N},
            value::{ColumnType, Value, ValueType},
        };

        use crate::support::document::Tree;

        fn call_tree(name: &str, args: Vec<(&str, Tree)>) -> Tree {
            Tree::map(vec![("name", Tree::text(name)), ("args", Tree::map(args))])
        }
        fn document(tree: Tree) -> Document {
            tree.document()
        }
        fn integer_function() -> Function {
            Function::admit(
                Language::Python,
                Definition {
                    name: "id".into(),
                    function: "project.identity".into(),
                    description: "Return the integer.".into(),
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
                            id: "ordinary".into(),
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

        #[test]
        fn references_and_explicit_literals_retain_exact_kinds_order_and_origins() {
            let d = document(call_tree(
                "id",
                vec![
                    ("reference", Tree::text("SRC.X")),
                    (
                        "text",
                        Tree::map(vec![("literal", Tree::text("e\u{301}\0😀"))]),
                    ),
                    (
                        "integer",
                        Tree::Scalar(N::Integer("9223372036854775807".into())),
                    ),
                    ("zero", Tree::Scalar(N::Float(-0.0))),
                    ("boolean", Tree::Scalar(N::Boolean(true))),
                    ("missing", Tree::Scalar(N::Null)),
                    ("date", Tree::map(vec![("date", Tree::text("2024-02-29"))])),
                    (
                        "datetime",
                        Tree::map(vec![("datetime", Tree::text("2024-02-29T12:34:56"))]),
                    ),
                ],
            ));
            let c = decode(&d, d.root(), "columns.VALUE.function").unwrap();
            assert_eq!(c.name, "id");
            assert_eq!(c.name_node, d.field(d.root(), "name").unwrap());
            assert_eq!(c.node, d.root());
            assert_eq!(
                c.arguments
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>(),
                [
                    "reference",
                    "text",
                    "integer",
                    "zero",
                    "boolean",
                    "missing",
                    "date",
                    "datetime"
                ]
            );
            assert_eq!(c.arguments[0].input, Input::Reference("SRC.X".into()));
            assert_eq!(
                c.arguments[1].input,
                Input::Literal(Value::Str("e\u{301}\0😀".into()))
            );
            assert_eq!(c.arguments[2].input, Input::Literal(Value::Int(i64::MAX)));
            let Input::Literal(Value::Float(zero)) = c.arguments[3].input else {
                panic!("float kind")
            };
            assert_eq!(zero.get().to_bits(), (-0.0_f64).to_bits());
            assert_eq!(c.arguments[4].input, Input::Literal(Value::Bool(true)));
            assert_eq!(c.arguments[5].input, Input::Literal(Value::Missing));
            assert_eq!(
                c.arguments[6].input,
                Input::Literal(Value::Date("2024-02-29".parse().unwrap()))
            );
            assert_eq!(
                c.arguments[7].input,
                Input::Literal(Value::DateTime("2024-02-29T12:34:56".parse().unwrap()))
            );
            let args = d.field(d.root(), "args").unwrap();
            for argument in c.arguments {
                assert_eq!(argument.node, d.field(args, &argument.name).unwrap());
            }
        }

        #[test]
        fn original_tagged_temporal_literals_keep_kind_and_calendar_admission() {
            // These are the six original validator operands, now observed directly
            // through the shared tagged function-argument decoder instead of a retired
            // Python function_value_type helper or a CSV scalar conversion.
            for (tag, text, expected_kind) in [
                ("date", "2025-01-15", Some(ValueType::Date)),
                ("date", "2025-02-30", None),
                ("datetime", "2025-01-15T03:04", Some(ValueType::DateTime)),
                ("datetime", "2025-01-15T03:04:05", Some(ValueType::DateTime)),
                ("datetime", "2025-01-15T03:04:60", None),
                ("datetime", "2025-02-30T03:04", None),
            ] {
                let d = document(call_tree(
                    "id",
                    vec![("x", Tree::map(vec![(tag, Tree::text(text))]))],
                ));
                let actual = decode(&d, d.root(), "call");
                if let Some(expected_kind) = expected_kind {
                    let call = actual.unwrap();
                    let Input::Literal(value) = &call.arguments[0].input else {
                        panic!("{tag}: {text}: tagged literal became a reference");
                    };
                    assert_eq!(value.value_type(), Some(expected_kind), "{tag}: {text}");
                } else {
                    let argument = d.field(d.field(d.root(), "args").unwrap(), "x").unwrap();
                    assert_eq!(
                        actual,
                        Err(vec![Finding {
                            path: "call.args.x".into(),
                            node: argument,
                            kind: if tag == "date" {
                                Kind::InvalidDate
                            } else {
                                Kind::InvalidDateTime
                            },
                            value: Some(text.into()),
                        }]),
                        "{tag}: {text}"
                    );
                }
            }
        }

        #[test]
        fn independent_invalid_scalars_keep_all_authored_nodes_without_partial_calls() {
            let d = document(call_tree(
                "id",
                vec![
                    (
                        "wide",
                        Tree::Scalar(N::Integer("9223372036854775808".into())),
                    ),
                    ("date", Tree::map(vec![("date", Tree::text("2023-02-29"))])),
                    (
                        "datetime",
                        Tree::map(vec![("datetime", Tree::Scalar(N::Boolean(false)))]),
                    ),
                ],
            ));
            let args = d.field(d.root(), "args").unwrap();
            let wide = d.field(args, "wide").unwrap();
            let date = d.field(args, "date").unwrap();
            let datetime = d.field(args, "datetime").unwrap();
            assert_eq!(
                decode(&d, d.root(), "call"),
                Err(vec![
                    Finding {
                        path: "call.args.wide".into(),
                        node: wide,
                        kind: Kind::IntegerRange,
                        value: Some("9223372036854775808".into()),
                    },
                    Finding {
                        path: "call.args.date".into(),
                        node: date,
                        kind: Kind::InvalidDate,
                        value: Some("2023-02-29".into()),
                    },
                    Finding {
                        path: "call.args.datetime".into(),
                        node: d.field(datetime, "datetime").unwrap(),
                        kind: Kind::NormalizedShape,
                        value: None,
                    },
                    // A datetime written as no text keeps no written value.
                    Finding {
                        path: "call.args.datetime".into(),
                        node: datetime,
                        kind: Kind::InvalidDateTime,
                        value: None,
                    },
                ])
            );
        }

        #[test]
        fn metadata_checks_all_reference_types_after_explicit_missing() {
            let d = document(call_tree(
                "id",
                vec![
                    ("x", Tree::Scalar(N::Null)),
                    ("other", Tree::text("UNKNOWN")),
                ],
            ));
            let c = decode(&d, d.root(), "call").unwrap();
            assert_eq!(
                c.validate(&integer_function(), &BTreeMap::new()),
                vec![
                    BindingFinding::UnknownReference { argument: 1 },
                    BindingFinding::Argument(CallFinding::UnknownArgument { argument: 1 }),
                ]
            );
            let d = document(call_tree("id", vec![("x", Tree::text("SRC.X"))]));
            let c = decode(&d, d.root(), "call").unwrap();
            let mut refs = BTreeMap::from([("SRC.X".into(), ValueType::Float)]);
            assert_eq!(
                c.validate(&integer_function(), &refs),
                vec![BindingFinding::Argument(CallFinding::ArgumentType {
                    argument: 0,
                    expected: ValueType::Int,
                    actual: ValueType::Float
                })]
            );
            refs.insert("SRC.X".into(), ValueType::Int);
            assert!(c.validate(&integer_function(), &refs).is_empty());
            let d = document(call_tree("other", vec![]));
            assert_eq!(
                decode(&d, d.root(), "call")
                    .unwrap()
                    .validate(&integer_function(), &refs),
                vec![BindingFinding::FunctionNameMismatch]
            );
        }

        #[test]
        fn retired_call_fields_and_non_text_literal_wrappers_never_enter_the_model() {
            let d = document(Tree::map(vec![
                ("name", Tree::text("id")),
                ("contract_version", Tree::text("1")),
                (
                    "args",
                    Tree::map(vec![(
                        "x",
                        Tree::map(vec![("literal", Tree::Scalar(N::Integer("7".into())))]),
                    )]),
                ),
            ]));
            let args = d.field(d.root(), "args").unwrap();
            let x = d.field(args, "x").unwrap();
            assert_eq!(
                decode(&d, d.root(), "call"),
                Err(vec![
                    Finding {
                        path: "call".into(),
                        node: d.root(),
                        kind: Kind::NormalizedShape,
                        value: None,
                    },
                    Finding {
                        path: "call.args.x".into(),
                        node: d.field(x, "literal").unwrap(),
                        kind: Kind::NormalizedShape,
                        value: None,
                    },
                ])
            );
            let d = document(call_tree(
                "id",
                vec![(
                    "x",
                    Tree::map(vec![
                        ("date", Tree::text("2024-02-29")),
                        ("datetime", Tree::text("2024-02-29T12:34:56")),
                    ]),
                )],
            ));
            let x = d.field(d.field(d.root(), "args").unwrap(), "x").unwrap();
            assert_eq!(
                decode(&d, d.root(), "call"),
                Err(vec![Finding {
                    path: "call.args.x".into(),
                    node: x,
                    kind: Kind::NormalizedShape,
                    value: None,
                },])
            );
        }
    }
    mod project_calls {
        use crate::support::roots::{Case, Definition, Parameter};
        use yamaa_core::{
            project_call_document::{Argument, Call, Input},
            project_calls::{Error, Finding, Kind, LocatedCall, ProjectCalls},
            project_function::{CallFinding, Function, Language},
            value::{ColumnType, Value, ValueType},
        };
        fn function(name: &str) -> Function {
            Function::admit(
                Language::Python,
                Definition {
                    name: name.into(),
                    function: format!("project.{name}"),
                    description: "Integer identity.".into(),
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
        fn call(name: &str, args: Vec<(&str, Input)>) -> LocatedCall {
            LocatedCall {
                path: format!("columns.{name}.function"),
                call: Call {
                    name: name.into(),
                    name_node: 5,
                    node: 9,
                    arguments: args
                        .into_iter()
                        .enumerate()
                        .map(|(node, (name, input))| Argument {
                            name: name.into(),
                            node,
                            input,
                        })
                        .collect(),
                },
            }
        }
        fn ordinary(name: &str) -> LocatedCall {
            call(name, vec![("x", Input::Literal(Value::Int(7)))])
        }

        #[test]
        fn slots_follow_environment_order_and_occurrences_keep_their_authored_order() {
            let functions = vec![function("alpha"), function("beta"), function("unused")];
            let calls = vec![ordinary("beta"), ordinary("alpha"), ordinary("beta")];
            let expected = calls.clone();
            let compiled = ProjectCalls::admit(&functions, calls).unwrap();
            assert_eq!(compiled.selected(), [0, 1]);
            assert_eq!(
                compiled
                    .plans()
                    .iter()
                    .map(|p| p.identity().name.as_str())
                    .collect::<Vec<_>>(),
                ["alpha", "beta"]
            );
            assert_eq!(
                compiled
                    .calls()
                    .iter()
                    .map(|c| c.slot())
                    .collect::<Vec<_>>(),
                [Some(1), Some(0), Some(1)]
            );
            assert_eq!(
                compiled
                    .calls()
                    .iter()
                    .map(|c| c.located.clone())
                    .collect::<Vec<_>>(),
                expected
            );
            for c in compiled.calls() {
                assert_eq!(
                    compiled.plans()[c.slot().unwrap()].identity().name,
                    c.located.call.name
                );
            }
        }

        #[test]
        fn all_unknown_name_and_independent_argument_faults_are_retained() {
            let functions = vec![function("id")];
            let calls = vec![
                ordinary("unknown"),
                call("id", vec![("x", Input::Literal(Value::float(7.0)))]),
                call("id", vec![("other", Input::Reference("SRC.X".into()))]),
            ];
            let Err(Error::Findings(findings)) = ProjectCalls::admit(&functions, calls) else {
                panic!("complete static findings")
            };
            assert_eq!(
                findings,
                vec![
                    Finding {
                        call: 0,
                        kind: Kind::UnknownFunction
                    },
                    Finding {
                        call: 1,
                        kind: Kind::Argument(CallFinding::ArgumentType {
                            argument: 0,
                            expected: ValueType::Int,
                            actual: ValueType::Float
                        })
                    },
                    Finding {
                        call: 2,
                        kind: Kind::Argument(CallFinding::UnknownArgument { argument: 0 })
                    },
                    Finding {
                        call: 2,
                        kind: Kind::Argument(CallFinding::MissingRequiredArgument { parameter: 0 })
                    },
                ]
            );
        }

        #[test]
        fn missing_literals_and_unbound_reference_types_do_not_invent_type_faults() {
            let functions = vec![function("id")];
            let missing = call("id", vec![("x", Input::Literal(Value::Missing))]);
            let reference = call("id", vec![("x", Input::Reference("SRC.X".into()))]);
            let compiled = ProjectCalls::admit(&functions, vec![missing, reference]).unwrap();
            assert_eq!(compiled.selected(), [0]);
            assert!(matches!(
                compiled.calls()[0].located.call.arguments[0].input,
                Input::Literal(Value::Missing)
            ));
            assert!(matches!(
                compiled.calls()[1].located.call.arguments[0].input,
                Input::Reference(_)
            ));
        }

        #[test]
        fn inherited_calls_keep_the_original_activation_slot() {
            let functions = vec![function("id")];
            let types = std::collections::BTreeMap::new();
            let admitted = ProjectCalls::admit_with_reference_types_and_uses(
                &functions,
                vec![ordinary("id")],
                &types,
                &[16],
            )
            .unwrap();
            assert_eq!(admitted.selected(), [0]);
            assert_eq!(admitted.calls()[0].slot(), Some(0));
        }

        #[test]
        fn fully_overridden_calls_are_validated_once_without_activation_or_binding() {
            let functions = vec![function("id")];
            let types = std::collections::BTreeMap::new();
            let admitted = ProjectCalls::admit_with_reference_types_and_uses(
                &functions,
                vec![ordinary("id")],
                &types,
                &[0],
            )
            .unwrap();
            assert!(admitted.selected().is_empty());
            assert!(admitted.plans().is_empty());
            assert_eq!(admitted.calls()[0].slot(), None);
            let catalog = yamaa_core::reference_binding::Catalog::compile(&[], &[]).unwrap();
            let context = yamaa_core::project_call_binding::Context {
                source_dataset: 0,
                source_mode: yamaa_core::project_call_binding::SourceMode::Record,
                available_outputs: Some(&[]),
                scope: yamaa_core::reference_scope::Scope {
                    drivers: &[],
                    current_driver: true,
                    reach: yamaa_core::reference_scope::Reach::Scalar,
                    joined: false,
                    phase: yamaa_core::reference_scope::Phase::Row { group_by: None },
                },
            };
            assert!(matches!(
                yamaa_core::project_call_binding::bind(&admitted, 0, &catalog, context),
                Err(yamaa_core::project_call_binding::Error::InactiveCall)
            ));
            assert!(
                matches!(ProjectCalls::admit_with_reference_types_and_uses(&functions, vec![call("id", vec![("x", Input::Literal(Value::float(7.0)))])], &types, &[0]), Err(Error::Findings(findings)) if findings.len() == 1)
            );
            let mixed = ProjectCalls::admit_with_reference_types_and_uses(
                &functions,
                vec![ordinary("id"), ordinary("id")],
                &types,
                &[0, 1],
            )
            .unwrap();
            assert_eq!(
                mixed.calls().iter().map(|c| c.slot()).collect::<Vec<_>>(),
                [None, Some(0)]
            );
        }

        #[test]
        fn invalid_use_metadata_fails_closed() {
            let functions = vec![function("id")];
            let types = std::collections::BTreeMap::new();
            assert!(matches!(
                ProjectCalls::admit_with_reference_types_and_uses(
                    &functions,
                    vec![ordinary("id")],
                    &types,
                    &[]
                ),
                Err(Error::InvalidCallUses)
            ));
        }

        #[test]
        fn empty_calls_select_no_functions_and_duplicate_definitions_never_ambiguously_bind() {
            let f = function("id");
            let compiled = ProjectCalls::admit(std::slice::from_ref(&f), vec![]).unwrap();
            assert!(
                compiled.selected().is_empty()
                    && compiled.plans().is_empty()
                    && compiled.calls().is_empty()
            );
            assert!(
                matches!(ProjectCalls::admit(&[f.clone(), f.clone(), f], vec![ordinary("id")]), Err(Error::DuplicateDefinitions(indices)) if indices == [1, 2])
            );
        }
    }
    mod project_codelist_binding {
        use crate::support::roots::{Codelist, CodelistType, External, Item, Source};
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{
            dataset::Check,
            project_codelist_binding::{bind, Binding, Cause},
            project_terminology::Catalogue,
            value::{ColumnType, Value},
        };
        fn catalogue(
            kind: CodelistType,
            values: Vec<Value>,
            extensible: bool,
            external: bool,
        ) -> Catalogue {
            Catalogue::admit(vec![Source {
                standard: None,
                codelists: vec![Codelist {
                    id: "CODE".into(),
                    name: "Codes".into(),
                    data_type: kind,
                    extensible,
                    alias: None,
                    format_name: None,
                    items: (!external).then(|| {
                        values
                            .into_iter()
                            .map(|value| Item {
                                value,
                                decode: None,
                                rank: None,
                                alias: None,
                                extended: false,
                            })
                            .collect()
                    }),
                    external: external.then(|| External {
                        dictionary: "dictionary".into(),
                        version: "1".into(),
                        href: Some("relative/path".into()),
                    }),
                }],
            }
            .root()])
            .unwrap()
        }
        fn binding<'a>(
            id: &'a str,
            kind: ColumnType,
            allowed_values: &'a [&'a str],
        ) -> Binding<'a> {
            Binding {
                column: 1,
                name: "VALUE",
                kind,
                path: yamaa_core::project_codelist_binding::Path::Authored(
                    "columns.VALUE.submission.codelist",
                ),
                codelist: id,
                allowed_values,
            }
        }
        #[test]
        fn complete_static_findings_preserve_all_bindings_and_name_the_redundant_verification() {
            let cat = catalogue(
                CodelistType::Text,
                vec![Value::Str("F".into()), Value::Str("M\0🙂".into())],
                false,
                false,
            );
            let allowed = ["columns.VALUE.verifications[0].allowed_values"];
            let Err(findings) = bind(
                &cat,
                &[
                    binding("UNKNOWN", ColumnType::Str, &[]),
                    binding("CODE", ColumnType::Int, &allowed),
                ],
            ) else {
                panic!("complete rejection")
            };
            assert_eq!(findings.len(), 3);
            for (finding, condition, requirement) in [
                (&findings[0], "unknown_codelist", "REQ-0953"),
                (&findings[1], "codelist_type_mismatch", "REQ-0954"),
                (&findings[2], "redundant_declaration", "REQ-0955"),
            ] {
                let d = finding.diagnostic();
                assert_eq!(
                    (
                        d.definition().phase,
                        d.definition().condition,
                        d.definition().requirement
                    ),
                    ("validation", condition, Some(requirement))
                );
                assert_eq!(d.context["column"], JsonValue::String("VALUE".into()));
            }
            let d = findings[2].diagnostic();
            assert_eq!(d.spec_paths, allowed);
            assert_eq!(
                d.context["stated_by"],
                JsonValue::String("columns.VALUE.submission.codelist".into())
            );
            assert_eq!(d.context["codelist"], JsonValue::String("CODE".into()));
            assert!(matches!(
                findings[1].cause(),
                Cause::TypeMismatch {
                    expected: ColumnType::Str,
                    actual: ColumnType::Int
                }
            ));
        }
        #[test]
        fn fixed_binding_keeps_numeric_authorship_and_rejects_every_restating_verification() {
            let cat = catalogue(
                CodelistType::Float,
                vec![Value::Int(9_007_199_254_740_993), Value::float(-0.0)],
                false,
                false,
            );
            let checks = bind(&cat, &[binding("CODE", ColumnType::Float, &[])]).unwrap();
            assert_eq!(checks.len(), 1);
            assert_eq!(checks[0].column, 1);
            let Check::Codelist { id, values } = &checks[0].verification.check else {
                panic!("fixed check")
            };
            assert_eq!(id, "CODE");
            assert_eq!(values[0], Value::Int(9_007_199_254_740_993));
            let Value::Float(zero) = values[1] else {
                panic!("authored float")
            };
            assert_eq!(zero.get().to_bits(), (-0.0f64).to_bits());
            // Equal or different, a second statement of the value set is redundant.
            let allowed = ["first", "second"];
            let Err(findings) = bind(&cat, &[binding("CODE", ColumnType::Float, &allowed)]) else {
                panic!("redundant verifications")
            };
            assert_eq!(
                findings
                    .iter()
                    .map(|finding| finding.diagnostic().spec_paths)
                    .collect::<Vec<_>>(),
                [["first"], ["second"]]
            );
        }
        #[test]
        fn extensible_lists_are_enforced_and_external_dictionaries_leave_verifications_independent()
        {
            let extensible = catalogue(
                CodelistType::Text,
                vec![Value::Str("F".into())],
                true,
                false,
            );
            let checks = bind(&extensible, &[binding("CODE", ColumnType::Str, &[])]).unwrap();
            assert!(matches!(
                &checks[0].verification.check,
                Check::Codelist { values, .. } if values == &[Value::Str("F".into())]
            ));
            assert!(
                matches!(bind(&extensible, &[binding("CODE", ColumnType::Str, &["allowed"])]), Err(f) if f.len() == 1)
            );
            let external = catalogue(CodelistType::Text, vec![], false, true);
            assert!(
                bind(&external, &[binding("CODE", ColumnType::Str, &["allowed"])])
                    .unwrap()
                    .is_empty()
            );
            assert!(
                matches!(bind(&external, &[binding("CODE", ColumnType::Int, &["allowed"])]), Err(f) if f.len() == 1)
            );
        }
    }
    mod project_codelist_checks {
        use yamaa_core::{
            dataset::Check,
            dataset_checks::{column_diagnostic, column_offenders},
            value::Value,
        };

        #[test]
        fn numeric_codes_compare_exactly_across_integer_binary64_and_zero_representations() {
            let check = Check::Codelist {
                id: "NUM".into(),
                values: vec![
                    Value::Int(9_007_199_254_740_993),
                    Value::float(1.0),
                    Value::float(-0.0),
                    Value::Int(i64::MAX),
                ],
            };
            let values = [
                Value::float(9_007_199_254_740_992.0),
                Value::Int(9_007_199_254_740_993),
                Value::Int(1),
                Value::float(0.0),
                Value::Missing,
                Value::float(9_223_372_036_854_775_808.0),
                Value::Int(i64::MAX),
            ];
            assert_eq!(column_offenders(&check, &values), [0, 5]);
            let finding = column_diagnostic(
                &check,
                "columns.NUM.submission.codelist".into(),
                "NUM".into(),
                2,
            )
            .unwrap();
            assert_eq!(
                (
                    finding.definition().phase,
                    finding.definition().condition,
                    finding.definition().requirement
                ),
                ("verification", "allowed_values_failed", Some("REQ-0957"))
            );
        }
        #[test]
        fn text_codes_use_scalar_equality_and_empty_fixed_lists_only_admit_missing() {
            let check = Check::Codelist {
                id: "TEXT".into(),
                values: vec![Value::Str("e\u{301}\0🙂".into())],
            };
            assert_eq!(
                column_offenders(
                    &check,
                    &[
                        Value::Str("e\u{301}\0🙂".into()),
                        Value::Str("é\0🙂".into()),
                        Value::Missing
                    ]
                ),
                [1]
            );
            let empty = Check::Codelist {
                id: "EMPTY".into(),
                values: vec![],
            };
            assert_eq!(
                column_offenders(
                    &empty,
                    &[Value::Missing, Value::Str("".into()), Value::Int(0)]
                ),
                [1, 2]
            );
            assert!(column_diagnostic(
                &empty,
                "columns.CODE.submission.codelist".into(),
                "CODE".into(),
                0
            )
            .is_none());
        }
    }
    mod project_codelist_diagnostics {
        use crate::support::project_codelist_compiler as support;
        use yamaa_core::{
            dataset::{CheckRecord, CodelistObservation, RowIdentity},
            diagnostic::ConditionCode,
            project_codelist_diagnostics::{diagnostics, Error},
            project_diagnostics::Fact,
            specification::PreparedSpecification,
            value::Value,
        };
        fn compiled() -> PreparedSpecification {
            PreparedSpecification::prepare_with_environment(
                &support::spec("SEX", "str", None, false, None),
                &support::environment(false),
            )
            .unwrap()
        }
        fn record(rows: &[RowIdentity], values: &[Value]) -> CheckRecord {
            CheckRecord {
                path: "columns.CODE.submission.codelist".into(),
                code: ConditionCode::VerificationCodelistFailed,
                evaluated_count: 2,
                failed_count: 1,
                output_rows: 2,
                offending_rows: rows.to_vec(),
                codelist: Some(CodelistObservation {
                    id: "SEX".into(),
                    values: values.to_vec(),
                }),
                inventory: None,
                row_count: None,
                entries: None,
            }
        }
        #[test]
        fn complete_context_borrows_exact_keys_values_and_selected_metadata() {
            let compiled = compiled();
            let rows = [RowIdentity {
                position: 1,
                values: vec![Value::Int(i64::MAX)],
            }];
            let values = [Value::Str("\u{e9}\0\u{1f642}".into())];
            let records = [record(&rows, &values)];
            let result = diagnostics(&compiled, &records).unwrap();
            assert_eq!(result.len(), 1);
            let diagnostic = &result[0];
            assert_eq!(diagnostic.code, ConditionCode::VerificationCodelistFailed);
            assert_eq!(diagnostic.spec_path, "columns.CODE.submission.codelist");
            assert_eq!(diagnostic.context.len(), 5);
            assert_eq!(diagnostic.context["column"], Fact::Text("CODE"));
            assert_eq!(diagnostic.context["codelist"], Fact::Text("SEX"));
            assert_eq!(diagnostic.context["failure_count"], Fact::Unsigned(1));
            let Fact::Sequence(keys) = &diagnostic.context["keys"] else {
                panic!("full keys")
            };
            let [Fact::Object(keys)] = keys.as_slice() else {
                panic!("one full row")
            };
            let Fact::Scalar(key) = keys["ID"] else {
                panic!("exact key")
            };
            assert!(std::ptr::eq(key, &records[0].offending_rows[0].values[0]));
            let Fact::Sequence(offenders) = &diagnostic.context["values"] else {
                panic!("all values")
            };
            let [Fact::Scalar(value)] = offenders.as_slice() else {
                panic!("one full value")
            };
            assert!(std::ptr::eq(
                *value,
                &records[0].codelist.as_ref().unwrap().values[0]
            ));
            assert_eq!(
                diagnostics(&compiled, &[record(&rows, &values)]).unwrap(),
                result
            );
        }
        #[test]
        fn late_invalid_observations_refuse_the_complete_report() {
            let compiled = compiled();
            let rows = [RowIdentity {
                position: 1,
                values: vec![Value::Int(i64::MAX)],
            }];
            let values = [Value::Str("bad".into())];
            let valid = record(&rows, &values);
            let mut invalid = record(&rows, &values);
            invalid.path = "columns.LATE.verifications[0]".into();
            assert!(matches!(
                diagnostics(&compiled, &[valid, invalid]),
                Err(Error::InvalidObservation)
            ));
            let mut invalid = record(&rows, &values);
            invalid.failed_count = 2;
            assert!(matches!(
                diagnostics(&compiled, &[invalid]),
                Err(Error::InvalidObservation)
            ));
            let missing = [Value::Missing];
            assert!(matches!(
                diagnostics(&compiled, &[record(&rows, &missing)]),
                Err(Error::InvalidObservation)
            ));
        }

        #[test]
        fn same_condition_with_a_different_typed_owner_refuses_codelist_projection() {
            let compiled = compiled();
            let rows = [RowIdentity {
                position: 1,
                values: vec![Value::Int(i64::MAX)],
            }];
            let values = [Value::Str("bad".into())];
            let mut wrong = record(&rows, &values);
            wrong.code = ConditionCode::VerificationAllowedValuesFailed;
            assert_eq!(wrong.code.definition().condition, "allowed_values_failed");
            assert_eq!(wrong.code.definition().requirement, Some("REQ-0376"));
            assert!(matches!(
                diagnostics(&compiled, &[record(&rows, &values), wrong],),
                Err(Error::InvalidObservation)
            ));
        }
    }
    mod project_collections {
        use crate::support::roots::{Case, Definition};
        use crate::support::roots::{Codelist, CodelistType, Item, Source};
        use yamaa_core::{
            project_environment::{Draft, Environment},
            project_function::{Function, Language},
            project_terminology::Catalogue,
            value::{ColumnType, Value},
        };
        fn definition() -> Definition {
            Definition {
                name: "constant".into(),
                function: "project.constant".into(),
                description: "μ\0".into(),
                params: vec![],
                returns: ColumnType::Int,
                may_return_missing: false,
                comparison_decimals: 4,
                tests: vec![Case {
                    id: "ordinary".into(),
                    covers: vec!["normal".into(), "boundary".into()],
                    args: vec![],
                    result: Value::Int(1),
                }],
            }
        }
        #[test]
        fn function_metadata_keeps_exact_utf8_and_nul_bytes() {
            let function =
                Function::admit(Language::Python, definition().root(Language::Python)).unwrap();
            assert_eq!(
                function.definition().description.text.as_bytes(),
                "μ\0".as_bytes()
            );
        }

        #[test]
        fn large_ordered_codelists_preserve_every_admitted_value() {
            let source = Source {
                standard: None,
                codelists: vec![Codelist {
                    id: "LARGE".into(),
                    name: "Large".into(),
                    data_type: CodelistType::Integer,
                    extensible: false,
                    alias: None,
                    format_name: None,
                    external: None,
                    items: Some(
                        (0..10_000)
                            .map(|i| Item {
                                value: Value::Int(i),
                                decode: None,
                                rank: None,
                                alias: None,
                                extended: false,
                            })
                            .collect(),
                    ),
                }],
            }
            .root();
            let catalogue = Catalogue::admit(vec![source.clone()]).unwrap();
            assert!(catalogue.bind("LARGE", ColumnType::Int, false).is_empty());
            let values: Vec<_> = (0..10_000).map(Value::Int).collect();
            assert!(catalogue
                .enforced_items("LARGE")
                .unwrap()
                .iter()
                .map(|item| &item.value)
                .eq(values.iter()));
        }

        #[test]
        fn many_small_functions_retain_independent_admission() {
            use yamaa_core::project_environment::{LockKind, LockReference};
            let functions: Vec<_> = (0..200)
                .map(|i| {
                    let mut def = definition();
                    def.name = format!("function_{i}");
                    def.root(Language::Python)
                })
                .collect();
            for function in &functions {
                assert!(Function::admit(Language::Python, function.clone()).is_ok());
            }
            let draft = Draft {
                language: Some(Language::Python),
                lock: Some(LockReference {
                    written: "uv.lock".into(),
                    kind: LockKind::Uv,
                }),
                functions: Some(functions),
                codelists: vec![],
            };
            assert!(Environment::admit(Language::Python, draft).is_ok());
        }
    }
}
