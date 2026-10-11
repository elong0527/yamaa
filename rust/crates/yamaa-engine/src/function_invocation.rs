//! Invoke an already-bound function once with exact logical argument/result types.
//!
//! This trusted typed service does not discover code, decode requests, activate
//! environments, or validate host-language identifiers. Those are outer gates.
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use yamaa_core::{
    table::ValueRef,
    value::{ColumnType, Value, ValueType},
};

pub use yamaa_core::function_signature::{
    FunctionIdentity, InvocationPlan, LogicalSignature, Parameter, PlanError, Presence,
    ProjectFunctionIdentity, ProjectInvocationPlan,
};

/// A borrowed mapped argument; callbacks receive nothing beyond this ordered list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Argument<'a> {
    pub name: &'a str,
    pub value: ValueRef<'a>,
}

/// Preserve opaque host failures separately from rejected host representations.
#[derive(Debug, PartialEq)]
pub enum HostError<E> {
    Raised(E),
    InvalidResult(E),
}

/// Stable activated callable slots; no Send/Sync/Clone requirement or retry.
/// Adapters normalize nonfinite returns, encode temporal arguments at the
/// REQ-0570 boundary, and contain host exceptions before returning here. The
/// owned result cannot borrow the input or callback's temporary host buffers.
pub trait FunctionPort {
    type Error;
    /// Borrow resolved metadata without executing project code. A prebound
    /// scalar-only port may leave metadata unavailable; dataset admission checks
    /// every selected slot before observing source cardinality or cells.
    fn signature(&self, _slot: usize) -> Option<&InvocationPlan> {
        None
    }
    /// Borrow an activated versionless package signature without host effects.
    fn project_signature(&self, _slot: usize) -> Option<&ProjectInvocationPlan> {
        None
    }
    /// Invoke on the caller's thread with declaration-order mapped arguments.
    fn call(
        &mut self,
        slot: usize,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>>;
}

/// No implicit function authority, shared by every non-function execution path.
pub struct NoFunctions<E>(core::marker::PhantomData<E>);
impl<E> NoFunctions<E> {
    pub const fn new() -> Self {
        Self(core::marker::PhantomData)
    }
}
impl<E> Default for NoFunctions<E> {
    fn default() -> Self {
        Self::new()
    }
}
impl<E> FunctionPort for NoFunctions<E> {
    type Error = E;
    fn call(&mut self, _: usize, _: &[Argument<'_>]) -> Result<Value, HostError<E>> {
        unreachable!("function metadata was admitted before source access")
    }
}

#[derive(Debug, PartialEq)]
pub enum FailureKind<E> {
    UnknownArguments(Vec<String>),
    MissingRequired {
        parameter: String,
    },
    ArgumentType {
        parameter: String,
        expected: ValueType,
        actual: ValueType,
    },
    CallFailed(E),
    InvalidHostResult(E),
    BooleanResult,
    UndeclaredMissing,
    ResultType {
        expected: ColumnType,
        actual: ValueType,
    },
}

/// Fatal failures bypass conversion handlers and retain the exact host payload.
#[derive(Debug, PartialEq)]
pub struct InvocationFailure<E, I = FunctionIdentity> {
    pub identity: I,
    pub kind: FailureKind<E>,
}
impl<E, I> InvocationFailure<E, I> {
    /// Every runtime function failure belongs to derivation, before conversion.
    pub fn phase(&self) -> &'static str {
        "derivation"
    }
    /// Return portable condition vocabulary without flattening diagnostic facts.
    pub fn condition(&self) -> &'static str {
        match self.kind {
            FailureKind::UnknownArguments(_)
            | FailureKind::MissingRequired { .. }
            | FailureKind::ArgumentType { .. } => "invalid_function_argument",
            FailureKind::CallFailed(_) => "function_call_failed",
            _ => "invalid_function_result",
        }
    }
    /// Name the normative owner of this failure; no handler can repair it.
    pub fn requirement(&self) -> &'static str {
        match self.kind {
            FailureKind::UnknownArguments(_)
            | FailureKind::MissingRequired { .. }
            | FailureKind::ArgumentType { .. } => "REQ-0700",
            FailureKind::CallFailed(_) => "REQ-0701",
            _ => "REQ-0702",
        }
    }
}

/// Check unknown names first, then parameters in declaration order, then call
/// once. Non-accepting missing short-circuits before later argument checks,
/// matching the reference runtime; static call preflight is a separate gate.
/// Reusing a plan repeats effects. Failures neither retry nor roll back a call.
pub fn invoke<P: FunctionPort>(
    plan: &InvocationPlan,
    supplied: &BTreeMap<String, Value>,
    port: &mut P,
) -> Result<Value, InvocationFailure<P::Error>> {
    invoke_at(plan, supplied, 0, port)
}

/// Invoke one admitted immutable callback slot through the scalar lifecycle.
pub fn invoke_at<P: FunctionPort + ?Sized>(
    plan: &InvocationPlan,
    supplied: &BTreeMap<String, Value>,
    slot: usize,
    port: &mut P,
) -> Result<Value, InvocationFailure<P::Error>> {
    invoke_signature(plan.identity(), plan.signature(), supplied, slot, port)
}

/// Run a versionless admitted package function through the same scalar semantics.
/// Lock verification and this build's complete test suite are prior engine gates.
pub fn invoke_project<P: FunctionPort>(
    plan: &ProjectInvocationPlan,
    supplied: &BTreeMap<String, Value>,
    port: &mut P,
) -> Result<Value, InvocationFailure<P::Error, ProjectFunctionIdentity>> {
    invoke_project_at(plan, supplied, 0, port)
}

/// Invoke one activated package slot without resolving or activating host code.
pub fn invoke_project_at<P: FunctionPort + ?Sized>(
    plan: &ProjectInvocationPlan,
    supplied: &BTreeMap<String, Value>,
    slot: usize,
    port: &mut P,
) -> Result<Value, InvocationFailure<P::Error, ProjectFunctionIdentity>> {
    invoke_signature(plan.identity(), plan.signature(), supplied, slot, port)
}

fn invoke_signature<P: FunctionPort + ?Sized, I: Clone>(
    identity: &I,
    signature: &LogicalSignature,
    supplied: &BTreeMap<String, Value>,
    slot: usize,
    port: &mut P,
) -> Result<Value, InvocationFailure<P::Error, I>> {
    let failure = |kind| InvocationFailure {
        identity: identity.clone(),
        kind,
    };
    let unknown: Vec<_> = supplied
        .keys()
        .filter(|name| !signature.parameters().iter().any(|p| &p.name == *name))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return Err(failure(FailureKind::UnknownArguments(unknown)));
    }
    let mut arguments = Vec::with_capacity(signature.parameters().len());
    for parameter in signature.parameters() {
        let value = match supplied.get(&parameter.name) {
            Some(value) => value,
            None => match &parameter.presence {
                Presence::Required => {
                    return Err(failure(FailureKind::MissingRequired {
                        parameter: parameter.name.clone(),
                    }));
                }
                Presence::Optional(default) => default,
            },
        };
        if let Some(actual) = value.value_type() {
            if actual != parameter.kind {
                return Err(failure(FailureKind::ArgumentType {
                    parameter: parameter.name.clone(),
                    expected: parameter.kind,
                    actual,
                }));
            }
        } else if !parameter.accepts_missing {
            return Ok(Value::Missing);
        }
        arguments.push(Argument {
            name: &parameter.host_name,
            value: value.into(),
        });
    }
    let returned = port.call(slot, &arguments).map_err(|error| {
        failure(match error {
            HostError::Raised(payload) => FailureKind::CallFailed(payload),
            HostError::InvalidResult(payload) => FailureKind::InvalidHostResult(payload),
        })
    })?;
    let actual = match returned.value_type() {
        None if signature.may_return_missing() => return Ok(returned),
        None => return Err(failure(FailureKind::UndeclaredMissing)),
        Some(ValueType::Bool) => return Err(failure(FailureKind::BooleanResult)),
        Some(actual) => actual,
    };
    let expected = match signature.returns() {
        ColumnType::Str => ValueType::Str,
        ColumnType::Int => ValueType::Int,
        ColumnType::Float => ValueType::Float,
        ColumnType::Date => ValueType::Date,
        ColumnType::DateTime => ValueType::DateTime,
    };
    if actual != expected {
        return Err(failure(FailureKind::ResultType {
            expected: signature.returns(),
            actual,
        }));
    }
    Ok(returned)
}
