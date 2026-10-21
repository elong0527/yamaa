//! Facade-owned project function capability.
//!
//! Hosts implement [`ProjectFunctions`] with facade scalar arguments; the
//! core batch port and activation port stay internal. Function invocation
//! happens on the calling thread of the operation, exactly as with the
//! current host crates.

use std::collections::BTreeMap;

use crate::error::{FunctionError, LockFinding, LockSyntaxError};
use crate::scalar::{Scalar, ScalarRef};

/// The host language selecting function admission. A host names its own
/// language; there is no default. An engine that claims no host language,
/// such as the command line's, is configured with
/// [`EngineOptions::neutral`](crate::EngineOptions::neutral) instead.
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

/// A declared column type, the vocabulary of a function's `returns`. There is
/// no boolean column type, although booleans remain valid function scalars.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnType {
    Str,
    Int,
    Float,
    Date,
    DateTime,
}

impl ColumnType {
    pub(crate) fn from_core(kind: yamaa_core::value::ColumnType) -> Self {
        use yamaa_core::value::ColumnType as Core;
        match kind {
            Core::Str => Self::Str,
            Core::Int => Self::Int,
            Core::Float => Self::Float,
            Core::Date => Self::Date,
            Core::DateTime => Self::DateTime,
        }
    }
}

/// One admitted logical signature: declaration-order host parameter names,
/// the declared result type and whether an invoked binding may return
/// missing. Hosts convert a returned host value by `returns`; the engine
/// still checks the result type and missing permission itself.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub parameters: Vec<Parameter>,
    pub returns: ColumnType,
    pub may_return_missing: bool,
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
            returns: ColumnType::from_core(signature.returns()),
            may_return_missing: signature.may_return_missing(),
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

/// What a Python host reports of its installation for [`verify_uv_lock`].
/// The engine reads the lock and compares versions; the host only observes.
/// Every method runs on the calling thread of the operation, and a failure
/// is the verification's failure, unchanged.
pub trait InstalledDistributions {
    /// The installed distributions that provide each of the top-level
    /// `modules`, by module. A module that no distribution provides may be
    /// absent or have no distributions.
    fn providers(&self, modules: &[String])
        -> Result<BTreeMap<String, Vec<String>>, FunctionError>;

    /// Whether `module` belongs to the interpreter's standard library, whose
    /// functions verification exempts.
    fn standard_library(&self, module: &str) -> Result<bool, FunctionError>;

    /// The installed version of the distribution with the normalized name
    /// `distribution`.
    fn version(&self, distribution: &str) -> Result<InstalledVersion, FunctionError>;

    /// The marker environment that a lock's `resolution-markers` select by:
    /// the values PEP 508 names, read from `sys`, `platform` and `os`.
    fn environment(&self) -> Result<BTreeMap<String, String>, FunctionError>;

    /// The interpreter's limit on the digits of an integer converted from
    /// text (`sys.get_int_max_str_digits()`), `None` when it is turned off.
    /// Verification asks only when a version it reads has a number of more
    /// than 640 digits, below which CPython converts whatever its limit.
    /// The provided method reports CPython's default, 4,300 digits.
    fn integer_digits(&self) -> Result<Option<usize>, FunctionError> {
        Ok(Some(yamaa_adapters::project_lock::VERSION_DIGITS))
    }
}

/// An installed distribution's version, as its metadata records it.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstalledVersion {
    /// No distribution of that name is installed.
    Missing,
    /// The version text that its metadata records.
    Version(String),
    /// Its metadata records no version text.
    Unreadable,
}

/// Verify a uv lock for the Python functions about to run (FUNC-3): yamaa
/// and each distribution that provides a called function's top-level module
/// must be installed at the one version that the lock selects for this
/// interpreter. A module that no distribution provides is exempt when it
/// belongs to the standard library. The engine reads the lock and compares
/// under PEP 440, 503 and 508, as Python's `packaging` 26 does, so a Python
/// host needs no packaging library; `installed` reports the installation.
///
/// Findings fail with [`FunctionError::Lock`]; a lock that cannot be read as
/// a uv lock with [`FunctionError::LockSyntax`]; an installed distribution
/// whose name is invalid with [`FunctionError::Protocol`]; and a failure of
/// `installed` propagates unchanged. No functions means no lock read and no
/// observation.
///
/// A version that the comparison reads with a number longer than the
/// interpreter's integer limit allows (`installed.integer_digits()`, 4,300
/// digits by default) fails with [`LockSyntaxError::Limit`], wherever it is
/// written: in the lock, in a resolution marker or in the installed
/// metadata, as `packaging` refused to convert it.
pub fn verify_uv_lock(
    lock: &[u8],
    functions: &[FunctionIdentity],
    installed: &dyn InstalledDistributions,
) -> Result<(), FunctionError> {
    use yamaa_adapters::project_lock::{verify_uv, UvFailure};
    /// The installation behind the adapters' observation port.
    struct Installation<'a>(&'a dyn InstalledDistributions);

    impl yamaa_adapters::project_lock::PythonInstallation for Installation<'_> {
        type Error = FunctionError;

        fn providers(
            &mut self,
            modules: &[String],
        ) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
            self.0.providers(modules)
        }

        fn standard_library(&mut self, module: &str) -> Result<bool, FunctionError> {
            self.0.standard_library(module)
        }

        fn version(
            &mut self,
            distribution: &str,
        ) -> Result<yamaa_adapters::project_lock::Installed, FunctionError> {
            use yamaa_adapters::project_lock::Installed;
            Ok(match self.0.version(distribution)? {
                InstalledVersion::Missing => Installed::Missing,
                InstalledVersion::Version(text) => Installed::Version(text),
                InstalledVersion::Unreadable => Installed::Unreadable,
            })
        }

        fn environment(&mut self) -> Result<BTreeMap<String, String>, FunctionError> {
            self.0.environment()
        }

        fn integer_digits(&mut self) -> Result<Option<usize>, FunctionError> {
            self.0.integer_digits()
        }
    }

    let calls = functions
        .iter()
        .map(|function| function.call.as_str())
        .collect::<Vec<_>>();
    match verify_uv(lock, &calls, &mut Installation(installed)) {
        Ok(findings) if findings.is_empty() => Ok(()),
        Ok(findings) => Err(FunctionError::Lock(
            findings.into_iter().map(LockFinding::from_core).collect(),
        )),
        Err(UvFailure::Lock(field)) => {
            Err(FunctionError::LockSyntax(LockSyntaxError::Shape { field }))
        }
        Err(UvFailure::Distribution) => Err(FunctionError::Protocol),
        Err(UvFailure::Digits {
            origin,
            digits,
            limit,
        }) => Err(FunctionError::LockSyntax(LockSyntaxError::Limit {
            resource: origin,
            limit,
            actual: Some(digits),
        })),
        Err(UvFailure::Host(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LockReason;

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
    fn signatures_carry_the_declared_result() {
        use yamaa_core::function_signature::{LogicalSignature, Parameter, Presence};
        use yamaa_core::value::{ColumnType as Core, ValueType};
        let parameter = Parameter {
            name: "x".into(),
            host_name: "x".into(),
            kind: ValueType::Int,
            accepts_missing: false,
            presence: Presence::Required,
        };
        for (core, facade) in [
            (Core::Str, ColumnType::Str),
            (Core::Int, ColumnType::Int),
            (Core::Float, ColumnType::Float),
            (Core::Date, ColumnType::Date),
            (Core::DateTime, ColumnType::DateTime),
        ] {
            for may_return_missing in [false, true] {
                let signature =
                    LogicalSignature::new(vec![parameter.clone()], core, may_return_missing)
                        .unwrap();
                let converted = Signature::from_core(&signature);
                assert_eq!(converted.returns, facade);
                assert_eq!(converted.may_return_missing, may_return_missing);
                assert_eq!(converted.parameters[0].name, "x");
            }
        }
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

    /// An installation with one installed distribution per module, named
    /// after it, which records the observations it is asked for.
    struct Installation {
        installed: &'static [(&'static str, &'static str)],
        interrupt: bool,
        observed: std::sync::Mutex<Vec<String>>,
    }

    impl Installation {
        fn new(installed: &'static [(&'static str, &'static str)]) -> Self {
            Self {
                installed,
                interrupt: false,
                observed: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn observe(&self, observation: String) {
            self.observed.lock().unwrap().push(observation);
        }
    }

    impl InstalledDistributions for Installation {
        fn providers(
            &self,
            modules: &[String],
        ) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
            self.observe("providers".to_owned());
            Ok(modules
                .iter()
                .map(|module| (module.clone(), vec![module.replace('_', " ")]))
                .collect())
        }

        fn standard_library(&self, _module: &str) -> Result<bool, FunctionError> {
            Ok(false)
        }

        fn version(&self, distribution: &str) -> Result<InstalledVersion, FunctionError> {
            self.observe(format!("version {distribution}"));
            if self.interrupt {
                return Err(FunctionError::Interrupted { payload: None });
            }
            Ok(self
                .installed
                .iter()
                .find(|(name, _)| *name == distribution)
                .map_or(InstalledVersion::Missing, |(_, version)| {
                    InstalledVersion::Version((*version).to_owned())
                }))
        }

        fn environment(&self) -> Result<BTreeMap<String, String>, FunctionError> {
            self.observe("environment".to_owned());
            Ok(BTreeMap::new())
        }
    }

    /// An installation whose interpreter has its integer limit turned off.
    struct Unlimited(Installation);

    impl InstalledDistributions for Unlimited {
        fn providers(
            &self,
            modules: &[String],
        ) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
            self.0.providers(modules)
        }

        fn standard_library(&self, module: &str) -> Result<bool, FunctionError> {
            self.0.standard_library(module)
        }

        fn version(&self, distribution: &str) -> Result<InstalledVersion, FunctionError> {
            self.0.version(distribution)
        }

        fn environment(&self) -> Result<BTreeMap<String, String>, FunctionError> {
            self.0.environment()
        }

        fn integer_digits(&self) -> Result<Option<usize>, FunctionError> {
            Ok(None)
        }
    }

    #[test]
    fn uv_locks_verify_against_the_host_installation() {
        let lock = b"version = 1\n[[package]]\nname = \"yamaa\"\nversion = \"1.0\"\n";
        let calls = [FunctionIdentity {
            name: "f".into(),
            call: "yamaa.spec".into(),
        }];
        let installed = Installation::new(&[("yamaa", "1.0.0")]);
        assert!(verify_uv_lock(lock, &calls, &installed).is_ok());
        assert_eq!(
            *installed.observed.lock().unwrap(),
            ["providers", "version yamaa"]
        );
        let installed = Installation::new(&[("yamaa", "2.0")]);
        match verify_uv_lock(lock, &calls, &installed) {
            Err(FunctionError::Lock(findings)) => assert_eq!(
                findings,
                [LockFinding::new(
                    "yamaa".into(),
                    LockReason::VersionMismatch,
                    vec!["1.0".into()],
                    Some("2.0".into()),
                )]
            ),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            verify_uv_lock(b"version = 2\npackage = []\n", &calls, &installed),
            Err(FunctionError::LockSyntax(LockSyntaxError::Shape {
                field: "uv version"
            }))
        ));
        // The installation names `not_a_name`'s provider `not a name`.
        let invalid = [FunctionIdentity {
            name: "g".into(),
            call: "not_a_name.f".into(),
        }];
        assert!(matches!(
            verify_uv_lock(lock, &invalid, &installed),
            Err(FunctionError::Protocol)
        ));
        let mut interrupted = Installation::new(&[]);
        interrupted.interrupt = true;
        assert!(matches!(
            verify_uv_lock(lock, &calls, &interrupted),
            Err(FunctionError::Interrupted { payload: None })
        ));
        let untouched = Installation::new(&[]);
        assert!(verify_uv_lock(b"not a lock", &[], &untouched).is_ok());
        assert!(untouched.observed.lock().unwrap().is_empty());
        // Under CPython's default limit, which the provided method reports, a
        // number of 4,301 digits refuses with its length and the limit.
        let past: &'static str = Box::leak("1".repeat(4301).into_boxed_str());
        let huge: &'static [(&'static str, &'static str)] = Box::leak(Box::new([("yamaa", past)]));
        assert!(matches!(
            verify_uv_lock(lock, &calls, &Installation::new(huge)),
            Err(FunctionError::LockSyntax(LockSyntaxError::Limit {
                resource: "installed version",
                limit: 4300,
                actual: Some(4301)
            }))
        ));
        // An interpreter that converts any length reads the version, which
        // the lock does not hold.
        match verify_uv_lock(lock, &calls, &Unlimited(Installation::new(huge))) {
            Err(FunctionError::Lock(findings)) => assert_eq!(
                findings,
                [LockFinding::new(
                    "yamaa".into(),
                    LockReason::VersionMismatch,
                    vec!["1.0".into()],
                    Some(past.into()),
                )]
            ),
            other => panic!("{other:?}"),
        }
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
