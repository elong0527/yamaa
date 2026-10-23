//! The callback adapters: a host's callbacks behind the facade's
//! cancellation probe and its host-functions port (the design on #2870,
//! section 3.5).
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
use std::ffi::{c_int, c_void};
use std::sync::Arc;

use yamaa::{
    Argument, CallError, CancellationToken, ColumnType, FunctionError, FunctionHandle,
    FunctionIdentity, HostPayload, LockContext, ProjectFunctions, ScalarRef,
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

/// `text` as UTF-8, or `None` when it is no text or not UTF-8.
///
/// # Safety
///
/// A non-null `text.ptr` points at `text.len` readable bytes.
unsafe fn text(text: Str) -> Option<String> {
    // SAFETY: the caller's contract.
    let bytes = unsafe { bytes(text) }?;
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

/// The bytes `text` borrows, or `None` for no text.
///
/// # Safety
///
/// As [`text`].
unsafe fn bytes<'a>(text: Str) -> Option<&'a [u8]> {
    // SAFETY: the caller's contract.
    (!text.ptr.is_null()).then(|| unsafe { std::slice::from_raw_parts(text.ptr, text.len) })
}

/// A host failure as the facade's function error, which takes its payload.
///
/// # Safety
///
/// The error's texts are valid, as [`text`] asks.
unsafe fn failure(error: &mut abi::HostError) -> FunctionError {
    let payload = Root::take(&mut error.payload).map(HostPayload::new);
    // SAFETY: the caller's contract.
    let lossy = |text: Str| unsafe { bytes(text) }.map(|b| String::from_utf8_lossy(b).into_owned());
    let message = lossy(error.message).unwrap_or_default();
    match error.kind {
        abi::ERROR_EXCEPTION => FunctionError::Exception {
            class: lossy(error.class).unwrap_or_default(),
            message,
            payload,
        },
        abi::ERROR_INTERRUPTED => FunctionError::Interrupted { payload },
        abi::ERROR_REJECTED => FunctionError::Rejected {
            reason: message,
            returned: lossy(error.returned),
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
        ScalarRef::DateTime(time) => (abi::SCALAR_DATETIME, time.epoch_seconds(), 0.0, Str::NONE),
        // The facade's scalar is non-exhaustive; no other kind exists.
        _ => {
            let code = "function_boundary";
            return Err(CallError::Raised(FunctionError::Internal { code }));
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
/// The record's text is valid, as [`text`] asks.
unsafe fn scalar(record: &abi::Scalar) -> Result<yamaa::Scalar, FunctionError> {
    let outside = |kind| rejected(&format!("a returned {kind} is outside the years 1 to 9999"));
    Ok(match record.kind {
        abi::SCALAR_MISSING => yamaa::Scalar::Missing,
        // SAFETY: the caller's contract.
        abi::SCALAR_TEXT => yamaa::Scalar::Str(
            unsafe { text(record.text) }
                .ok_or_else(|| rejected("a returned string is not valid UTF-8"))?,
        ),
        abi::SCALAR_INT => yamaa::Scalar::Int(record.int),
        abi::SCALAR_FLOAT => yamaa::Scalar::float(record.float),
        abi::SCALAR_BOOL if record.int == 0 || record.int == 1 => {
            yamaa::Scalar::Bool(record.int == 1)
        }
        abi::SCALAR_DATE => yamaa::Scalar::Date(
            yamaa::Date::from_epoch_days(record.int).ok_or_else(|| outside("date"))?,
        ),
        abi::SCALAR_DATETIME => yamaa::Scalar::DateTime(
            yamaa::DateTime::from_epoch_seconds(record.int).ok_or_else(|| outside("datetime"))?,
        ),
        _ => return Err(FunctionError::Protocol),
    })
}

/// A bound function: the host's callable, kept until the build drops it.
struct Bound(Option<Root>);

/// The host-functions port over a host's callbacks. It calls a function
/// once per row, as the facade's port does by default, and refuses to
/// verify a declared lock until the hosts' verification callbacks join.
struct Port(Arc<Host>);

impl ProjectFunctions for Port {
    fn verify_lock(
        &self,
        _: &LockContext<'_>,
        _: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        Err(FunctionError::Protocol)
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
            returns: match signature.returns {
                ColumnType::Str => abi::COLUMN_STR,
                ColumnType::Int => abi::COLUMN_INT,
                ColumnType::Float => abi::COLUMN_FLOAT,
                ColumnType::Date => abi::COLUMN_DATE,
                // The facade's column type is non-exhaustive; datetime is last.
                _ => abi::COLUMN_DATETIME,
            },
            may_return_missing: u32::from(signature.may_return_missing),
        };
        let identity = abi::Identity {
            name: Str::of(&identity.name),
            call: Str::of(&identity.call),
        };
        self.0.answer(
            // SAFETY: see the module documentation.
            |context, reply| unsafe { bind(context, &identity, &signature, reply) },
            |status, reply| match status {
                abi::ANSWERED => Ok(FunctionHandle::new(Bound(Root::take(&mut reply.function)))),
                // SAFETY: the host filled the reply.
                abi::RAISED => Err(unsafe { failure(&mut reply.error) }),
                _ => Err(FunctionError::Protocol),
            },
        )
    }

    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> Result<yamaa::Scalar, CallError> {
        let invoke = self.0.callbacks.invoke;
        let invoke = invoke.ok_or(CallError::Raised(FunctionError::Protocol))?;
        let function = handle
            .with::<Bound, _>(|bound| bound.0.as_ref().map_or(std::ptr::null_mut(), Root::pointer))
            .ok_or(CallError::Raised(FunctionError::Protocol))?;
        let arguments = arguments
            .iter()
            .map(|argument| {
                let value = scalar_record(argument.value)?;
                Ok(abi::Argument {
                    name: Str::of(argument.name),
                    value,
                })
            })
            .collect::<Result<Vec<_>, CallError>>()?;
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
            |status, reply| unsafe {
                match status {
                    abi::ANSWERED => scalar(&reply.scalar).map_err(CallError::InvalidResult),
                    abi::RAISED => Err(CallError::Raised(failure(&mut reply.error))),
                    abi::INVALID => Err(CallError::InvalidResult(failure(&mut reply.error))),
                    _ => Err(CallError::Raised(FunctionError::Protocol)),
                }
            },
        )
    }
}
