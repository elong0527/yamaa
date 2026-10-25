//! Project retained codec facts without repeating capture or decoding effects.
use crate::{csv_source::TextTableError, source_decoding::Error};
use yamaa_core::diagnostic::{Diagnostic, SourceFinding};
use yamaa_core::specification::SourceDeclaration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceProjectionError {
    InvalidContext,
}

fn walk<'a>(
    error: &'a Error,
    source: Option<&'a SourceDeclaration>,
    mut visit: impl FnMut(
        &'a Error,
        Option<&'a SourceDeclaration>,
    ) -> Result<std::ops::ControlFlow<()>, SourceProjectionError>,
) -> Result<(), SourceProjectionError> {
    let mut frames: Vec<std::slice::Iter<'a, (SourceDeclaration, Error)>> = Vec::new();
    let mut current = Some((error, source));
    loop {
        if let Some((error, source)) = current.take() {
            if let Error::Sources(errors) = error {
                if let Some((first, rest)) = errors.split_first() {
                    frames.push(rest.iter());
                    current = Some((&first.1, Some(&first.0)));
                    continue;
                }
            } else if visit(error, source)?.is_break() {
                return Ok(());
            }
        }
        while let Some(frame) = frames.last_mut() {
            if let Some((source, error)) = frame.next() {
                current = Some((error, Some(source)));
                break;
            }
            frames.pop();
        }
        if current.is_none() {
            return Ok(());
        }
    }
}
fn semantic(error: &Error) -> bool {
    match error {
        Error::Source(TextTableError::Csv(error)) => error.finding("", "").is_some(),
        Error::TypedSource(crate::typed_csv::Error::Typing(error)) => error.finding("").is_some(),
        Error::ParquetSource(error) => error.finding("", "").is_some(),
        Error::SasSource(error) => error.finding("", "").is_some(),
        Error::DatasetJsonSource(error) => error.finding("", "").is_some(),
        Error::Ordinal(error) => error.finding("", "").is_some(),
        Error::OdmFields(_) | Error::OdmValues(_) | Error::ProducerMetadata(_) => true,
        Error::OdmSource(error) => error.refuses_document(),
        _ => false,
    }
}
fn finding<'a>(error: &'a Error, source: &'a SourceDeclaration) -> Option<SourceFinding<'a>> {
    match error {
        Error::Source(TextTableError::Csv(error)) => error.finding(&source.name, &source.path),
        Error::TypedSource(crate::typed_csv::Error::Typing(error)) => error.finding(&source.name),
        Error::ParquetSource(error) => error.finding(&source.name, &source.path),
        Error::SasSource(error) => error.finding(&source.name, &source.path),
        Error::DatasetJsonSource(error) => error.finding(&source.name, &source.path),
        Error::Ordinal(error) => error.finding(&source.name, source.ordinal.as_deref()?),
        _ => None,
    }
}

fn append(
    error: &Error,
    source: Option<&SourceDeclaration>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), SourceProjectionError> {
    if let Error::ProducerMetadata(finding) = error {
        diagnostics.push(finding.clone());
        return Ok(());
    }
    let source = source.ok_or(SourceProjectionError::InvalidContext)?;
    match error {
        Error::OdmFields(fields) => diagnostics.extend(fields.iter().map(|finding| {
            SourceFinding::Odm {
                dataset: &source.name,
                finding,
            }
            .diagnostic()
        })),
        Error::OdmValues(values) => diagnostics.extend(values.iter().map(|&finding| {
            SourceFinding::OdmValues {
                dataset: &source.name,
                finding,
            }
            .diagnostic()
        })),
        // Reported at the input's path, naming the path as declared and the
        // dataset (ODM-1). Only a refusal of the document gets here.
        Error::OdmSource(crate::odm_xml::Error::Read(refusal)) => diagnostics.push(
            yamaa_core::diagnostic::OdmReadFinding {
                refusal,
                path: &source.path,
                dataset: Some(&source.name),
            }
            .diagnostic(),
        ),
        _ => diagnostics.push(
            finding(error, source)
                .ok_or(SourceProjectionError::InvalidContext)?
                .diagnostic(),
        ),
    }
    Ok(())
}

/// Preserve authored input order and the original opaque/resource barrier.
/// Validate every source index before allocating a diagnostic context.
pub fn admitted_findings(
    errors: &[(usize, yamaa_core::specification_run::SourceIssue<Error>)],
    sources: &[SourceDeclaration],
) -> Result<Option<Vec<Diagnostic>>, SourceProjectionError> {
    use yamaa_core::specification_run::SourceIssue;
    let mut missing_source = false;
    for (index, issue) in errors {
        let source = sources.get(*index);
        missing_source |= source.is_none();
        if let SourceIssue::Decode(error) = issue {
            let mut semantic_only = true;
            walk(error, source, |error, source| {
                if !semantic(error) {
                    semantic_only = false;
                    return Ok(std::ops::ControlFlow::Break(()));
                }
                missing_source |= source.is_none() && !matches!(error, Error::ProducerMetadata(_));
                Ok(std::ops::ControlFlow::Continue(()))
            })?;
            if !semantic_only {
                return Ok(None);
            }
        }
    }
    if missing_source {
        return Err(SourceProjectionError::InvalidContext);
    }
    let mut diagnostics = Vec::new();
    for (index, issue) in errors {
        let source = &sources[*index];
        match issue {
            SourceIssue::Decode(error) => walk(error, Some(source), |error, source| {
                append(error, source, &mut diagnostics)?;
                Ok(std::ops::ControlFlow::Continue(()))
            })?,
            SourceIssue::OdmValues(values) => diagnostics.extend(values.iter().map(|&finding| {
                SourceFinding::OdmValues {
                    dataset: &source.name,
                    finding,
                }
                .diagnostic()
            })),
        }
    }
    Ok(Some(diagnostics))
}

/// Inspect retained errors only. Missing declaration authority refuses without
/// fabricating identities; any reached opaque/resource barrier keeps precedence.
pub fn findings(
    error: &Error,
    source: Option<&SourceDeclaration>,
) -> Result<Option<Vec<Diagnostic>>, SourceProjectionError> {
    findings_in(&[(error, source)])
}

/// [`findings`] of several sources' failures read as one failure that lists
/// them, each with its own declaration.
pub fn findings_of(
    failures: &[(&SourceDeclaration, &Error)],
) -> Result<Option<Vec<Diagnostic>>, SourceProjectionError> {
    let roots = failures
        .iter()
        .map(|&(source, error)| (error, Some(source)))
        .collect::<Vec<_>>();
    findings_in(&roots)
}

fn findings_in(
    roots: &[(&Error, Option<&SourceDeclaration>)],
) -> Result<Option<Vec<Diagnostic>>, SourceProjectionError> {
    let mut semantic_only = true;
    let mut missing_source = false;
    for &(error, source) in roots {
        walk(error, source, |error, source| {
            if !semantic(error) {
                semantic_only = false;
                return Ok(std::ops::ControlFlow::Break(()));
            }
            missing_source |= source.is_none() && !matches!(error, Error::ProducerMetadata(_));
            Ok(std::ops::ControlFlow::Continue(()))
        })?;
        if !semantic_only {
            return Ok(None);
        }
    }
    if missing_source {
        return Err(SourceProjectionError::InvalidContext);
    }
    let mut diagnostics = Vec::new();
    for &(error, source) in roots {
        walk(error, source, |error, source| {
            append(error, source, &mut diagnostics)?;
            Ok(std::ops::ControlFlow::Continue(()))
        })?;
    }
    Ok(Some(diagnostics))
}
