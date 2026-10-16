//! Public project preparation and static diagnostics over the approved resources.
//! Installed bindings supply host capabilities only after this shared admission.
use crate::{
    file_application::{self, Failure},
    file_project::{self, FileProject},
    project_source::Failure as EnvironmentFailure,
    specification_source::Error as SourceError,
};
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
) -> Result<FileProject, Error> {
    let (resources, entry) =
        crate::file_configuration::resources(specification).map_err(Error::Configuration)?;
    let environment = environment_path(environment)
        .map_err(|error| Error::Project(file_project::Error::Resource(error)))?;
    FileProject::prepare(
        resources,
        &entry,
        &environment,
        host,
        crate::shipped_schema::capture().map_err(Error::Schema)?,
        crate::shipped_schema::capture_environment().map_err(Error::Schema)?,
    )
    .map_err(Error::Project)
}
pub(crate) fn environment_path(environment: &str) -> Result<String, crate::file_resources::Error> {
    use crate::file_resources::Error;
    if environment.is_empty() || environment.contains('\0') {
        return Err(Error::InvalidPath);
    }
    let environment = std::path::absolute(environment).map_err(|_| Error::InvalidPath)?;
    crate::file_resources::caller_path(&environment)
}
fn refused(stage: &str) -> Failure {
    file_application::rejected(stage, "issue_projection")
}
pub fn failure(error: &Error, host: Language) -> Failure {
    match error {
        Error::Configuration(crate::file_configuration::Error::Path(error)) => {
            file_application::resource_failure("prepare", error)
        }
        Error::Schema(_) => file_application::rejected("prepare", "shipped_schema"),
        Error::Project(error) => project_failure(error, host),
    }
}
fn project_failure(error: &file_project::Error, host: Language) -> Failure {
    match error {
        file_project::Error::Resource(error) => {
            file_application::resource_failure("environment", error)
        }
        file_project::Error::Document(error) => match error {
            crate::file_preparation::Error::Resource(error) => {
                file_application::resource_failure("prepare", error)
            }
            crate::file_preparation::Error::ResourceFinding { diagnostic, .. }
            | crate::file_preparation::Error::PreparationFinding { diagnostic, .. } => {
                file_application::classified_resource(diagnostic)
            }
            crate::file_preparation::Error::Preparation(error) => {
                crate::specification_diagnostics::inheritance_failure_ref(error)
                    .map(file_application::preparation_failure)
                    .unwrap_or_else(|_| refused("prepare"))
            }
            crate::file_preparation::Error::Compile(
                error @ crate::specification_run::Error::Prepare(
                    PrepareError::Limit(_) | PrepareError::Numeric { .. },
                ),
            ) => file_application::preparation_failure(crate::specification_diagnostics::failure(
                error, None,
            )),
            crate::file_preparation::Error::Compile(_) => refused("prepare"),
            crate::file_preparation::Error::Inventory(_) => {
                file_application::rejected("prepare", "inventory")
            }
        },
        file_project::Error::Compile(run) => match &run.rejection.error {
            PrepareError::Invalid(_) => {
                crate::project_check::rejected(run).unwrap_or_else(|_| refused("check"))
            }
            PrepareError::Unsupported(features) => features
                .iter()
                .flat_map(|feature| {
                    file_application::unsupported(&feature.operation, Some(&feature.path))
                })
                .collect(),
            error @ (PrepareError::Limit(_) | PrepareError::Numeric { .. }) => {
                crate::specification_diagnostics::compiler_failure(error)
            }
            _ => file_application::rejected("check", "compilation_boundary"),
        },
        file_project::Error::Environment(error) => environment_failure(error, host),
    }
}
fn environment_failure(
    error: &EnvironmentFailure<crate::file_resources::Error>,
    host: Language,
) -> Failure {
    use crate::project_environment_diagnostics as diagnostics;
    let projected = match error {
        EnvironmentFailure::Root(SourceError::Findings(captured)) => {
            diagnostics::schema_root(captured)
        }
        EnvironmentFailure::RootScalar { root, findings } => {
            diagnostics::scalar_root_with_host(root, findings, host)
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
            return file_application::resource_failure("environment", error)
        }
        EnvironmentFailure::Root(error @ SourceError::Decode { identity, .. }) => {
            diagnostics::root_boundary(error, identity)
        }
        EnvironmentFailure::Root(error) => {
            if let Some(error) = diagnostics::root_limit(error) {
                return environment_projection_failure(error);
            }
            return file_application::rejected("environment", "environment_boundary");
        }
    };
    projected.unwrap_or_else(environment_projection_failure)
}
fn environment_projection_failure(error: crate::project_environment_diagnostics::Error) -> Failure {
    match error {
        crate::project_environment_diagnostics::Error::ResourceLimit(refusal) => {
            refusal.issues("environment", "resource_limit")
        }
        _ => refused("environment"),
    }
}
pub fn check(specification: &str, environment: &str, host: Language) -> Failure {
    crate::issue_rows::collect(
        match crate::file_workflow::prepare(specification, environment, host) {
            Ok(graph) => {
                crate::producer_check::prepared(&graph).unwrap_or_else(|_| refused("check"))
            }
            Err(error) => workflow_issues(&error, host),
        },
    )
}
fn workflow_issues(error: &crate::file_workflow::Error, host: Language) -> Failure {
    crate::producer_check::rejected(error, host).unwrap_or_else(|error| match error {
        crate::producer_check::Error::Original(error) => workflow_failure(error, host),
        crate::producer_check::Error::Projection(_) => refused("check"),
        crate::producer_check::Error::ResourceLimit(refusal) => {
            refusal.issues("environment", "resource_limit")
        }
    })
}

/// Selected preparation retains the original owner in the caller. Diagnostic
/// projection is complete and bounded; no feature dispatch depends on failure.
pub fn selection_failure(error: &crate::file_workflow::SelectionError, host: Language) -> Failure {
    match error {
        crate::file_workflow::SelectionError::Plain(error) => match error.error() {
            crate::file_preparation::Error::Resource(error) => {
                file_application::resource_failure("prepare", error)
            }
            crate::file_preparation::Error::ResourceFinding { diagnostic, .. }
            | crate::file_preparation::Error::PreparationFinding { diagnostic, .. } => {
                file_application::classified_resource(diagnostic)
            }
            crate::file_preparation::Error::Preparation(error) => {
                crate::specification_diagnostics::inheritance_failure_ref(error)
                    .map(file_application::preparation_failure)
                    .unwrap_or_else(|_| refused("prepare"))
            }
            crate::file_preparation::Error::Inventory(_) => {
                file_application::rejected("prepare", "inventory")
            }
            crate::file_preparation::Error::Compile(error) => {
                file_application::preparation_failure(crate::specification_diagnostics::failure(
                    error, None,
                ))
            }
        },
        crate::file_workflow::SelectionError::Preparation(error) => workflow_issues(error, host),
        crate::file_workflow::SelectionError::Ordinary(error) => match &error.run().rejection.error
        {
            PrepareError::Invalid(_) => {
                crate::project_check::rejected(error.run()).unwrap_or_else(|_| refused("check"))
            }
            PrepareError::Unsupported(features) => features
                .iter()
                .flat_map(|feature| {
                    file_application::unsupported(&feature.operation, Some(&feature.path))
                })
                .collect(),
            error @ (PrepareError::Limit(_) | PrepareError::Numeric { .. }) => {
                crate::specification_diagnostics::compiler_failure(error)
            }
            _ => file_application::rejected("check", "compilation_boundary"),
        },
    }
}
fn workflow_failure(error: &crate::file_workflow::Error, host: Language) -> Failure {
    use crate::file_workflow::Error as W;
    match error {
        W::Configuration(crate::file_configuration::Error::Path(error)) => {
            file_application::resource_failure("prepare", error)
        }
        W::Schema(_) => file_application::rejected("prepare", "shipped_schema"),
        W::Environment(environment) => project_failure(environment.error(), host),
        W::Graph(graph) => graph_failure(graph.error(), |node| {
            graph
                .documents()
                .get(node)
                .map(|document| document.source().identity.as_str())
        }),
    }
}
/// Graph failures that the static projection does not own keep their typed
/// cause: resource and limit facts, or the named boundary and its source.
fn graph_failure<'a>(
    error: &crate::file_graph::Error,
    source: impl Fn(usize) -> Option<&'a str>,
) -> Failure {
    use crate::{
        file_graph::Error as G, file_preparation::Error as D, producer_admission::Error as P,
        producer_graph::Error as A,
    };
    use yamaa_core::producer_graph::Error as C;
    match error {
        G::Resource(error) | G::Document(D::Resource(error)) => {
            file_application::resource_failure("prepare", error)
        }
        G::Snapshot { identity, error } => {
            file_application::resource_failure_from("prepare", error, identity)
        }
        G::Document(D::ResourceFinding { diagnostic, .. })
        | G::Document(D::PreparationFinding { diagnostic, .. }) => {
            file_application::classified_resource(diagnostic)
        }
        G::Document(D::Preparation(error)) => preparation_boundary(error),
        G::Document(D::Compile(
            error @ crate::specification_run::Error::Prepare(
                PrepareError::Limit(_) | PrepareError::Numeric { .. },
            ),
        )) => file_application::preparation_failure(crate::specification_diagnostics::failure(
            error, None,
        )),
        G::Document(D::Compile(_)) => refused("prepare"),
        G::Document(D::Inventory(_)) => file_application::rejected("prepare", "inventory"),
        G::Boundary(reason) => file_application::graph_boundary("prepare", reason, None),
        G::Graph(A::Boundary { node, reason }) => {
            file_application::graph_boundary("check", reason, source(*node))
        }
        G::Graph(A::Graph(C::Boundary { node, reason })) => {
            file_application::graph_boundary("check", reason, node.and_then(&source))
        }
        G::Graph(A::Metadata {
            node,
            error: P::Boundary,
        }) => file_application::graph_boundary("check", "producer_admission", source(*node)),
        G::Graph(A::Metadata {
            node,
            error: P::Admission(error),
        })
        | G::Graph(A::Graph(C::Admission { node, error })) => {
            admission_failure(error, source(*node))
        }
        G::Graph(A::Graph(C::Compilation { error, .. })) => compilation_failure(error),
        // A cycle is a semantic finding projected by the static preflight; only
        // a refused projection reaches this fallback.
        G::Graph(A::Graph(C::Cycle { .. })) => refused("check"),
    }
}
fn admission_failure(
    error: &yamaa_core::producer_admission::Error,
    source: Option<&str>,
) -> Failure {
    use yamaa_core::producer_admission::Error as E;
    match error {
        E::Limit(limit) => file_application::resource_limit_from("prepare", limit, source),
        E::Boundary => file_application::graph_boundary("check", "producer_admission", source),
        E::Compilation(error) => compilation_failure(error),
        // Admission findings are projected by the static preflight; only a
        // refused projection reaches this fallback.
        E::Invalid(_) => refused("check"),
    }
}
fn compilation_failure(error: &PrepareError) -> Failure {
    match error {
        PrepareError::Limit(_) | PrepareError::Numeric { .. } => {
            crate::specification_diagnostics::compiler_failure(error)
        }
        _ => file_application::rejected("check", "compilation_boundary"),
    }
}
// Semantic findings have already passed the complete producer_check preflight.
// These entry failures encode fixed boundary codes without cloning diagnostics,
// source bytes or an inherited arena. Opaque transport causes remain retained.
fn preparation_boundary(
    error: &crate::specification_source::InheritanceError<crate::file_resources::Error>,
) -> Failure {
    use crate::specification_source::{Error as S, InheritanceError as I};
    let I::Entry(source) = error else {
        let Ok(message) = crate::specification_diagnostics::inheritance_failure_ref(error) else {
            return refused("prepare");
        };
        let Ok(value) = serde_json::from_str::<yamaa_core::json::Value>(&message) else {
            return refused("prepare");
        };
        return if value["outcome"]["status"] == "resource_limit" {
            file_application::preparation_failure(message)
        } else {
            refused("prepare")
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
            return refused("prepare");
        }
    }
    crate::specification_diagnostics::inheritance_failure_ref(error)
        .map(file_application::preparation_failure)
        .unwrap_or_else(|_| refused("prepare"))
}

#[cfg(test)]
mod resource_tests {
    use super::*;

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
            let mut context = yamaa_core::json::json!({
                "stage":"environment", "code":"resource_limit", "resource":resource, "limit":limit,
            });
            if let Some(actual) = actual {
                context["actual"] = actual.into();
            }
            for host in [Language::Python, Language::R] {
                let rows = failure(&error, host);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].phase, "engine");
                assert_eq!(
                    rows[0].condition,
                    yamaa_core::diagnostic::ConditionCode::ApplicationRejected
                        .definition()
                        .condition
                );
                assert_eq!(rows[0].requirement, None);
                assert_eq!(rows[0].spec_paths, ["$"]);
                assert_eq!(rows[0].context, context.to_string());
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
        let original = file_application::preparation_failure(inventory().into_message());
        let project = failure(
            &Error::Project(file_project::Error::Document(inventory())),
            Language::Python,
        );
        assert_eq!(project, original);
        assert_eq!(project.len(), 1);
        assert_eq!(
            serde_json::from_str::<yamaa_core::json::Value>(&project[0].context).unwrap(),
            yamaa_core::json::json!({"stage":"prepare", "code":"inventory"})
        );
    }

    #[test]
    fn graph_fallbacks_keep_their_typed_cause_and_source_instead_of_a_catch_all() {
        use crate::{
            file_graph::Error as G, producer_admission::Error as P, producer_graph::Error as A,
        };
        use yamaa_core::{producer_admission::Error as Admission, producer_graph::Error as C};
        let documents = ["/study/adsl.yaml", "/study/producer.yaml"];
        let limit = || yamaa_core::limits::Limits::inputs(17).unwrap_err();
        let cases = [
            (
                G::Snapshot {
                    identity: "/study/input/dm.csv".into(),
                    error: crate::file_resources::Error::Changed,
                },
                yamaa_core::json::json!({
                    "stage":"prepare", "code":"resource_changed", "source":"/study/input/dm.csv",
                }),
            ),
            (
                G::Graph(A::Graph(C::Admission {
                    node: 1,
                    error: Admission::Limit(limit()),
                })),
                yamaa_core::json::json!({
                    "stage":"prepare", "code":"resource_limit", "resource":"inputs",
                    "limit":16, "actual":17, "source":"/study/producer.yaml",
                }),
            ),
            (
                G::Graph(A::Metadata {
                    node: 0,
                    error: P::Admission(Admission::Limit(limit())),
                }),
                yamaa_core::json::json!({
                    "stage":"prepare", "code":"resource_limit", "resource":"inputs",
                    "limit":16, "actual":17, "source":"/study/adsl.yaml",
                }),
            ),
            (
                G::Graph(A::Boundary {
                    node: 1,
                    reason: "contradictory_snapshot",
                }),
                yamaa_core::json::json!({
                    "stage":"check", "code":"graph_boundary", "reason":"contradictory_snapshot",
                    "source":"/study/producer.yaml",
                }),
            ),
            (
                G::Graph(A::Graph(C::Boundary {
                    node: None,
                    reason: "root",
                })),
                yamaa_core::json::json!({"stage":"check", "code":"graph_boundary", "reason":"root"}),
            ),
            (
                G::Graph(A::Metadata {
                    node: 0,
                    error: P::Boundary,
                }),
                yamaa_core::json::json!({
                    "stage":"check", "code":"graph_boundary", "reason":"producer_admission",
                    "source":"/study/adsl.yaml",
                }),
            ),
            (
                G::Boundary("input_origin"),
                yamaa_core::json::json!({
                    "stage":"prepare", "code":"graph_boundary", "reason":"input_origin",
                }),
            ),
        ];
        for (error, context) in cases {
            let rows = graph_failure(&error, |node| documents.get(node).copied());
            assert_eq!(rows.len(), 1, "{error:?}");
            assert_eq!(rows[0].phase, "engine");
            assert_eq!(
                rows[0].condition,
                yamaa_core::diagnostic::ConditionCode::ApplicationRejected
                    .definition()
                    .condition
            );
            assert_eq!(rows[0].requirement, None);
            assert_eq!(
                serde_json::from_str::<yamaa_core::json::Value>(&rows[0].context).unwrap(),
                context,
                "{error:?}"
            );
        }
    }

    #[test]
    fn layered_validation_depth_refusal_keeps_its_preparation_outcome_on_both_routes() {
        use yamaa_core::schema::ValidationError;
        let error = crate::specification_source::InheritanceError::<crate::file_resources::Error>::Preparation {
            schema: crate::shipped_schema::capture().unwrap(),
            error: Box::new(yamaa_core::inheritance_preparation::Error::Validation(
                ValidationError::Depth { limit: 128 },
            )),
            captured_parents: Vec::new(),
        };
        let plain = file_application::preparation_failure(
            crate::specification_diagnostics::inheritance_failure_ref(&error).unwrap(),
        );
        let project = preparation_boundary(&error);
        assert_eq!(plain, project);
        assert_eq!(plain.len(), 1);
        let issue = &plain[0];
        assert_eq!(issue.phase, "engine");
        assert_eq!(
            issue.condition,
            yamaa_core::diagnostic::ConditionCode::ApplicationRejected
                .definition()
                .condition
        );
        assert_eq!(issue.requirement, None);
        assert_eq!(
            serde_json::from_str::<yamaa_core::json::Value>(&issue.context).unwrap(),
            yamaa_core::json::json!({
                "code":"resource_limit", "stage":"prepare", "phase":"validation", "resource":"semantic_depth", "limit":128,
            })
        );
    }
}
