//! Public project preparation and static diagnostics over the approved resources.
//! Installed bindings supply host capabilities only after this shared admission.
use crate::{
    file_application::{self, Failure},
    file_preparation::FileSpecification,
    file_project,
    project_source::Failure as EnvironmentFailure,
    specification_diagnostics::{self as diagnostics, Rejection},
    specification_source::Error as SourceError,
};
use yamaa_core::application_issue::{Fault, Step};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{project_function::Language, specification::PrepareError};

#[derive(Debug)]
pub enum Error {
    Configuration(crate::file_configuration::Error),
    Schema(SourceError),
    Project(file_project::Error),
}
pub fn prepare(
    specification: &str,
    environment: &str,
    host: Language,
) -> Result<FileSpecification, Error> {
    let (mut resources, entry) =
        crate::file_configuration::resources(specification).map_err(Error::Configuration)?;
    let environment = environment_path(environment)
        .map_err(|error| Error::Project(file_project::Error::Resource(error)))?;
    resources.select_caller_parent(&environment);
    file_project::prepare(
        resources,
        &entry,
        &environment,
        host,
        crate::document::Closures::shipped().map_err(Error::Schema)?,
    )
    .map_err(Error::Project)
}
pub(crate) fn environment_path(environment: &str) -> Result<String, crate::file_resources::Error> {
    use crate::file_resources::Error;
    if environment.is_empty() || environment.contains('\0') {
        return Err(Error::InvalidPath);
    }
    let environment = std::path::absolute(environment).map_err(|_| Error::InvalidPath)?;
    crate::file_resolver::caller_path(&environment)
}
fn refused(step: Step) -> Failure {
    file_application::rejected(step, Fault::IssueProjection)
}
/// The issues of a refused inherited preparation; an opaque host failure
/// reports the projection fault of the preparation stage.
fn inherited(
    error: &crate::specification_source::InheritanceError<crate::file_resources::Error>,
) -> Failure {
    diagnostics::inheritance_failure_ref(error)
        .map(Rejection::into_issues)
        .unwrap_or_else(|_| refused(Step::Prepare))
}
pub fn failure(error: &Error) -> Failure {
    match error {
        Error::Configuration(crate::file_configuration::Error::Path(error)) => {
            file_application::resource_failure(Step::Prepare, error)
        }
        Error::Schema(_) => file_application::rejected(Step::Prepare, Fault::ShippedSchema),
        Error::Project(error) => project_failure(error),
    }
}
fn project_failure(error: &file_project::Error) -> Failure {
    match error {
        file_project::Error::Resource(error) => {
            file_application::resource_failure(Step::Environment, error)
        }
        file_project::Error::Document(error) => match error {
            crate::file_preparation::Error::Entry(error)
            | crate::file_preparation::Error::Resource(error) => {
                file_application::resource_failure(Step::Prepare, error)
            }
            crate::file_preparation::Error::ResourceFinding { diagnostic, .. }
            | crate::file_preparation::Error::PreparationFinding { diagnostic, .. }
            | crate::file_preparation::Error::Reference(diagnostic) => {
                file_application::classified_resource(diagnostic)
            }
            crate::file_preparation::Error::Preparation(error) => inherited(error),
            crate::file_preparation::Error::Compile(
                error @ crate::specification_run::Error::Prepare(
                    PrepareError::Limit(_) | PrepareError::Numeric { .. },
                ),
            ) => diagnostics::failure(error, None).into_issues(),
            crate::file_preparation::Error::Compile(_) => refused(Step::Prepare),
            crate::file_preparation::Error::Inventory(_) => {
                file_application::rejected(Step::Prepare, Fault::Inventory)
            }
        },
        file_project::Error::Compile(run) => compile_failure(run),
        file_project::Error::Environment(error) => environment_failure(error),
    }
}
/// The issues of a compilation the compiler refused: its findings, its
/// unsupported uses, or the preparation fault that stopped it.
fn compile_failure(run: &crate::specification_run::RejectedRun) -> Failure {
    match &run.rejection.error {
        PrepareError::Invalid(_) => {
            crate::specification_check::rejected(run).unwrap_or_else(|_| refused(Step::Check))
        }
        PrepareError::Unsupported(features) => features
            .iter()
            .flat_map(|feature| {
                file_application::unsupported(&feature.operation, Some(&feature.path))
            })
            .collect(),
        error => diagnostics::preparation(error).into_issues(),
    }
}
fn environment_failure(error: &EnvironmentFailure<crate::file_resources::Error>) -> Failure {
    use crate::project_environment_diagnostics as diagnostics;
    let projected = match error {
        EnvironmentFailure::Root(SourceError::Findings(captured)) => {
            diagnostics::schema_root(captured)
        }
        EnvironmentFailure::RootScalar { root, findings } => {
            diagnostics::scalar_root(root, findings)
        }
        EnvironmentFailure::Rejected(rejected) => {
            let mut issues = match diagnostics::admission(rejected) {
                Ok(issues) => issues,
                Err(error) => return environment_projection_failure(error),
            };
            for projection in [
                diagnostics::scalar_sources(rejected),
                diagnostics::schema_sources(rejected),
                diagnostics::capture_sources(rejected),
            ] {
                match projection {
                    Ok(rows) => issues.extend(rows),
                    Err(error) => return environment_projection_failure(error),
                }
            }
            Ok(issues)
        }
        EnvironmentFailure::Interrupted { error, .. } => {
            return file_application::resource_failure(Step::Environment, error)
        }
        EnvironmentFailure::Root(error @ SourceError::Decode { identity, .. }) => {
            diagnostics::root_boundary(error, identity)
        }
        EnvironmentFailure::Root(error) => {
            if let Some(error) = diagnostics::root_limit(error) {
                return environment_projection_failure(error);
            }
            return file_application::rejected(Step::Environment, Fault::EnvironmentBoundary);
        }
    };
    projected.unwrap_or_else(environment_projection_failure)
}
fn environment_projection_failure(error: crate::project_environment_diagnostics::Error) -> Failure {
    match error {
        crate::project_environment_diagnostics::Error::ResourceLimit(refusal) => {
            refusal.issues(Step::Environment, Fault::ResourceLimit)
        }
        // A file a 1.0 project names failed for a host reason no condition
        // classifies: the opaque resource refusal, as for the project file.
        crate::project_environment_diagnostics::Error::Resource(fault) => {
            file_application::rejected(Step::Environment, fault)
        }
        _ => refused(Step::Environment),
    }
}
pub fn check(specification: &str, environment: &str, host: Language) -> Failure {
    file_application::collect(
        match crate::file_workflow::select(specification, environment, host) {
            Ok(prepared) => crate::specification_check::issues(prepared.run()),
            Err(error) => selection_failure(&error),
        },
    )
}
/// The issues of a refused workflow preparation, such as a project file that
/// cannot be captured, exactly as a check of a specification reports them.
pub fn workflow_rejection(error: &crate::file_workflow::Error) -> Failure {
    file_application::collect(workflow_issues(error))
}
fn workflow_issues(error: &crate::file_workflow::Error) -> Failure {
    preparation_issues(error).unwrap_or_else(|error| match error {
        IssueError::Original(error) => workflow_failure(error),
        IssueError::Projection => refused(Step::Check),
        IssueError::ResourceLimit(refusal) => {
            refusal.issues(Step::Environment, Fault::ResourceLimit)
        }
    })
}

/// Selected preparation retains the original owner in the caller. Diagnostic
/// projection is complete and bounded; no feature dispatch depends on failure.
pub fn selection_failure(error: &crate::file_workflow::SelectionError) -> Failure {
    match error {
        crate::file_workflow::SelectionError::Preparation(error) => workflow_issues(error),
        crate::file_workflow::SelectionError::Compile(error) => compile_failure(error.run()),
    }
}
fn workflow_failure(error: &crate::file_workflow::Error) -> Failure {
    use crate::file_workflow::Error as W;
    match error {
        W::Caller { path, error } => vec![crate::file_resources::caller_read_failure(path, error)],
        W::Schema(_) => file_application::rejected(Step::Prepare, Fault::ShippedSchema),
        W::Environment(environment) => project_failure(environment.error()),
        W::Entry(entry) => entry_failure(entry.error()),
    }
}
/// Entry failures that the static projection does not own keep their typed
/// cause: the caller's own path, resource and limit facts, or the named
/// boundary.
fn entry_failure(error: &crate::file_entry::Error) -> Failure {
    use crate::{file_entry::Error as E, file_preparation::Error as D};
    match error {
        E::Caller { path, error } => vec![crate::file_resources::caller_read_failure(path, error)],
        E::Document(D::Entry(error) | D::Resource(error)) => {
            file_application::resource_failure(Step::Prepare, error)
        }
        E::Snapshot { identity, error } => {
            file_application::resource_failure_from(Step::Prepare, error, identity)
        }
        E::Document(D::ResourceFinding { diagnostic, .. })
        | E::Document(D::PreparationFinding { diagnostic, .. })
        | E::Document(D::Reference(diagnostic)) => {
            file_application::classified_resource(diagnostic)
        }
        E::Document(D::Preparation(error)) => preparation_boundary(error),
        E::Document(D::Compile(
            error @ crate::specification_run::Error::Prepare(
                PrepareError::Limit(_) | PrepareError::Numeric { .. },
            ),
        )) => diagnostics::failure(error, None).into_issues(),
        E::Document(D::Compile(_)) => refused(Step::Prepare),
        E::Document(D::Inventory(_)) => file_application::rejected(Step::Prepare, Fault::Inventory),
    }
}
// Semantic findings have already passed the complete static preflight. These
// entry failures keep only bounded refusals and fixed boundary codes, without
// cloning source bytes or an inherited arena. Opaque transport causes remain retained.
fn preparation_boundary(
    error: &crate::specification_source::InheritanceError<crate::file_resources::Error>,
) -> Failure {
    use crate::specification_source::{Error as S, InheritanceError as I};
    let I::Entry(source) = error else {
        return match diagnostics::inheritance_failure_ref(error) {
            Ok(rejection @ Rejection::Limit(_)) => rejection.into_issues(),
            _ => refused(Step::Prepare),
        };
    };
    if matches!(source, S::Findings(_) | S::Decode { .. }) {
        // Decode semantic rows are handled by the bounded static projection;
        // only its fixed internal/limit refusal reaches this fallback.
        if !matches!(
            source,
            S::Decode {
                error: crate::yaml_decode::DecodeFailure::Internal
                    | crate::yaml_decode::DecodeFailure::Limit(_),
                ..
            }
        ) {
            return refused(Step::Prepare);
        }
    }
    inherited(error)
}

/// A static issue projection refusal retains the exact original preparation
/// error beside it.
#[derive(Debug)]
pub enum IssueError<'a> {
    Original(&'a crate::file_workflow::Error),
    Projection,
    ResourceLimit(crate::resource_diagnostics::Refusal),
}
fn invalid() -> IssueError<'static> {
    IssueError::Projection
}
/// Project the semantic findings of a refused environment or entry capture;
/// any other preparation failure stays with its original owner.
pub fn preparation_issues(
    error: &crate::file_workflow::Error,
) -> Result<Vec<Diagnostic>, IssueError<'_>> {
    use crate::file_workflow::Error as W;
    match error {
        W::Environment(environment) => environment_issues(error, environment.error()),
        W::Entry(entry) => match entry.error() {
            crate::file_entry::Error::Document(crate::file_preparation::Error::Preparation(
                cause,
            )) => inheritance_issues(error, cause),
            _ => Err(IssueError::Original(error)),
        },
        _ => Err(IssueError::Original(error)),
    }
}

fn environment_issues<'a>(
    owner: &'a crate::file_workflow::Error,
    cause: &file_project::Error,
) -> Result<Vec<Diagnostic>, IssueError<'a>> {
    use crate::project_environment_diagnostics as D;
    use crate::project_source::Failure as F;
    use crate::specification_source::Error as S;
    let projection = |error: D::Error| match error {
        D::Error::ResourceLimit(refusal) => IssueError::ResourceLimit(refusal),
        // The opaque resource refusal stays with the original owner, whose
        // failure reports it.
        D::Error::Resource(_) => IssueError::Original(owner),
        _ => invalid(),
    };
    // A project file that cannot be read is the caller's own path, which
    // the workflow reports before any environment is captured (#3321).
    let file_project::Error::Environment(cause) = cause else {
        return Err(IssueError::Original(owner));
    };
    let mut issues = Vec::new();
    match cause {
        F::Root(S::Findings(captured)) => {
            issues.extend(D::schema_root(captured).map_err(|_| invalid())?)
        }
        F::Root(error) => {
            let crate::file_workflow::Error::Environment(environment) = owner else {
                return Err(invalid());
            };
            let written = environment.environment().ok_or_else(invalid)?;
            issues.extend(D::root_boundary(error, written).map_err(projection)?);
        }
        F::RootScalar { root, findings } => {
            issues.extend(D::scalar_root(root, findings).map_err(|_| invalid())?)
        }
        F::Rejected(rejected) => {
            for rows in [
                D::admission(rejected),
                D::scalar_sources(rejected),
                D::schema_sources(rejected),
                D::capture_sources(rejected),
            ] {
                issues.extend(rows.map_err(projection)?);
            }
        }
        _ => return Err(IssueError::Original(owner)),
    }
    Ok(issues)
}

fn inheritance_issues<'a>(
    owner: &'a crate::file_workflow::Error,
    cause: &crate::specification_source::InheritanceError<crate::file_resources::Error>,
) -> Result<Vec<Diagnostic>, IssueError<'a>> {
    use crate::specification_source::{Error as S, InheritanceError as I, SourceFailure};
    use crate::yaml_decode::DecodeFailure;
    use yamaa_core::{
        dictionary_preparation::Cause, inheritance::Error as G, resolution::Error as D,
        schema::NormalizationError as N,
    };
    // Text the YAML reader refuses, in the entry or in a parent layer, is a
    // finding of the file it refused (SCHEMA-3, SPEC-8, VALUE-11).
    let refused = |error: &DecodeFailure| {
        matches!(
            error,
            DecodeFailure::NonAscii(_)
                | DecodeFailure::InvalidYaml { .. }
                | DecodeFailure::InvalidText(_)
        )
    };
    let supported = match cause {
        I::Entry(S::Findings(_)) => true,
        I::Entry(S::Decode { error, .. }) => refused(error),
        I::Dictionary { error, .. } => matches!(
            &error.cause,
            Cause::Capture {
                failure: Some(_),
                ..
            } | Cause::Admission {
                error: N::Invalid(_),
                ..
            }
        ),
        I::Versioned { error, .. } => {
            use crate::document::{ProducerFailure as P, SpecificationError as V};
            match error.as_ref() {
                V::Entry(S::Findings(_))
                | V::Clear { .. }
                | V::Copy { .. }
                | V::Producer(P::Findings(_)) => true,
                // A copy entry's producer that did not prepare reports its
                // own findings, as a link's producer does (INPUT-10).
                V::Producer(P::Preparation(producer)) => {
                    return inheritance_issues(owner, producer)
                }
                V::Entry(S::Decode { error, .. })
                | V::Traversal(G::Host(SourceFailure::Admission(S::Decode { error, .. }))) => {
                    refused(error)
                }
                V::Entry(S::Dependencies(error)) => matches!(error, D::Invalid(_)),
                V::Traversal(
                    G::Unavailable { .. }
                    | G::InvalidParent { .. }
                    | G::Cycle { .. }
                    | G::Repeated { .. }
                    | G::Fragment { .. }
                    | G::Included { .. }
                    | G::CallerParent { .. }
                    | G::NotFragment { .. }
                    | G::Header { .. },
                ) => true,
                V::Traversal(G::Layer {
                    error: N::Invalid(_),
                    ..
                }) => true,
                _ => false,
            }
        }
        _ => false,
    };
    if !supported {
        return Err(IssueError::Original(owner));
    }
    // Typed cause selection precedes projection; only language findings are
    // kept, and any other refusal stays with its original owner.
    match diagnostics::inheritance_failure_ref(cause) {
        Ok(Rejection::Invalid(findings) | Rejection::Unsupported(findings))
            if findings.is_empty() =>
        {
            Err(invalid())
        }
        Ok(Rejection::Invalid(findings)) => Ok(findings),
        _ => Err(IssueError::Original(owner)),
    }
}

#[cfg(all(test, feature = "yaml"))]
mod resource_tests {
    use super::*;
    use yamaa_core::json::Value;

    #[test]
    fn retained_root_resource_and_semantic_limits_keep_their_public_boundary_facts() {
        use yamaa_core::{
            limits::Limits,
            schema::{NormalizationError, NormalizationResource},
        };
        let byte_limit = Limits::spec_bytes(Limits::SPEC_BYTES + 1).unwrap_err();
        for (cause, resource, limit, actual) in [
            (
                SourceError::Limit(byte_limit),
                "spec_bytes",
                Limits::SPEC_BYTES,
                Some(Limits::SPEC_BYTES + 1),
            ),
            (
                SourceError::Normalize(NormalizationError::Limit {
                    resource: NormalizationResource::Depth,
                    limit: Limits::SEMANTIC_DEPTH,
                }),
                "semantic_depth",
                Limits::SEMANTIC_DEPTH,
                None,
            ),
        ] {
            let error = Error::Project(file_project::Error::Environment(EnvironmentFailure::Root(
                cause,
            )));
            let mut context = crate::json!({
                "step":"environment", "code":"resource_limit", "resource":resource, "limit":limit,
            });
            if let Some(actual) = actual {
                context["actual"] = actual.into();
            }
            {
                let rows = failure(&error);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].phase(), "engine");
                assert_eq!(
                    rows[0].condition(),
                    yamaa_core::diagnostic::ConditionCode::ApplicationRejected
                        .definition()
                        .condition
                );
                assert_eq!(rows[0].rule(), None);
                assert_eq!(rows[0].spec_paths, ["$"]);
                assert_eq!(Value::Object(rows[0].context.clone()), context);
            }
        }
    }

    #[test]
    fn inventory_rejection_retains_the_preparation_stage_and_inventory_cause() {
        let inventory = || {
            crate::file_preparation::Error::Inventory(
                yamaa_core::submission_inventory::MetadataError::Shape,
            )
        };
        let original = file_application::rejected(Step::Prepare, Fault::Inventory);
        let project = failure(&Error::Project(file_project::Error::Document(inventory())));
        assert_eq!(project, original);
        assert_eq!(project.len(), 1);
        assert_eq!(
            Value::Object(project[0].context.clone()),
            crate::json!({"step":"prepare", "code":"inventory"})
        );
    }

    #[test]
    fn entry_fallbacks_keep_their_typed_cause_and_source_instead_of_a_catch_all() {
        use crate::{file_entry::Error as E, file_preparation::Error as D};
        let cases = [
            (
                E::Snapshot {
                    identity: "/study/input/dm.csv".into(),
                    error: crate::file_resources::Error::Changed,
                },
                crate::json!({
                    "step":"prepare", "code":"resource_changed", "source":"/study/input/dm.csv",
                }),
            ),
            (
                E::Document(D::Resource(crate::file_resources::Error::OutsideRoots)),
                crate::json!({"step":"prepare", "code":"resource_outside_roots"}),
            ),
            (
                E::Document(D::Inventory(
                    yamaa_core::submission_inventory::MetadataError::Shape,
                )),
                crate::json!({"step":"prepare", "code":"inventory"}),
            ),
        ];
        for (error, context) in cases {
            let rows = entry_failure(&error);
            assert_eq!(rows.len(), 1, "{error:?}");
            assert_eq!(rows[0].phase(), "engine");
            assert_eq!(
                rows[0].condition(),
                yamaa_core::diagnostic::ConditionCode::ApplicationRejected
                    .definition()
                    .condition
            );
            assert_eq!(rows[0].rule(), None);
            assert_eq!(Value::Object(rows[0].context.clone()), context, "{error:?}");
        }
        // A classified link failure keeps its authored validation finding.
        let diagnostic = yamaa_core::resource::ResourceFailure::Missing
            .field_diagnostic("input.P.schema", "producer.yaml");
        let rows = entry_failure(&E::Document(D::ResourceFinding {
            error: crate::file_resources::Error::Missing,
            diagnostic: Box::new(diagnostic),
        }));
        let missing = yamaa_core::diagnostic::ConditionCode::ResourceMissing.definition();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].phase(), missing.phase);
        assert_eq!(rows[0].condition(), missing.condition);
        assert_eq!(rows[0].spec_paths, ["input.P.schema"]);
        assert_eq!(
            Value::Object(rows[0].context.clone()),
            crate::json!({"path":"producer.yaml"})
        );
    }

    #[test]
    fn layered_validation_depth_refusal_keeps_its_preparation_outcome_on_both_routes() {
        use yamaa_core::schema::ValidationError;
        // A parent layer whose validation exceeds the semantic depth.
        let error = crate::specification_source::InheritanceError::<crate::file_resources::Error>::Versioned {
            schema: crate::shipped_schema::capture_versioned(yamaa_core::language::FileType::Spec)
                .unwrap(),
            entry: "entry.yaml".into(),
            error: Box::new(crate::document::SpecificationError::Traversal(
                yamaa_core::inheritance::Error::Layer {
                    source: "parent.yaml".into(),
                    entry: "entry.yaml".into(),
                    input: crate::yaml_decode::decode_yaml(b"yamaa: '1.0'\n")
                        .unwrap()
                        .document,
                    error: yamaa_core::schema::NormalizationError::Validation(
                        ValidationError::Depth { limit: 128 },
                    ),
                },
            )),
        };
        let rejection = crate::specification_diagnostics::inheritance_failure_ref(&error).unwrap();
        assert!(matches!(rejection, Rejection::Limit(_)));
        let plain = rejection.into_issues();
        let project = preparation_boundary(&error);
        assert_eq!(plain, project);
        assert_eq!(plain.len(), 1);
        let issue = &plain[0];
        assert_eq!(issue.phase(), "engine");
        assert_eq!(
            issue.condition(),
            yamaa_core::diagnostic::ConditionCode::ApplicationRejected
                .definition()
                .condition
        );
        assert_eq!(issue.rule(), None);
        assert_eq!(
            Value::Object(issue.context.clone()),
            crate::json!({
                "code":"resource_limit", "step":"prepare", "phase":"validation", "resource":"semantic_depth", "limit":128,
            })
        );
    }
}
