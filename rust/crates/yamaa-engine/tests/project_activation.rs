use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{LockKind, LockReference},
    project_function::{Case, Definition, Function, Language, Parameter},
    table::ValueRef,
    value::{ColumnType, Value, ValueType},
};
use yamaa_engine::{
    function_invocation::{Argument, HostError},
    project_activation::{activate, ActivatedFunction, ActivationPort, Failure, TestFailure},
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
        },
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
fn successful_activation_handles_feed_versionless_dataset_without_rebinding_or_retesting() {
    use std::cell::RefCell;
    use yamaa_core::bound_expression::Read;
    use yamaa_core::table::{CellError, Column, TableAccess, TableSchema};
    use yamaa_engine::dataset::{
        Assignment, BoundProjectFunction, DatasetExecution, DatasetPlan, Expression,
        FunctionArgument, FunctionInput, Limits, RowMode, RowTemplate,
    };
    struct Table {
        schema: TableSchema,
        values: Vec<Value>,
        reads: RefCell<Vec<usize>>,
    }
    impl TableAccess for Table {
        type Error = Payload;
        fn schema(&self) -> &TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            self.values.len()
        }
        fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Payload>> {
            assert_eq!(column, 0);
            self.reads.borrow_mut().push(row);
            Ok(ValueRef::from(&self.values[row]))
        }
    }
    let schema = TableSchema::new(vec![Column {
        name: "X".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    let source = Table {
        schema: schema.clone(),
        values: vec![Value::Int(7), Value::Int(i64::MIN)],
        reads: RefCell::default(),
    };
    let functions = [function("identity")];
    let plan = DatasetPlan::new(
        schema.clone(),
        schema,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![Assignment {
                column: 0,
                expression: Expression::ProjectFunction(
                    BoundProjectFunction::new_project(
                        0,
                        functions[0].invocation_plan().unwrap(),
                        vec![FunctionArgument {
                            name: "x".into(),
                            input: FunctionInput::Read(Read::Source(0)),
                        }],
                    )
                    .unwrap(),
                ),
                path: "columns.X.derivation.function".into(),
            }],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let limits = Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 10,
        key_cells: 10,
        work_cells: 100,
        scalar_text_bytes: 100,
        output_text_bytes: 100,
        identity_cells: 100,
        identity_text_bytes: 100,
    };
    let mut port = Port::new();
    for _ in 0..2 {
        let activated = activate(Language::Python, &lock(), &functions, &mut port).unwrap();
        let mut bindings = yamaa_engine::project_activation::Bindings::new(&activated, &mut port);
        let result = plan
            .execute_observed_functions(&source, &[], &mut bindings, limits)
            .result
            .unwrap();
        assert_eq!(result.dataset.cell(0, 0).unwrap(), ValueRef::Int(7));
        assert_eq!(result.dataset.cell(1, 0).unwrap(), ValueRef::Int(i64::MIN));
    }
    assert_eq!(*source.reads.borrow(), vec![0, 1, 0, 1]);
    let once = [
        "lock:yamaa+called-packages",
        "bind:identity",
        "call:identity:7",
        "call:identity:-9223372036854775808",
        "call:identity:7",
        "call:identity:-9223372036854775808",
    ];
    assert_eq!(
        port.trace,
        once.iter()
            .chain(&once)
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
    );
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
            yamaa_engine::function_invocation::FailureKind::CallFailed(Payload(
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
        yamaa_engine::function_invocation::FailureKind::CallFailed(Payload("user interrupt"))
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
    definition.function = "program::ridentity".into();
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
fn selected_function_and_case_limits_precede_every_host_port() {
    use yamaa_engine::project_activation::{activate_with_limits, Limits, Resource};
    let functions = [function("identity")];
    for (limits, resource) in [
        (
            Limits {
                functions: 0,
                ..Limits::default()
            },
            Resource::Functions,
        ),
        (
            Limits {
                cases: 2,
                ..Limits::default()
            },
            Resource::Cases,
        ),
        (
            Limits {
                parameters: 0,
                ..Limits::default()
            },
            Resource::Parameters,
        ),
        (
            Limits {
                metadata_text_bytes: 49,
                ..Limits::default()
            },
            Resource::MetadataText,
        ),
    ] {
        let mut port = Port::new();
        assert!(
            matches!(activate_with_limits(Language::Python, &lock(), &functions, &mut port, limits), Err(Failure::Limit(actual)) if actual == resource)
        );
        assert!(port.trace.is_empty());
    }
}

#[test]
fn aggregate_signature_copies_are_bounded_before_lock_and_include_utf8_defaults() {
    use yamaa_engine::project_activation::{activate_with_limits, Limits, Resource};
    let mut definition = function("identity").definition().clone();
    definition.params.push(Parameter {
        name: "label".into(),
        kind: ValueType::Str,
        required: false,
        default: Some(Value::Str("e\u{301}\0\u{1f600}".into())),
        accepts_missing: false,
    });
    definition.tests[0].covers.push("default:label".into());
    definition.tests.push(Case {
        id: "missing-label".into(),
        covers: vec!["short-circuit-missing:label".into()],
        args: vec![
            ("x".into(), Value::Int(7)),
            ("label".into(), Value::Missing),
        ],
        result: Value::Missing,
    });
    let function_with_default = Function::admit(Language::Python, definition).unwrap();
    // Two copies each of identity/program.identity: 48; logical/host x: 2;
    // logical/host label: 10; default e+combining accent+NUL+emoji: 8.
    for (functions, bytes) in [
        (vec![function("identity")], 50),
        (vec![function_with_default], 68),
    ] {
        for (maximum, admitted) in [(bytes - 1, false), (bytes, true)] {
            let mut port = Port::new();
            port.lock_failure = true;
            let result = activate_with_limits(
                Language::Python,
                &lock(),
                &functions,
                &mut port,
                Limits {
                    metadata_text_bytes: maximum,
                    ..Limits::default()
                },
            );
            if admitted {
                assert!(matches!(
                    result,
                    Err(Failure::Lock(Payload("version-mismatch")))
                ));
                assert_eq!(port.trace, vec!["lock:yamaa+called-packages"]);
            } else {
                assert!(matches!(
                    result,
                    Err(Failure::Limit(Resource::MetadataText))
                ));
                assert!(port.trace.is_empty());
            }
        }
    }
    let functions = [function("first"), function("second")];
    let mut port = Port::new();
    assert!(matches!(
        activate_with_limits(
            Language::Python,
            &lock(),
            &functions,
            &mut port,
            Limits {
                parameters: 1,
                ..Limits::default()
            }
        ),
        Err(Failure::Limit(Resource::Parameters))
    ));
    assert!(port.trace.is_empty());
}
#[test]
fn retained_case_text_has_an_exact_cumulative_limit_and_never_grants_partial_activation() {
    use yamaa_engine::project_activation::{activate_with_limits, Limits, Resource};
    let mut definition = function("identity").definition().clone();
    definition.returns = ColumnType::Str;
    for case in &mut definition.tests[..2] {
        case.result = Value::Str("abcdefghi".into());
    }
    let functions = [Function::admit(Language::Python, definition).unwrap()];
    // Two nine-byte results + six/eight/nine-byte case IDs = 41 UTF-8 bytes.
    for (maximum, accepted) in [(41, true), (40, false), (15, false)] {
        let mut port = Port::new();
        port.text_result = Some("abcdefghi".into());
        let result = activate_with_limits(
            Language::Python,
            &lock(),
            &functions,
            &mut port,
            Limits {
                retained_text_bytes: maximum,
                ..Limits::default()
            },
        );
        if accepted {
            assert_eq!(result.unwrap()[0].cases.len(), 3);
        } else {
            assert!(matches!(
                result,
                Err(Failure::Limit(Resource::RetainedText))
            ));
        }
        assert!(!port.trace.iter().any(|event| event == "study.read"));
    }
}
#[test]
fn repeated_owned_failure_identities_are_charged_and_whole_exhaustion_has_no_partial_findings() {
    use yamaa_engine::project_activation::{activate_with_limits, Limits, Resource};
    let functions = [function("identity")];
    // identity/program.identity = 8+16 bytes for each of two failed invocations;
    // the successful short-circuit retains its nine-byte case ID: total 57.
    for (maximum, exhausted) in [(56, true), (57, false)] {
        let mut port = Port::new();
        port.raised = true;
        let result = activate_with_limits(
            Language::Python,
            &lock(),
            &functions,
            &mut port,
            Limits {
                retained_text_bytes: maximum,
                ..Limits::default()
            },
        );
        if exhausted {
            assert!(matches!(
                result,
                Err(Failure::Limit(Resource::RetainedText))
            ));
        } else {
            assert!(matches!(result, Err(Failure::Tests(failures)) if failures.len()==2));
        }
        assert!(!port.trace.iter().any(|event| event == "study.read"));
    }
}

#[test]
fn borrowed_compiler_selection_activates_only_held_definitions_and_repeats_cases() {
    use yamaa_engine::project_activation::activate_references;
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
            activate_references(Language::Python, &lock(), &selected, &mut port).unwrap();
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
    let error = activate_references(Language::Python, &lock(), &selected, &mut port).unwrap_err();
    assert_eq!(
        error,
        Failure::Bindings(vec![yamaa_engine::project_activation::BindingFailure {
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
fn borrowed_selection_keeps_empty_duplicate_and_quota_gates_before_ports() {
    use yamaa_engine::project_activation::{
        activate_references, activate_references_with_limits, Limits, Resource,
    };
    let environment = [function("first"), function("second")];
    let mut port = Port::new();
    port.lock_failure = true;
    assert!(
        activate_references(Language::Python, &lock(), &[], &mut port)
            .unwrap()
            .is_empty()
    );
    assert!(port.trace.is_empty());
    assert_eq!(
        activate_references(
            Language::Python,
            &lock(),
            &[&environment[0], &environment[0]],
            &mut port
        )
        .unwrap_err(),
        Failure::DuplicateSelection { function: 1 }
    );
    assert!(port.trace.is_empty());
    assert_eq!(
        activate_references_with_limits(
            Language::Python,
            &lock(),
            &[&environment[0], &environment[1]],
            &mut port,
            Limits {
                functions: 1,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Failure::Limit(Resource::Functions)
    );
    assert!(port.trace.is_empty());
    assert_eq!(
        activate_references_with_limits(
            Language::Python,
            &lock(),
            &[&environment[1]],
            &mut port,
            Limits {
                cases: 2,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Failure::Limit(Resource::Cases)
    );
    assert!(port.trace.is_empty());
    assert_eq!(
        activate_references_with_limits(
            Language::Python,
            &lock(),
            &[&environment[1]],
            &mut port,
            Limits {
                metadata_text_bytes: 0,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Failure::Limit(Resource::MetadataText)
    );
    assert!(port.trace.is_empty());
}

mod observed {
    use super::*;
    use yamaa_engine::project_activation::{
        activate_references_observed, BindingObservation, BindingOutcome, Limits, LockObservation,
        Observations, Resource, TestObservation, TestOutcome,
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
        limits: Limits,
        observations: &mut Observations,
    ) -> Result<Vec<ActivatedFunction<Handle>>, Failure<Payload>> {
        activate_references_observed(
            Language::Python,
            &lock(),
            &functions.iter().collect::<Vec<_>>(),
            port,
            limits,
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
            run(&functions, &mut port, Limits::default(), &mut observations).unwrap_err(),
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
            run(&functions, &mut port, Limits::default(), &mut observations),
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
        }) = run(&functions, &mut port, Limits::default(), &mut observations)
        else {
            panic!("original interrupt");
        };
        assert_eq!(
            error.kind,
            yamaa_engine::function_invocation::FailureKind::CallFailed(Payload("user interrupt"))
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
            run(&functions, &mut port, Limits::default(), &mut observations),
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
            let failure =
                run(&functions, &mut port, Limits::default(), &mut observations).unwrap_err();
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
                Limits::default(),
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
    fn observed_text_copies_share_the_exact_cumulative_utf8_budget() {
        let mut definition = function("first").definition().clone();
        definition.returns = ColumnType::Str;
        for case in &mut definition.tests[..2] {
            case.result = Value::Str("é".into());
        }
        let functions = [Function::admit(Language::Python, definition).unwrap()];
        // IDs: 6 + 8 + 9 bytes. Two UTF-8 results, each retained in activation
        // and observations: 2 * 2 * 2 bytes. No copies of opaque host errors.
        let mut port = ObservedPort::new();
        port.inner.text_result = Some("é".into());
        let mut observations = Observations::default();
        assert!(run(
            &functions,
            &mut port,
            Limits {
                retained_text_bytes: 31,
                ..Limits::default()
            },
            &mut observations
        )
        .is_ok());
        assert_eq!(observations.tests.len(), 3);
        assert_eq!(
            run(
                &functions,
                &mut port,
                Limits {
                    retained_text_bytes: 30,
                    ..Limits::default()
                },
                &mut observations
            )
            .unwrap_err(),
            Failure::Limit(Resource::RetainedText)
        );
        assert_eq!(
            run(
                &functions,
                &mut port,
                Limits {
                    retained_text_bytes: 1,
                    ..Limits::default()
                },
                &mut observations
            )
            .unwrap_err(),
            Failure::Limit(Resource::RetainedText)
        );
        assert_eq!(
            observations.tests,
            [TestObservation {
                function: 0,
                case: 0,
                outcome: TestOutcome::Passed,
                invoked: true,
                actual: None
            }]
        );
    }
    #[test]
    fn repeated_lock_refusal_empty_selection_and_static_limit_clear_previous_evidence() {
        let functions = [function("first")];
        let mut port = ObservedPort::new();
        let mut observations = Observations::default();
        run(&functions, &mut port, Limits::default(), &mut observations).unwrap();
        port.inner.lock_failure = true;
        assert!(matches!(
            run(&functions, &mut port, Limits::default(), &mut observations),
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
        assert!(run(&[], &mut port, Limits::default(), &mut observations)
            .unwrap()
            .is_empty());
        assert_eq!(observations, Observations::default());
        assert_eq!(port.inner.trace.len(), before);
        assert!(matches!(
            run(
                &functions,
                &mut port,
                Limits {
                    cases: 0,
                    ..Limits::default()
                },
                &mut observations
            ),
            Err(Failure::Limit(Resource::Cases))
        ));
        assert_eq!(observations, Observations::default());
        assert_eq!(port.inner.trace.len(), before);
    }
}
