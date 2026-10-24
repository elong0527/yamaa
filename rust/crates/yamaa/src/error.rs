//! The one error of every facade operation, [`Failure`], and the
//! facade-owned failures it can carry.
//!
//! Every fallible facade operation returns `Result<_, Failure>` (#3765): a
//! check, run, stream, resolve or submission, a project's names and listed
//! specifications, a save, `read_odm`, `file_type`, `save_table`, `lint` and
//! `schema`, and a streamed run's result and reader. A failure's
//! [`FailureKind`] says what refused the operation, and its message is the
//! hosts' error text (D39). Internal error enums (per-module failures, decode
//! failures, resource errors) never appear in a facade signature; they
//! convert into these types at the boundary.
//!
//! The ports keep their own errors, which a `Failure` never carries:
//! [`FunctionError`] and [`CallError`](crate::CallError) are the
//! [`ProjectFunctions`](crate::ProjectFunctions) port's, [`ResolverFailure`]
//! the resolver's, and [`LockSyntaxError`] and
//! [`ScalarDecodeError`](crate::ScalarDecodeError) belong to the port
//! helpers; [`InputFrame::import`](crate::InputFrame::import), a value
//! constructor with no action to name, keeps [`TableError`]. Each carries a
//! stable code and a message.

use std::any::Any;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Report};
use crate::resolver::ResourceKind;
use crate::table::Table;
use crate::text::{self, Action};

/// A refused operation, the one error of every facade operation (#3765):
/// what refused it, D39's error text, the issues that explain it, and the
/// files it wrote before it was refused (P-API-1).
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Failure {
    /// What refused the operation. Only a [`FailureKind::Rejected`] failure
    /// carries issues.
    pub kind: FailureKind,
    /// D39's error text: the call that refused, then the first issue's line
    /// and the count of the other issues, such as `cannot save ADAE: column
    /// ASTDY reads TRTSDT, which is declared after it [forward_reference,
    /// COLUMN-5] (and 2 more issues)`, or, for a failure no issue explains,
    /// what failed, such as `cannot read adae.yaml: operation cancelled`.
    /// Only ASCII; see [`crate::text`].
    pub message: String,
    /// The issues table both hosts attach to their error (D28).
    pub issues: Table,
    /// The same issues as typed records, first issue first, for the command
    /// line's printed lines and JSON.
    pub diagnostics: Vec<Diagnostic>,
    /// The files a refused save wrote before it stopped, such as a failed
    /// run's report, in publication order and spelled as `save` returns
    /// them; empty for any other refusal.
    pub paths: Vec<String>,
}

/// What refused a facade operation, the kind of a [`Failure`]: its issues,
/// or one of the failures that no issue explains, each carrying what names
/// it. A table's failure is boxed, so every `Result<_, Failure>` stays
/// small.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The operation was refused with issues: the failure's `diagnostics`
    /// and `issues`.
    Rejected,
    /// The operation was cancelled: its token was cancelled, or a project
    /// function raised the host's interrupt, as
    /// [`ProjectFunctions::is_interrupt`](crate::ProjectFunctions::is_interrupt)
    /// classifies it (D46). The operation returns no result and no issue.
    Cancelled,
    /// A table could not be built or imported.
    Table(Box<TableError>),
    /// A format feature is not compiled in.
    Unavailable { feature: &'static str },
    /// A stable internal code with no internal payload.
    Internal { code: &'static str },
    /// A streamed run refused a reader or its result (#3697), with the
    /// stable `code` that names why: `host_thread`, a run that can call its
    /// host's project functions would be built off the thread that started
    /// it; `stream_taken`, its output stream was already started or read;
    /// `stream_open`, its result was asked for while a reader's stream has
    /// not ended; and `busy`, another call, such as a project function its
    /// build called, uses it. See [`crate::stream`].
    Stream { code: &'static str },
}

impl FailureKind {
    /// What a failure of this kind says after its action when no issue
    /// explains it; none for a refusal with issues, whose first issue
    /// speaks.
    fn detail(&self) -> Option<String> {
        let detail = match self {
            Self::Rejected => return None,
            Self::Cancelled => "operation cancelled".to_owned(),
            Self::Table(failure) => {
                format!("table failure ({}): {}", failure.code(), failure.message())
            }
            Self::Unavailable { feature } => format!("format feature not compiled in: {feature}"),
            Self::Internal { code } => format!("internal failure: {code}"),
            Self::Stream { code } => {
                let reason = match *code {
                    "host_thread" => {
                        "a run that can call project functions is built only on the thread that started it"
                    }
                    "stream_taken" => "the run's output stream has already been read",
                    "stream_open" => "the run's output stream has not been read to its end",
                    "busy" => "the streamed run is in use",
                    _ => "the streamed run refused",
                };
                format!("{reason} [{code}]")
            }
        };
        Some(detail)
    }
}

impl Failure {
    /// The refusal of `action` that `report`'s issues explain.
    pub(crate) fn rejected(action: Action<'_>, report: &Report) -> Self {
        Self {
            kind: FailureKind::Rejected,
            message: text::error_text(action, &report.diagnostics, None),
            issues: report.table(),
            diagnostics: report.diagnostics.clone(),
            paths: Vec::new(),
        }
    }

    /// The refusal of `action` for a failure of `kind` that no issue
    /// explains: the kind's own detail follows the action.
    pub(crate) fn of(action: Action<'_>, kind: FailureKind) -> Self {
        let detail = kind.detail();
        Self {
            kind,
            message: text::error_text(action, &[], detail.as_deref()),
            ..Self::rejected(action, &Report::default())
        }
    }

    /// The cancellation of `action`.
    pub(crate) fn cancelled(action: Action<'_>) -> Self {
        Self::of(action, FailureKind::Cancelled)
    }

    /// The same refusal, after a save wrote `paths`.
    pub(crate) fn with_paths(mut self, paths: Vec<String>) -> Self {
        self.paths = paths;
        self
    }

    /// The refusal of `action` that `issues`, which no file locates,
    /// explain.
    pub(crate) fn from_issues(
        action: Action<'_>,
        issues: &[yamaa_core::diagnostic::Diagnostic],
    ) -> Self {
        Self::rejected(action, &Report::from_issues(issues))
    }

    /// A refusal that one registered finding explains.
    pub(crate) fn from_finding(
        action: Action<'_>,
        finding: yamaa_core::diagnostic::Diagnostic,
    ) -> Self {
        Self::from_issues(action, std::slice::from_ref(&finding))
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

/// A caller-supplied resolver failure, carrying its stable code and message.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolverFailure {
    code: &'static str,
    message: String,
}

impl ResolverFailure {
    /// Return the stable resolver failure code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Return the resolver failure message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Build a resolver failure from its stable code. Unknown codes reject
    /// the request, matching the engine boundary.
    pub fn from_code(code: &str) -> Self {
        Self::from_adapter_code(code)
    }

    /// Report that the requested name reaches an entry of `kind` instead of
    /// a regular file; its code is the kind's name. The engine judges the
    /// name from this report as [`ResourceKind`] describes.
    pub fn from_kind(kind: ResourceKind) -> Self {
        Self::from_adapter(yamaa_adapters::host_resources::Failure::from(
            kind.into_core(),
        ))
    }

    /// The kind this failure reports, when it reports one: a kind failure,
    /// or `missing`.
    pub fn kind(&self) -> Option<ResourceKind> {
        yamaa_adapters::host_resources::Failure::from_name(self.code)
            .resource_kind()
            .map(ResourceKind::from_core)
    }

    pub(crate) fn from_adapter_code(code: &str) -> Self {
        Self::from_adapter(yamaa_adapters::host_resources::Failure::from_name(code))
    }

    pub(crate) fn from_adapter(failure: yamaa_adapters::host_resources::Failure) -> Self {
        use yamaa_adapters::host_resources::{Failure, ResourceKind as Kind};
        let (code, message) = match failure {
            Failure::Missing | Failure::Kind(Kind::Missing) => {
                ("missing", "resolver reports the resource as missing")
            }
            Failure::Changed => ("changed", "resolver reports the resource as changed"),
            Failure::Cancelled => ("cancelled", "resolver reports cancellation"),
            Failure::Publication => ("publication", "resolver reports a publication failure"),
            Failure::Cleanup => ("cleanup", "resolver reports a cleanup failure"),
            Failure::Unreadable => ("unreadable", "resolver reports the resource as unreadable"),
            Failure::Kind(kind) => (
                kind.name(),
                match kind {
                    Kind::File => "resolver reports a file where a directory is required",
                    Kind::Directory => "resolver reports a directory",
                    Kind::SymbolicLink => "resolver reports a symbolic link",
                    Kind::Junction => "resolver reports a junction",
                    Kind::ReparsePoint => "resolver reports a reparse point",
                    Kind::Other | Kind::Missing => {
                        "resolver reports an entry that is not a regular file"
                    }
                },
            ),
            Failure::Rejected | Failure::Limit(_) | Failure::Path(_) => {
                ("rejected", "resolver rejects the request")
            }
        };
        Self {
            code,
            message: message.to_owned(),
        }
    }
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
    /// A caller-supplied registry has no binding for the requested call.
    MissingBinding(String),
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
            Self::MissingBinding(_) => "missing_binding",
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
            Self::MissingBinding(call) => format!("missing registry binding: {call}"),
            Self::Internal { code } => (*code).to_owned(),
        }
    }
}

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

/// A table that could not be built or imported, carrying its stable code and
/// message.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableError {
    code: &'static str,
    message: String,
}

impl TableError {
    /// Return the stable table failure code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Return the table failure message.
    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn from_public(error: yamaa_adapters::public_table::TableError) -> Self {
        use yamaa_adapters::public_table::TableError as Public;
        let code = match error {
            Public::Limit { .. } => "limit",
            Public::Representation { .. } => "representation",
            Public::InvalidTable => "invalid_table",
            Public::Internal => "internal",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }

    #[cfg(feature = "arrow")]
    pub(crate) fn from_table(error: yamaa_adapters::column_table::TableError) -> Self {
        use yamaa_adapters::column_table::TableError as Table;
        let code = match error {
            Table::Limit { .. } => "limit",
            Table::SchemaMismatch { .. } => "schema_mismatch",
            Table::ArrayType { .. } => "array_type",
            Table::TemporalRange { .. } => "temporal",
            Table::Representation { .. } => "representation",
            Table::Arrow(_) => "arrow",
        };
        Self {
            code,
            message: format!("{error:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_failures_carry_stable_codes_and_messages() {
        use yamaa_adapters::host_resources::Failure;
        for (failure, code) in [
            (Failure::Missing, "missing"),
            (Failure::Changed, "changed"),
            (Failure::Cancelled, "cancelled"),
            (Failure::Rejected, "rejected"),
            (Failure::Publication, "publication"),
            (Failure::Cleanup, "cleanup"),
            (Failure::Unreadable, "unreadable"),
            (Failure::Limit(Some(9)), "rejected"),
        ] {
            let converted = ResolverFailure::from_adapter(failure);
            assert_eq!(converted.code(), code);
            assert!(!converted.message().is_empty());
            assert_eq!(ResolverFailure::from_code(code), converted);
        }
    }

    #[test]
    fn resolver_kind_failures_round_trip_through_their_codes() {
        for kind in [
            ResourceKind::File,
            ResourceKind::Directory,
            ResourceKind::SymbolicLink,
            ResourceKind::Junction,
            ResourceKind::ReparsePoint,
            ResourceKind::Other,
            ResourceKind::Missing,
        ] {
            let failure = ResolverFailure::from_kind(kind);
            assert_eq!(failure.code(), kind.name());
            assert_eq!(failure.kind(), Some(kind));
            assert_eq!(ResolverFailure::from_code(kind.name()), failure);
            assert!(!failure.message().is_empty());
        }
        assert_eq!(
            ResolverFailure::from_code("missing").kind(),
            Some(ResourceKind::Missing)
        );
        assert_eq!(ResolverFailure::from_code("changed").kind(), None);
        assert_eq!(ResolverFailure::from_code("link").code(), "rejected");
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
            (FunctionError::MissingBinding("f".into()), "missing_binding"),
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

    #[test]
    fn table_errors_carry_stable_codes() {
        use yamaa_adapters::public_table::TableError as Public;
        for (error, code) in [
            (
                Public::Limit {
                    resource: "table_rows",
                    limit: 1,
                    required: None,
                },
                "limit",
            ),
            (
                Public::Representation {
                    representation: "text",
                },
                "representation",
            ),
            (Public::InvalidTable, "invalid_table"),
            (Public::Internal, "internal"),
        ] {
            let converted = TableError::from_public(error);
            assert_eq!(converted.code(), code);
            assert!(!converted.message().is_empty());
        }
    }

    #[test]
    #[cfg(feature = "arrow")]
    fn table_import_errors_carry_stable_codes() {
        use yamaa_adapters::column_table::TableError as Table;
        for (error, code) in [
            (
                Table::Limit {
                    resource: "table_rows",
                    limit: 1,
                    required: None,
                },
                "limit",
            ),
            (Table::SchemaMismatch { batch: 0 }, "schema_mismatch"),
            (
                Table::ArrayType {
                    batch: 0,
                    column: 1,
                },
                "array_type",
            ),
            (
                Table::TemporalRange {
                    batch: 0,
                    column: 0,
                    row: 0,
                },
                "temporal",
            ),
            (
                Table::Representation {
                    representation: "text",
                },
                "representation",
            ),
        ] {
            let converted = TableError::from_table(error);
            assert_eq!(converted.code(), code);
            assert!(!converted.message().is_empty());
        }
    }

    /// D39's error text of one failure of each kind (#3765): the action,
    /// then the first issue's line and the count of the others for a refusal
    /// with issues, or else the kind's own detail. Only the refusal with
    /// issues carries them, and no failure but a save's carries paths.
    #[test]
    fn a_failure_of_each_kind_names_its_action_then_what_refused_it() {
        use yamaa_core::application_issue::{Fault, Step};
        // Two engine faults; the first is the one the message prints, its
        // text the registry's engine fault message for the resolve step.
        let fault = yamaa_core::diagnostic::ConditionCode::ApplicationRejected.definition();
        let issues = [Step::Resolve, Step::Execute]
            .into_iter()
            .flat_map(|step| {
                yamaa_adapters::file_application::rejected(step, Fault::ArtifactRepresentation)
            })
            .collect::<Vec<_>>();
        let read = |subject| Action::Read(subject);
        let cases = [
            (
                "rejected",
                Failure::rejected(Action::Save("ADAE"), &Report::from_issues(&issues)),
                format!(
                    "cannot save ADAE: the engine refused the operation in its resolve step: \
                     artifact_representation [{}] (and 1 more issue)",
                    fault.condition
                ),
                2,
            ),
            (
                "cancelled",
                Failure::of(read("adsl.yaml"), FailureKind::Cancelled),
                "cannot read adsl.yaml: operation cancelled".to_owned(),
                0,
            ),
            (
                "table",
                Failure::of(
                    read("edc/odm.xml"),
                    FailureKind::Table(Box::new(TableError::from_public(
                        yamaa_adapters::public_table::TableError::InvalidTable,
                    ))),
                ),
                "cannot read edc/odm.xml: table failure (invalid_table): invalid table schema \
                 or value"
                    .to_owned(),
                0,
            ),
            (
                "unavailable",
                Failure::of(
                    read("edc/odm.xml"),
                    FailureKind::Unavailable { feature: "odm" },
                ),
                "cannot read edc/odm.xml: format feature not compiled in: odm".to_owned(),
                0,
            ),
            (
                "internal",
                Failure::of(
                    read("TEST"),
                    FailureKind::Internal {
                        code: "stream_panic",
                    },
                ),
                "cannot read TEST: internal failure: stream_panic".to_owned(),
                0,
            ),
            (
                "stream",
                Failure::of(
                    read("TEST"),
                    FailureKind::Stream {
                        code: "stream_taken",
                    },
                ),
                "cannot read TEST: the run's output stream has already been read [stream_taken]"
                    .to_owned(),
                0,
            ),
        ];
        for (kind, failure, message, issues) in cases {
            // Exhaustive, so a new kind fails to compile here until it is
            // named; give it a case in the table above too.
            let named = match failure.kind {
                FailureKind::Rejected => "rejected",
                FailureKind::Cancelled => "cancelled",
                FailureKind::Table(_) => "table",
                FailureKind::Unavailable { .. } => "unavailable",
                FailureKind::Internal { .. } => "internal",
                FailureKind::Stream { .. } => "stream",
            };
            assert_eq!(named, kind);
            assert_eq!(
                (
                    failure.message.as_str(),
                    failure.to_string(),
                    failure.diagnostics.len(),
                    failure.issues.rows(),
                    failure.paths.len(),
                ),
                (message.as_str(), message.clone(), issues, issues, 0),
                "{kind}"
            );
        }
    }
}
