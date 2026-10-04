//! Invoke an already-bound function once with exact logical argument/result types.
//!
//! This trusted typed service does not discover code, decode requests, activate
//! environments, or validate host-language identifiers. Those are outer gates.
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use yamaa_core::{
    table::ValueRef,
    value::{ColumnType, Value, ValueType},
};

/// Resolved identity retained on every fatal invocation failure (REQ-0704).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionIdentity {
    pub name: String,
    pub contract_version: String,
    pub implementation_version: String,
    pub call: String,
}

/// Required parameters have no default; optional parameters always have one.
/// A missing default remains distinct from an absent default (REQ-0676).
#[derive(Clone, Debug, PartialEq)]
pub enum Presence {
    Required,
    Optional(Value),
}

/// One normalized parameter and its already-resolved host name.
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub host_name: String,
    pub kind: ValueType,
    pub accepts_missing: bool,
    pub presence: Presence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    EmptyIdentity,
    InvalidName { parameter: usize },
    DuplicateName { parameter: usize },
    EmptyHostName { parameter: usize },
    DuplicateHostName { parameter: usize },
    InvalidDefault { parameter: usize },
}

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

/// Synchronous already-bound callable; no Send/Sync/Clone requirement or retry.
/// Adapters normalize nonfinite returns, encode temporal arguments at the
/// REQ-0570 boundary, and contain host exceptions before returning here. The
/// owned result cannot borrow the input or callback's temporary host buffers.
pub trait FunctionPort {
    type Error;
    /// Invoke on the caller's thread with declaration-order mapped arguments.
    fn call(&mut self, arguments: &[Argument<'_>]) -> Result<Value, HostError<Self::Error>>;
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
pub struct InvocationFailure<E> {
    pub identity: FunctionIdentity,
    pub kind: FailureKind<E>,
}
impl<E> InvocationFailure<E> {
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

/// Immutable logical invocation plan. Admission budgets for materialized names,
/// values and callbacks belong to the caller; this is not an untrusted byte API.
#[derive(Clone, Debug, PartialEq)]
pub struct InvocationPlan {
    identity: FunctionIdentity,
    parameters: Vec<Parameter>,
    returns: ColumnType,
    may_return_missing: bool,
}

impl InvocationPlan {
    /// Validate the normalized closed signature without calling project code.
    /// Host identifier syntax, qualified callable resolution and activation are
    /// prerequisites owned by the selected host's environment compiler.
    pub fn new(
        identity: FunctionIdentity,
        parameters: Vec<Parameter>,
        returns: ColumnType,
        may_return_missing: bool,
    ) -> Result<Self, PlanError> {
        if [
            &identity.name,
            &identity.contract_version,
            &identity.implementation_version,
            &identity.call,
        ]
        .iter()
        .any(|s| s.is_empty())
        {
            return Err(PlanError::EmptyIdentity);
        }
        let mut names = BTreeSet::new();
        let mut host_names = BTreeSet::new();
        for (parameter, item) in parameters.iter().enumerate() {
            let mut bytes = item.name.bytes();
            if !bytes
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(PlanError::InvalidName { parameter });
            }
            if !names.insert(&item.name) {
                return Err(PlanError::DuplicateName { parameter });
            }
            if item.host_name.is_empty() {
                return Err(PlanError::EmptyHostName { parameter });
            }
            if !host_names.insert(&item.host_name) {
                return Err(PlanError::DuplicateHostName { parameter });
            }
            if let Presence::Optional(default) = &item.presence {
                if default
                    .value_type()
                    .map_or(!item.accepts_missing, |t| t != item.kind)
                {
                    return Err(PlanError::InvalidDefault { parameter });
                }
            }
        }
        Ok(Self {
            identity,
            parameters,
            returns,
            may_return_missing,
        })
    }

    /// Retain immutable identity for diagnostics surrounding this application step.
    pub fn identity(&self) -> &FunctionIdentity {
        &self.identity
    }

    /// Inspect the immutable logical signature for static argument binding.
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    /// Check unknown names first, then parameters in declaration order, then call
    /// once. Non-accepting missing short-circuits before later argument checks,
    /// matching the reference runtime; static call preflight is a separate gate.
    /// Reusing a plan repeats effects. Failures neither retry nor roll back a call.
    pub fn invoke<P: FunctionPort>(
        &self,
        supplied: &BTreeMap<String, Value>,
        port: &mut P,
    ) -> Result<Value, InvocationFailure<P::Error>> {
        let failure = |kind| InvocationFailure {
            identity: self.identity.clone(),
            kind,
        };
        let unknown: Vec<_> = supplied
            .keys()
            .filter(|name| !self.parameters.iter().any(|p| &p.name == *name))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            return Err(failure(FailureKind::UnknownArguments(unknown)));
        }
        let mut arguments = Vec::with_capacity(self.parameters.len());
        for parameter in &self.parameters {
            let value = match supplied.get(&parameter.name) {
                Some(value) => value,
                None => match &parameter.presence {
                    Presence::Required => {
                        return Err(failure(FailureKind::MissingRequired {
                            parameter: parameter.name.clone(),
                        }))
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
        let returned = port.call(&arguments).map_err(|error| {
            failure(match error {
                HostError::Raised(payload) => FailureKind::CallFailed(payload),
                HostError::InvalidResult(payload) => FailureKind::InvalidHostResult(payload),
            })
        })?;
        let actual = match returned.value_type() {
            None if self.may_return_missing => return Ok(returned),
            None => return Err(failure(FailureKind::UndeclaredMissing)),
            Some(ValueType::Bool) => return Err(failure(FailureKind::BooleanResult)),
            Some(actual) => actual,
        };
        let expected = match self.returns {
            ColumnType::Str => ValueType::Str,
            ColumnType::Int => ValueType::Int,
            ColumnType::Float => ValueType::Float,
            ColumnType::Date => ValueType::Date,
            ColumnType::DateTime => ValueType::DateTime,
        };
        if actual != expected {
            return Err(failure(FailureKind::ResultType {
                expected: self.returns,
                actual,
            }));
        }
        Ok(returned)
    }
}
