//! Installed R metadata and namespace capabilities on the calling R thread,
//! behind the client's callbacks. Original condition objects are rooted in
//! the objects the engine keeps; no longjmp crosses Rust because installed
//! host helpers return closed reply envelopes. Project functions receive and
//! return plain R values: arguments convert before the call and results
//! convert by the declared column type.
use extendr_api::prelude::*;
use yamaa_boundary::client::{
    Argument, CallFailure, Callbacks, ColumnType, Failure, FailureKind, Identity, LockFinding,
    Scalar, Signature,
};

use crate::thread_bound::ThreadBound;

/// A host object the engine keeps: a bound R function with its declared
/// result type, or an original condition, rooted while the engine keeps the
/// failure that carries it and never read back.
pub enum Object {
    Function(ThreadBound<Function>, ColumnType),
    Condition { _rooted: ThreadBound<Robj> },
}

type HostFailure = Failure<Object>;

/// The installed capabilities: lock verification, binding resolution and
/// host condition rendering. Constructing them does not query versions,
/// load project namespaces or invoke any project function.
pub struct Functions {
    verify: Function,
    resolve: Function,
    formatter: Function,
}

impl Functions {
    pub fn new(verify: Function, resolve: Function, formatter: Function) -> Self {
        Self {
            verify,
            resolve,
            formatter,
        }
    }
}

/// The callbacks of one call: the R interrupt probe, and the installed
/// project functions when the call activates them.
pub struct Host {
    functions: Option<Functions>,
}

impl Host {
    pub fn new(functions: Option<Functions>) -> Self {
        Self { functions }
    }

    /// Whether the call offers the engine project functions.
    pub fn offers_functions(&self) -> bool {
        self.functions.is_some()
    }

    fn functions(&self) -> Result<&Functions, HostFailure> {
        self.functions.as_ref().ok_or_else(Failure::protocol)
    }
}

fn condition(value: Robj) -> Option<Object> {
    Some(Object::Condition {
        _rooted: ThreadBound::new(value),
    })
}

/// One closed host reply: a completed payload or a classified failure status.
enum Status {
    Completed(Robj),
    Host(Robj),
    Interrupt(Robj),
}

fn sdk_payload(error: &extendr_api::Error) -> Option<Object> {
    match error {
        extendr_api::Error::EvalError(value) | extendr_api::Error::Panic(value) => {
            condition(value.clone())
        }
        _ => None,
    }
}

fn sdk_failure(error: &extendr_api::Error) -> HostFailure {
    Failure {
        payload: sdk_payload(error),
        ..Failure::new(FailureKind::SdkBoundary, "")
    }
}

fn call_status(function: &Function, arguments: Pairlist) -> Result<Status, HostFailure> {
    let result = function
        .call(arguments)
        .map_err(|error| sdk_failure(&error))?;
    let result = result
        .as_list()
        .filter(|list| list.len() == 2)
        .ok_or_else(Failure::protocol)?;
    let status = result
        .elt(0)
        .map_err(|error| sdk_failure(&error))?
        .as_integer()
        .ok_or_else(Failure::protocol)?;
    let payload = result.elt(1).map_err(|error| sdk_failure(&error))?;
    match status {
        0 => Ok(Status::Completed(payload)),
        1 => Ok(Status::Host(payload)),
        3 => Ok(Status::Interrupt(payload)),
        _ => Err(Failure::protocol()),
    }
}

fn text(value: &Robj) -> Result<String, HostFailure> {
    let raw = value.as_raw().ok_or_else(Failure::protocol)?;
    std::str::from_utf8(raw.as_slice())
        .map(str::to_owned)
        .map_err(|_| Failure::protocol())
}

fn optional_text(value: &Robj) -> Result<Option<String>, HostFailure> {
    if value.is_null() {
        Ok(None)
    } else {
        text(value).map(Some)
    }
}

/// The installed helper renders only explicitly ordinary conditions. A failed
/// secondary renderer cannot replace the original payload in the attempt.
fn ordinary(condition: &Robj, formatter: &Function) -> Option<(String, String)> {
    let reply = formatter
        .call(pairlist!(condition = condition.clone()))
        .ok()?;
    let reply = reply.as_list().filter(|reply| reply.len() == 2)?;
    let class = crate::function_callback::detail(&reply.elt(0).ok()?, "R condition").ok()?;
    let message =
        crate::function_callback::detail(&reply.elt(1).ok()?, "unavailable R condition message")
            .ok()?;
    Some((class, message))
}

fn classify_host(formatter: &Function, raised: &Robj) -> HostFailure {
    match ordinary(raised, formatter) {
        Some((class, message)) => Failure {
            kind: FailureKind::Exception,
            class,
            message,
            returned: None,
            payload: condition(raised.clone()),
        },
        None => Failure::new(FailureKind::Internal, "host_classification"),
    }
}

/// Unwrap a completed reply, classifying a raised condition or an interrupt.
fn completed(formatter: &Function, status: Status) -> Result<Robj, HostFailure> {
    match status {
        Status::Completed(payload) => Ok(payload),
        Status::Host(raised) => Err(classify_host(formatter, &raised)),
        Status::Interrupt(raised) => {
            crate::interrupt::remember(raised.clone());
            Err(Failure {
                payload: condition(raised),
                ..Failure::new(FailureKind::Interrupted, "")
            })
        }
    }
}

/// Plain R arguments in declaration order, named for the host call. A value
/// R cannot hold exactly fails the call before any project code runs.
fn plain_arguments<'a>(
    arguments: &[Argument<'a>],
) -> Result<Vec<(&'a str, Robj)>, CallFailure<Object>> {
    arguments
        .iter()
        .map(|argument| {
            crate::function_callback::argument(argument.value)
                .map(|value| (argument.name, value))
                .map_err(|reason| {
                    let message = format!("argument {}: {reason}", argument.name);
                    CallFailure::Raised(Failure::new(FailureKind::Representation, message))
                })
        })
        .collect()
}

/// Convert a returned plain R value by the declared column type; any other
/// value is an invalid result with the reason it was refused.
fn plain_result(value: &Robj, returns: ColumnType) -> Result<Scalar, CallFailure<Object>> {
    crate::function_callback::result(value, returns)
        .map_err(|message| CallFailure::Invalid(Failure::new(FailureKind::Representation, message)))
}

/// The bound function an object holds, on the R thread.
fn bound(function: &Object) -> Result<(&Function, ColumnType), CallFailure<Object>> {
    let thread = || CallFailure::Raised(Failure::new(FailureKind::Internal, "host_thread"));
    match function {
        Object::Function(function, returns) => Ok((function.get().ok_or_else(thread)?, *returns)),
        Object::Condition { .. } => Err(CallFailure::Raised(Failure::protocol())),
    }
}

impl Callbacks for Host {
    type Object = Object;

    fn cancelled(&self) -> bool {
        crate::interrupt::poll(false)
    }

    fn bind(
        &self,
        identity: &Identity<'_>,
        signature: &Signature<'_>,
    ) -> Result<Object, HostFailure> {
        let functions = self.functions()?;
        let names = List::from_values(
            signature
                .parameters
                .iter()
                .map(|parameter| Raw::from_bytes(parameter.as_bytes())),
        );
        let status = call_status(
            &functions.resolve,
            pairlist!(
                call = Raw::from_bytes(identity.call.as_bytes()),
                parameters = names
            ),
        )?;
        completed(&functions.formatter, status)?
            .as_function()
            .ok_or_else(Failure::protocol)
            .map(|function| Object::Function(ThreadBound::new(function), signature.returns))
    }

    fn invoke(
        &self,
        function: &Object,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallFailure<Object>> {
        let functions = self.functions().map_err(CallFailure::Raised)?;
        let (function, returns) = bound(function)?;
        let arguments = List::from_pairs(plain_arguments(arguments)?);
        let status =
            call_status(function, pairlist!(arguments = arguments)).map_err(CallFailure::Raised)?;
        let value = completed(&functions.formatter, status).map_err(CallFailure::Raised)?;
        plain_result(&value, returns)
    }

    fn compare_locked(
        &self,
        packages: &[(&str, &str)],
        calls: &[&str],
    ) -> Result<Vec<LockFinding>, HostFailure> {
        let functions = self.functions()?;
        let versions = List::from_pairs(
            packages
                .iter()
                .map(|(name, version)| (*name, Robj::from(Raw::from_bytes(version.as_bytes())))),
        );
        let calls = List::from_values(calls.iter().map(|call| Raw::from_bytes(call.as_bytes())));
        let status = call_status(
            &functions.verify,
            pairlist!(versions = versions, calls = calls),
        )?;
        let rows = completed(&functions.formatter, status)?;
        let rows = rows.as_list().ok_or_else(Failure::protocol)?;
        rows.iter()
            .map(|(_, row)| {
                let row = row
                    .as_list()
                    .filter(|row| row.len() == 4)
                    .ok_or_else(Failure::protocol)?;
                let cell = |index: usize| row.elt(index).map_err(|_| Failure::protocol());
                Ok(LockFinding {
                    package: text(&cell(0)?)?,
                    reason: text(&cell(1)?)?,
                    expected: optional_text(&cell(2)?)?,
                    actual: optional_text(&cell(3)?)?,
                })
            })
            .collect()
    }
}

/// Caller-supplied registry port: bindings come from an explicit map of
/// logical call strings to already-resolved R functions, never from a
/// namespace lookup. Declared-lock auditing stays separate: this port offers
/// no lock verification, so the engine refuses one rather than silently
/// skipping it. It is staged for the registry activation path; dataset
/// execution still uses the locked port, so no entry point in this crate
/// constructs it yet.
#[allow(dead_code)]
pub struct RegistryPort {
    registry: std::collections::BTreeMap<String, ThreadBound<Function>>,
}

#[allow(dead_code)]
impl RegistryPort {
    pub fn new(registry: std::collections::BTreeMap<String, Function>) -> Self {
        Self {
            registry: registry
                .into_iter()
                .map(|(call, handle)| (call, ThreadBound::new(handle)))
                .collect(),
        }
    }
}

impl Callbacks for RegistryPort {
    type Object = Object;

    fn bind(
        &self,
        identity: &Identity<'_>,
        signature: &Signature<'_>,
    ) -> Result<Object, HostFailure> {
        let bound = self
            .registry
            .get(identity.call)
            .ok_or_else(|| Failure::new(FailureKind::MissingBinding, identity.call))?;
        let thread = || Failure::new(FailureKind::Internal, "host_thread");
        let handle = bound.get().ok_or_else(thread)?.clone();
        // Parity with the Python registry port: the registered function's
        // formals must name exactly the logical parameters.
        let formals = handle.formals().ok_or_else(Failure::protocol)?;
        let actual: std::collections::BTreeSet<&str> = formals.names().collect();
        let expected: std::collections::BTreeSet<&str> =
            signature.parameters.iter().copied().collect();
        if actual.contains("...") || actual != expected {
            return Err(Failure::protocol());
        }
        Ok(Object::Function(
            ThreadBound::new(handle),
            signature.returns,
        ))
    }

    fn invoke(
        &self,
        function: &Object,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallFailure<Object>> {
        // A registered function is called directly with its named plain
        // arguments. With no installed reply envelope or formatter, a raised
        // R condition stays an opaque payload; the core still validates the
        // converted result before accepting any output.
        let (function, returns) = bound(function)?;
        let value = function
            .call(Pairlist::from_pairs(plain_arguments(arguments)?))
            .map_err(|error| CallFailure::Raised(sdk_failure(&error)))?;
        plain_result(&value, returns)
    }
}
