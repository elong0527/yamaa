//! Project only retained static admission, restoring each captured source origin.
//! Capture failures and opaque host payloads remain in the rejected environment.
use crate::{
    issue_rows::Issue,
    project_source::{CaptureFailure, Origin, RejectedEnvironment},
    specification_source::{CapturedDocument, CapturedFindings, Error as SourceError},
};
use std::collections::BTreeMap;
use yamaa_core::{
    diagnostic::{ContextValue, Diagnostic},
    project_environment::Finding,
    project_environment_diagnostics as core,
    project_limits::{AdmissionError, Limit},
    value::Value,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    AdmissionLimit(Limit),
    Core(core::Error),
    InvalidOrigin,
    Projection,
    Limit,
}

fn scalar_issues(
    document: &CapturedDocument,
    findings: &[yamaa_core::project_function_document::Finding],
    entry: &str,
    declaration: &str,
    budget: &mut Budget,
    issues: &mut Vec<Issue>,
) -> Result<(), Error> {
    let diagnostics =
        core::scalar_diagnostics(&document.normalized().document, findings).map_err(Error::Core)?;
    core_issues(diagnostics, document, entry, declaration, budget, issues)
}
fn core_issues(
    diagnostics: Vec<Diagnostic>,
    document: &CapturedDocument,
    entry: &str,
    declaration: &str,
    budget: &mut Budget,
    issues: &mut Vec<Issue>,
) -> Result<(), Error> {
    let source = document.source().identity.as_str();
    for mut diagnostic in diagnostics {
        budget.diagnostic(&diagnostic)?;
        for text in [source, entry, declaration] {
            budget.text(text, 12)?;
        }
        for (key, text) in [
            ("source", source),
            ("entry", entry),
            ("environment_path", declaration),
        ] {
            if diagnostic
                .context
                .insert(key.into(), ContextValue::Scalar(Value::Str(text.into())))
                .is_some()
            {
                return Err(Error::Projection);
            }
        }
        issues.push(Issue::from_core(diagnostic).map_err(|_| Error::Projection)?);
    }
    Ok(())
}
/// Resolve failed inline scalars from the held root, preserving complete paths.
pub fn scalar_root(
    root: &CapturedDocument,
    findings: &[yamaa_core::project_function_document::Finding],
) -> Result<Vec<Issue>, Error> {
    let maximum = crate::specification_check::MAX_ISSUE_BYTES;
    let mut budget = Budget(maximum);
    let mut issues = Vec::new();
    scalar_issues(
        root,
        findings,
        &root.source().identity,
        "$",
        &mut budget,
        &mut issues,
    )?;
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Limit);
    }
    Ok(issues)
}
/// Complete independent root obligations before projecting failed inline scalars.
/// Host selection is explicit; authored lock syntax still requires capture.
pub fn scalar_root_with_host(
    root: &CapturedDocument,
    findings: &[yamaa_core::project_function_document::Finding],
    host: yamaa_core::project_function::Language,
) -> Result<Vec<Issue>, Error> {
    let maximum = crate::specification_check::MAX_ISSUE_BYTES;
    let mut budget = Budget(maximum);
    let mut issues = Vec::new();
    let header = core::root_diagnostics(&root.normalized().document, host).map_err(Error::Core)?;
    core_issues(
        header,
        root,
        &root.source().identity,
        "$",
        &mut budget,
        &mut issues,
    )?;
    scalar_issues(
        root,
        findings,
        &root.source().identity,
        "$",
        &mut budget,
        &mut issues,
    )?;
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Limit);
    }
    Ok(issues)
}
/// Independent failed external scalars retain their own captured arenas. Other
/// capture failures remain available to their typed boundary projections.
pub fn scalar_sources<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Issue>, Error> {
    let maximum = crate::specification_check::MAX_ISSUE_BYTES;
    let mut budget = Budget(maximum);
    let mut issues = Vec::new();
    if rejected.sources.len() > 128 {
        return Err(Error::Limit);
    }
    for source in &rejected.sources {
        let CaptureFailure::Scalar { document, findings } = &source.error else {
            continue;
        };
        if !std::sync::Arc::ptr_eq(document.schema(), rejected.root.schema()) {
            return Err(Error::InvalidOrigin);
        }
        budget.charge(128)?;
        let declaration = match &source.origin {
            Origin::Function(name) => {
                budget.text(name, 12)?;
                format!("functions.{name}")
            }
            Origin::Codelist(index) => format!("codelists[{index}]"),
            Origin::Lock => return Err(Error::InvalidOrigin),
        };
        scalar_issues(
            document,
            findings,
            &rejected.root.source().identity,
            &declaration,
            &mut budget,
            &mut issues,
        )?;
    }
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Limit);
    }
    Ok(issues)
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
    fn scalar(&mut self, value: &ContextValue) -> Result<(), Error> {
        match value {
            ContextValue::Scalar(Value::Str(text)) => self.text(text, 12),
            ContextValue::Integer(text) => self.text(text, 2),
            ContextValue::Scalar(_) => self.charge(64),
            // Core project admission contributes only scalars. A new family must
            // explicitly qualify its structured context before using this route.
            ContextValue::Sequence(_) => Err(Error::Projection),
        }
    }
    fn diagnostic(&mut self, diagnostic: &Diagnostic) -> Result<(), Error> {
        self.charge(512)?;
        for path in &diagnostic.spec_paths {
            self.text(path, 6)?;
        }
        for (key, value) in &diagnostic.context {
            self.text(key, 12)?;
            self.scalar(value)?;
        }
        Ok(())
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
    let functions = rejected.draft.functions.as_deref().unwrap_or(&[]);
    if rejected.origins.functions.len() != functions.len()
        || rejected.origins.codelists.len() != rejected.draft.codelists.len()
        || rejected
            .origins
            .functions
            .iter()
            .zip(functions)
            .any(|(name, function)| name != &function.name)
    {
        return Err(Error::InvalidOrigin);
    }
    if rejected.captures.len() > 128 {
        return Err(Error::Limit);
    }
    let mut function_sources = BTreeMap::new();
    let mut codelist_sources = BTreeMap::new();
    for capture in &rejected.captures {
        let duplicate = match &capture.origin {
            Origin::Function(name) => function_sources
                .insert(name.as_str(), capture.document.source().identity.as_str())
                .is_some(),
            Origin::Codelist(index) => codelist_sources
                .insert(*index, capture.document.source().identity.as_str())
                .is_some(),
            Origin::Lock => return Err(Error::InvalidOrigin),
        };
        if duplicate {
            return Err(Error::InvalidOrigin);
        }
    }
    let diagnostics = core::diagnostics(&rejected.draft, findings).map_err(Error::Core)?;
    let entry = rejected.root.source().identity.as_str();
    let mut budget = Budget(maximum);
    let mut issues = Vec::new();
    for (finding, mut diagnostic) in findings.iter().zip(diagnostics) {
        budget.diagnostic(&diagnostic)?;
        let (source, declaration, path) = match finding {
            Finding::Function { function, finding } => {
                let name = rejected
                    .origins
                    .functions
                    .get(*function)
                    .ok_or(Error::InvalidOrigin)?;
                budget.text(name, 18)?;
                budget.charge(128)?;
                let declaration = format!("functions.{name}");
                let external =
                    if matches!(finding, yamaa_core::project_function::Finding::InvalidName) {
                        None
                    } else {
                        function_sources.get(name.as_str()).copied()
                    };
                let source = external.unwrap_or(entry);
                let original = diagnostic.spec_paths.first().ok_or(Error::Projection)?;
                let path = if external.is_none() {
                    original.as_str()
                } else {
                    original
                        .strip_prefix(&declaration)
                        .and_then(|p| p.strip_prefix('.'))
                        .ok_or(Error::Projection)?
                };
                budget.text(path, 6)?;
                (source, declaration, path.to_owned())
            }
            Finding::Codelist(finding) => {
                let index = *rejected
                    .origins
                    .codelists
                    .get(finding.source)
                    .ok_or(Error::InvalidOrigin)?;
                budget.charge(128)?;
                let original = diagnostic.spec_paths.first().ok_or(Error::Projection)?;
                let typed = format!("codelists[{}].", finding.source);
                let suffix = original.strip_prefix(&typed).ok_or(Error::Projection)?;
                let declaration = format!("codelists[{index}]");
                let external = codelist_sources.get(&index).copied();
                budget.text(suffix, 6)?;
                let path = if external.is_some() {
                    suffix.to_owned()
                } else {
                    format!("{declaration}.{suffix}")
                };
                (external.unwrap_or(entry), declaration, path)
            }
            _ => {
                let path = diagnostic.spec_paths.first().ok_or(Error::Projection)?;
                budget.text(path, 18)?;
                (entry, path.clone(), path.clone())
            }
        };
        // Charge source metadata before any copy. Twelve bounds both JSON string
        // escaping and the second encoding of issue-row context JSON text.
        for text in [source, entry, declaration.as_str()] {
            budget.text(text, 12)?;
        }
        diagnostic.spec_paths = vec![path];
        for (key, text) in [
            ("source", source),
            ("entry", entry),
            ("environment_path", declaration.as_str()),
        ] {
            if diagnostic
                .context
                .insert(key.into(), ContextValue::Scalar(Value::Str(text.into())))
                .is_some()
            {
                return Err(Error::Projection);
            }
        }
        issues.push(Issue::from_core(diagnostic).map_err(|_| Error::Projection)?);
    }
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Limit);
    }
    Ok(issues)
}
