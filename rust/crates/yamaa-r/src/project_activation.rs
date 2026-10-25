//! Installed R metadata and namespace capabilities on the calling R thread,
//! behind the facade's project-functions port. Original condition objects
//! are rooted in owned error payloads; no longjmp
//! crosses Rust because installed host helpers return closed reply envelopes.
//! Project functions receive and return plain R values: arguments convert
//! before the call and results convert by the declared column type.
use extendr_api::prelude::*;
use std::sync::Mutex;
use yamaa::host::functions::{
    Argument, CallError, ColumnType, FunctionError, FunctionHandle, FunctionIdentity, HostPayload,
    LockContext, LockFinding, LockKind, LockReason, Signature,
};
use yamaa::scalar::Scalar;
use yamaa::{HostLanguage, ProjectFunctions};

use crate::thread_bound::ThreadBound;

/// The value `bound` holds, on the R thread that made it; any other thread
/// gets the `host_thread` failure instead of touching R.
fn on_thread<T>(bound: &ThreadBound<T>) -> Result<&T, FunctionError> {
    bound.get().ok_or(FunctionError::Internal {
        code: "host_thread",
    })
}

/// Installed capabilities: lock verification, binding resolution and host
/// condition rendering. Constructing this host port does not query versions,
/// load project namespaces or invoke any project function.
pub struct Functions {
    verify: Mutex<ThreadBound<Function>>,
    resolve: Mutex<ThreadBound<Function>>,
    formatter: Mutex<ThreadBound<Function>>,
}

impl Functions {
    pub fn new(verify: Function, resolve: Function, formatter: Function) -> Self {
        Self {
            verify: Mutex::new(ThreadBound::new(verify)),
            resolve: Mutex::new(ThreadBound::new(resolve)),
            formatter: Mutex::new(ThreadBound::new(formatter)),
        }
    }
}

/// One bound R callable and the declared column type its result converts to.
struct Bound {
    function: ThreadBound<Function>,
    returns: ColumnType,
}

impl Bound {
    fn handle(function: Function, returns: ColumnType) -> FunctionHandle {
        FunctionHandle::new(Self {
            function: ThreadBound::new(function),
            returns,
        })
    }

    /// Borrow the callable and its declared result type on the owning thread.
    fn get(handle: &FunctionHandle) -> std::result::Result<(Function, ColumnType), CallError> {
        handle
            .with::<Self, _>(|bound| {
                on_thread(&bound.function).map(|function| (function.clone(), bound.returns))
            })
            .ok_or(CallError::Raised(FunctionError::Protocol))?
            .map_err(CallError::Raised)
    }
}

/// One closed host reply: a completed payload or a classified failure status.
enum Status {
    Completed(Robj),
    Host(Robj),
    Interrupt(Robj),
}

fn sdk_payload(error: &extendr_api::Error) -> Option<HostPayload> {
    let condition = match error {
        extendr_api::Error::EvalError(value) | extendr_api::Error::Panic(value) => {
            Some(value.clone())
        }
        _ => None,
    };
    condition.map(|condition| HostPayload::new(ThreadBound::new(condition)))
}

fn call_status(
    function: &Function,
    arguments: Pairlist,
) -> std::result::Result<Status, FunctionError> {
    let result = function
        .call(arguments)
        .map_err(|error| FunctionError::SdkBoundary {
            payload: sdk_payload(&error),
        })?;
    let result = result
        .as_list()
        .filter(|list| list.len() == 2)
        .ok_or(FunctionError::Protocol)?;
    let status = result
        .elt(0)
        .map_err(|error| FunctionError::SdkBoundary {
            payload: sdk_payload(&error),
        })?
        .as_integer()
        .ok_or(FunctionError::Protocol)?;
    let payload = result.elt(1).map_err(|error| FunctionError::SdkBoundary {
        payload: sdk_payload(&error),
    })?;
    match status {
        0 => Ok(Status::Completed(payload)),
        1 => Ok(Status::Host(payload)),
        3 => Ok(Status::Interrupt(payload)),
        _ => Err(FunctionError::Protocol),
    }
}

fn text(value: &Robj) -> std::result::Result<String, FunctionError> {
    let raw = value.as_raw().ok_or(FunctionError::Protocol)?;
    std::str::from_utf8(raw.as_slice())
        .map(str::to_owned)
        .map_err(|_| FunctionError::Protocol)
}

fn optional_text(value: &Robj) -> std::result::Result<Option<String>, FunctionError> {
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

fn classify_host(formatter: &Function, condition: &Robj) -> FunctionError {
    match ordinary(condition, formatter) {
        Some((class, message)) => FunctionError::Exception {
            class,
            message,
            payload: Some(HostPayload::new(ThreadBound::new(condition.clone()))),
        },
        None => FunctionError::Internal {
            code: "host_classification",
        },
    }
}

/// Unwrap a completed reply, classifying a raised condition or an interrupt.
fn completed(formatter: &Function, status: Status) -> std::result::Result<Robj, FunctionError> {
    match status {
        Status::Completed(payload) => Ok(payload),
        Status::Host(condition) => Err(classify_host(formatter, &condition)),
        Status::Interrupt(condition) => {
            crate::interrupt::remember(condition.clone());
            Err(FunctionError::Interrupted {
                payload: Some(HostPayload::new(ThreadBound::new(condition))),
            })
        }
    }
}

/// Plain R arguments in declaration order, named for the host call. A value
/// R cannot hold exactly fails the call before any project code runs.
fn plain_arguments<'a>(
    arguments: &[Argument<'a>],
) -> std::result::Result<Vec<(&'a str, Robj)>, CallError> {
    arguments
        .iter()
        .map(|argument| {
            crate::function_callback::argument(argument.value)
                .map(|value| (argument.name, value))
                .map_err(|reason| {
                    CallError::Raised(FunctionError::Representation {
                        message: format!("argument {}: {reason}", argument.name),
                        payload: None,
                    })
                })
        })
        .collect()
}

/// Convert a returned plain R value by the declared column type; any other
/// value is an invalid result with the reason it was refused.
fn plain_result(value: &Robj, returns: ColumnType) -> std::result::Result<Scalar, CallError> {
    crate::function_callback::result(value, returns).map_err(|message| {
        CallError::InvalidResult(FunctionError::Representation {
            message,
            payload: None,
        })
    })
}

impl ProjectFunctions for Functions {
    fn verify_lock(
        &self,
        lock: &LockContext<'_>,
        functions: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        if lock.language != HostLanguage::R || lock.kind != LockKind::Renv {
            return Err(FunctionError::Protocol);
        }
        let locked = yamaa::host::functions::decode_renv_lock(lock.bytes)
            .map_err(FunctionError::LockSyntax)?;
        let verify_guard = self
            .verify
            .lock()
            .map_err(|_| FunctionError::Internal { code: "host_port" })?;
        let verify = on_thread(&verify_guard)?;
        let formatter_guard = self
            .formatter
            .lock()
            .map_err(|_| FunctionError::Internal { code: "host_port" })?;
        let formatter = on_thread(&formatter_guard)?;
        let versions = List::from_pairs(locked.packages.iter().map(|(name, version)| {
            (
                name.as_str(),
                Robj::from(Raw::from_bytes(version.as_bytes())),
            )
        }));
        let calls = List::from_values(
            functions
                .iter()
                .map(|function| Raw::from_bytes(function.call.as_bytes())),
        );
        let status = call_status(verify, pairlist!(versions = versions, calls = calls))?;
        let rows = completed(formatter, status)?;
        let rows = rows.as_list().ok_or(FunctionError::Protocol)?;
        let mut findings = Vec::with_capacity(rows.len());
        for (_, row) in rows.iter() {
            let row = row
                .as_list()
                .filter(|row| row.len() == 4)
                .ok_or(FunctionError::Protocol)?;
            let actual = |index: usize| {
                row.elt(index)
                    .map_err(|_| FunctionError::Protocol)
                    .and_then(|value| {
                        if value.is_null() {
                            Ok(None)
                        } else {
                            text(&value).map(Some)
                        }
                    })
            };
            findings.push(LockFinding::new(
                text(&row.elt(0).map_err(|_| FunctionError::Protocol)?)?,
                LockReason::parse(&text(&row.elt(1).map_err(|_| FunctionError::Protocol)?)?)
                    .ok_or(FunctionError::Protocol)?,
                optional_text(&row.elt(2).map_err(|_| FunctionError::Protocol)?)?
                    .into_iter()
                    .collect(),
                actual(3)?,
            ));
        }
        if findings.is_empty() {
            Ok(())
        } else {
            Err(FunctionError::Lock(findings))
        }
    }

    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &Signature,
    ) -> Result<FunctionHandle, FunctionError> {
        let resolve_guard = self
            .resolve
            .lock()
            .map_err(|_| FunctionError::Internal { code: "host_port" })?;
        let resolve = on_thread(&resolve_guard)?;
        let formatter_guard = self
            .formatter
            .lock()
            .map_err(|_| FunctionError::Internal { code: "host_port" })?;
        let formatter = on_thread(&formatter_guard)?;
        let names = List::from_values(
            signature
                .parameters
                .iter()
                .map(|parameter| Raw::from_bytes(parameter.name.as_bytes())),
        );
        let status = call_status(
            resolve,
            pairlist!(
                call = Raw::from_bytes(identity.call.as_bytes()),
                parameters = names
            ),
        )?;
        completed(formatter, status)?
            .as_function()
            .ok_or(FunctionError::Protocol)
            .map(|function| Bound::handle(function, signature.returns))
    }

    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> std::result::Result<Scalar, CallError> {
        let (function, returns) = Bound::get(handle)?;
        let formatter_guard = self
            .formatter
            .lock()
            .map_err(|_| CallError::Raised(FunctionError::Internal { code: "host_port" }))?;
        let formatter = on_thread(&formatter_guard).map_err(CallError::Raised)?;
        let arguments = List::from_pairs(plain_arguments(arguments)?);
        let status =
            call_status(&function, pairlist!(arguments = arguments)).map_err(CallError::Raised)?;
        let value = completed(formatter, status).map_err(CallError::Raised)?;
        plain_result(&value, returns)
    }
}

/// Caller-supplied registry port: bindings come from an explicit map of
/// logical call strings to already-resolved R functions, never from a
/// namespace lookup. Declared-lock auditing stays separate: this port
/// refuses lock verification rather than silently skipping it. It is staged
/// for the registry activation path; dataset execution still uses the locked
/// port, so no entry point in this crate constructs it yet.
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
#[allow(dead_code)]
impl ProjectFunctions for RegistryPort {
    fn verify_lock(
        &self,
        _lock: &LockContext<'_>,
        _functions: &[FunctionIdentity],
    ) -> std::result::Result<(), FunctionError> {
        Err(FunctionError::Protocol)
    }
    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &Signature,
    ) -> std::result::Result<FunctionHandle, FunctionError> {
        let bound = self
            .registry
            .get(&identity.call)
            .ok_or_else(|| FunctionError::MissingBinding(identity.call.clone()))?;
        let handle = on_thread(bound)?.clone();
        // Parity with the Python registry port: the registered function's
        // formals must name exactly the logical parameters.
        let formals = handle.formals().ok_or(FunctionError::Protocol)?;
        let actual: std::collections::BTreeSet<&str> = formals.names().collect();
        let expected: std::collections::BTreeSet<&str> = signature
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect();
        if actual.contains("...") || actual != expected {
            return Err(FunctionError::Protocol);
        }
        Ok(Bound::handle(handle, signature.returns))
    }
    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> std::result::Result<Scalar, CallError> {
        // A registered function is called directly with its named plain
        // arguments. With no installed reply envelope or formatter, a raised
        // R condition stays an opaque payload; the core still validates the
        // converted result before accepting any output.
        let (function, returns) = Bound::get(handle)?;
        let value = function
            .call(Pairlist::from_pairs(plain_arguments(arguments)?))
            .map_err(|error| {
                CallError::Raised(FunctionError::SdkBoundary {
                    payload: sdk_payload(&error),
                })
            })?;
        plain_result(&value, returns)
    }
}
