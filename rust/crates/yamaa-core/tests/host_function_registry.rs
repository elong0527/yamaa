//! Caller-supplied host function registry and batch invocation contracts.
//!
//! Expected values in this file are authored literals, never derived from
//! the implementation under test. Scalar equivalence is proved against the
//! independent scalar entry point `invoke_project`.
//!
//! The seam is the host-functions port, scripted by counting batch doubles:
//! the registry's state, the batches and arguments each call receives, the
//! calls it makes and chunk cancellation, which neither the benchmark reader
//! nor the host fixtures reach, since they cannot script a host.
use crate::support::roots::{Case, Definition, Parameter as DefParam};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use yamaa_core::function_batch::{
    invoke_batch, invoke_batch_chunked, BatchArgument, BatchFailureKind,
};
use yamaa_core::function_invocation::{invoke_project, Argument, HostError};
use yamaa_core::function_signature::{
    LogicalSignature, Parameter, Presence, ProjectFunctionIdentity, ProjectInvocationPlan,
};
use yamaa_core::host_function_registry::{
    activate_registered, FunctionRegistry, RegistryActivationFailure, RegistryError,
};
use yamaa_core::host_functions::{HostFunctions, Lock};
use yamaa_core::project_function::{Function, Language};
use yamaa_core::table::ValueRef;
use yamaa_core::temporal::{Date, DateTime, Precision};
use yamaa_core::value::{ColumnType, Value, ValueType};

fn identity() -> ProjectFunctionIdentity {
    ProjectFunctionIdentity {
        name: "add_or_echo".into(),
        call: "synthetic.add_or_echo".into(),
    }
}

/// x: int, rejects missing, required. y: int, accepts missing, default 10.
fn plan() -> ProjectInvocationPlan {
    ProjectInvocationPlan::new(
        identity(),
        vec![
            Parameter {
                name: "x".into(),
                host_name: "x".into(),
                kind: ValueType::Int,
                accepts_missing: false,
                presence: Presence::Required,
            },
            Parameter {
                name: "y".into(),
                host_name: "y".into(),
                kind: ValueType::Int,
                accepts_missing: true,
                presence: Presence::Optional(Value::Int(10)),
            },
        ],
        ColumnType::Int,
        false,
    )
    .unwrap()
}

fn row(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

/// A counting vectorized host: sums x+y per row (missing y counts as 0),
/// records every column it was given, and can be told to misbehave.
#[derive(Default)]
struct VectorPort {
    calls: usize,
    seen_rows: usize,
    seen: Vec<(String, Vec<Value>)>,
    mode: Mode,
}
#[derive(Default, PartialEq)]
enum Mode {
    #[default]
    Sum,
    WrongLength,
    WrongType,
    Boolean,
    UndeclaredMissing,
    Raise,
    Interrupt,
}
#[derive(Debug, PartialEq)]
struct Payload(&'static str);

/// The half of the host-functions port a batch double never serves: the
/// registry resolves its bindings, verifies no lock and invokes in batches.
macro_rules! batches_only {
    () => {
        fn verify_lock(
            &mut self,
            _: Language,
            _: Lock<'_>,
            _: &[ProjectFunctionIdentity],
        ) -> Result<(), Payload> {
            unreachable!("a registry activation verifies no lock")
        }
        fn bind(
            &mut self,
            _: &ProjectFunctionIdentity,
            _: &LogicalSignature,
        ) -> Result<(), Payload> {
            unreachable!("a registry activation binds through its registry")
        }
        fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<Payload>> {
            unreachable!("a batch double is invoked in batches")
        }
    };
}

impl HostFunctions for VectorPort {
    type Handle = ();
    type Error = Payload;
    batches_only!();
    fn is_interrupt(&self, error: &Payload) -> bool {
        error.0 == "interrupt"
    }
    fn call_batch(
        &mut self,
        _handle: &(),
        rows: usize,
        arguments: &[BatchArgument],
    ) -> Result<Vec<Value>, HostError<Payload>> {
        assert_eq!(rows, arguments.first().map_or(0, |a| a.values.len()));
        self.calls += 1;
        self.seen_rows = arguments.first().map_or(0, |a| a.values.len());
        self.seen = arguments
            .iter()
            .map(|a| (a.name.clone(), a.values.clone()))
            .collect();
        match self.mode {
            Mode::Raise => return Err(HostError::Raised(Payload("boom"))),
            Mode::Interrupt => return Err(HostError::Raised(Payload("interrupt"))),
            _ => {}
        }
        let rows = self.seen_rows;
        let mut out: Vec<Value> = (0..rows)
            .map(|i| {
                let x = match &arguments[0].values[i] {
                    Value::Int(n) => *n,
                    _ => 0,
                };
                let y = match &arguments[1].values[i] {
                    Value::Int(n) => *n,
                    _ => 0,
                };
                Value::Int(x + y)
            })
            .collect();
        match self.mode {
            Mode::WrongLength => out.push(Value::Int(0)),
            Mode::WrongType => {
                if let Some(first) = out.first_mut() {
                    *first = Value::Str("no".into());
                }
            }
            Mode::Boolean => {
                if let Some(first) = out.first_mut() {
                    *first = Value::Bool(true);
                }
            }
            Mode::UndeclaredMissing => {
                if let Some(first) = out.first_mut() {
                    *first = Value::Missing;
                }
            }
            _ => {}
        }
        Ok(out)
    }
}

#[test]
fn registry_refuses_duplicates_and_unregistered_replacement_then_freezes() {
    let mut registry = FunctionRegistry::new();
    let signature = plan().signature().clone();
    registry
        .register(identity(), signature.clone(), ())
        .unwrap();
    assert_eq!(
        registry.register(identity(), signature.clone(), ()),
        Err(RegistryError::Duplicate {
            call: "synthetic.add_or_echo".into()
        })
    );
    let other = ProjectFunctionIdentity {
        name: "other".into(),
        call: "synthetic.other".into(),
    };
    assert_eq!(
        registry.replace(other, signature.clone(), ()),
        Err(RegistryError::NotRegistered {
            call: "synthetic.other".into()
        })
    );
    registry.replace(identity(), signature, ()).unwrap();
    let frozen = registry.freeze();
    assert_eq!(frozen.len(), 1);
    assert!(frozen.resolve(&identity()).is_ok());
    let missing = ProjectFunctionIdentity {
        name: "nope".into(),
        call: "synthetic.nope".into(),
    };
    assert_eq!(frozen.resolve(&missing).unwrap_err().call, "synthetic.nope");
}

#[test]
fn batch_scatters_in_original_order_and_short_circuits_missing() {
    // Independently authored expected results: rows 1 and 3 are
    // short-circuited (x rejects missing) and never reach the callback.
    let rows = vec![
        row(&[("x", Value::Int(1)), ("y", Value::Int(2))]),
        row(&[("x", Value::Missing), ("y", Value::Int(5))]),
        row(&[("x", Value::Int(3))]), // y defaults to 10
        row(&[("x", Value::Missing)]),
        row(&[("x", Value::Int(4)), ("y", Value::Missing)]), // y accepts missing
    ];
    let mut port = VectorPort::default();
    let results = invoke_batch(&plan(), &(), &rows, &mut port).unwrap();
    assert_eq!(
        results,
        [
            Value::Int(3),
            Value::Missing,
            Value::Int(13),
            Value::Missing,
            Value::Int(4)
        ]
    );
    assert_eq!(port.calls, 1);
    assert_eq!(port.seen_rows, 3);
    // Defaults were broadcast and accepting-missing rows carried nulls.
    assert_eq!(
        port.seen,
        [
            (
                "x".into(),
                vec![Value::Int(1), Value::Int(3), Value::Int(4)]
            ),
            (
                "y".into(),
                vec![Value::Int(2), Value::Int(10), Value::Missing]
            ),
        ]
    );
}

#[test]
fn batch_empty_and_all_missing_never_invoke() {
    let mut port = VectorPort::default();
    assert_eq!(invoke_batch(&plan(), &(), &[], &mut port).unwrap(), []);
    let rows = vec![row(&[("x", Value::Missing)]), row(&[("x", Value::Missing)])];
    assert_eq!(
        invoke_batch(&plan(), &(), &rows, &mut port).unwrap(),
        [Value::Missing, Value::Missing]
    );
    assert_eq!(port.calls, 0);
}

#[test]
fn batch_rejects_wrong_length_and_wrong_type_before_accepting_output() {
    let rows = vec![row(&[("x", Value::Int(1))])];
    for (mode, check) in [
        (Mode::WrongLength, "length"),
        (Mode::WrongType, "type"),
        (Mode::Boolean, "bool"),
        (Mode::UndeclaredMissing, "missing"),
    ] {
        let mut port = VectorPort {
            mode,
            ..Default::default()
        };
        let failure = invoke_batch(&plan(), &(), &rows, &mut port).unwrap_err();
        match (check, &failure.kind) {
            ("length", BatchFailureKind::WrongLength { expected, actual }) => {
                assert_eq!((*expected, *actual), (1, 2));
                assert_eq!(failure.row, None);
            }
            ("type", BatchFailureKind::Scalar(kind)) => {
                assert!(matches!(
                    kind,
                    yamaa_core::function_invocation::FailureKind::ResultType { .. }
                ));
                assert_eq!(failure.row, Some(0));
            }
            ("bool", BatchFailureKind::Scalar(kind)) => {
                assert!(matches!(
                    kind,
                    yamaa_core::function_invocation::FailureKind::BooleanResult
                ));
            }
            ("missing", BatchFailureKind::Scalar(kind)) => {
                assert!(matches!(
                    kind,
                    yamaa_core::function_invocation::FailureKind::UndeclaredMissing
                ));
            }
            other => panic!("unexpected failure {other:?}"),
        }
        assert_eq!(failure.identity, identity());
        assert_eq!(port.calls, 1);
    }
}

#[test]
fn batch_argument_failures_abort_before_any_invocation() {
    let rows = vec![
        row(&[("x", Value::Int(1))]),
        row(&[("x", Value::Str("bad".into()))]),
    ];
    let mut port = VectorPort::default();
    let failure = invoke_batch(&plan(), &(), &rows, &mut port).unwrap_err();
    assert_eq!(failure.row, Some(1));
    assert!(matches!(
        failure.kind,
        BatchFailureKind::Scalar(yamaa_core::function_invocation::FailureKind::ArgumentType { .. })
    ));
    assert_eq!(port.calls, 0);
    // Callback exceptions keep their payload.
    let mut port = VectorPort {
        mode: Mode::Raise,
        ..Default::default()
    };
    let failure = invoke_batch(&plan(), &(), &rows[..1], &mut port).unwrap_err();
    assert!(matches!(
        failure.kind,
        BatchFailureKind::Scalar(yamaa_core::function_invocation::FailureKind::CallFailed(
            Payload("boom")
        ))
    ));
}

#[test]
fn chunked_batch_checks_cancellation_between_chunks_and_discards_partials() {
    let rows: Vec<_> = (0..6)
        .map(|i| row(&[("x", Value::Int(i)), ("y", Value::Int(1))]))
        .collect();
    let checks = Arc::new(Mutex::new(0usize));
    let counter = checks.clone();
    let cancel_after_two = move || {
        let mut n = counter.lock().unwrap();
        *n += 1;
        *n > 2
    };
    let mut port = VectorPort::default();
    let failure =
        invoke_batch_chunked(&plan(), &(), &rows, 2, &mut port, &cancel_after_two).unwrap_err();
    assert_eq!(failure.kind, BatchFailureKind::Cancelled);
    assert_eq!(port.calls, 2);
    // Uncancelled chunked runs equal the single-batch result exactly.
    let mut port = VectorPort::default();
    let chunked = invoke_batch_chunked(&plan(), &(), &rows, 4, &mut port, &|| false).unwrap();
    assert_eq!(
        chunked,
        (0..6).map(|i| Value::Int(i + 1)).collect::<Vec<_>>()
    );
    assert_eq!(
        invoke_batch_chunked(&plan(), &(), &rows, 0, &mut VectorPort::default(), &|| {
            false
        })
        .unwrap_err()
        .kind,
        BatchFailureKind::InvalidChunkSize
    );
}

/// Scalar reference port used only to cross-check equivalence; expected
/// literals below are authored independently of both implementations. As a
/// host-functions port it serves batches through the provided row-by-row
/// `call_batch`.
struct ScalarPort;
impl ScalarPort {
    fn sum(arguments: &[Argument<'_>]) -> Value {
        let get = |name: &str| arguments.iter().find(|a| a.name == name).unwrap().value;
        let x = match get("x") {
            ValueRef::Int(n) => n,
            _ => 0,
        };
        let y = match get("y") {
            ValueRef::Int(n) => n,
            _ => 0,
        };
        Value::Int(x + y)
    }
}
impl yamaa_core::function_invocation::FunctionPort for ScalarPort {
    type Error = Payload;
    fn call(
        &mut self,
        _slot: usize,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Payload>> {
        Ok(Self::sum(arguments))
    }
}
impl HostFunctions for ScalarPort {
    type Handle = ();
    type Error = Payload;
    fn verify_lock(
        &mut self,
        _: Language,
        _: Lock<'_>,
        _: &[ProjectFunctionIdentity],
    ) -> Result<(), Payload> {
        unreachable!("a batch invocation verifies no lock")
    }
    fn bind(&mut self, _: &ProjectFunctionIdentity, _: &LogicalSignature) -> Result<(), Payload> {
        unreachable!("a batch invocation is given its handle")
    }
    fn invoke(&mut self, _: &(), arguments: &[Argument<'_>]) -> Result<Value, HostError<Payload>> {
        Ok(Self::sum(arguments))
    }
}

/// The vectorized double and the port's provided row-by-row batch both equal
/// scalar invocation, partitioned however the rows are chunked.
#[test]
fn batch_matches_scalar_invocation_across_partitions() {
    let rows = vec![
        row(&[("x", Value::Int(i64::MAX - 1)), ("y", Value::Int(1))]),
        row(&[("x", Value::Missing), ("y", Value::Int(9))]),
        row(&[("x", Value::Int(-5))]),
        row(&[("x", Value::Int(7)), ("y", Value::Missing)]),
    ];
    let expected = [
        Value::Int(i64::MAX),
        Value::Missing,
        Value::Int(5),
        Value::Int(7),
    ];
    let mut scalar = ScalarPort;
    for (r, want) in rows.iter().zip(&expected) {
        assert_eq!(&invoke_project(&plan(), r, &mut scalar).unwrap(), want);
    }
    for chunk in [1usize, 2, 3, 100] {
        let mut port = VectorPort::default();
        let got = invoke_batch_chunked(&plan(), &(), &rows, chunk, &mut port, &|| false).unwrap();
        assert_eq!(got, expected, "vectorized, chunk {chunk}");
        let got =
            invoke_batch_chunked(&plan(), &(), &rows, chunk, &mut ScalarPort, &|| false).unwrap();
        assert_eq!(got, expected, "provided row by row, chunk {chunk}");
    }
}

fn definition() -> Function {
    Function::admit(
        Language::Python,
        Definition {
            name: "add_or_echo".into(),
            function: "synthetic.add_or_echo".into(),
            description: "Synthetic unlocked registry demonstration.".into(),
            params: vec![
                DefParam {
                    name: "x".into(),
                    kind: ValueType::Int,
                    required: true,
                    default: None,
                    accepts_missing: false,
                },
                DefParam {
                    name: "y".into(),
                    kind: ValueType::Int,
                    required: false,
                    default: Some(Value::Int(10)),
                    accepts_missing: true,
                },
            ],
            returns: ColumnType::Int,
            may_return_missing: false,
            comparison_decimals: 0,
            tests: vec![
                Case {
                    id: "normal".into(),
                    covers: vec!["normal".into()],
                    args: vec![("x".into(), Value::Int(2)), ("y".into(), Value::Int(3))],
                    result: Value::Int(5),
                },
                Case {
                    id: "boundary".into(),
                    covers: vec!["boundary".into()],
                    args: vec![
                        ("x".into(), Value::Int(i64::MAX - 1)),
                        ("y".into(), Value::Int(1)),
                    ],
                    result: Value::Int(i64::MAX),
                },
                Case {
                    id: "default".into(),
                    covers: vec!["default:y".into()],
                    args: vec![("x".into(), Value::Int(1))],
                    result: Value::Int(11),
                },
                Case {
                    id: "short".into(),
                    covers: vec!["short-circuit-missing:x".into()],
                    args: vec![("x".into(), Value::Missing)],
                    result: Value::Missing,
                },
                Case {
                    id: "accepted".into(),
                    covers: vec!["accepted-missing:y".into()],
                    args: vec![("x".into(), Value::Int(4)), ("y".into(), Value::Missing)],
                    result: Value::Int(4),
                },
            ],
        }
        .root(Language::Python),
    )
    .unwrap()
}

#[test]
fn registered_activation_runs_all_cases_and_never_caches_success() {
    let function = definition();
    let signature = function.invocation_plan().unwrap().signature().clone();
    let mut registry = FunctionRegistry::new();
    registry.register(identity(), signature, ()).unwrap();
    let frozen = registry.freeze();
    for attempt in 1..=2 {
        let mut port = VectorPort::default();
        let activated =
            activate_registered(Language::Python, &[&function], &frozen, &mut port).unwrap();
        assert_eq!(activated.len(), 1);
        assert_eq!(activated[0].cases.len(), 5, "attempt {attempt}");
        assert_eq!(port.calls, 1, "attempt {attempt}");
        assert_eq!(port.seen_rows, 4, "short-circuited case excluded");
        let invoked: Vec<bool> = activated[0].cases.iter().map(|c| c.invoked).collect();
        assert_eq!(invoked, [true, true, true, false, true]);
    }
}

#[test]
fn registered_activation_refuses_missing_binding_and_signature_mismatch() {
    let function = definition();
    let frozen = FunctionRegistry::<()>::new().freeze();
    let mut port = VectorPort::default();
    assert!(matches!(
        activate_registered(Language::Python, &[&function], &frozen, &mut port),
        Err(RegistryActivationFailure::MissingBinding { function: 0, .. })
    ));
    assert_eq!(port.calls, 0);

    // A registered signature for a different return type must not activate.
    let wrong = ProjectInvocationPlan::new(
        identity(),
        plan().signature().parameters().to_vec(),
        ColumnType::Str,
        false,
    )
    .unwrap();
    let mut registry = FunctionRegistry::new();
    registry
        .register(identity(), wrong.signature().clone(), ())
        .unwrap();
    let frozen = registry.freeze();
    assert!(matches!(
        activate_registered(Language::Python, &[&function], &frozen, &mut port),
        Err(RegistryActivationFailure::SignatureMismatch { function: 0 })
    ));
    assert_eq!(port.calls, 0);
}

#[test]
fn registered_activation_reports_case_failures_and_interrupts() {
    let function = definition();
    let signature = function.invocation_plan().unwrap().signature().clone();
    let mut registry = FunctionRegistry::new();
    registry.register(identity(), signature, ()).unwrap();
    let frozen = registry.freeze();

    let mut port = VectorPort {
        mode: Mode::WrongType,
        ..Default::default()
    };
    assert!(matches!(
        activate_registered(Language::Python, &[&function], &frozen, &mut port),
        Err(RegistryActivationFailure::Tests(_))
    ));
    let mut port = VectorPort {
        mode: Mode::Interrupt,
        ..Default::default()
    };
    assert!(matches!(
        activate_registered(Language::Python, &[&function], &frozen, &mut port),
        Err(RegistryActivationFailure::Interrupted {
            function: 0,
            case: 0
        })
    ));
}

#[test]
fn batch_value_contract_keeps_exact_types_and_temporal_precision() {
    // A text/temporal plan proves the batch contract is not numeric-only:
    // exact int64 is covered above; here UTF-8, the empty string distinct
    // from missing, and temporal precision pass through untouched.
    let date_plan = ProjectInvocationPlan::new(
        ProjectFunctionIdentity {
            name: "echo_date".into(),
            call: "synthetic.echo_date".into(),
        },
        vec![Parameter {
            name: "d".into(),
            host_name: "d".into(),
            kind: ValueType::Date,
            accepts_missing: true,
            presence: Presence::Required,
        }],
        ColumnType::Date,
        true,
    )
    .unwrap();
    struct EchoPort;
    impl HostFunctions for EchoPort {
        type Handle = ();
        type Error = Payload;
        batches_only!();
        fn call_batch(
            &mut self,
            _handle: &(),
            rows: usize,
            arguments: &[BatchArgument],
        ) -> Result<Vec<Value>, HostError<Payload>> {
            assert_eq!(rows, arguments[0].values.len());
            Ok(arguments[0].values.clone())
        }
    }
    let year = Date::new(2025, 1, 1, Precision::Year).unwrap();
    let rows = vec![
        row(&[("d", Value::Date(year))]),
        row(&[("d", Value::Missing)]),
    ];
    let mut port = EchoPort;
    let results = invoke_batch(&date_plan, &(), &rows, &mut port).unwrap();
    assert_eq!(results, [Value::Date(year), Value::Missing]);
    if let Value::Date(date) = results[0] {
        assert_eq!(date.collected_precision(), Precision::Year);
    } else {
        panic!("date")
    }
    // Civil datetime with second precision round-trips exactly.
    let moment = DateTime::new(
        Date::new(2025, 3, 4, Precision::Day).unwrap(),
        5,
        6,
        7,
        Precision::Second,
    )
    .unwrap();
    let dt_plan = ProjectInvocationPlan::new(
        ProjectFunctionIdentity {
            name: "echo_dt".into(),
            call: "synthetic.echo_dt".into(),
        },
        vec![Parameter {
            name: "t".into(),
            host_name: "t".into(),
            kind: ValueType::DateTime,
            accepts_missing: false,
            presence: Presence::Required,
        }],
        ColumnType::DateTime,
        false,
    )
    .unwrap();
    let rows = vec![row(&[("t", Value::DateTime(moment))])];
    assert_eq!(
        invoke_batch(&dt_plan, &(), &rows, &mut port).unwrap(),
        [Value::DateTime(moment)]
    );
}

#[test]
fn zero_parameter_batch_uses_the_explicit_row_count() {
    let constant_plan = ProjectInvocationPlan::new(
        ProjectFunctionIdentity {
            name: "constant".into(),
            call: "synthetic.constant".into(),
        },
        vec![],
        ColumnType::Int,
        false,
    )
    .unwrap();
    struct ConstantPort;
    impl HostFunctions for ConstantPort {
        type Handle = ();
        type Error = Payload;
        batches_only!();
        fn call_batch(
            &mut self,
            _handle: &(),
            rows: usize,
            arguments: &[BatchArgument],
        ) -> Result<Vec<Value>, HostError<Payload>> {
            assert!(arguments.is_empty());
            Ok((0..rows).map(|i| Value::Int(i as i64)).collect())
        }
    }
    let rows = vec![row(&[]), row(&[]), row(&[])];
    let mut port = ConstantPort;
    assert_eq!(
        invoke_batch(&constant_plan, &(), &rows, &mut port).unwrap(),
        [Value::Int(0), Value::Int(1), Value::Int(2)]
    );
}
