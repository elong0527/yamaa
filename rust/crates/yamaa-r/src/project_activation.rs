//! Installed R metadata and namespace capabilities on the calling R thread.
//! Original condition objects are rooted in owned error payloads; no longjmp
//! crosses Rust because installed host helpers return closed reply envelopes.
use extendr_api::prelude::*;
use std::sync::Mutex;
use yamaa::{
    Argument, CallError, FunctionError, FunctionHandle, FunctionIdentity, HostLanguage,
    HostPayload, LockContext, LockFinding, LockKind, LockReason, LockSyntaxError, ProjectFunctions,
    Scalar, ScalarRef, Signature,
};

/// Installed capabilities: lock verification, binding resolution and host
/// condition rendering. Constructing this host port does not query versions,
/// load project namespaces or invoke any project function.
pub struct Functions {
    verify: Mutex<crate::thread_bound::ThreadBound<Function>>,
    resolve: Mutex<crate::thread_bound::ThreadBound<Function>>,
    formatter: Mutex<crate::thread_bound::ThreadBound<Function>>,
}

impl Functions {
    pub fn new(verify: Function, resolve: Function, formatter: Function) -> Self {
        use crate::thread_bound::ThreadBound;
        Self {
            verify: Mutex::new(ThreadBound::new(verify)),
            resolve: Mutex::new(ThreadBound::new(resolve)),
            formatter: Mutex::new(ThreadBound::new(formatter)),
        }
    }
}

/// One closed host reply: a completed payload or a classified failure status.
enum Status {
    Completed(Robj),
    Host(Robj),
    Representation(Robj),
    Interrupt(Robj),
}

fn sdk_payload(error: &extendr_api::Error) -> Option<HostPayload> {
    let condition = match error {
        extendr_api::Error::EvalError(value) | extendr_api::Error::Panic(value) => {
            Some(value.clone())
        }
        _ => None,
    };
    condition.map(|condition| HostPayload::new(crate::thread_bound::ThreadBound::new(condition)))
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
        2 => Ok(Status::Representation(payload)),
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
fn ordinary(condition: &Robj, formatter: &Function, rejected: bool) -> Option<(String, String)> {
    let reply = formatter
        .call(pairlist!(condition = condition.clone()))
        .ok()?;
    let reply = reply.as_list().filter(|reply| reply.len() == 2)?;
    let class = crate::function_callback::detail(&reply.elt(0).ok()?, "R condition").ok()?;
    let message =
        crate::function_callback::detail(&reply.elt(1).ok()?, "unavailable R condition message")
            .ok()?;
    let _ = rejected;
    Some((class, message))
}

fn classify_host(formatter: &Function, condition: &Robj) -> FunctionError {
    match ordinary(condition, formatter, false) {
        Some((class, message)) => FunctionError::Exception {
            class,
            message,
            payload: Some(HostPayload::new(crate::thread_bound::ThreadBound::new(
                condition.clone(),
            ))),
        },
        None => FunctionError::Internal {
            code: "host_classification",
        },
    }
}

fn classify_representation(formatter: &Function, condition: &Robj) -> FunctionError {
    match ordinary(condition, formatter, true) {
        Some((_class, message)) => FunctionError::Representation {
            message,
            payload: Some(HostPayload::new(crate::thread_bound::ThreadBound::new(
                condition.clone(),
            ))),
        },
        None => FunctionError::Internal {
            code: "host_classification",
        },
    }
}

fn classify_status(formatter: &Function, status: Status) -> std::result::Result<Robj, CallError> {
    match status {
        Status::Completed(payload) => Ok(payload),
        Status::Host(condition) => Err(CallError::Raised(classify_host(formatter, &condition))),
        Status::Representation(condition) => Err(CallError::InvalidResult(
            classify_representation(formatter, &condition),
        )),
        Status::Interrupt(condition) => Err(CallError::Raised(FunctionError::Interrupted {
            payload: Some(HostPayload::new(crate::thread_bound::ThreadBound::new(
                condition,
            ))),
        })),
    }
}

fn owned_scalar(value: ScalarRef<'_>) -> std::result::Result<Scalar, CallError> {
    // No wildcard: `ScalarRef` may grow under the stability policy, and a new
    // argument variant must fail loudly rather than arrive as `NA`.
    Ok(match value {
        ScalarRef::Missing => Scalar::Missing,
        ScalarRef::Str(value) => Scalar::Str(value.to_owned()),
        ScalarRef::Int(value) => Scalar::Int(value),
        ScalarRef::Float(value) => Scalar::Float(value),
        ScalarRef::Bool(value) => Scalar::Bool(value),
        ScalarRef::Date(value) => Scalar::Date(value),
        ScalarRef::DateTime(value) => Scalar::DateTime(value),
        _ => {
            return Err(CallError::Raised(FunctionError::Protocol));
        }
    })
}

fn invoke_scalar(
    handle: &Function,
    formatter: &Function,
    arguments: &[Argument<'_>],
) -> std::result::Result<Scalar, CallError> {
    let mut encoded = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let value = yamaa::EncodedScalar::encode(owned_scalar(argument.value)?);
        encoded.push((
            argument.name,
            Robj::from(list!(
                tag = value.tag,
                payload = Raw::from_bytes(&value.payload)
            )),
        ));
    }
    let status = call_status(handle, pairlist!(encoded = List::from_pairs(encoded)))
        .map_err(CallError::Raised)?;
    let result = classify_status(formatter, status)?;
    let result = result
        .as_list()
        .filter(|list| list.len() == 2)
        .ok_or(CallError::InvalidResult(FunctionError::Protocol))?;
    let tag = result
        .elt(0)
        .map_err(|error| {
            CallError::InvalidResult(FunctionError::SdkBoundary {
                payload: sdk_payload(&error),
            })
        })?
        .as_integer()
        .ok_or(CallError::InvalidResult(FunctionError::Protocol))?;
    let payload = result
        .elt(1)
        .map_err(|error| {
            CallError::InvalidResult(FunctionError::SdkBoundary {
                payload: sdk_payload(&error),
            })
        })?
        .as_raw()
        .ok_or(CallError::InvalidResult(FunctionError::Protocol))?;
    yamaa::EncodedScalar::decode(tag, payload.as_slice()).map_err(|error| {
        CallError::InvalidResult(FunctionError::Scalar {
            reason: error.reason(),
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
        let locked = yamaa::decode_renv_lock(lock.bytes).map_err(FunctionError::LockSyntax)?;
        let verify_guard = self
            .verify
            .lock()
            .map_err(|_| FunctionError::Internal { code: "host_port" })?;
        let verify = verify_guard.get()?;
        let formatter_guard = self
            .formatter
            .lock()
            .map_err(|_| FunctionError::Internal { code: "host_port" })?;
        let formatter = formatter_guard.get()?;
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
        let rows = match status {
            Status::Completed(rows) => rows,
            Status::Host(condition) => return Err(classify_host(formatter, &condition)),
            Status::Representation(condition) => {
                return Err(classify_representation(formatter, &condition));
            }
            Status::Interrupt(condition) => {
                return Err(FunctionError::Interrupted {
                    payload: Some(HostPayload::new(crate::thread_bound::ThreadBound::new(
                        condition,
                    ))),
                });
            }
        };
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
        let resolve = resolve_guard.get()?;
        let formatter_guard = self
            .formatter
            .lock()
            .map_err(|_| FunctionError::Internal { code: "host_port" })?;
        let formatter = formatter_guard.get()?;
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
        let handle = match status {
            Status::Completed(handle) => handle,
            Status::Host(condition) => return Err(classify_host(formatter, &condition)),
            Status::Representation(condition) => {
                return Err(classify_representation(formatter, &condition));
            }
            Status::Interrupt(condition) => {
                return Err(FunctionError::Interrupted {
                    payload: Some(HostPayload::new(crate::thread_bound::ThreadBound::new(
                        condition,
                    ))),
                });
            }
        };
        handle
            .as_function()
            .ok_or(FunctionError::Protocol)
            .map(|handle| FunctionHandle::new(crate::thread_bound::ThreadBound::new(handle)))
    }

    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> std::result::Result<Scalar, CallError> {
        let handle = handle
            .with::<crate::thread_bound::ThreadBound<Function>, _>(|handle| handle.get().cloned())
            .ok_or(CallError::Raised(FunctionError::Protocol))?
            .map_err(CallError::Raised)?;
        let formatter_guard = self
            .formatter
            .lock()
            .map_err(|_| CallError::Raised(FunctionError::Internal { code: "host_port" }))?;
        let formatter = formatter_guard.get().map_err(CallError::Raised)?;
        invoke_scalar(&handle, formatter, arguments)
    }
}

/// Caller-supplied registry port: bindings come from an explicit map of
/// (staged for the registry activation path; dataset execution still uses
/// the locked port, so this surface is intentionally not yet constructed
/// by an entry point in this crate).
/// logical call strings to already-resolved R functions, never from a
/// namespace lookup. Declared-lock auditing stays separate: this port
/// refuses lock verification rather than silently skipping it.
#[allow(dead_code)]
pub struct RegistryPort {
    registry: std::collections::BTreeMap<String, crate::thread_bound::ThreadBound<Function>>,
}
#[allow(dead_code)]
impl RegistryPort {
    pub fn new(registry: std::collections::BTreeMap<String, Function>) -> Self {
        use crate::thread_bound::ThreadBound;
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
        let handle = bound.get()?.clone();
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
        Ok(FunctionHandle::new(crate::thread_bound::ThreadBound::new(
            handle,
        )))
    }
    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> std::result::Result<Scalar, CallError> {
        // Registry invocation has no installed formatter, so raised host
        // payloads stay opaque: the core still validates lengths and exact
        // types before accepting any output.
        let handle = handle
            .with::<crate::thread_bound::ThreadBound<Function>, _>(|handle| handle.get().cloned())
            .ok_or(CallError::Raised(FunctionError::Protocol))?
            .map_err(CallError::Raised)?;
        let mut encoded = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = yamaa::EncodedScalar::encode(owned_scalar(argument.value)?);
            encoded.push((
                argument.name,
                Robj::from(list!(
                    tag = value.tag,
                    payload = Raw::from_bytes(&value.payload)
                )),
            ));
        }
        let status = call_status(&handle, pairlist!(encoded = List::from_pairs(encoded)))
            .map_err(CallError::Raised)?;
        let result = match status {
            Status::Completed(payload) => payload,
            Status::Host(_) | Status::Representation(_) => {
                return Err(CallError::InvalidResult(FunctionError::Internal {
                    code: "host_classification",
                }));
            }
            Status::Interrupt(condition) => {
                return Err(CallError::Raised(FunctionError::Interrupted {
                    payload: Some(HostPayload::new(crate::thread_bound::ThreadBound::new(
                        condition,
                    ))),
                }));
            }
        };
        let result = result
            .as_list()
            .filter(|list| list.len() == 2)
            .ok_or(CallError::InvalidResult(FunctionError::Protocol))?;
        let tag = result
            .elt(0)
            .map_err(|error| {
                CallError::InvalidResult(FunctionError::SdkBoundary {
                    payload: sdk_payload(&error),
                })
            })?
            .as_integer()
            .ok_or(CallError::InvalidResult(FunctionError::Protocol))?;
        let payload = result
            .elt(1)
            .map_err(|error| {
                CallError::InvalidResult(FunctionError::SdkBoundary {
                    payload: sdk_payload(&error),
                })
            })?
            .as_raw()
            .ok_or(CallError::InvalidResult(FunctionError::Protocol))?;
        yamaa::EncodedScalar::decode(tag, payload.as_slice()).map_err(|error| {
            CallError::InvalidResult(FunctionError::Scalar {
                reason: error.reason(),
            })
        })
    }
}

/// Mechanical SDK facts, distinct from projected semantic issues. Original
/// R conditions remain rooted objects; no host condition is stringified.
pub(crate) fn facts(error: &FunctionError) -> Robj {
    let condition = |payload: &Option<HostPayload>| {
        payload
            .as_ref()
            .and_then(|payload| {
                // Best-effort evidence: a payload created on another thread
                // cannot be touched here, so it reports as absent.
                payload
                    .with::<crate::thread_bound::ThreadBound<Robj>, _>(|condition| {
                        condition.get().cloned().ok()
                    })
                    .flatten()
            })
            .unwrap_or_else(|| NULL.into_robj())
    };
    match error {
        FunctionError::Exception { payload, .. } => {
            list!(kind = "host", condition = condition(payload)).into_robj()
        }
        FunctionError::Representation { payload, .. } => {
            list!(kind = "representation", condition = condition(payload)).into_robj()
        }
        FunctionError::Scalar { reason } => {
            list!(kind = "scalar", reason = Raw::from_bytes(reason.as_bytes())).into_robj()
        }
        FunctionError::Lock(findings) => list!(
            kind = "lock",
            findings = List::from_values(findings.iter().map(|finding| {
                list!(
                    package = Raw::from_bytes(finding.package.as_bytes()),
                    reason = finding.reason.as_str(),
                    expected = List::from_values(
                        finding
                            .expected
                            .iter()
                            .map(|value| Raw::from_bytes(value.as_bytes()))
                    ),
                    actual = finding
                        .actual
                        .as_ref()
                        .map(|value| Raw::from_bytes(value.as_bytes()))
                )
            }))
        )
        .into_robj(),
        FunctionError::LockSyntax(error) => match error {
            LockSyntaxError::Limit {
                resource,
                limit,
                actual,
            } => list!(
                kind = "lock_limit",
                resource = *resource,
                limit = *limit as f64,
                actual = actual.map(|value| value as f64)
            )
            .into_robj(),
            LockSyntaxError::Shape { field } => {
                list!(kind = "lock_shape", field = *field).into_robj()
            }
            LockSyntaxError::Json { line, column } => list!(
                kind = "lock_json",
                line = *line as f64,
                column = *column as f64
            )
            .into_robj(),
            LockSyntaxError::Document { resource_refusal } => {
                list!(kind = "lock_document", resource_refusal = *resource_refusal).into_robj()
            }
            _ => list!(kind = "unclassified").into_robj(),
        },
        FunctionError::Interrupted { payload } => {
            list!(kind = "interrupt", condition = condition(payload)).into_robj()
        }
        FunctionError::SdkBoundary { payload } => {
            list!(kind = "sdk_boundary", condition = condition(payload)).into_robj()
        }
        FunctionError::Protocol => list!(kind = "protocol").into_robj(),
        FunctionError::MissingBinding(call) => list!(
            kind = "missing_binding",
            call = Raw::from_bytes(call.as_bytes())
        )
        .into_robj(),
        FunctionError::Rejected { reason, .. } => list!(
            kind = "rejected",
            reason = Raw::from_bytes(reason.as_bytes())
        )
        .into_robj(),
        FunctionError::Internal { code } => list!(kind = "internal", code = *code).into_robj(),
        _ => list!(kind = "unclassified").into_robj(),
    }
}
