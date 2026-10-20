//! Project only retained static admission, restoring each captured source origin.
//! Capture failures and opaque host payloads remain in the rejected environment.
use crate::{
    project_source::{CaptureFailure, Origin, RejectedEnvironment},
    specification_source::{
        CapturedDocument, CapturedFindings, CapturedSchema, Error as SourceError,
    },
};
use yamaa_core::{
    ast::Surface,
    diagnostic::Diagnostic,
    language::{self, FileType, LanguageVersion},
    project_environment_diagnostics::{self as core, projection},
    schema::DocumentNode as N,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    ResourceLimit(crate::resource_diagnostics::Refusal),
    Core(core::Error),
    InvalidOrigin,
    Projection,
}

fn identity(document: &CapturedDocument) -> &str {
    &document.source().identity
}
fn projection_error(error: projection::Error) -> Error {
    match error {
        projection::Error::Core(error) => Error::Core(error),
        projection::Error::InvalidOrigin => Error::InvalidOrigin,
    }
}
/// The surface a held project file is written in: its own header says
/// (SCHEMA-2).
fn surface(document: &CapturedDocument) -> Surface {
    let document = &document.normalized().document;
    if !language::declares_header(document) {
        return Surface::V0_5;
    }
    let minor = document
        .field(document.root(), "yamaa")
        .and_then(|node| match &document.nodes()[node] {
            N::Text(text) => LanguageVersion::parse(text),
            _ => None,
        })
        .map_or(0, LanguageVersion::minor);
    Surface::V1 { minor }
}
/// Report failed inline scalars of the held project file, preserving full
/// paths.
pub fn scalar_root(
    root_document: &CapturedDocument,
    findings: &[yamaa_core::project_function_document::Finding],
) -> Result<Vec<Diagnostic>, Error> {
    let report =
        projection::admit_root_scalars(identity(root_document), surface(root_document), findings)
            .map_err(projection_error)?;
    report.diagnostics().map_err(projection_error)
}
/// The projection origin of one captured dependency. A lock carries no document.
fn origin(origin: &Origin) -> Option<projection::Origin<'_>> {
    match origin {
        Origin::Environment(language) => Some(projection::Origin::Environment(language.key())),
        Origin::Function(name) => Some(projection::Origin::Function(name)),
        Origin::FunctionFile(index) => Some(projection::Origin::FunctionFile(*index)),
        Origin::Codelist(index) => Some(projection::Origin::Codelist(*index)),
        Origin::Lock => None,
    }
}
fn versioned<E>(rejected: &RejectedEnvironment<E>) -> bool {
    matches!(rejected.project.header.surface, Surface::V1 { .. })
}
/// In 0.5, codelist sources share the project schema, and the environment
/// file and its function files share the environment schema. In 1.0 each
/// listed file is admitted against the closure of its own type.
fn same_schema<E>(
    rejected: &RejectedEnvironment<E>,
    origin: &Origin,
    schema: &CapturedSchema,
) -> bool {
    let root_class =
        |file_type: FileType| schema.structure().root_class().name == file_type.root_class();
    match origin {
        Origin::Codelist(_) if versioned(rejected) => root_class(FileType::Codelists),
        Origin::FunctionFile(_) => versioned(rejected) && root_class(FileType::Function),
        Origin::Codelist(_) => std::ptr::eq(schema, rejected.root.schema().as_ref()),
        Origin::Function(_) => rejected
            .environment_file
            .as_ref()
            .is_some_and(|environment| std::ptr::eq(schema, environment.schema().as_ref())),
        // A failed environment file has no retained sibling to compare, so its
        // captured schema must at least be an environment root.
        Origin::Environment(_) => {
            schema.structure().root_class().name == "environment_class"
                && rejected.environment_file.is_none()
        }
        Origin::Lock => false,
    }
}
/// Independent failed external scalars retain their own captured arenas. Other
/// capture failures remain available to their typed boundary projections.
pub fn scalar_sources<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Diagnostic>, Error> {
    for source in &rejected.sources {
        let CaptureFailure::Scalar { document, .. } = &source.error else {
            continue;
        };
        if !same_schema(rejected, &source.origin, document.schema()) {
            return Err(Error::InvalidOrigin);
        }
    }
    let sources = rejected.sources.iter().filter_map(|source| {
        let CaptureFailure::Scalar { document, findings } = &source.error else {
            return None;
        };
        Some(projection::ScalarSource {
            capture: projection::Capture {
                origin: origin(&source.origin)?,
                identity: identity(document),
            },
            findings,
        })
    });
    let report =
        projection::admit_scalar_sources(&rejected.project, identity(&rejected.root), sources)
            .map_err(projection_error)?;
    report.diagnostics().map_err(projection_error)
}

fn schema_issues(
    captured: &CapturedFindings,
    entry: &str,
    declaration: &str,
    issues: &mut Vec<Diagnostic>,
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
    issues.extend(rows);
    Ok(())
}
/// Resolve root schema findings against their actual retained validation input.
pub fn schema_root(captured: &CapturedFindings) -> Result<Vec<Diagnostic>, Error> {
    let mut issues = Vec::new();
    schema_issues(captured, &captured.source().identity, "$", &mut issues)?;
    Ok(issues)
}
/// Project independent failed source schemas against their retained arenas.
/// Opaque errors, decoding failures and limits retain their original typed forms.
pub fn schema_sources<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Diagnostic>, Error> {
    let mut issues = Vec::new();
    for source in &rejected.sources {
        let CaptureFailure::Structural(SourceError::Findings(captured)) = &source.error else {
            continue;
        };
        if !same_schema(rejected, &source.origin, captured.schema()) {
            return Err(Error::InvalidOrigin);
        }
        let declaration = metadata_path(&source.origin, rejected);
        schema_issues(
            captured,
            &rejected.root.source().identity,
            &declaration,
            &mut issues,
        )?;
    }
    Ok(issues)
}
/// The field that declares a captured file: in 0.5 the lock and function
/// files are declared by the environment file, and in 1.0 every file by the
/// project file (D8).
fn metadata_path<E>(origin: &Origin, rejected: &RejectedEnvironment<E>) -> String {
    match origin {
        Origin::Environment(language) => format!("environments.{}", language.key()),
        Origin::Lock if versioned(rejected) => {
            format!("environments.{}.lock", rejected.host.key())
        }
        Origin::Lock => "lock".into(),
        Origin::Function(name) => format!("functions.{name}"),
        Origin::FunctionFile(index) => format!("functions[{index}]"),
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
    use yamaa_core::schema::*;
    let (resource, limit) = match error {
        ValidationError::Depth { limit }
        | ValidationError::Type(TypeError::Limit {
            resource: TypeResource::Depth,
            limit,
        }) => ("semantic_depth", *limit),
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
) -> Result<Diagnostic, Error> {
    use crate::file_resources::Error as E;
    let cause = match error {
        E::Host(_) => "host_resource_failure",
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
    Ok(diagnostic)
}

pub(crate) fn root_resource(
    error: &crate::file_resources::Error,
    written: &str,
) -> Result<Vec<Diagnostic>, Error> {
    Ok(vec![read_issue(error, written, written, "environment")?])
}

fn decode_issues(
    error: &SourceError,
    entry: &str,
    written: &str,
    declaration: &str,
) -> Result<Vec<Diagnostic>, Error> {
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
    let mut rows = crate::specification_diagnostics::capture_failure(error).into_issues();
    for row in &mut rows {
        for (key, value) in [
            ("entry", entry),
            ("path", written),
            ("environment_path", declaration),
        ] {
            row.context.insert(key.into(), value.into());
        }
    }
    Ok(rows)
}

/// Project a root decoder failure with the caller's explicit environment path.
pub(crate) fn root_boundary(error: &SourceError, written: &str) -> Result<Vec<Diagnostic>, Error> {
    decode_issues(error, written, written, "$")
}

pub(crate) fn root_limit(error: &SourceError) -> Option<Error> {
    source_limit(error).map(|bound| resource_limit(bound, "$"))
}

/// Native metadata failures retain the authored dependency path and declaration.
/// A fixed resource boundary refuses the whole projection beside its original owner.
pub(crate) fn capture_sources(
    rejected: &RejectedEnvironment<crate::file_resources::Error>,
) -> Result<Vec<Diagnostic>, Error> {
    // A 1.0 project's listed files and lock are read as every file a run
    // reads; projecting their read failures comes with #3277, before public
    // calls read 1.0 files (P1-9).
    if versioned(rejected) {
        return Err(Error::Projection);
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
        let declaration = metadata_path(&source.origin, rejected);
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
pub fn admission<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Diagnostic>, Error> {
    let Some(findings) = &rejected.admission else {
        return Ok(vec![]);
    };
    for capture in &rejected.captures {
        if matches!(capture.origin, Origin::Lock | Origin::Environment(_))
            || !same_schema(rejected, &capture.origin, capture.document.schema())
        {
            return Err(Error::InvalidOrigin);
        }
    }
    let captures = rejected.captures.iter().filter_map(|capture| {
        Some(projection::Capture {
            origin: origin(&capture.origin)?,
            identity: identity(&capture.document),
        })
    });
    let report = projection::admit_admission(
        projection::Admission {
            project: &rejected.project,
            root: identity(&rejected.root),
            environment: rejected.environment_file.as_ref().map(identity),
            draft: &rejected.draft,
            function_origins: &rejected.origins.functions,
            function_files: &rejected.origins.function_files,
            codelist_origins: &rejected.origins.codelists,
            captures,
        },
        findings,
    )
    .map_err(projection_error)?;
    report.diagnostics().map_err(projection_error)
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;
    use crate::{file_resources, project_source, specification_source::Source};
    use yamaa_core::{limits::Limits, project_function::Language};

    struct Missing;
    impl project_source::ProjectCapturePort for Missing {
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
            crate::shipped_schema::capture_project().unwrap(),
            Source {
                identity: "held/project.yaml".into(),
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
