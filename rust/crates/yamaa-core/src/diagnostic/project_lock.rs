//! Classified host packaging facts projected without metadata or version effects.
use super::{ContextValue, Diagnostic};
use crate::{application_issue, value::Value};
use alloc::{string::String, vec::Vec};

pub const MAX_LOCK_DIAGNOSTIC_BYTES: usize = 16_777_216;
const MAX_FINDINGS: usize = 2_049;
const MAX_VERSIONS: usize = 65_536;
const MAX_FIELD_BYTES: usize = 2_048;
const MAX_ORIGIN_BYTES: usize = 65_536;

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
    pub source: &'a str,
    pub lock_source: &'a str,
    /// Authored spelling from the retained raw root, not a resolved executable path.
    pub written: &'a str,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockDiagnosticError {
    InvalidContext,
    Limit,
}
struct Budget(usize);
impl Budget {
    fn text(&mut self, text: &str) -> Result<(), LockDiagnosticError> {
        // Two JSON escaping layers cost at most twelve bytes per source byte.
        // Per-fact overhead covers every fixed context key and diagnostic field.
        let charge = text
            .len()
            .checked_mul(12)
            .and_then(|n| n.checked_add(128))
            .ok_or(LockDiagnosticError::Limit)?;
        self.0 = self
            .0
            .checked_sub(charge)
            .ok_or(LockDiagnosticError::Limit)?;
        Ok(())
    }
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
pub fn lock_diagnostics(
    origins: Option<LockOrigins<'_>>,
    findings: &[LockFinding],
) -> Result<Vec<Diagnostic>, LockDiagnosticError> {
    lock_diagnostics_with_limit(origins, findings, MAX_LOCK_DIAGNOSTIC_BYTES)
}
/// Admit the entire ordered report before copying the first authored fact. A
/// late malformed finding or quota failure returns no partial diagnostic list.
pub fn lock_diagnostics_with_limit(
    origins: Option<LockOrigins<'_>>,
    findings: &[LockFinding],
    maximum: usize,
) -> Result<Vec<Diagnostic>, LockDiagnosticError> {
    use LockDiagnosticError::{InvalidContext, Limit};
    if findings.is_empty() {
        return Ok(Vec::new());
    }
    if findings.len() > MAX_FINDINGS {
        return Err(Limit);
    }
    let origins = origins.ok_or(InvalidContext)?;
    let mut budget = Budget(maximum);
    let mut versions = 0usize;
    for finding in findings {
        if finding.package.is_empty() || finding.package.contains('\0') || !shape(finding) {
            return Err(InvalidContext);
        }
        if finding.package.len() > MAX_FIELD_BYTES {
            return Err(Limit);
        }
        versions = versions
            .checked_add(finding.expected.len())
            .filter(|&count| count <= MAX_VERSIONS)
            .ok_or(Limit)?;
        for text in [
            origins.source,
            origins.source,
            origins.lock_source,
            origins.written,
            "lock",
            &finding.package,
            finding.reason.as_str(),
        ]
        .into_iter()
        .chain(finding.expected.iter().map(String::as_str))
        .chain(finding.actual.as_deref())
        {
            if text.len() > MAX_ORIGIN_BYTES {
                return Err(Limit);
            }
            budget.text(text)?;
        }
        if finding
            .expected
            .iter()
            .any(|text| text.len() > MAX_FIELD_BYTES)
            || finding
                .actual
                .as_ref()
                .is_some_and(|text| text.len() > MAX_FIELD_BYTES)
        {
            return Err(Limit);
        }
    }
    let scalar = |text: &str| ContextValue::Scalar(Value::Str(text.into()));
    Ok(findings
        .iter()
        .map(|finding| {
            application_issue::lock_mismatch(
                [
                    ("source".into(), scalar(origins.source)),
                    ("entry".into(), scalar(origins.source)),
                    ("lock_source".into(), scalar(origins.lock_source)),
                    ("lock".into(), scalar(origins.written)),
                    ("package".into(), scalar(&finding.package)),
                    ("reason".into(), scalar(finding.reason.as_str())),
                    (
                        "expected".into(),
                        ContextValue::Sequence(
                            finding.expected.iter().map(|text| scalar(text)).collect(),
                        ),
                    ),
                    (
                        "actual".into(),
                        finding
                            .actual
                            .as_deref()
                            .map_or(ContextValue::Scalar(Value::Missing), scalar),
                    ),
                ]
                .into(),
            )
        })
        .collect())
}
