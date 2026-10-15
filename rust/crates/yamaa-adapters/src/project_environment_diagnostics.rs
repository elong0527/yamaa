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
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    ResourceLimit(crate::resource_diagnostics::Refusal),
    Core(core::Error),
    InvalidOrigin,
    Projection,
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
    }
}
fn issues(diagnostics: Vec<Diagnostic>) -> Result<Vec<Issue>, Error> {
    let issues = diagnostics
        .into_iter()
        .map(|diagnostic| Issue::from_core(diagnostic).map_err(|_| Error::Projection))
        .collect::<Result<Vec<_>, _>>()?;
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
    let report = projection::admit_root_scalars(root(root_document), findings, host)
        .map_err(projection_error)?;
    issues(report.diagnostics().map_err(projection_error)?)
}
/// Independent failed external scalars retain their own captured arenas. Other
/// capture failures remain available to their typed boundary projections.
pub fn scalar_sources<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Issue>, Error> {
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
    let report = projection::admit_scalar_sources(root(&rejected.root), sources)
        .map_err(projection_error)?;
    issues(report.diagnostics().map_err(projection_error)?)
}

fn schema_issues(
    captured: &CapturedFindings,
    entry: &str,
    declaration: &str,
    issues: &mut Vec<Issue>,
) -> Result<(), Error> {
    let rows = crate::specification_diagnostics::captured_findings(
        captured,
        &[
            ("source", &captured.source().identity),
            ("entry", entry),
            ("environment_path", declaration),
        ],
    )
    .map_err(|error| match error {
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
    let mut issues = Vec::new();
    schema_issues(captured, &captured.source().identity, "$", &mut issues)?;
    Ok(issues)
}
/// Project independent failed source schemas against their retained arenas.
/// Opaque errors, decoding failures and limits retain their original typed forms.
pub fn schema_sources<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Issue>, Error> {
    let mut issues = Vec::new();
    for source in &rejected.sources {
        let CaptureFailure::Structural(SourceError::Findings(captured)) = &source.error else {
            continue;
        };
        if !std::ptr::eq(captured.schema(), rejected.root.schema().as_ref()) {
            return Err(Error::InvalidOrigin);
        }
        let declaration = match &source.origin {
            Origin::Function(name) => {
                format!("functions.{name}")
            }
            Origin::Codelist(index) => format!("codelists[{index}]"),
            Origin::Lock => return Err(Error::InvalidOrigin),
        };
        schema_issues(
            captured,
            &rejected.root.source().identity,
            &declaration,
            &mut issues,
        )?;
    }
    Ok(issues)
}
fn metadata_path(origin: &Origin) -> String {
    match origin {
        Origin::Lock => "lock".into(),
        Origin::Function(name) => format!("functions.{name}"),
        Origin::Codelist(index) => format!("codelists[{index}]"),
    }
}
fn resource_limit(bound: yamaa_core::limits::LimitExceeded, declaration: &str) -> Error {
    Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
        bound.resource,
        bound.limit,
        bound.actual,
        vec![declaration.into()],
    ))
}
fn validation_limit(
    error: &yamaa_core::schema::ValidationError,
) -> Option<yamaa_core::limits::LimitExceeded> {
    use yamaa_core::{regex, schema::*};
    let (resource, limit) = match error {
        ValidationError::Depth { limit }
        | ValidationError::Type(TypeError::Limit {
            resource: TypeResource::Depth,
            limit,
        }) => ("semantic_depth", *limit),
        ValidationError::Constraint(ConstraintError::Regex(error)) => (
            match error.resource {
                regex::Resource::Depth => "semantic_depth",
                regex::Resource::Work => "regex_steps",
            },
            error.limit,
        ),
        _ => return None,
    };
    Some(yamaa_core::limits::LimitExceeded {
        resource,
        limit,
        actual: None,
    })
}
fn source_limit(error: &SourceError) -> Option<yamaa_core::limits::LimitExceeded> {
    use yamaa_core::schema::*;
    let (resource, limit) = match error {
        SourceError::Limit(bound)
        | SourceError::Decode {
            error: crate::yaml_decode::DecodeFailure::Limit(bound),
            ..
        } => return Some(*bound),
        SourceError::Validation(error)
        | SourceError::Normalize(NormalizationError::Validation(error)) => {
            return validation_limit(error);
        }
        SourceError::Normalize(NormalizationError::Limit {
            resource: NormalizationResource::Depth,
            limit,
        }) => ("semantic_depth", *limit),
        SourceError::Normalize(NormalizationError::Document(DocumentError::Limit {
            resource: DocumentResource::Depth,
            limit,
        })) => ("yaml_depth", *limit),
        _ => return None,
    };
    Some(yamaa_core::limits::LimitExceeded {
        resource,
        limit,
        actual: None,
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
        E::Limit(bound) => return Err(resource_limit(*bound, declaration)),
    };
    let mut diagnostic = Diagnostic {
        code: yamaa_core::diagnostic::ConditionCode::ProjectEnvironmentInvalid,
        spec_paths: vec![declaration.into()],
        context: Default::default(),
        source_span: None,
        operand_route: None,
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

pub(crate) fn root_resource(
    error: &crate::file_resources::Error,
    written: &str,
) -> Result<Vec<Issue>, Error> {
    Ok(vec![read_issue(error, written, written, "environment")?])
}

fn decode_issues(
    error: &SourceError,
    entry: &str,
    written: &str,
    declaration: &str,
) -> Result<Vec<Issue>, Error> {
    if let Some(bound) = source_limit(error) {
        return Err(resource_limit(bound, declaration));
    }
    if matches!(
        error,
        SourceError::Decode {
            error: crate::yaml_decode::DecodeFailure::Internal,
            ..
        }
    ) {
        return Err(Error::Projection);
    }
    let message = crate::specification_diagnostics::capture_failure(error);
    let mut rows = crate::file_application::preparation_failure(message);
    for row in &mut rows {
        let mut context: yamaa_core::json::Value =
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
    decode_issues(error, written, written, "$")
}

pub(crate) fn root_limit(error: &SourceError) -> Option<Error> {
    source_limit(error).map(|bound| resource_limit(bound, "$"))
}

/// Native metadata failures retain the authored dependency path and declaration.
/// A fixed resource boundary refuses the whole projection beside its original owner.
pub(crate) fn capture_sources(
    rejected: &RejectedEnvironment<crate::file_resources::Error>,
) -> Result<Vec<Issue>, Error> {
    let mut rows = Vec::new();
    for source in &rejected.sources {
        if matches!(
            source.error,
            CaptureFailure::Scalar { .. } | CaptureFailure::Structural(SourceError::Findings(_))
        ) {
            continue;
        }
        let entry = &rejected.root.source().identity;
        let declaration = metadata_path(&source.origin);
        match &source.error {
            CaptureFailure::Port(error) | CaptureFailure::Interrupted(error) => {
                rows.push(read_issue(error, entry, &source.written, &declaration)?);
            }
            CaptureFailure::Structural(error) => {
                rows.extend(decode_issues(error, entry, &source.written, &declaration)?)
            }
            CaptureFailure::Limit(bound) => return Err(resource_limit(*bound, &declaration)),
            CaptureFailure::Protocol => return Err(Error::Projection),
            CaptureFailure::Scalar { .. } => unreachable!("handled scalar finding"),
        }
    }
    Ok(rows)
}

/// Return admission issues in their original order. This borrows the rejected
/// draft and held capture records; it never reads, decodes or activates anything.
/// Resource-limit failures have no fabricated semantic condition.
pub fn admission<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Issue>, Error> {
    let Some(findings) = &rejected.admission else {
        return Ok(vec![]);
    };
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
    )
    .map_err(projection_error)?;
    issues(report.diagnostics().map_err(projection_error)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{file_resources, project_source, specification_source::Source};
    use yamaa_core::{limits::Limits, project_function::Language};

    struct Missing;
    impl project_source::CapturePort for Missing {
        type Error = file_resources::Error;
        fn capture(
            &mut self,
            _: project_source::Request<'_>,
        ) -> Result<project_source::Reply, Self::Error> {
            Err(file_resources::Error::Missing)
        }
    }

    #[test]
    fn retained_structural_and_preparation_limits_refuse_the_complete_metadata_projection() {
        let project_source::Failure::Rejected(mut rejected) = project_source::prepare(
            crate::shipped_schema::capture_environment().unwrap(),
            Source {
                identity: "held/environment.yaml".into(),
                bytes: b"schema_version: '1.0'\ncodelists: [missing.yaml, ct.yaml]\n".to_vec(),
            },
            Language::Python,
            &mut Missing,
        )
        .unwrap_err() else {
            panic!("retained native capture errors")
        };
        let bound = Limits::spec_bytes(Limits::SPEC_BYTES + 1).unwrap_err();
        let expected = resource_limit(bound, "codelists[1]");
        for error in [
            CaptureFailure::Structural(SourceError::Limit(bound)),
            CaptureFailure::Structural(SourceError::Decode {
                identity: "held/ct.yaml".into(),
                error: crate::yaml_decode::DecodeFailure::Limit(bound),
            }),
            CaptureFailure::Limit(bound),
        ] {
            rejected.sources[1].error = error;
            assert_eq!(capture_sources(&rejected), Err(expected.clone()));
            assert_eq!(capture_sources(&rejected), Err(expected.clone()));
            assert!(matches!(
                rejected.sources[0].error,
                CaptureFailure::Port(file_resources::Error::Missing)
            ));
        }
        rejected.sources[1].error = CaptureFailure::Structural(SourceError::Normalize(
            yamaa_core::schema::NormalizationError::Limit {
                resource: yamaa_core::schema::NormalizationResource::Depth,
                limit: Limits::SEMANTIC_DEPTH,
            },
        ));
        assert_eq!(
            capture_sources(&rejected),
            Err(Error::ResourceLimit(
                crate::resource_diagnostics::Refusal::new(
                    "semantic_depth",
                    Limits::SEMANTIC_DEPTH,
                    None,
                    vec!["codelists[1]".into()],
                )
            ))
        );
    }
}
