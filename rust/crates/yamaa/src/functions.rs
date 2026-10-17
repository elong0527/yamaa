//! Facade-owned project function capability.
//!
//! Hosts implement [`ProjectFunctions`] with facade scalar arguments; the
//! core batch port and activation port stay internal. Function invocation
//! happens on the calling thread of the operation, exactly as with the
//! current host crates.

use std::collections::BTreeMap;

use crate::error::{FunctionError, LockSyntaxError};
use crate::scalar::{Scalar, ScalarRef};

/// The host language selecting function admission.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostLanguage {
    Python,
    R,
}

impl HostLanguage {
    pub(crate) fn into_core(self) -> yamaa_core::project_function::Language {
        use yamaa_core::project_function::Language;
        match self {
            Self::Python => Language::Python,
            Self::R => Language::R,
        }
    }
}

/// The packaging-lock format of one activation request.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockKind {
    Uv,
    Renv,
}

impl LockKind {
    pub(crate) fn from_core(kind: yamaa_core::project_environment::LockKind) -> Self {
        use yamaa_core::project_environment::LockKind as Core;
        match kind {
            Core::Uv => Self::Uv,
            Core::Renv => Self::Renv,
        }
    }
}

/// The lock facts for one activation request: the declared kind plus the
/// exact captured lock bytes. Hosts verify versions from these facts without
/// importing code; an empty byte slice means no lock was declared.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockContext<'a> {
    pub language: HostLanguage,
    pub kind: LockKind,
    pub bytes: &'a [u8],
}

/// A project function identity: the logical call string hosts resolve.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionIdentity {
    pub name: String,
    pub call: String,
}

impl FunctionIdentity {
    pub(crate) fn from_core(
        identity: &yamaa_core::function_signature::ProjectFunctionIdentity,
    ) -> Self {
        Self {
            name: identity.name.clone(),
            call: identity.call.clone(),
        }
    }
}

/// One admitted logical parameter, named for the host call.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
}

/// One admitted logical signature: declaration-order host parameter names.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub parameters: Vec<Parameter>,
}

impl Signature {
    pub(crate) fn from_core(signature: &yamaa_core::function_signature::LogicalSignature) -> Self {
        Self {
            parameters: signature
                .parameters()
                .iter()
                .map(|parameter| Parameter {
                    name: parameter.name.clone(),
                })
                .collect(),
        }
    }
}

/// A borrowed mapped argument; callbacks receive nothing beyond this list.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Argument<'a> {
    pub name: &'a str,
    pub value: ScalarRef<'a>,
}

/// One named batch argument: declaration-order name plus one logical value
/// per row the callback is actually given, in original row order.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchArgument {
    pub name: String,
    pub values: Vec<Scalar>,
}

/// An opaque bound-function handle. Hosts create handles in [`bind`] and the
/// facade passes them back to [`invoke`] and [`call_batch`]; the contents are
/// never inspected by the engine.
///
/// [`bind`]: ProjectFunctions::bind
/// [`invoke`]: ProjectFunctions::invoke
/// [`call_batch`]: ProjectFunctions::call_batch
pub struct FunctionHandle {
    inner: std::sync::Arc<std::sync::Mutex<dyn std::any::Any + Send>>,
}

impl Clone for FunctionHandle {
    fn clone(&self) -> Self {
        Self {
            inner: std::sync::Arc::clone(&self.inner),
        }
    }
}

impl std::fmt::Debug for FunctionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FunctionHandle(..)")
    }
}

impl FunctionHandle {
    /// Wrap a host-owned bound handle opaquely. Handles are `Send` so builds
    /// stay `Send`; the interior lock keeps non-`Sync` interpreter handles
    /// shareable without aliasing mutable state.
    pub fn new<T: std::any::Any + Send>(handle: T) -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(handle)),
        }
    }

    /// Clone the wrapped handle when it holds the requested host type.
    /// The closure runs under the handle lock; it must not call back into
    /// the facade.
    pub fn with<T: std::any::Any + Send, R>(&self, with: impl FnOnce(&T) -> R) -> Option<R> {
        self.inner
            .lock()
            .ok()
            .and_then(|guard| guard.downcast_ref::<T>().map(with))
    }
}

/// A single function-call outcome: raised host failures stay distinct from
/// rejected host representations, preserving the engine diagnostics.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum CallError {
    Raised(FunctionError),
    InvalidResult(FunctionError),
}

impl CallError {
    /// Return the carried function failure.
    pub fn error(&self) -> &FunctionError {
        match self {
            Self::Raised(error) | Self::InvalidResult(error) => error,
        }
    }
}

/// Installed metadata, callable resolution and scalar host invocation.
///
/// The facade adapts this port to the internal activation port once per
/// operation. All methods take shared access so one engine serves concurrent
/// independent attempts; hosts use interior mutability when they must. Every
/// method runs on the calling thread of the operation.
pub trait ProjectFunctions: Send + Sync {
    /// Verify the declared packaging lock for the functions about to run.
    /// A non-empty finding list fails activation with those findings.
    fn verify_lock(
        &self,
        lock: &LockContext<'_>,
        functions: &[FunctionIdentity],
    ) -> Result<(), FunctionError>;

    /// Resolve the logical call to a host callable with exactly the logical
    /// parameter names, without invoking project code.
    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &Signature,
    ) -> Result<FunctionHandle, FunctionError>;

    /// Hosts classify user interrupts; their payload propagates immediately.
    fn is_interrupt(&self, error: &FunctionError) -> bool {
        matches!(error, FunctionError::Interrupted { .. })
    }

    /// Invoke an already-bound function once with declaration-order arguments.
    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallError>;

    /// Invoke one bound callable once per batch. The default implementation
    /// invokes row by row with scalar semantics; hosts with a native
    /// vectorized callable may override it. The core re-validates length and
    /// exact result types regardless.
    fn call_batch(
        &self,
        handle: &FunctionHandle,
        rows: usize,
        arguments: &[BatchArgument],
    ) -> Result<Vec<Scalar>, CallError> {
        let mut results = Vec::with_capacity(rows);
        for row in 0..rows {
            let borrowed: Vec<Argument<'_>> = arguments
                .iter()
                .map(|argument| Argument {
                    name: &argument.name,
                    value: ScalarRef::from_owned(&argument.values[row]),
                })
                .collect();
            results.push(self.invoke(handle, &borrowed)?);
        }
        Ok(results)
    }
}

/// A decoded renv lock: the runtime version plus package versions by name.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenvLock {
    pub runtime_version: String,
    pub packages: BTreeMap<String, String>,
}

/// Decode an renv lock without adding a JSON runtime dependency to the host.
/// Shape, limit, JSON and document failures surface as [`LockSyntaxError`].
pub fn decode_renv_lock(bytes: &[u8]) -> Result<RenvLock, LockSyntaxError> {
    yamaa_adapters::project_lock::decode_renv(bytes)
        .map(|locked| RenvLock {
            runtime_version: locked.runtime_version,
            packages: locked.packages,
        })
        .map_err(|error| LockSyntaxError::from_adapter(&error))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl ProjectFunctions for Echo {
        fn verify_lock(
            &self,
            _lock: &LockContext<'_>,
            _functions: &[FunctionIdentity],
        ) -> Result<(), FunctionError> {
            Ok(())
        }

        fn bind(
            &self,
            _identity: &FunctionIdentity,
            _signature: &Signature,
        ) -> Result<FunctionHandle, FunctionError> {
            Ok(FunctionHandle::new(()))
        }

        fn invoke(
            &self,
            _handle: &FunctionHandle,
            arguments: &[Argument<'_>],
        ) -> Result<Scalar, CallError> {
            arguments
                .first()
                .map(|argument| match argument.value {
                    ScalarRef::Int(value) => Ok(Scalar::Int(value + 1)),
                    _ => Err(CallError::InvalidResult(FunctionError::Rejected {
                        reason: "expected int".into(),
                        returned: None,
                    })),
                })
                .unwrap_or(Ok(Scalar::Missing))
        }
    }

    #[test]
    fn default_batch_loops_scalar_semantics() {
        let port = Echo;
        let handle = FunctionHandle::new(());
        assert!(handle.with::<(), _>(|_| true).unwrap_or(false));
        assert!(handle.with::<u8, _>(|_| true).is_none());
        let arguments = vec![BatchArgument {
            name: "x".into(),
            values: vec![Scalar::Int(1), Scalar::Int(2)],
        }];
        assert_eq!(
            port.call_batch(&handle, 2, &arguments).unwrap(),
            vec![Scalar::Int(2), Scalar::Int(3)]
        );
        assert!(port.call_batch(&handle, 0, &[]).unwrap().is_empty());
    }

    #[test]
    fn interrupts_default_to_the_interrupted_variant() {
        let port = Echo;
        assert!(port.is_interrupt(&FunctionError::Interrupted { payload: None }));
        assert!(!port.is_interrupt(&FunctionError::Protocol));
    }

    #[test]
    fn identities_convert_without_host_names() {
        let identity = yamaa_core::function_signature::ProjectFunctionIdentity {
            name: "id".into(),
            call: "pkg.fn".into(),
        };
        let converted = FunctionIdentity::from_core(&identity);
        assert_eq!(converted.name, "id");
        assert_eq!(converted.call, "pkg.fn");
    }

    #[cfg(feature = "yaml")]
    #[test]
    fn renv_locks_decode_without_a_host_json_runtime() {
        let lock =
            br#"{"R":{"Version":"4.3.1"},"Packages":{"pkgA":{"Package":"pkgA","Version":"1.0"}}}"#;
        let decoded = decode_renv_lock(lock).unwrap();
        assert_eq!(decoded.runtime_version, "4.3.1");
        assert_eq!(
            decoded.packages.get("pkgA").map(String::as_str),
            Some("1.0")
        );
        let error = decode_renv_lock(b"not json").unwrap_err();
        assert!(matches!(error, LockSyntaxError::Json { .. }));
        let error = decode_renv_lock(br#"{"R":{}}"#).unwrap_err();
        assert!(matches!(error, LockSyntaxError::Shape { .. }));
    }
}
