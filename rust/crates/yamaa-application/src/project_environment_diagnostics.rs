//! Project only retained static admission, restoring each captured source origin.
//! Capture failures and opaque host payloads remain in the rejected environment.
use crate::{
    project_source::{CaptureFailure, Origin, RejectedEnvironment},
    specification_source::{
        CapturedDocument, CapturedFindings, CapturedSchema, Error as SourceError, RefusedHeader,
    },
};
use yamaa_core::{
    application_issue::Fault,
    diagnostic::Diagnostic,
    language::{self, FileType, HeaderFault, ReferenceFault},
    project_environment_diagnostics::{self as core, projection},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    ResourceLimit(crate::resource_diagnostics::Refusal),
    /// A file a project names failed to be read for a host reason no
    /// condition classifies: the whole preparation is refused with the opaque
    /// resource refusal of this code, as any file a run reads is (FILE-11).
    Resource(Fault),
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
/// Report failed inline scalars of the held project file, preserving full
/// paths.
pub fn scalar_root(
    root_document: &CapturedDocument,
    findings: &[yamaa_core::project_function_document::Finding],
) -> Result<Vec<Diagnostic>, Error> {
    let report = projection::admit_root_scalars(identity(root_document), findings)
        .map_err(projection_error)?;
    report.diagnostics().map_err(projection_error)
}
/// The projection origin of one captured dependency. A lock carries no document.
fn origin(origin: &Origin) -> Option<projection::Origin> {
    match origin {
        Origin::FunctionFile(index) => Some(projection::Origin::FunctionFile(*index)),
        Origin::Codelist(index) => Some(projection::Origin::Codelist(*index)),
        Origin::Lock => None,
    }
}
/// Each listed file is admitted against the closure of its own type.
fn same_schema(origin: &Origin, schema: &CapturedSchema) -> bool {
    let root_class =
        |file_type: FileType| schema.structure().root_class().name == file_type.root_class();
    match origin {
        Origin::Codelist(_) => root_class(FileType::Codelists),
        Origin::FunctionFile(_) => root_class(FileType::Function),
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
        if !same_schema(&source.origin, document.schema()) {
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
        if !same_schema(&source.origin, captured.schema()) {
            return Err(Error::InvalidOrigin);
        }
        let declaration = metadata_path(&source.origin, rejected)?;
        schema_issues(
            captured,
            &rejected.root.source().identity,
            &declaration,
            &mut issues,
        )?;
    }
    Ok(issues)
}
/// The field of the project file that declares a captured file (D8). Only a
/// host language reads a lock.
fn metadata_path<E>(origin: &Origin, rejected: &RejectedEnvironment<E>) -> Result<String, Error> {
    Ok(match origin {
        Origin::Lock => {
            let host = rejected.host.ok_or(Error::InvalidOrigin)?;
            format!("environments.{}.lock", host.key())
        }
        Origin::FunctionFile(index) => format!("functions[{index}]"),
        Origin::Codelist(index) => format!("codelists[{index}]"),
    })
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
///
/// A file a project names that cannot be read fails as any declared file a
/// run reads (FUNC-2, FILE-11), and a listed file whose header the engine
/// does not read is refused at the field that lists it (SCHEMA-2).
pub(crate) fn capture_sources(
    rejected: &RejectedEnvironment<crate::file_resources::Error>,
) -> Result<Vec<Diagnostic>, Error> {
    let mut rows = Vec::new();
    for source in &rejected.sources {
        if matches!(
            source.error,
            CaptureFailure::Scalar { .. } | CaptureFailure::Structural(SourceError::Findings(_))
        ) {
            continue;
        }
        let entry = &rejected.root.source().identity;
        let declaration = metadata_path(&source.origin, rejected)?;
        match &source.error {
            CaptureFailure::Port(error) | CaptureFailure::Interrupted(error) => {
                rows.push(declared_read_issue(&source.written, error, &declaration)?);
            }
            CaptureFailure::Structural(SourceError::Reference(refused)) => {
                rows.push(reference_issue(rejected, &declaration, refused)?);
            }
            CaptureFailure::Structural(error) => {
                rows.extend(decode_issues(error, entry, &source.written, &declaration)?)
            }
            CaptureFailure::Limit(bound) => return Err(resource_limit(*bound, &declaration)),
            // A lock that is no packaging lock fails as the project's function
            // files do (FUNC-3).
            CaptureFailure::Lock => {
                rows.push(core::invalid_packaging_lock(
                    rejected.host.ok_or(Error::InvalidOrigin)?,
                    entry,
                    &source.written,
                ));
            }
            CaptureFailure::Scalar { .. } => unreachable!("handled scalar finding"),
        }
    }
    Ok(rows)
}

/// A file a project names, at `declaration`, that cannot be read: one of
/// FILE-11's conditions at that field with the path as written, as a schema
/// link or a changed held file reports it; a bound keeps its resource refusal,
/// and any other host failure refuses the whole preparation with the opaque
/// resource refusal (FUNC-2).
fn declared_read_issue(
    written: &str,
    error: &crate::file_resources::Error,
    declaration: &str,
) -> Result<Diagnostic, Error> {
    use crate::file_resources::Error as E;
    match error {
        E::Limit(bound) => Err(resource_limit(*bound, declaration)),
        error => match crate::file_resources::resource_failure(error) {
            Some(failure) => Ok(failure.field_diagnostic(declaration, written)),
            None => Err(Error::Resource(crate::file_resources::resource_fault(
                error,
            ))),
        },
    }
}

/// A file a project lists at `declaration` whose header declares a version
/// the engine does not read, or another type: refused at that field, naming
/// the file as `source`, as the project's referencing field reports the file
/// it names (SCHEMA-2).
fn reference_issue<E>(
    rejected: &RejectedEnvironment<E>,
    declaration: &str,
    refused: &RefusedHeader,
) -> Result<Diagnostic, Error> {
    let reference = language::references(&rejected.root.raw().document, FileType::Project)
        .into_iter()
        .find(|reference| reference.field == declaration)
        .ok_or(Error::InvalidOrigin)?;
    if matches!(refused.fault(), HeaderFault::FileType { expected, .. } if *expected != reference.expected)
    {
        return Err(Error::InvalidOrigin);
    }
    match reference.refuse(&refused.source().identity, refused.fault()) {
        ReferenceFault::Field(diagnostic) => Ok(diagnostic),
        // A malformed header is a finding in the file, never a refusal held
        // for its field.
        ReferenceFault::Finding(_) => Err(Error::InvalidOrigin),
    }
}

/// Return admission issues in their original order. This borrows the rejected
/// draft and held capture records; it never reads, decodes or activates anything.
/// Resource-limit failures have no fabricated semantic condition.
pub fn admission<E>(rejected: &RejectedEnvironment<E>) -> Result<Vec<Diagnostic>, Error> {
    let Some(findings) = &rejected.admission else {
        return Ok(vec![]);
    };
    for capture in &rejected.captures {
        if !same_schema(&capture.origin, capture.document.schema()) {
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

    /// A resolver that finds no file the project names.
    struct Missing;
    impl yamaa_core::resolver::Resolver for Missing {
        type Error = file_resources::Error;
        fn inspect(
            &mut self,
            _: yamaa_core::resolver::Name<'_>,
        ) -> Result<yamaa_core::resolver::Located, Self::Error> {
            Err(file_resources::Error::Missing)
        }
        fn capture(
            &mut self,
            _: yamaa_core::resolver::Name<'_>,
        ) -> Result<yamaa_core::resolver::Captured, Self::Error> {
            unreachable!("a file no inspection located is never captured")
        }
        fn failure(
            &self,
            _: yamaa_core::resolver::Name<'_>,
            _: &Self::Error,
        ) -> Option<yamaa_core::resolver::Failure> {
            None
        }
        fn rebase(
            &mut self,
            _: &yamaa_core::resolver::Located,
            _: &yamaa_core::resolver::Located,
            _: &str,
        ) -> Result<String, Self::Error> {
            unreachable!("a project file is never rebased")
        }
    }

    #[test]
    fn retained_structural_and_preparation_limits_refuse_the_complete_metadata_projection() {
        let project_source::Failure::Rejected(mut rejected) = project_source::prepare(
            crate::document::Closures::shipped().unwrap(),
            Source {
                identity: "held/project.yaml".into(),
                bytes: b"yamaa: '1.0'\ntype: project\ncodelists: [missing.yaml, ct.yaml]\n"
                    .to_vec(),
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
