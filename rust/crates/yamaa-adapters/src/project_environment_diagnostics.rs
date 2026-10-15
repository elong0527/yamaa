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

fn metadata_path(origin: &Origin, budget: &mut Budget) -> Result<String, Error> {
    budget.charge(128)?;
    Ok(match origin {
        Origin::Lock => "lock".into(),
        Origin::Function(name) => {
            budget.text(name, 12)?;
            format!("functions.{name}")
        }
        Origin::Codelist(index) => format!("codelists[{index}]"),
    })
}

fn read_issue(
    error: &crate::file_resources::Error,
    entry: &str,
    written: &str,
    declaration: &str,
) -> Result<Issue, Error> {
    use crate::file_resources::Error as E;
    let cause = match error {
        E::Missing => "missing",
        E::Unreadable => "unreadable",
        E::NotRegularFile => "not_regular_file",
        E::InvalidRoot => "invalid_root",
        E::InvalidBase => "invalid_base",
        E::InvalidPath | E::WrittenForm(_) => "invalid_path",
        E::OutsideRoots => "outside_roots",
        E::Symlink => "symbolic_link",
        E::Changed => "content_changed",
        E::InvalidLock => "invalid_packaging_lock",
        E::Limit => "resource_limit",
    };
    let mut diagnostic = if matches!(error, E::Limit) {
        yamaa_core::application_issue::rejected("environment", cause)
    } else {
        Diagnostic {
            code: yamaa_core::diagnostic::ConditionCode::ProjectEnvironmentInvalid,
            spec_paths: vec![declaration.into()],
            context: Default::default(),
            source_span: None,
            operand_route: None,
        }
    };
    for (key, value) in [
        ("entry", entry),
        ("path", written),
        ("environment_path", declaration),
        ("cause", cause),
    ] {
        diagnostic.context.insert(key.into(), value.into());
    }
    Issue::from_core(diagnostic).map_err(|_| Error::Projection)
}

fn decode_issues(
    error: &SourceError,
    entry: &str,
    written: &str,
    declaration: &str,
    budget: &mut Budget,
) -> Result<Vec<Issue>, Error> {
    if let SourceError::Decode { error, .. } = error {
        use crate::yaml_decode::DecodeFailure as D;
        match error {
            D::InvalidYaml { reason, .. } => budget.text(reason, 12)?,
            D::InvalidText(findings) => {
                for finding in findings {
                    budget.charge(256)?;
                    budget.text(&finding.path, 12)?;
                }
            }
            D::NonAscii(_) => (),
            D::Limit(_) | D::Internal => return Err(Error::Limit),
        }
    }
    let message = crate::specification_diagnostics::capture_failure(error);
    let mut rows = crate::file_application::preparation_failure(message);
    for row in &mut rows {
        let mut context: serde_json::Value =
            serde_json::from_str(&row.context).map_err(|_| Error::Projection)?;
        for (key, value) in [
            ("entry", entry),
            ("path", written),
            ("environment_path", declaration),
        ] {
            context[key] = value.into();
        }
        row.context = context.to_string();
    }
    Ok(rows)
}

/// Project a root decoder failure with the caller's explicit environment path.
pub(crate) fn root_boundary(error: &SourceError, written: &str) -> Result<Vec<Issue>, Error> {
    let maximum = crate::specification_check::MAX_ISSUE_BYTES;
    let mut budget = Budget(maximum);
    budget.text(written, 36)?;
    let rows = decode_issues(error, written, written, "$", &mut budget)?;
    if !crate::issue_rows::within_limit(&rows, maximum) {
        return Err(Error::Limit);
    }
    Ok(rows)
}

/// Native metadata failures retain the authored dependency path and declaration.
/// Schema/scalar findings are projected separately from their retained arenas.
pub(crate) fn capture_sources(
    rejected: &RejectedEnvironment<crate::file_resources::Error>,
) -> Result<Vec<Issue>, Error> {
    let maximum = crate::specification_check::MAX_ISSUE_BYTES;
    let mut budget = Budget(maximum);
    if rejected.sources.len() > 128 {
        return Err(Error::Limit);
    }
    let mut rows = Vec::new();
    for source in &rejected.sources {
        if matches!(
            source.error,
            CaptureFailure::Scalar { .. } | CaptureFailure::Structural(SourceError::Findings(_))
        ) {
            continue;
        }
        let entry = &rejected.root.source().identity;
        budget.charge(512)?;
        budget.text(entry, 12)?;
        budget.text(&source.written, 12)?;
        let declaration = metadata_path(&source.origin, &mut budget)?;
        match &source.error {
            CaptureFailure::Port(error) | CaptureFailure::Interrupted(error) => {
                rows.push(read_issue(error, entry, &source.written, &declaration)?);
            }
            CaptureFailure::Structural(error) => rows.extend(decode_issues(
                error,
                entry,
                &source.written,
                &declaration,
                &mut budget,
            )?),
            CaptureFailure::Limit(_) => return Err(Error::Limit),
            CaptureFailure::Protocol => return Err(Error::Projection),
            CaptureFailure::Scalar { .. } => unreachable!("handled scalar finding"),
        }
    }
    if !crate::issue_rows::within_limit(&rows, maximum) {
        return Err(Error::Limit);
    }
    Ok(rows)
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
