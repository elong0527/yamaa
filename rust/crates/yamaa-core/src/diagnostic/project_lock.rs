//! Classified host packaging facts projected without metadata or version effects.
use super::Diagnostic;
use crate::application_issue;
use crate::json::Value as JsonValue;

/// These tokens are already classified by a host packaging tool. They are not
/// exception text, and core never decides which installed version is equivalent.
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
    /// Decode only the closed classified token, never an arbitrary host message.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "distribution_not_identified" => Self::DistributionNotIdentified,
            "version_not_locked" => Self::VersionNotLocked,
            "ambiguous_lock_version" => Self::AmbiguousLockVersion,
            "invalid_locked_version" => Self::InvalidLockedVersion,
            "package_not_installed" => Self::PackageNotInstalled,
            "invalid_installed_version" => Self::InvalidInstalledVersion,
            "version_mismatch" => Self::VersionMismatch,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DistributionNotIdentified => "distribution_not_identified",
            Self::VersionNotLocked => "version_not_locked",
            Self::AmbiguousLockVersion => "ambiguous_lock_version",
            Self::InvalidLockedVersion => "invalid_locked_version",
            Self::PackageNotInstalled => "package_not_installed",
            Self::InvalidInstalledVersion => "invalid_installed_version",
            Self::VersionMismatch => "version_mismatch",
        }
    }
}
/// Full ordered facts remain on the native owner; projection only borrows them.
#[derive(Debug, PartialEq, Eq)]
pub struct LockFinding {
    pub package: String,
    pub reason: LockReason,
    pub expected: Vec<String>,
    pub actual: Option<String>,
}
#[derive(Clone, Copy, Debug)]
pub struct LockOrigins<'a> {
    /// The environment file that declares the lock.
    pub source: &'a str,
    /// The caller-supplied project file that links that environment file.
    pub entry: &'a str,
    pub lock_source: &'a str,
    /// Authored spelling from the retained raw root, not a resolved executable path.
    pub written: &'a str,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LockDiagnosticError {
    #[error("diagnostic project lock invalid context")]
    InvalidContext,
}
fn shape(finding: &LockFinding) -> bool {
    match finding.reason {
        LockReason::DistributionNotIdentified | LockReason::VersionNotLocked => {
            finding.expected.is_empty() && finding.actual.is_none()
        }
        LockReason::AmbiguousLockVersion => finding.expected.len() > 1 && finding.actual.is_none(),
        LockReason::InvalidLockedVersion => finding.expected.len() <= 1 && finding.actual.is_none(),
        LockReason::PackageNotInstalled => finding.expected.len() == 1 && finding.actual.is_none(),
        LockReason::InvalidInstalledVersion => finding.expected.len() == 1,
        LockReason::VersionMismatch => finding.expected.len() == 1 && finding.actual.is_some(),
    }
}
/// Validate every retained finding before materializing the ordered report.
pub fn lock_diagnostics(
    origins: Option<LockOrigins<'_>>,
    findings: &[LockFinding],
) -> Result<Vec<Diagnostic>, LockDiagnosticError> {
    use LockDiagnosticError::InvalidContext;
    if findings.is_empty() {
        return Ok(Vec::new());
    }
    let origins = origins.ok_or(InvalidContext)?;
    for finding in findings {
        if finding.package.is_empty() || finding.package.contains('\0') || !shape(finding) {
            return Err(InvalidContext);
        }
    }
    let scalar = |text: &str| JsonValue::String(text.into());
    Ok(findings
        .iter()
        .map(|finding| {
            application_issue::lock_mismatch(
                [
                    ("source".into(), scalar(origins.source)),
                    ("entry".into(), scalar(origins.entry)),
                    ("lock_source".into(), scalar(origins.lock_source)),
                    ("lock".into(), scalar(origins.written)),
                    ("package".into(), scalar(&finding.package)),
                    ("reason".into(), scalar(finding.reason.as_str())),
                    (
                        "expected".into(),
                        JsonValue::Array(
                            finding.expected.iter().map(|text| scalar(text)).collect(),
                        ),
                    ),
                    (
                        "actual".into(),
                        finding.actual.as_deref().map_or(JsonValue::Null, scalar),
                    ),
                ]
                .into(),
            )
        })
        .collect())
}
