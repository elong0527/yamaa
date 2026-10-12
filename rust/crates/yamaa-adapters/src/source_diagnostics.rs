//! Extract retained codec facts; core admits their complete ordered projection.
use crate::{csv_source::TextTableError, specification_run::Error};
use yamaa_core::diagnostic::{
    admit_source_diagnostics, Diagnostic, ProjectionError, SourceFinding, MAX_PROJECTION_BYTES,
};
use yamaa_engine::specification::SourceDeclaration;

/// Structural admission is separate from the caller's semantic-context quota.
/// Traversal holds at most this many active borrowed child iterators, never a
/// breadth-sized pending list. The fixed frame array needs no heap allocation.
pub const MAX_SOURCE_REPORT_FRAMES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceProjectionError {
    TraversalLimit,
    Context(ProjectionError),
}
impl From<ProjectionError> for SourceProjectionError {
    fn from(error: ProjectionError) -> Self {
        Self::Context(error)
    }
}

fn walk<'a>(
    error: &'a Error,
    source: Option<&'a SourceDeclaration>,
    mut visit: impl FnMut(
        &'a Error,
        Option<&'a SourceDeclaration>,
    ) -> Result<std::ops::ControlFlow<()>, SourceProjectionError>,
) -> Result<(), SourceProjectionError> {
    let mut frames: [Option<std::slice::Iter<'a, (SourceDeclaration, Error)>>;
        MAX_SOURCE_REPORT_FRAMES] = std::array::from_fn(|_| None);
    let mut depth = 0;
    let mut current = Some((error, source));
    loop {
        if let Some((error, source)) = current.take() {
            if let Error::Sources(errors) = error {
                // Empty collections need no active frame and preserve the old
                // empty report. Check depth before storing the next iterator.
                if let Some((first, rest)) = errors.split_first() {
                    if depth == MAX_SOURCE_REPORT_FRAMES {
                        return Err(SourceProjectionError::TraversalLimit);
                    }
                    frames[depth] = Some(rest.iter());
                    depth += 1;
                    current = Some((&first.1, Some(&first.0)));
                    continue;
                }
            } else if visit(error, source)?.is_break() {
                // An already reached opaque/resource barrier retains precedence
                // over any later structural refusal or semantic context quota.
                return Ok(());
            }
        }
        while depth != 0 {
            if let Some((source, error)) = frames[depth - 1].as_mut().expect("active frame").next()
            {
                current = Some((error, Some(source)));
                break;
            }
            depth -= 1;
            frames[depth] = None;
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
        Error::Ordinal(error) => error.finding("", "").is_some(),
        Error::OdmFields(_) | Error::OdmValues(_) => true,
        _ => false,
    }
}
fn finding<'a>(error: &'a Error, source: &'a SourceDeclaration) -> Option<SourceFinding<'a>> {
    match error {
        Error::Source(TextTableError::Csv(error)) => error.finding(&source.name, &source.path),
        Error::TypedSource(crate::typed_csv::Error::Typing(error)) => error.finding(&source.name),
        Error::ParquetSource(error) => error.finding(&source.name, &source.path),
        Error::Ordinal(error) => error.finding(&source.name, source.ordinal.as_deref()?),
        _ => None,
    }
}

fn count(error: &Error) -> usize {
    match error {
        Error::OdmFields(fields) => fields.len(),
        Error::OdmValues(values) => values.len(),
        _ => 1,
    }
}

fn append<'a>(
    error: &'a Error,
    source: &'a SourceDeclaration,
    borrowed: &mut Vec<SourceFinding<'a>>,
) -> Result<(), SourceProjectionError> {
    match error {
        Error::OdmFields(fields) => {
            borrowed.extend(fields.iter().map(|finding| SourceFinding::Odm {
                dataset: &source.name,
                finding,
            }))
        }
        Error::OdmValues(values) => {
            borrowed.extend(values.iter().map(|&finding| SourceFinding::OdmValues {
                dataset: &source.name,
                finding,
            }))
        }
        _ => borrowed.push(finding(error, source).ok_or(ProjectionError::InvalidContext)?),
    }
    Ok(())
}

pub fn findings(
    error: &Error,
    source: Option<&SourceDeclaration>,
) -> Result<Option<Vec<Diagnostic>>, SourceProjectionError> {
    findings_with_limit(error, source, MAX_PROJECTION_BYTES)
}

/// Preserve authored input order across codec findings and engine-owned required
/// identifier findings. The whole report is admitted before any context is copied.
pub fn admitted_findings(
    errors: &[(usize, yamaa_engine::specification_run::SourceIssue<Error>)],
    sources: &[SourceDeclaration],
    maximum: usize,
) -> Result<Option<Vec<Diagnostic>>, SourceProjectionError> {
    use yamaa_engine::specification_run::SourceIssue;
    let mut entries = 0usize;
    let mut missing_source = false;
    for (index, issue) in errors {
        let source = sources.get(*index);
        missing_source |= source.is_none();
        match issue {
            SourceIssue::Decode(error) => {
                let mut semantic_only = true;
                walk(error, source, |error, source| {
                    if !semantic(error) {
                        semantic_only = false;
                        return Ok(std::ops::ControlFlow::Break(()));
                    }
                    missing_source |= source.is_none();
                    entries = entries
                        .checked_add(count(error))
                        .ok_or(ProjectionError::Limit)?;
                    Ok(std::ops::ControlFlow::Continue(()))
                })?;
                if !semantic_only {
                    return Ok(None);
                }
            }
            SourceIssue::OdmValues(values) => {
                entries = entries
                    .checked_add(values.len())
                    .ok_or(ProjectionError::Limit)?
            }
        }
    }
    if missing_source {
        return Err(ProjectionError::InvalidContext.into());
    }
    if entries > maximum / 128 {
        return Err(ProjectionError::Limit.into());
    }
    let mut borrowed = Vec::with_capacity(entries);
    for (index, issue) in errors {
        let source = &sources[*index];
        match issue {
            SourceIssue::Decode(error) => walk(error, Some(source), |error, source| {
                append(
                    error,
                    source.ok_or(ProjectionError::InvalidContext)?,
                    &mut borrowed,
                )?;
                Ok(std::ops::ControlFlow::Continue(()))
            })?,
            SourceIssue::OdmValues(values) => {
                borrowed.extend(values.iter().map(|&finding| SourceFinding::OdmValues {
                    dataset: &source.name,
                    finding,
                }))
            }
        }
    }
    Ok(Some(
        admit_source_diagnostics(&borrowed, maximum)?
            .iter()
            .collect(),
    ))
}
/// Structural traversal is admitted separately, before frame growth. Within that
/// bound, the first nonsemantic/resource barrier stops inspection and precedes
/// semantic context quotas. A barrier reached before a later deep collection also
/// precedes that unreachable structural refusal.
/// Missing declaration authority refuses explicitly; no identities are fabricated.
pub fn findings_with_limit(
    error: &Error,
    source: Option<&SourceDeclaration>,
    maximum: usize,
) -> Result<Option<Vec<Diagnostic>>, SourceProjectionError> {
    let mut semantic_only = true;
    let mut missing_source = false;
    let mut entries = 0usize;
    walk(error, source, |error, source| {
        if !semantic(error) {
            semantic_only = false;
            return Ok(std::ops::ControlFlow::Break(()));
        }
        missing_source |= source.is_none();
        entries = entries
            .checked_add(count(error))
            .ok_or(ProjectionError::Limit)?;
        Ok(std::ops::ControlFlow::Continue(()))
    })?;
    if !semantic_only {
        return Ok(None);
    }
    if missing_source {
        return Err(ProjectionError::InvalidContext.into());
    }
    // Core's fixed per-record charge bounds borrowed staging before allocation.
    if entries > maximum / 128 {
        return Err(ProjectionError::Limit.into());
    }
    let mut borrowed = Vec::with_capacity(entries);
    walk(error, source, |error, source| {
        append(
            error,
            source.ok_or(ProjectionError::InvalidContext)?,
            &mut borrowed,
        )?;
        Ok(std::ops::ControlFlow::Continue(()))
    })?;
    let report = admit_source_diagnostics(&borrowed, maximum)?;
    let diagnostics = report.iter().collect();
    Ok(Some(diagnostics))
}
