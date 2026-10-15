//! Project only retained static admission, restoring each captured source origin.
//! Capture failures and opaque host payloads remain in the rejected environment.
use crate::{
    issue_rows::Issue,
    project_source::{CaptureFailure, Origin, RejectedEnvironment},
    specification_source::{CapturedDocument, CapturedFindings, Error as SourceError},
};
use yamaa_core::{
    diagnostic::Diagnostic,
    project_environment_diagnostics::{self as core, projection},
    project_limits::{AdmissionError, Limit},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    AdmissionLimit(Limit),
    Core(core::Error),
    InvalidOrigin,
    Projection,
    Limit,
}

fn root(document: &CapturedDocument) -> projection::Root<'_> {
    projection::Root {
        document: &document.normalized().document,
        identity: &document.source().identity,
    }
}
fn projection_error(error: projection::Error) -> Error {
    match error {
        projection::Error::Core(error) => Error::Core(error),
        projection::Error::InvalidOrigin => Error::InvalidOrigin,
        projection::Error::IssueLimit => Error::Limit,
    }
}
fn issues(diagnostics: Vec<Diagnostic>, maximum: usize) -> Result<Vec<Issue>, Error> {
    let issues = diagnostics
        .into_iter()
        .map(|diagnostic| Issue::from_core(diagnostic).map_err(|_| Error::Projection))
        .collect::<Result<Vec<_>, _>>()?;
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Limit);
    }
    Ok(issues)
}
/// Resolve failed inline scalars from the exact held root, preserving full paths.
pub fn scalar_root(
    root_document: &CapturedDocument,
    findings: &[yamaa_core::project_function_document::Finding],
) -> Result<Vec<Issue>, Error> {
    root_scalars(root_document, findings, None)
}
/// Complete independent root obligations before projecting failed inline scalars.
/// Host selection is explicit; authored lock syntax still requires capture.
pub fn scalar_root_with_host(
    root_document: &CapturedDocument,
    findings: &[yamaa_core::project_function_document::Finding],
    host: yamaa_core::project_function::Language,
) -> Result<Vec<Issue>, Error> {
    root_scalars(root_document, findings, Some(host))
}
fn root_scalars(
    root_document: &CapturedDocument,
    findings: &[yamaa_core::project_function_document::Finding],
    host: Option<yamaa_core::project_function::Language>,
) -> Result<Vec<Issue>, Error> {
    let maximum = crate::specification_check::MAX_ISSUE_BYTES;
    let report = projection::admit_root_scalars(
        root(root_document),
        findings,
        host,
        projection::Limits {
            issue_bytes: maximum,
            ..Default::default()
        },
    )
    .map_err(projection_error)?;
    issues(report.diagnostics().map_err(projection_error)?, maximum)
}
/// Independent failed external scalars retain their own captured arenas. Other
/// capture failures remain available to their typed boundary projections.
pub fn scalar_sources<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Issue>, Error> {
    let maximum = crate::specification_check::MAX_ISSUE_BYTES;
    if rejected.sources.len() > 128 {
        return Err(Error::Limit);
    }
    for source in &rejected.sources {
        let CaptureFailure::Scalar { document, .. } = &source.error else {
            continue;
        };
        if !std::sync::Arc::ptr_eq(document.schema(), rejected.root.schema())
            || matches!(source.origin, Origin::Lock)
        {
            return Err(Error::InvalidOrigin);
        }
    }
    let sources = rejected.sources.iter().filter_map(|source| {
        let CaptureFailure::Scalar { document, findings } = &source.error else {
            return None;
        };
        let origin = match &source.origin {
            Origin::Function(name) => projection::Origin::Function(name),
            Origin::Codelist(index) => projection::Origin::Codelist(*index),
            Origin::Lock => return None,
        };
        Some(projection::ScalarSource {
            capture: projection::Capture {
                origin,
                document: &document.normalized().document,
                identity: &document.source().identity,
            },
            findings,
        })
    });
    let report = projection::admit_scalar_sources(
        root(&rejected.root),
        sources,
        projection::Limits {
            issue_bytes: maximum,
            ..Default::default()
        },
    )
    .map_err(projection_error)?;
    issues(report.diagnostics().map_err(projection_error)?, maximum)
}

fn schema_issues(
    captured: &CapturedFindings,
    entry: &str,
    declaration: &str,
    budget: &mut crate::specification_diagnostics::CapturedBudget,
    issues: &mut Vec<Issue>,
) -> Result<(), Error> {
    let rows = crate::specification_diagnostics::captured_findings(
        captured,
        &[
            ("source", &captured.source().identity),
            ("entry", entry),
            ("environment_path", declaration),
        ],
        budget,
    )
    .map_err(|error| match error {
        crate::specification_diagnostics::CapturedProjectionError::Limit => Error::Limit,
        crate::specification_diagnostics::CapturedProjectionError::InvalidContext => {
            Error::Projection
        }
    })?;
    for row in rows {
        issues.push(Issue::from_diagnostic(row).map_err(|_| Error::Projection)?);
    }
    Ok(())
}
/// Resolve root schema findings against their actual retained validation input.
pub fn schema_root(captured: &CapturedFindings) -> Result<Vec<Issue>, Error> {
    let mut budget = crate::specification_diagnostics::CapturedBudget::new();
    let mut issues = Vec::new();
    schema_issues(
        captured,
        &captured.source().identity,
        "$",
        &mut budget,
        &mut issues,
    )?;
    if !crate::issue_rows::within_limit(&issues, crate::specification_check::MAX_ISSUE_BYTES) {
        return Err(Error::Limit);
    }
    Ok(issues)
}
/// Project independent failed source schemas with one cumulative resolver budget.
/// Opaque errors, decoding failures and limits retain their original typed forms.
pub fn schema_sources<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Issue>, Error> {
    let mut budget = crate::specification_diagnostics::CapturedBudget::new();
    let mut metadata = Budget(crate::specification_check::MAX_ISSUE_BYTES);
    let mut issues = Vec::new();
    if rejected.sources.len() > 128 {
        return Err(Error::Limit);
    }
    for source in &rejected.sources {
        let CaptureFailure::Structural(SourceError::Findings(captured)) = &source.error else {
            continue;
        };
        if !std::ptr::eq(captured.schema(), rejected.root.schema().as_ref()) {
            return Err(Error::InvalidOrigin);
        }
        metadata.charge(128)?;
        let declaration = match &source.origin {
            Origin::Function(name) => {
                metadata.text(name, 12)?;
                format!("functions.{name}")
            }
            Origin::Codelist(index) => format!("codelists[{index}]"),
            Origin::Lock => return Err(Error::InvalidOrigin),
        };
        schema_issues(
            captured,
            &rejected.root.source().identity,
            &declaration,
            &mut budget,
            &mut issues,
        )?;
    }
    if !crate::issue_rows::within_limit(&issues, crate::specification_check::MAX_ISSUE_BYTES) {
        return Err(Error::Limit);
    }
    Ok(issues)
}
struct Budget(usize);
impl Budget {
    fn charge(&mut self, bytes: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(bytes).ok_or(Error::Limit)?;
        Ok(())
    }
    fn text(&mut self, text: &str, expansion: usize) -> Result<(), Error> {
        self.charge(text.len().checked_mul(expansion).ok_or(Error::Limit)?)
    }
}
/// Return admission issues in their original order. This borrows the rejected
/// draft and held capture records; it never reads, decodes or activates anything.
/// Resource-limit failures have no fabricated semantic condition.
pub fn admission<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Issue>, Error> {
    admission_with_limit(rejected, crate::specification_check::MAX_ISSUE_BYTES)
}
pub fn admission_with_limit<E>(
    rejected: &RejectedEnvironment<E>,
    maximum: usize,
) -> Result<Vec<Issue>, Error> {
    let findings = match &rejected.admission {
        None => return Ok(vec![]),
        Some(AdmissionError::Limit(limit)) => return Err(Error::AdmissionLimit(*limit)),
        Some(AdmissionError::Findings(findings)) => findings,
    };
    if rejected.captures.len() > 128 {
        return Err(Error::Limit);
    }
    for capture in &rejected.captures {
        if matches!(capture.origin, Origin::Lock)
            || !std::sync::Arc::ptr_eq(capture.document.schema(), rejected.root.schema())
        {
            return Err(Error::InvalidOrigin);
        }
    }
    let captures = rejected.captures.iter().filter_map(|capture| {
        let origin = match &capture.origin {
            Origin::Function(name) => projection::Origin::Function(name),
            Origin::Codelist(index) => projection::Origin::Codelist(*index),
            Origin::Lock => return None,
        };
        Some(projection::Capture {
            origin,
            document: &capture.document.normalized().document,
            identity: &capture.document.source().identity,
        })
    });
    let report = projection::admit_admission(
        projection::Admission {
            root: root(&rejected.root),
            draft: &rejected.draft,
            function_origins: &rejected.origins.functions,
            codelist_origins: &rejected.origins.codelists,
            captures,
        },
        findings,
        projection::Limits {
            issue_bytes: maximum,
            ..Default::default()
        },
    )
    .map_err(projection_error)?;
    issues(report.diagnostics().map_err(projection_error)?, maximum)
}
