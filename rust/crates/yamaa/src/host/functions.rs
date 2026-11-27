//! The project-function port: [`ProjectFunctions`], which a host implements
//! with the scalar values of [`crate::scalar`], and every other type its
//! methods take and return, its failures included.
//!
//! The trait is the public form of the engine's one host-functions port,
//! whose core trait stays internal. Function invocation happens on the
//! calling thread of the operation, exactly as with the current host crates.
//! A project function's failure is a [`FunctionError`], which the engine
//! reports as an issue of the result being built; one that
//! [`ProjectFunctions::is_interrupt`] classifies as the host's interrupt
//! cancels the call instead. No [`Failure`](crate::Failure) carries the
//! error itself.

use std::any::Any;
use std::sync::Arc;

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
    /// The language's key, `python` or `r`: the key of its `environments`
    /// entry and of its `implementations` entry in a function file, and the
    /// value the command line's `--host` takes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::R => "r",
        }
    }

    /// The language whose key is `text`, as [`HostLanguage::as_str`] spells
    /// it; none for any other text.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "python" => Some(Self::Python),
            "r" => Some(Self::R),
            _ => None,
        }
    }

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
    Time,
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
            Core::Time => Self::Time,
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

/// An opaque bound-function handle. Hosts create handles in [`bind`] and the
/// facade passes them back to [`invoke`]; the contents are never inspected by
/// the engine.
///
/// [`bind`]: ProjectFunctions::bind
/// [`invoke`]: ProjectFunctions::invoke
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

/// `the function raised` or `the function returned an invalid result`; the
/// carried failure is its [`source`](std::error::Error::source).
impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Raised(_) => "the function raised",
            Self::InvalidResult(_) => "the function returned an invalid result",
        })
    }
}

impl std::error::Error for CallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.error())
    }
}

/// Installed metadata, callable resolution and scalar host invocation.
///
/// The engine's host-functions port: each build verifies, binds and invokes
/// through it directly. All methods take shared access so one engine serves
/// concurrent independent attempts; hosts use interior mutability when they
/// must. Every method runs on the calling thread of the operation.
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
}

/// An opaque host-owned payload retained with a function failure.
///
/// Hosts attach their original failure objects (which stay visible
/// through identity-preserving accessors) without exposing host types in the
/// facade surface. Payloads are `Send` so builds stay `Send`; the interior
/// lock keeps non-`Sync` host objects (such as single-threaded interpreter
/// handles) shareable by reference without ever aliasing mutable state.
pub struct HostPayload {
    inner: Arc<std::sync::Mutex<dyn Any + Send>>,
}

impl Clone for HostPayload {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl std::fmt::Debug for HostPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostPayload(..)")
    }
}

impl HostPayload {
    /// Attach an opaque host-owned payload to a function failure.
    pub fn new<T: Any + Send>(value: T) -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(value)),
        }
    }

    /// Clone the attached payload when it holds the requested host type.
    /// The closure runs under the payload lock; it must not call back into
    /// the facade.
    pub fn with<T: Any + Send, R>(&self, with: impl FnOnce(&T) -> R) -> Option<R> {
        self.inner
            .lock()
            .ok()
            .and_then(|guard| guard.downcast_ref::<T>().map(with))
    }
}

/// A caller-supplied project function failure, the public form of the host
/// classification. Variants that carry ordinary facts project into report
/// diagnostics; opaque variants stay held without classification facts.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum FunctionError {
    /// A raised host exception with its class and message.
    Exception {
        class: String,
        message: String,
        payload: Option<HostPayload>,
    },
    /// A rejected host result with its reason and optional returned type name.
    Rejected {
        reason: String,
        returned: Option<String>,
    },
    /// A host wire-representation failure; classifies as a rejection.
    Representation {
        message: String,
        payload: Option<HostPayload>,
    },
    /// A rejected host scalar with its stable reason.
    Scalar { reason: &'static str },
    /// Lock verification findings.
    Lock(Vec<LockFinding>),
    /// A lock-syntax admission failure.
    LockSyntax(LockSyntaxError),
    /// A host-classified interrupt with its optional original payload.
    Interrupted { payload: Option<HostPayload> },
    /// A host SDK boundary failure with its optional original payload.
    SdkBoundary { payload: Option<HostPayload> },
    /// A host protocol violation.
    Protocol,
    /// A stable internal code with no internal payload.
    Internal { code: &'static str },
}

impl FunctionError {
    /// Return the stable function-failure code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Exception { .. } => "exception",
            Self::Rejected { .. } => "rejected",
            Self::Representation { .. } => "representation",
            Self::Scalar { .. } => "scalar",
            Self::Lock(_) => "lock",
            Self::LockSyntax(_) => "lock_syntax",
            Self::Interrupted { .. } => "interrupted",
            Self::SdkBoundary { .. } => "sdk_boundary",
            Self::Protocol => "protocol",
            Self::Internal { code } => code,
        }
    }

    /// Return the function-failure message.
    pub fn message(&self) -> String {
        match self {
            Self::Exception { class, message, .. } => format!("{class}: {message}"),
            Self::Rejected { reason, .. } => reason.clone(),
            Self::Representation { message, .. } => message.clone(),
            Self::Scalar { reason } => (*reason).to_owned(),
            Self::Lock(findings) => format!(
                "lock verification failed with {} finding(s)",
                findings.len()
            ),
            Self::LockSyntax(error) => format!("invalid lock syntax: {}", error.message()),
            Self::Interrupted { .. } => "host interrupt".to_owned(),
            Self::SdkBoundary { .. } => "host SDK boundary failure".to_owned(),
            Self::Protocol => "host protocol violation".to_owned(),
            Self::Internal { code } => (*code).to_owned(),
        }
    }
}

/// `<message> [<code>]`, the message and the stable code.
impl std::fmt::Display for FunctionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [{}]", self.message(), self.code())
    }
}

/// Its message already says a lock-syntax failure's own, so it names no
/// source.
impl std::error::Error for FunctionError {}

/// A classified host packaging finding, the public form of the lock finding.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockFinding {
    pub package: String,
    pub reason: LockReason,
    pub expected: Vec<String>,
    pub actual: Option<String>,
}

impl LockFinding {
    /// Retain one classified host packaging finding.
    pub fn new(
        package: String,
        reason: LockReason,
        expected: Vec<String>,
        actual: Option<String>,
    ) -> Self {
        Self {
            package,
            reason,
            expected,
            actual,
        }
    }

    pub(crate) fn into_core(self) -> yamaa_core::diagnostic::LockFinding {
        yamaa_core::diagnostic::LockFinding {
            package: self.package,
            reason: self.reason.into_core(),
            expected: self.expected,
            actual: self.actual,
        }
    }

    pub(crate) fn from_core(finding: yamaa_core::diagnostic::LockFinding) -> Self {
        Self {
            package: finding.package,
            reason: LockReason::from_core(finding.reason),
            expected: finding.expected,
            actual: finding.actual,
        }
    }
}

/// A closed classified lock token, the public form of the lock reason.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockReason {
    DistributionNotIdentified,
    VersionNotLocked,
    AmbiguousLockVersion,
    InvalidLockedVersion,
    PackageNotInstalled,
    InvalidInstalledVersion,
    VersionMismatch,
}

impl LockReason {
    /// Decode only the closed classified token, never a host message.
    pub fn parse(value: &str) -> Option<Self> {
        yamaa_core::diagnostic::LockReason::parse(value).map(Self::from_core)
    }

    /// Return the closed classified token.
    pub fn as_str(self) -> &'static str {
        self.into_core().as_str()
    }

    pub(crate) fn into_core(self) -> yamaa_core::diagnostic::LockReason {
        use yamaa_core::diagnostic::LockReason as Core;
        match self {
            Self::DistributionNotIdentified => Core::DistributionNotIdentified,
            Self::VersionNotLocked => Core::VersionNotLocked,
            Self::AmbiguousLockVersion => Core::AmbiguousLockVersion,
            Self::InvalidLockedVersion => Core::InvalidLockedVersion,
            Self::PackageNotInstalled => Core::PackageNotInstalled,
            Self::InvalidInstalledVersion => Core::InvalidInstalledVersion,
            Self::VersionMismatch => Core::VersionMismatch,
        }
    }

    pub(crate) fn from_core(reason: yamaa_core::diagnostic::LockReason) -> Self {
        use yamaa_core::diagnostic::LockReason as Core;
        match reason {
            Core::DistributionNotIdentified => Self::DistributionNotIdentified,
            Core::VersionNotLocked => Self::VersionNotLocked,
            Core::AmbiguousLockVersion => Self::AmbiguousLockVersion,
            Core::InvalidLockedVersion => Self::InvalidLockedVersion,
            Core::PackageNotInstalled => Self::PackageNotInstalled,
            Core::InvalidInstalledVersion => Self::InvalidInstalledVersion,
            Core::VersionMismatch => Self::VersionMismatch,
        }
    }
}

/// A lock-syntax admission failure, the public form of the lock error.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockSyntaxError {
    Limit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
    Shape {
        field: &'static str,
    },
    Json {
        line: usize,
        column: usize,
    },
    Document {
        resource_refusal: bool,
    },
}

impl LockSyntaxError {
    pub(crate) fn from_adapter(error: &yamaa_adapters::project_lock::Error) -> Self {
        use yamaa_adapters::project_lock::Error;
        match error {
            Error::Limit(limit) => Self::Limit {
                resource: limit.resource,
                limit: limit.limit,
                actual: limit.actual,
            },
            Error::Shape(field) => Self::Shape { field },
            Error::Json(error) => Self::Json {
                line: error.line(),
                column: error.column(),
            },
            Error::Document(error) => Self::Document {
                resource_refusal: matches!(
                    error,
                    yamaa_adapters::yaml_decode::DecodeFailure::Limit(_)
                ),
            },
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Limit {
                resource,
                limit,
                actual,
            } => format!("{resource}: required {actual:?}, limit {limit}"),
            Self::Shape { field } => format!("invalid lock shape: {field}"),
            Self::Json { line, column } => format!("invalid lock JSON at {line}:{column}"),
            Self::Document { .. } => "invalid lock document".to_owned(),
        }
    }
}

/// The lock-syntax message, as a lock-syntax function failure says it.
impl std::fmt::Display for LockSyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for LockSyntaxError {}

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

    /// The handle's typed access, the seam each host's port reads its
    /// callable through: it yields only the type the handle was made with.
    #[test]
    fn a_handle_yields_only_the_type_it_holds() {
        let handle = FunctionHandle::new(());
        assert!(handle.with::<(), _>(|_| true).unwrap_or(false));
        assert!(handle.with::<u8, _>(|_| true).is_none());
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

    #[test]
    fn function_errors_carry_stable_codes_and_messages() {
        let cases: Vec<(FunctionError, &str)> = vec![
            (
                FunctionError::Exception {
                    class: "ValueError".into(),
                    message: "bad".into(),
                    payload: None,
                },
                "exception",
            ),
            (
                FunctionError::Rejected {
                    reason: "no scalar".into(),
                    returned: Some("list".into()),
                },
                "rejected",
            ),
            (
                FunctionError::Representation {
                    message: "bad wire".into(),
                    payload: None,
                },
                "representation",
            ),
            (FunctionError::Scalar { reason: "bad int" }, "scalar"),
            (FunctionError::Lock(vec![]), "lock"),
            (
                FunctionError::LockSyntax(LockSyntaxError::Shape { field: "R" }),
                "lock_syntax",
            ),
            (FunctionError::Interrupted { payload: None }, "interrupted"),
            (FunctionError::SdkBoundary { payload: None }, "sdk_boundary"),
            (FunctionError::Protocol, "protocol"),
            (FunctionError::Internal { code: "custom" }, "custom"),
        ];
        for (error, code) in cases {
            assert_eq!(error.code(), code);
            assert!(!error.message().is_empty());
        }
    }

    #[test]
    fn host_payloads_round_trip_host_types() {
        let payload = HostPayload::new(42u32);
        assert_eq!(payload.with::<u32, _>(|value| *value), Some(42u32));
        assert_eq!(payload.with::<String, _>(|value| value.clone()), None);
        let cloned = payload.clone();
        assert_eq!(cloned.with::<u32, _>(|value| *value), Some(42u32));
        let debug = format!("{payload:?}");
        assert_eq!(debug, "HostPayload(..)");
    }

    #[test]
    fn lock_reasons_round_trip_closed_tokens() {
        for token in [
            "distribution_not_identified",
            "version_not_locked",
            "ambiguous_lock_version",
            "invalid_locked_version",
            "package_not_installed",
            "invalid_installed_version",
            "version_mismatch",
        ] {
            let reason = LockReason::parse(token).unwrap();
            assert_eq!(reason.as_str(), token);
        }
        assert_eq!(LockReason::parse("other"), None);
        let finding = LockFinding::new(
            "pkg".into(),
            LockReason::VersionMismatch,
            vec!["1.0".into()],
            Some("2.0".into()),
        );
        let core = finding.into_core();
        assert_eq!(core.package, "pkg");
        assert_eq!(core.reason.as_str(), "version_mismatch");
    }
}
