//! Project classified installed-version facts against the held environment lock.
//! This has no metadata, import, version-comparison or resource capability.
use crate::{
    issue_rows::Issue,
    project_lock::{Finding, Reason},
    project_source::CapturedEnvironment,
};
use yamaa_core::{diagnostic::ContextValue, value::Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidContext,
    Limit,
    Projection,
}
struct Budget(usize);
impl Budget {
    fn text(&mut self, text: &str) -> Result<(), Error> {
        // Context is JSON text within another JSON envelope. Admit both escaping
        // layers and field overhead before copying any retained origin or fact.
        let charge = text
            .len()
            .checked_mul(12)
            .and_then(|size| size.checked_add(128))
            .ok_or(Error::Limit)?;
        self.0 = self.0.checked_sub(charge).ok_or(Error::Limit)?;
        Ok(())
    }
}
fn shape(finding: &Finding) -> bool {
    match finding.reason {
        Reason::DistributionNotIdentified | Reason::VersionNotLocked => {
            finding.expected.is_empty() && finding.actual.is_none()
        }
        Reason::AmbiguousLockVersion => finding.expected.len() > 1 && finding.actual.is_none(),
        Reason::InvalidLockedVersion => finding.expected.len() <= 1 && finding.actual.is_none(),
        Reason::PackageNotInstalled => finding.expected.len() == 1 && finding.actual.is_none(),
        Reason::InvalidInstalledVersion => finding.expected.len() == 1,
        Reason::VersionMismatch => finding.expected.len() == 1 && finding.actual.is_some(),
    }
}
pub fn issues(
    environment: &CapturedEnvironment,
    findings: &[Finding],
) -> Result<Vec<Issue>, Error> {
    issues_with_limit(
        environment,
        findings,
        crate::specification_check::MAX_ISSUE_BYTES,
    )
}
pub fn issues_with_limit(
    environment: &CapturedEnvironment,
    findings: &[Finding],
    maximum: usize,
) -> Result<Vec<Issue>, Error> {
    if findings.is_empty() {
        return Ok(Vec::new());
    }
    // Python can retain one unidentified provider per selected root as well as
    // yamaa and every installed provider. R needs at most 1,025 package records.
    if findings.len() > 2_049 {
        return Err(Error::Limit);
    }
    let lock = environment.lock().ok_or(Error::InvalidContext)?;
    let document = &environment.root().raw().document;
    // Use the authored spelling from the held root, never an executable alias.
    let written = document
        .field(document.root(), "lock")
        .and_then(|node| document.nodes().get(node))
        .and_then(|node| match node {
            yamaa_core::schema::DocumentNode::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .ok_or(Error::InvalidContext)?;
    let source = environment.root().source().identity.as_str();
    let captured = lock.source.identity.as_str();
    let mut budget = Budget(maximum);
    let mut versions = 0usize;
    for finding in findings {
        if finding.package.is_empty() || finding.package.contains('\0') || !shape(finding) {
            return Err(Error::InvalidContext);
        }
        if finding.package.len() > 2_048 {
            return Err(Error::Limit);
        }
        versions = versions
            .checked_add(finding.expected.len())
            .filter(|&count| count <= 65_536)
            .ok_or(Error::Limit)?;
        for text in [
            source,
            source,
            captured,
            written,
            "lock",
            &finding.package,
            finding.reason.as_str(),
        ]
        .into_iter()
        .chain(finding.expected.iter().map(String::as_str))
        .chain(finding.actual.as_deref())
        {
            if text.len() > 65_536 {
                return Err(Error::Limit);
            }
            budget.text(text)?;
        }
        if finding.expected.iter().any(|text| text.len() > 2_048)
            || finding
                .actual
                .as_ref()
                .is_some_and(|text| text.len() > 2_048)
        {
            return Err(Error::Limit);
        }
    }
    let scalar = |text: &str| ContextValue::Scalar(Value::Str(text.into()));
    let mut result = Vec::with_capacity(findings.len());
    for finding in findings {
        let context = [
            ("source".into(), scalar(source)),
            ("entry".into(), scalar(source)),
            ("lock_source".into(), scalar(captured)),
            ("lock".into(), scalar(written)),
            ("package".into(), scalar(&finding.package)),
            ("reason".into(), scalar(finding.reason.as_str())),
            (
                "expected".into(),
                ContextValue::Sequence(finding.expected.iter().map(|text| scalar(text)).collect()),
            ),
            (
                "actual".into(),
                finding
                    .actual
                    .as_deref()
                    .map_or(ContextValue::Scalar(Value::Missing), scalar),
            ),
        ]
        .into();
        result.push(
            Issue::from_core(yamaa_core::application_issue::lock_mismatch(context))
                .map_err(|_| Error::Projection)?,
        );
    }
    if !crate::issue_rows::within_limit(&result, maximum) {
        return Err(Error::Limit);
    }
    Ok(result)
}
