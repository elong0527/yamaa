//! The callback adapters: a host's callbacks behind the facade's
//! cancellation probe, its host-functions port and the two lock
//! verifications (the design on #2870, section 3.5).
//!
//! # Safety
//!
//! Every unsafe operation here calls a host callback or reads what one
//! answered. Their soundness rests on the contract of
//! [`yamaa_boundary::abi`]: a callback is a valid function for the life of
//! the call that received it, it reads the borrowed records the engine
//! passes only while it runs, and a reply it fills points at memory that
//! stays valid until the engine calls the reply's `release`. The engine
//! calls the callbacks only on the entry's thread, while the entry runs:
//! [`Host`] and [`Root`] are `Send` and `Sync` only because the facade's
//! port and token types ask for it, and nothing here moves them to another
//! thread.
use std::collections::BTreeMap;
use std::ffi::{c_int, c_void};
use std::sync::Arc;

use yamaa::{
    Argument, BatchArgument, CallError, CancellationToken, ColumnType, FunctionError,
    FunctionHandle, FunctionIdentity, HostLanguage, HostPayload, InstalledDistributions,
    InstalledVersion, LockContext, LockFinding, LockKind, LockReason, ProjectFunctions, ScalarRef,
};
use yamaa_boundary::abi::{self, Reply, Str};

/// A host object the engine keeps, released once when it drops.
pub(crate) struct Root(abi::Root);

// SAFETY: see the module documentation; a root is released on the thread
// that drops its last owner, as the ABI states.
unsafe impl Send for Root {}

impl Root {
    /// Take the object `root` holds, leaving it empty; `None` when it holds
    /// none.
    pub(crate) fn take(root: &mut abi::Root) -> Option<Root> {
        let taken = std::mem::replace(root, abi::Root::NONE);
        (!taken.pointer.is_null()).then_some(Root(taken))
    }

    fn pointer(&self) -> *mut c_void {
        self.0.pointer
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        if let Some(release) = self.0.release.take() {
            // SAFETY: the host gave the object with this release, which the
            // engine calls exactly once.
            unsafe { release(self.0.pointer) }
        }
    }
}

/// The callbacks of one call, which keep its context until the last of the
/// call's tokens and ports drops.
pub(crate) struct Host {
    callbacks: abi::Call,
    context: Option<Root>,
}

// SAFETY: see the module documentation.
unsafe impl Send for Host {}
// SAFETY: see the module documentation.
unsafe impl Sync for Host {}

impl Host {
    /// The callbacks of `call`, whose context they take.
    pub(crate) fn adopt(call: &mut abi::Call) -> Arc<Host> {
        let context = Root::take(&mut call.context);
        Arc::new(Host {
            callbacks: *call,
            context,
        })
    }

    fn context(&self) -> *mut c_void {
        self.context
            .as_ref()
            .map_or(std::ptr::null_mut(), Root::pointer)
    }

    /// Ask a callback with a fresh reply, read its answer, then release the
    /// reply: every root left in it, and its host memory.
    fn answer<T>(
        &self,
        ask: impl FnOnce(*mut c_void, *mut Reply) -> c_int,
        read: impl FnOnce(c_int, &mut Reply) -> T,
    ) -> T {
        struct Held(Reply);
        impl Drop for Held {
            fn drop(&mut self) {
                drop(Root::take(&mut self.0.function));
                drop(Root::take(&mut self.0.error.payload));
                if let Some(release) = self.0.release.take() {
                    // SAFETY: the host filled the reply with this release,
                    // which the engine calls once, after reading the reply.
                    unsafe { release(&mut self.0) }
                }
            }
        }
        let mut reply = Held(Reply::empty());
        let status = ask(self.context(), &mut reply.0);
        read(status, &mut reply.0)
    }

    /// The call's token: it asks the host's probe, when it offers one.
    pub(crate) fn token(self: &Arc<Self>) -> CancellationToken {
        let Some(probe) = self.callbacks.probe else {
            return CancellationToken::new();
        };
        let host = Arc::clone(self);
        CancellationToken::with_probe(move || {
            // SAFETY: see the module documentation.
            match unsafe { probe(host.context()) } {
                abi::GO_ON => false,
                abi::CANCEL => true,
                // The token has no way to fail, so the call unwinds to its
                // entry, which refuses it at the boundary.
                answer => panic!("the host's probe answered {answer}, which is neither 0 nor 1"),
            }
        })
    }

    /// The call's host-functions port, when the host binds and invokes.
    pub(crate) fn port(self: &Arc<Self>) -> Option<Arc<dyn ProjectFunctions>> {
        let offered = self.callbacks.bind.is_some() && self.callbacks.invoke.is_some();
        offered.then(|| Arc::new(Port(Arc::clone(self))) as Arc<dyn ProjectFunctions>)
    }
}

/// A bound function: the host's callable, kept until the build drops it.
struct Bound(Option<Root>);

/// The host-functions port over a host's callbacks.
struct Port(Arc<Host>);

/// The bytes `text` borrows, or `None` for no text.
///
/// # Safety
///
/// A non-null `text.ptr` points at `text.len` readable bytes.
unsafe fn bytes<'a>(text: Str) -> Option<&'a [u8]> {
    // SAFETY: the caller's contract.
    (!text.ptr.is_null()).then(|| unsafe { std::slice::from_raw_parts(text.ptr, text.len) })
}

/// The items of a list the host answered; a null list is empty.
///
/// # Safety
///
/// A non-null `items` points at `len` readable items.
unsafe fn items<'a, T>(items: *const T, len: usize) -> &'a [T] {
    if items.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: the caller's contract.
        unsafe { std::slice::from_raw_parts(items, len) }
    }
}

/// `text` as UTF-8, or `None` when it is no text or not UTF-8.
///
/// # Safety
///
/// As [`bytes`].
unsafe fn text(text: Str) -> Option<String> {
    // SAFETY: the caller's contract.
    let bytes = unsafe { bytes(text) }?;
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

/// `text` for a message, with any invalid UTF-8 replaced.
///
/// # Safety
///
/// As [`bytes`].
unsafe fn lossy(text: Str) -> String {
    // SAFETY: the caller's contract.
    unsafe { bytes(text) }.map_or_else(String::new, |bytes| {
        String::from_utf8_lossy(bytes).into_owned()
    })
}

/// A host failure as the facade's function error, which takes its payload.
///
/// # Safety
///
/// The error's texts are valid, as [`bytes`] asks.
unsafe fn failure(error: &mut abi::HostError) -> FunctionError {
    let payload = Root::take(&mut error.payload).map(HostPayload::new);
    // SAFETY: the caller's contract.
    let (class, message) = unsafe { (lossy(error.class), lossy(error.message)) };
    match error.kind {
        abi::ERROR_EXCEPTION => FunctionError::Exception {
            class,
            message,
            payload,
        },
        abi::ERROR_INTERRUPTED => FunctionError::Interrupted { payload },
        abi::ERROR_REJECTED => FunctionError::Rejected {
            reason: message,
            // SAFETY: the caller's contract.
            returned: unsafe { bytes(error.returned) }
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
        },
        abi::ERROR_REPRESENTATION => FunctionError::Representation { message, payload },
        abi::ERROR_SDK_BOUNDARY => FunctionError::SdkBoundary { payload },
        abi::ERROR_INTERNAL => FunctionError::Internal {
            code: match message.as_str() {
                "host_classification" => "host_classification",
                "host_port" => "host_port",
                _ => "function_boundary",
            },
        },
        abi::ERROR_MISSING_BINDING => FunctionError::MissingBinding(message),
        _ => FunctionError::Protocol,
    }
}

/// The answer of a query: `read` of an answered reply, the host's failure,
/// or a protocol fault.
///
/// # Safety
///
/// The reply was filled by the host as the ABI states.
unsafe fn queried<T>(
    status: c_int,
    reply: &mut Reply,
    read: impl FnOnce(&mut Reply) -> Result<T, FunctionError>,
) -> Result<T, FunctionError> {
    match status {
        abi::ANSWERED => read(reply),
        // SAFETY: the caller's contract.
        abi::RAISED => Err(unsafe { failure(&mut reply.error) }),
        _ => Err(FunctionError::Protocol),
    }
}

/// The texts of an answered reply, `width` to a row.
///
/// # Safety
///
/// As [`queried`].
unsafe fn rows(reply: &Reply, width: usize) -> Result<Vec<Vec<Option<String>>>, FunctionError> {
    // SAFETY: the caller's contract.
    let texts = unsafe { items(reply.texts, reply.texts_len) };
    if texts.len() % width != 0 {
        return Err(FunctionError::Protocol);
    }
    texts
        .chunks(width)
        .map(|row| {
            row.iter()
                .map(|item| {
                    if item.ptr.is_null() {
                        return Ok(None);
                    }
                    // SAFETY: the caller's contract.
                    unsafe { text(*item) }
                        .map(Some)
                        .ok_or(FunctionError::Protocol)
                })
                .collect()
        })
        .collect()
}

/// Pairs of required texts, grouped by their first item in order.
fn grouped(rows: Vec<Vec<Option<String>>>) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
    let mut grouped = BTreeMap::<String, Vec<String>>::new();
    for row in rows {
        let [Some(key), Some(value)]: [Option<String>; 2] =
            row.try_into().map_err(|_| FunctionError::Protocol)?
        else {
            return Err(FunctionError::Protocol);
        };
        grouped.entry(key).or_default().push(value);
    }
    Ok(grouped)
}

fn rejected(reason: &str) -> FunctionError {
    FunctionError::Rejected {
        reason: reason.to_owned(),
        returned: None,
    }
}

/// An argument for the host, borrowing the engine's text.
fn scalar_record(value: ScalarRef<'_>) -> Result<abi::Scalar, CallError> {
    let (kind, int, float, text) = match value {
        ScalarRef::Missing => (abi::SCALAR_MISSING, 0, 0.0, Str::NONE),
        ScalarRef::Str(text) => (abi::SCALAR_TEXT, 0, 0.0, Str::of(text)),
        ScalarRef::Int(value) => (abi::SCALAR_INT, value, 0.0, Str::NONE),
        ScalarRef::Float(value) => (abi::SCALAR_FLOAT, 0, value, Str::NONE),
        ScalarRef::Bool(value) => (abi::SCALAR_BOOL, i64::from(value), 0.0, Str::NONE),
        ScalarRef::Date(date) => (
            abi::SCALAR_DATE,
            i64::from(date.epoch_days()),
            0.0,
            Str::NONE,
        ),
        ScalarRef::DateTime(moment) => {
            (abi::SCALAR_DATETIME, moment.epoch_seconds(), 0.0, Str::NONE)
        }
        // The facade's scalar is non-exhaustive; no other kind exists.
        _ => {
            return Err(CallError::Raised(FunctionError::Internal {
                code: "function_boundary",
            }))
        }
    };
    Ok(abi::Scalar {
        kind,
        int,
        float,
        text,
    })
}

/// A result the host answered, as the facade's scalar.
///
/// # Safety
///
/// The record's text is valid, as [`bytes`] asks.
unsafe fn scalar(record: &abi::Scalar) -> Result<yamaa::Scalar, FunctionError> {
    Ok(match record.kind {
        abi::SCALAR_MISSING => yamaa::Scalar::Missing,
        // SAFETY: the caller's contract.
        abi::SCALAR_TEXT => yamaa::Scalar::Str(
            unsafe { text(record.text) }
                .ok_or_else(|| rejected("a returned string is not valid UTF-8"))?,
        ),
        abi::SCALAR_INT => yamaa::Scalar::Int(record.int),
        abi::SCALAR_FLOAT => yamaa::Scalar::float(record.float),
        abi::SCALAR_BOOL => match record.int {
            0 => yamaa::Scalar::Bool(false),
            1 => yamaa::Scalar::Bool(true),
            _ => return Err(FunctionError::Protocol),
        },
        abi::SCALAR_DATE => yamaa::Scalar::Date(
            yamaa::Date::from_epoch_days(record.int)
                .ok_or_else(|| rejected("a returned date is outside the years 1 to 9999"))?,
        ),
        abi::SCALAR_DATETIME => yamaa::Scalar::DateTime(
            yamaa::DateTime::from_epoch_seconds(record.int)
                .ok_or_else(|| rejected("a returned datetime is outside the years 1 to 9999"))?,
        ),
        _ => return Err(FunctionError::Protocol),
    })
}

/// How the host answered a call of a project function.
///
/// # Safety
///
/// As [`queried`].
unsafe fn called<T>(
    status: c_int,
    reply: &mut Reply,
    read: impl FnOnce(&Reply) -> Result<T, FunctionError>,
) -> Result<T, CallError> {
    match status {
        abi::ANSWERED => read(reply).map_err(CallError::InvalidResult),
        // SAFETY: the caller's contract.
        abi::RAISED => Err(CallError::Raised(unsafe { failure(&mut reply.error) })),
        // SAFETY: the caller's contract.
        abi::INVALID => Err(CallError::InvalidResult(unsafe {
            failure(&mut reply.error)
        })),
        _ => Err(CallError::Raised(FunctionError::Protocol)),
    }
}

fn column(returns: ColumnType) -> u32 {
    match returns {
        ColumnType::Str => abi::COLUMN_STR,
        ColumnType::Int => abi::COLUMN_INT,
        ColumnType::Float => abi::COLUMN_FLOAT,
        ColumnType::Date => abi::COLUMN_DATE,
        // The facade's column type is non-exhaustive; datetime is the last.
        _ => abi::COLUMN_DATETIME,
    }
}

impl Port {
    fn function(handle: &FunctionHandle) -> Result<*mut c_void, CallError> {
        handle
            .with::<Bound, _>(|bound| bound.0.as_ref().map_or(std::ptr::null_mut(), Root::pointer))
            .ok_or(CallError::Raised(FunctionError::Protocol))
    }

    /// Call the bound `function` once with `arguments`.
    fn call(
        &self,
        function: *mut c_void,
        arguments: &[abi::Argument],
    ) -> Result<yamaa::Scalar, CallError> {
        let invoke = self
            .0
            .callbacks
            .invoke
            .ok_or(CallError::Raised(FunctionError::Protocol))?;
        self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe {
                invoke(
                    context,
                    function,
                    arguments.as_ptr(),
                    arguments.len(),
                    reply,
                )
            },
            // SAFETY: the host filled the reply.
            |status, reply| unsafe { called(status, reply, |reply| scalar(&reply.scalar)) },
        )
    }
}

impl ProjectFunctions for Port {
    fn verify_lock(
        &self,
        lock: &LockContext<'_>,
        functions: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        match (lock.language, lock.kind) {
            (HostLanguage::Python, LockKind::Uv) => {
                yamaa::verify_uv_lock(lock.bytes, functions, &Installed(&self.0))
            }
            (HostLanguage::R, LockKind::Renv) => self.compare_locked(lock.bytes, functions),
            _ => Err(FunctionError::Protocol),
        }
    }

    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &yamaa::Signature,
    ) -> Result<FunctionHandle, FunctionError> {
        let bind = self.0.callbacks.bind.ok_or(FunctionError::Protocol)?;
        let parameters: Vec<Str> = signature
            .parameters
            .iter()
            .map(|parameter| Str::of(&parameter.name))
            .collect();
        let signature = abi::Signature {
            parameters: parameters.as_ptr(),
            parameters_len: parameters.len(),
            returns: column(signature.returns),
            may_return_missing: u32::from(signature.may_return_missing),
        };
        let identity = abi::Identity {
            name: Str::of(&identity.name),
            call: Str::of(&identity.call),
        };
        self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe { bind(context, &identity, &signature, reply) },
            // SAFETY: the host filled the reply.
            |status, reply| unsafe {
                queried(status, reply, |reply| {
                    Ok(FunctionHandle::new(Bound(Root::take(&mut reply.function))))
                })
            },
        )
    }

    fn is_interrupt(&self, error: &FunctionError) -> bool {
        let Some(classify) = self.0.callbacks.is_interrupt else {
            return matches!(error, FunctionError::Interrupted { .. });
        };
        let kept = |payload: &Option<HostPayload>| {
            payload
                .as_ref()
                .and_then(|payload| payload.with::<Root, _>(Root::pointer))
                .unwrap_or(std::ptr::null_mut())
        };
        let (kind, payload) = match error {
            FunctionError::Exception { payload, .. } => (abi::ERROR_EXCEPTION, kept(payload)),
            FunctionError::Interrupted { payload } => (abi::ERROR_INTERRUPTED, kept(payload)),
            FunctionError::Representation { payload, .. } => {
                (abi::ERROR_REPRESENTATION, kept(payload))
            }
            FunctionError::SdkBoundary { payload } => (abi::ERROR_SDK_BOUNDARY, kept(payload)),
            FunctionError::Rejected { .. } => (abi::ERROR_REJECTED, std::ptr::null_mut()),
            FunctionError::Internal { .. } => (abi::ERROR_INTERNAL, std::ptr::null_mut()),
            FunctionError::MissingBinding(_) => (abi::ERROR_MISSING_BINDING, std::ptr::null_mut()),
            FunctionError::Protocol => (abi::ERROR_PROTOCOL, std::ptr::null_mut()),
            _ => (0, std::ptr::null_mut()),
        };
        // SAFETY: see the module documentation.
        unsafe { classify(self.0.context(), kind, payload) == 1 }
    }

    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> Result<yamaa::Scalar, CallError> {
        let arguments = arguments
            .iter()
            .map(|argument| {
                Ok(abi::Argument {
                    name: Str::of(argument.name),
                    value: scalar_record(argument.value)?,
                })
            })
            .collect::<Result<Vec<_>, CallError>>()?;
        self.call(Port::function(handle)?, &arguments)
    }

    fn call_batch(
        &self,
        handle: &FunctionHandle,
        rows: usize,
        arguments: &[BatchArgument],
    ) -> Result<Vec<yamaa::Scalar>, CallError> {
        let function = Port::function(handle)?;
        let Some(call_batch) = self.0.callbacks.call_batch else {
            // Without the host's batch call, one call per row, as the
            // facade's port does by default.
            return (0..rows)
                .map(|row| {
                    let arguments = arguments
                        .iter()
                        .map(|argument| {
                            let value = argument.values.get(row).ok_or(CallError::Raised(
                                FunctionError::Internal {
                                    code: "function_boundary",
                                },
                            ))?;
                            Ok(abi::Argument {
                                name: Str::of(&argument.name),
                                value: scalar_record(ScalarRef::from_owned(value))?,
                            })
                        })
                        .collect::<Result<Vec<_>, CallError>>()?;
                    self.call(function, &arguments)
                })
                .collect();
        };
        let values = arguments
            .iter()
            .map(|argument| {
                argument
                    .values
                    .iter()
                    .map(|value| scalar_record(ScalarRef::from_owned(value)))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let arguments: Vec<abi::BatchArgument> = arguments
            .iter()
            .zip(&values)
            .map(|(argument, values)| abi::BatchArgument {
                name: Str::of(&argument.name),
                values: values.as_ptr(),
                values_len: values.len(),
            })
            .collect();
        self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe {
                call_batch(
                    context,
                    function,
                    rows,
                    arguments.as_ptr(),
                    arguments.len(),
                    reply,
                )
            },
            // SAFETY: the host filled the reply.
            |status, reply| unsafe {
                called(status, reply, |reply| {
                    items(reply.scalars, reply.scalars_len)
                        .iter()
                        .map(|record| scalar(record))
                        .collect()
                })
            },
        )
    }
}

impl Port {
    /// Verify an renv lock: the engine decodes it, and the host compares
    /// the locked versions with the installed ones, as R does today.
    fn compare_locked(
        &self,
        lock: &[u8],
        functions: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        let compare = self
            .0
            .callbacks
            .compare_locked
            .ok_or(FunctionError::Protocol)?;
        let locked = yamaa::decode_renv_lock(lock).map_err(FunctionError::LockSyntax)?;
        let packages: Vec<Str> = locked
            .packages
            .iter()
            .flat_map(|(name, version)| [Str::of(name), Str::of(version)])
            .collect();
        let calls: Vec<Str> = functions
            .iter()
            .map(|function| Str::of(&function.call))
            .collect();
        let findings = self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe {
                compare(
                    context,
                    packages.as_ptr(),
                    packages.len(),
                    calls.as_ptr(),
                    calls.len(),
                    reply,
                )
            },
            // SAFETY: the host filled the reply.
            |status, reply| unsafe { queried(status, reply, |reply| rows(reply, 4)) },
        )?;
        let findings = findings
            .into_iter()
            .map(|row| match <[Option<String>; 4]>::try_from(row) {
                Ok([Some(package), Some(reason), expected, actual]) => Ok(LockFinding::new(
                    package,
                    LockReason::parse(&reason).ok_or(FunctionError::Protocol)?,
                    expected.into_iter().collect(),
                    actual,
                )),
                _ => Err(FunctionError::Protocol),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if findings.is_empty() {
            Ok(())
        } else {
            Err(FunctionError::Lock(findings))
        }
    }
}

/// What a Python host reports of its installation, through its five
/// queries; the engine reads the uv lock and compares (D172).
struct Installed<'a>(&'a Host);

impl Installed<'_> {
    fn query<Q: Copy>(&self, query: Option<Q>) -> Result<Q, FunctionError> {
        query.ok_or(FunctionError::Protocol)
    }
}

impl InstalledDistributions for Installed<'_> {
    fn providers(
        &self,
        modules: &[String],
    ) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
        let providers = self.query(self.0.callbacks.providers)?;
        let modules: Vec<Str> = modules.iter().map(|module| Str::of(module)).collect();
        let rows = self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe { providers(context, modules.as_ptr(), modules.len(), reply) },
            // SAFETY: the host filled the reply.
            |status, reply| unsafe { queried(status, reply, |reply| rows(reply, 2)) },
        )?;
        grouped(rows)
    }

    fn standard_library(&self, module: &str) -> Result<bool, FunctionError> {
        let standard_library = self.query(self.0.callbacks.standard_library)?;
        self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe { standard_library(context, Str::of(module), reply) },
            // SAFETY: the host filled the reply.
            |status, reply| unsafe {
                queried(status, reply, |reply| match scalar(&reply.scalar)? {
                    yamaa::Scalar::Bool(answer) => Ok(answer),
                    _ => Err(FunctionError::Protocol),
                })
            },
        )
    }

    fn version(&self, distribution: &str) -> Result<InstalledVersion, FunctionError> {
        let version = self.query(self.0.callbacks.distribution_version)?;
        self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe { version(context, Str::of(distribution), reply) },
            // SAFETY: the host filled the reply.
            |status, reply| unsafe {
                queried(status, reply, |reply| match reply.scalar.kind {
                    abi::SCALAR_MISSING => Ok(InstalledVersion::Missing),
                    abi::SCALAR_TEXT => text(reply.scalar.text)
                        .map(InstalledVersion::Version)
                        .ok_or(FunctionError::Protocol),
                    _ => Ok(InstalledVersion::Unreadable),
                })
            },
        )
    }

    fn environment(&self) -> Result<BTreeMap<String, String>, FunctionError> {
        let environment = self.query(self.0.callbacks.environment)?;
        let rows = self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe { environment(context, reply) },
            // SAFETY: the host filled the reply.
            |status, reply| unsafe { queried(status, reply, |reply| rows(reply, 2)) },
        )?;
        let mut values = BTreeMap::new();
        for (name, mut found) in grouped(rows)? {
            match (found.pop(), found.is_empty()) {
                (Some(value), true) => values.insert(name, value),
                _ => return Err(FunctionError::Protocol),
            };
        }
        Ok(values)
    }

    fn integer_digits(&self) -> Result<Option<usize>, FunctionError> {
        let integer_digits = self.query(self.0.callbacks.integer_digits)?;
        self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe { integer_digits(context, reply) },
            // SAFETY: the host filled the reply.
            |status, reply| unsafe {
                queried(status, reply, |reply| match scalar(&reply.scalar)? {
                    // Zero turns the limit off.
                    yamaa::Scalar::Int(0) => Ok(None),
                    yamaa::Scalar::Int(digits) => usize::try_from(digits)
                        .map(Some)
                        .map_err(|_| FunctionError::Protocol),
                    _ => Err(FunctionError::Protocol),
                })
            },
        )
    }
}
