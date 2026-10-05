//! Bounded protocol for an explicit already-bound callable, not artifact activation.
use crate::{
    function_signature::{check_name, Identity, Kind, Signature, WireParameter, MAX_PARAMETERS},
    scalar_transport::ScalarValue,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    io::{self, Write},
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_core::value::{ColumnType, Value, ValueType};
use yamaa_engine::function_invocation::{
    FailureKind, FunctionPort, InvocationFailure, InvocationPlan,
};

pub const MAX_REQUEST_BYTES: usize = 1_048_576;
pub const MAX_RESULT_BYTES: usize = 1_048_576;
pub const MAX_OUTPUT_BYTES: usize = 8 * 1_048_576;
pub const MAX_ERROR_BYTES: usize = 8192;
const PROTOCOL: &str = "function/1";

/// Boundary failures are distinct from normative function conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionTransportError {
    RequestLimit,
    InvalidRequest,
    UnsupportedProtocol,
    InvalidScalar,
    InvalidSignature,
    InvalidHostName,
    OutputLimit,
    Internal,
    Interrupted,
}
impl fmt::Display for FunctionTransportError {
    /// Stable boundary messages never echo raw requests or panic payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RequestLimit => "function request exceeds resource limit",
            Self::InvalidRequest => "invalid function request",
            Self::UnsupportedProtocol => "unsupported function protocol",
            Self::InvalidScalar => "invalid function scalar",
            Self::InvalidSignature => "invalid function signature",
            Self::InvalidHostName => "invalid function host argument name",
            Self::OutputLimit => "function output exceeds resource limit",
            Self::Internal => "internal function transport failure",
            Self::Interrupted => "function callback interrupted",
        })
    }
}
impl std::error::Error for FunctionTransportError {}
type Error = FunctionTransportError;

/// Host-owned details; the transport enforces byte caps before serializing them.
#[derive(Debug)]
pub enum CallbackError {
    Exception {
        class: String,
        message: String,
        truncated: bool,
    },
    Rejected {
        reason: String,
        returned: Option<String>,
    },
    Boundary(Error),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    name: String,
    value: ScalarValue,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    identity: Identity,
    parameters: Vec<WireParameter>,
    returns: Kind,
    may_return_missing: bool,
    arguments: Vec<Binding>,
}

/// An admitted immutable request; labels are caller-supplied, not authenticated
/// evidence of artifact ownership. Production activation must bind that identity.
pub struct PreparedInvocation {
    plan: InvocationPlan,
    arguments: BTreeMap<String, Value>,
}
impl PreparedInvocation {
    /// Decode/admit the entire request before project code can be invoked.
    pub fn parse(request: &str) -> Result<Self, Error> {
        if request.len() > MAX_REQUEST_BYTES {
            return Err(Error::RequestLimit);
        }
        catch_unwind(|| Self::decode(request)).map_err(|_| Error::Internal)?
    }
    /// Apply fixed transport budgets and build the trusted application plan.
    fn decode(request: &str) -> Result<Self, Error> {
        let wire: Request = serde_json::from_str(request).map_err(|_| Error::InvalidRequest)?;
        if wire.protocol != PROTOCOL {
            return Err(Error::UnsupportedProtocol);
        }
        if wire.parameters.len() > MAX_PARAMETERS || wire.arguments.len() > MAX_PARAMETERS {
            return Err(Error::RequestLimit);
        }
        let plan = Signature {
            identity: wire.identity,
            parameters: wire.parameters,
            returns: wire.returns,
            may_return_missing: wire.may_return_missing,
        }
        .prepare()?;
        let mut arguments = BTreeMap::new();
        for binding in wire.arguments {
            check_name(&binding.name)?;
            let value = binding
                .value
                .into_core()
                .map_err(|_| Error::InvalidScalar)?;
            if arguments.insert(binding.name, value).is_some() {
                return Err(Error::InvalidRequest);
            }
        }
        Ok(Self { plan, arguments })
    }
    /// Inspect admitted host mappings for language-specific identifier checks.
    pub fn host_names(&self) -> impl Iterator<Item = &str> {
        self.plan.parameters().iter().map(|p| p.host_name.as_str())
    }
    /// Invoke synchronously once and contain unwind failures without replay.
    /// A callback may have changed host state before any returned failure. This
    /// boundary does not resume or reuse an interrupted evaluation's temporary state.
    pub fn invoke<P: FunctionPort<Error = CallbackError>>(
        &self,
        port: &mut P,
    ) -> Result<String, Error> {
        catch_unwind(AssertUnwindSafe(|| {
            let outcome = match self.plan.invoke(&self.arguments, port) {
                Ok(value) => {
                    if let Value::Str(text) = &value {
                        if text.len() > MAX_RESULT_BYTES {
                            return Err(Error::OutputLimit);
                        }
                    }
                    Outcome::Value {
                        value: ScalarValue::from_core(value),
                    }
                }
                Err(failure) => Outcome::Condition {
                    diagnostic: diagnostic(failure)?,
                },
            };
            let mut writer = LimitedWriter(Vec::new());
            serde_json::to_writer(
                &mut writer,
                &Envelope {
                    protocol: PROTOCOL,
                    outcome,
                },
            )
            .map_err(|_| Error::OutputLimit)?;
            String::from_utf8(writer.0).map_err(|_| Error::Internal)
        }))
        .map_err(|_| Error::Internal)?
    }
}
#[derive(Serialize)]
struct Envelope {
    protocol: &'static str,
    outcome: Outcome,
}
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum Outcome {
    Value { value: ScalarValue },
    Condition { diagnostic: Diagnostic },
}
#[derive(Serialize)]
pub(crate) struct Diagnostic {
    phase: &'static str,
    condition: &'static str,
    requirement: &'static str,
    applicable_handler: Option<&'static str>,
    context: BTreeMap<&'static str, serde_json::Value>,
}
/// Render facts already validated by the service; no error string is executable.
pub(crate) fn diagnostic(failure: InvocationFailure<CallbackError>) -> Result<Diagnostic, Error> {
    use serde_json::{json, Value as Json};
    let mut result = Diagnostic {
        phase: failure.phase(),
        condition: failure.condition(),
        requirement: failure.requirement(),
        applicable_handler: None,
        context: BTreeMap::new(),
    };
    let context = &mut result.context;
    context.insert("function", json!(failure.identity.name));
    context.insert("contract_version", json!(failure.identity.contract_version));
    context.insert(
        "implementation_version",
        json!(failure.identity.implementation_version),
    );
    match failure.kind {
        FailureKind::UnknownArguments(names) => {
            context.insert("unknown", json!(names));
        }
        FailureKind::MissingRequired { parameter } => {
            context.insert("parameter", json!(parameter));
            context.insert("reason", json!("a required argument was not supplied"));
        }
        FailureKind::ArgumentType {
            parameter,
            expected,
            actual,
        } => {
            context.insert("parameter", json!(parameter));
            context.insert("expected", json!(type_name(expected)));
            context.insert("actual", json!(type_name(actual)));
        }
        FailureKind::CallFailed(e) | FailureKind::InvalidHostResult(e) => match e {
            CallbackError::Boundary(e) => return Err(e),
            CallbackError::Exception {
                class,
                message,
                truncated,
            } => {
                if class.len() > MAX_ERROR_BYTES || message.len() > MAX_ERROR_BYTES {
                    return Err(Error::OutputLimit);
                }
                context.insert("call", json!(failure.identity.call));
                context.insert("host_error", json!(class));
                context.insert("host_message", json!(message));
                if truncated {
                    context.insert("host_details_truncated", Json::Bool(true));
                }
            }
            CallbackError::Rejected { reason, returned } => {
                if reason.len() > MAX_ERROR_BYTES
                    || returned.as_ref().is_some_and(|s| s.len() > MAX_ERROR_BYTES)
                {
                    return Err(Error::OutputLimit);
                }
                context.insert("reason", json!(reason));
                if let Some(returned) = returned {
                    context.insert("returned", json!(returned));
                }
            }
        },
        FailureKind::BooleanResult => {
            context.insert("reason", json!("a binding returned a Boolean"));
            context.insert("returned", json!("bool"));
        }
        FailureKind::UndeclaredMissing => {
            context.insert(
                "reason",
                json!("an invoked binding returned an undeclared missing"),
            );
        }
        FailureKind::ResultType { expected, actual } => {
            let expected = match expected {
                ColumnType::Str => "str",
                ColumnType::Int => "int",
                ColumnType::Float => "float",
                ColumnType::Date => "date",
                ColumnType::DateTime => "datetime",
            };
            context.insert("expected", json!(expected));
            context.insert("actual", json!(type_name(actual)));
        }
    }
    Ok(result)
}
/// One spelling shared by argument and result diagnostic type facts.
fn type_name(kind: ValueType) -> &'static str {
    match kind {
        ValueType::Str => "str",
        ValueType::Int => "int",
        ValueType::Float => "float",
        ValueType::Bool => "bool",
        ValueType::Date => "date",
        ValueType::DateTime => "datetime",
    }
}
struct LimitedWriter(Vec<u8>);
impl Write for LimitedWriter {
    /// Check escaped JSON growth before allocating/appending each output segment.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_OUTPUT_BYTES - self.0.len() {
            return Err(io::Error::other("output limit"));
        }
        self.0.try_reserve(bytes.len()).map_err(io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    /// Owned memory has no external flush action.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
