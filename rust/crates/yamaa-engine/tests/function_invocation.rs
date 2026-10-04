use std::collections::BTreeMap;
use yamaa_core::{
    table::ValueRef,
    temporal::{Date, DatePrecision},
    value::{ColumnType, Value, ValueType},
};
use yamaa_engine::function_invocation::*;

/// Independently authored input spelling; expected bits are never generated here.
fn value(token: &str) -> Value {
    if token == "missing" {
        return Value::Missing;
    }
    let (kind, text) = token.split_once(':').unwrap();
    match kind {
        "int" => Value::Int(text.parse().unwrap()),
        "float" => Value::float(text.parse().unwrap()),
        "str" => Value::Str(text.into()),
        "bool" => Value::Bool(text.parse().unwrap()),
        "date" => Value::Date(text.parse().unwrap()),
        "datetime" => Value::DateTime(text.parse().unwrap()),
        _ => panic!("invalid fixture type"),
    }
}
/// Preserve exact type and float bits when observing invocation/return values.
fn encode(v: ValueRef<'_>) -> String {
    match v {
        ValueRef::Missing => "missing".into(),
        ValueRef::Int(n) => format!("int:{n}"),
        ValueRef::Float(n) => format!("float:{:016x}", n.get().to_bits()),
        ValueRef::Str(s) => format!("str:{s}"),
        ValueRef::Bool(b) => format!("bool:{b}"),
        ValueRef::Date(d) => format!("date:{d}"),
        ValueRef::DateTime(d) => format!("datetime:{d}"),
    }
}
/// Decode only the closed logical type vocabulary.
fn kind(text: &str) -> ValueType {
    match text {
        "int" => ValueType::Int,
        "float" => ValueType::Float,
        "str" => ValueType::Str,
        "bool" => ValueType::Bool,
        "date" => ValueType::Date,
        "datetime" => ValueType::DateTime,
        _ => panic!("invalid fixture type"),
    }
}
/// Return types deliberately exclude Boolean values.
fn result_kind(text: &str) -> ColumnType {
    match text {
        "int" => ColumnType::Int,
        "float" => ColumnType::Float,
        "str" => ColumnType::Str,
        "date" => ColumnType::Date,
        "datetime" => ColumnType::DateTime,
        _ => panic!("invalid result type"),
    }
}
/// Fixed resolved identity must accompany every failure without host rediscovery.
fn identity() -> FunctionIdentity {
    FunctionIdentity {
        name: "example".into(),
        contract_version: "1".into(),
        implementation_version: "2".into(),
        call: "artifact.example".into(),
    }
}
/// Build declarations in authored order, retaining the host mapping and defaults.
fn parameters(text: &str) -> Vec<Parameter> {
    if text == "-" {
        return vec![];
    }
    text.split(';')
        .map(|p| {
            let f: Vec<_> = p.split('/').collect();
            Parameter {
                name: f[0].into(),
                host_name: f[4].into(),
                kind: kind(f[1]),
                accepts_missing: f[2].parse().unwrap(),
                presence: if f[3] == "required" {
                    Presence::Required
                } else {
                    Presence::Optional(value(f[3]))
                },
            }
        })
        .collect()
}
/// A non-Clone host error demonstrates that payloads are moved, not reconstructed.
#[derive(Debug, PartialEq)]
struct Payload(String);
struct Port<'a> {
    action: &'a str,
    trace: Vec<String>,
    calls: usize,
}
impl FunctionPort for Port<'_> {
    type Error = Payload;
    /// Record one effect before returning a normalized scalar or host failure.
    fn call(&mut self, args: &[Argument<'_>]) -> Result<Value, HostError<Payload>> {
        self.calls += 1;
        self.trace.extend(
            args.iter()
                .map(|a| format!("{}={}", a.name, encode(a.value))),
        );
        if args.is_empty() {
            self.trace.push("called".into());
        }
        if self.action == "raise" {
            return Err(HostError::Raised(Payload("ValueError:boom".into())));
        }
        if let Some(reason) = self.action.strip_prefix("invalid:") {
            return Err(HostError::InvalidResult(Payload(reason.into())));
        }
        if let Some(name) = self.action.strip_prefix("echo:") {
            return Ok(match args.iter().find(|a| a.name == name).unwrap().value {
                ValueRef::Missing => Value::Missing,
                ValueRef::Str(s) => Value::Str(s.into()),
                ValueRef::Int(n) => Value::Int(n),
                ValueRef::Float(n) => Value::Float(n),
                ValueRef::Bool(b) => Value::Bool(b),
                ValueRef::Date(d) => Value::Date(d),
                ValueRef::DateTime(d) => Value::DateTime(d),
            });
        }
        Ok(value(self.action))
    }
}
/// Render portable failure facts and assert normative metadata before comparison.
fn failure(f: InvocationFailure<Payload>) -> String {
    assert_eq!(f.identity, identity());
    assert_eq!(f.phase(), "derivation");
    let (condition, requirement) = match &f.kind {
        FailureKind::UnknownArguments(_)
        | FailureKind::MissingRequired { .. }
        | FailureKind::ArgumentType { .. } => ("invalid_function_argument", "REQ-0700"),
        FailureKind::CallFailed(_) => ("function_call_failed", "REQ-0701"),
        _ => ("invalid_function_result", "REQ-0702"),
    };
    assert_eq!((f.condition(), f.requirement()), (condition, requirement));
    match f.kind {
        FailureKind::UnknownArguments(names) => format!("error:unknown:{}", names.join(",")),
        FailureKind::MissingRequired { parameter } => format!("error:missing:{parameter}"),
        FailureKind::ArgumentType {
            parameter,
            expected,
            actual,
        } => format!("error:argument:{parameter}:{expected:?}:{actual:?}").to_lowercase(),
        FailureKind::CallFailed(Payload(s)) => format!("error:raised:{s}"),
        FailureKind::InvalidHostResult(Payload(s)) => format!("error:invalid:{s}"),
        FailureKind::BooleanResult => "error:boolean".into(),
        FailureKind::UndeclaredMissing => "error:undeclared-missing".into(),
        FailureKind::ResultType { expected, actual } => {
            format!("error:result:{expected:?}:{actual:?}").to_lowercase()
        }
    }
}
/// Shared hand-written truth also runs through the real Python BoundFunction.
#[test]
fn shared_function_invocation_vectors() {
    let mut count = 0;
    for row in include_str!("fixtures/function_invocation.tsv")
        .lines()
        .skip(1)
    {
        count += 1;
        let f: Vec<_> = row.split('\t').collect();
        assert_eq!(f.len(), 8);
        let plan = InvocationPlan::new(
            identity(),
            parameters(f[1]),
            result_kind(f[4]),
            f[5].parse().unwrap(),
        )
        .unwrap();
        let supplied = if f[2] == "-" {
            BTreeMap::new()
        } else {
            f[2].split(';')
                .map(|s| {
                    let (k, v) = s.split_once('=').unwrap();
                    (k.into(), value(v))
                })
                .collect()
        };
        let mut port = Port {
            action: f[3],
            trace: vec![],
            calls: 0,
        };
        let actual = match plan.invoke(&supplied, &mut port) {
            Ok(v) => encode((&v).into()),
            Err(e) => failure(e),
        };
        assert_eq!(actual, f[6], "{}", f[0]);
        assert_eq!(port.calls, usize::from(f[7] != "-"), "{}", f[0]);
        assert_eq!(
            if port.trace.is_empty() {
                "-".into()
            } else {
                port.trace.join(";")
            },
            f[7],
            "{}",
            f[0]
        );
    }
    assert_eq!(count, 42);
}

/// No accepted plan can carry duplicate names, invalid defaults or empty identity.
#[test]
fn plans_reject_invalid_normalized_signatures() {
    let base = parameters("x/int/false/required/host_x");
    let build = |p| InvocationPlan::new(identity(), p, ColumnType::Int, false);
    for name in ["", "1x", "bad-name", "snow\u{96ea}"] {
        let mut p = base.clone();
        p[0].name = name.into();
        assert_eq!(build(p), Err(PlanError::InvalidName { parameter: 0 }));
    }
    assert_eq!(
        build(vec![base[0].clone(), base[0].clone()]),
        Err(PlanError::DuplicateName { parameter: 1 })
    );
    let mut p = vec![base[0].clone(), base[0].clone()];
    p[1].name = "y".into();
    assert_eq!(build(p), Err(PlanError::DuplicateHostName { parameter: 1 }));
    let mut p = base.clone();
    p[0].host_name.clear();
    assert_eq!(build(p), Err(PlanError::EmptyHostName { parameter: 0 }));
    for default in [Value::Missing, Value::float(1.0), Value::Bool(true)] {
        let mut p = base.clone();
        p[0].presence = Presence::Optional(default);
        assert_eq!(build(p), Err(PlanError::InvalidDefault { parameter: 0 }));
    }
    for slot in 0..4 {
        let mut id = identity();
        [
            &mut id.name,
            &mut id.contract_version,
            &mut id.implementation_version,
            &mut id.call,
        ][slot]
            .clear();
        assert_eq!(
            InvocationPlan::new(id, base.clone(), ColumnType::Int, false),
            Err(PlanError::EmptyIdentity)
        );
    }
}

/// Reuse repeats effects, including after failures; returned text owns its memory.
#[test]
fn synchronous_effects_are_never_retried_memoized_or_rolled_back() {
    let plan = InvocationPlan::new(
        identity(),
        parameters("x/str/false/required/host_x"),
        ColumnType::Str,
        false,
    )
    .unwrap();
    let mut port = Port {
        action: "echo:host_x",
        trace: vec![],
        calls: 0,
    };
    let owned = {
        let input = BTreeMap::from([("x".into(), Value::Str("owned".into()))]);
        plan.invoke(&input, &mut port).unwrap()
    };
    assert_eq!(owned, Value::Str("owned".into()));
    let input = BTreeMap::from([("x".into(), Value::Str("again".into()))]);
    for action in ["raise", "invalid:list", "int:1", "echo:host_x"] {
        port.action = action;
        let result = plan.invoke(&input, &mut port);
        assert_eq!(result.is_ok(), action == "echo:host_x");
    }
    assert_eq!(port.calls, 5);
    assert_eq!(port.trace.len(), 5);
    assert_eq!(plan.identity(), &identity());
}

/// Host-independent values retain precision until an actual host encoder consumes them.
#[test]
fn typed_port_retains_temporal_precision_and_needs_no_send_bound() {
    struct Local(std::rc::Rc<std::cell::Cell<usize>>);
    impl FunctionPort for Local {
        type Error = std::convert::Infallible;
        /// Observe the borrowed precision and return the same owned civil value.
        fn call(&mut self, args: &[Argument<'_>]) -> Result<Value, HostError<Self::Error>> {
            self.0.set(self.0.get() + 1);
            let ValueRef::Date(date) = args[0].value else {
                panic!("date")
            };
            assert_eq!(date.collected_precision(), DatePrecision::Year);
            Ok(Value::Date(date))
        }
    }
    let date = Date::new(2025, 1, 1, DatePrecision::Year).unwrap();
    let plan = InvocationPlan::new(
        identity(),
        parameters("x/date/false/required/host_x"),
        ColumnType::Date,
        false,
    )
    .unwrap();
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut port = Local(calls.clone());
    let result = plan
        .invoke(
            &BTreeMap::from([("x".into(), Value::Date(date))]),
            &mut port,
        )
        .unwrap();
    let Value::Date(result) = result else {
        panic!("date")
    };
    assert_eq!(result.collected_precision(), DatePrecision::Year);
    assert_eq!(calls.get(), 1);
}
