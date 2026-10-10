//! Public project preparation and static diagnostics over the approved resources.
//! Installed bindings supply host capabilities only after this shared admission.
use crate::{
    file_application::{self, Failure},
    file_project::{self, FileProject},
    project_source::{CaptureFailure, Failure as EnvironmentFailure},
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
    if environment.is_empty() || environment.len() > 65536 || environment.contains('\0') {
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
        Error::Configuration(_) => file_application::rejected("prepare", "project_configuration"),
        Error::Schema(_) => file_application::rejected("prepare", "shipped_schema"),
        Error::Project(file_project::Error::Resource(error)) => {
            file_application::resource_failure("environment", error)
        }
        Error::Project(file_project::Error::Document(error)) => match error {
            crate::file_preparation::Error::Resource(error) => {
                file_application::resource_failure("prepare", error)
            }
            crate::file_preparation::Error::Preparation(error) => {
                crate::specification_diagnostics::inheritance_failure_ref(error)
                    .map(file_application::preparation_failure)
                    .unwrap_or_else(|_| refused("prepare"))
            }
            crate::file_preparation::Error::Compile(_) => refused("prepare"),
        },
        Error::Project(file_project::Error::Compile(run)) => match &run.rejection.error {
            PrepareError::Invalid(_) => {
                crate::project_check::rejected(run).unwrap_or_else(|_| refused("check"))
            }
            PrepareError::Unsupported(features) => features
                .iter()
                .flat_map(|feature| {
                    file_application::unsupported(&feature.operation, Some(&feature.path))
                })
                .collect(),
            PrepareError::Limit(_) => file_application::rejected("check", "compilation_limit"),
            _ => file_application::rejected("check", "compilation_boundary"),
        },
        Error::Project(file_project::Error::Environment(error)) => environment_failure(error, host),
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
                Err(_) => return refused("environment"),
            };
            for projection in [
                diagnostics::scalar_sources(rejected),
                diagnostics::schema_sources(rejected),
            ] {
                match projection {
                    Ok(rows) => issues.extend(rows),
                    Err(_) => return refused("environment"),
                }
            }
            for source in &rejected.sources {
                match &source.error {
                    CaptureFailure::Port(error) | CaptureFailure::Interrupted(error) => {
                        issues.extend(file_application::resource_failure("environment", error))
                    }
                    CaptureFailure::Structural(SourceError::Findings(_))
                    | CaptureFailure::Scalar { .. } => (),
                    _ => issues.extend(file_application::rejected(
                        "environment",
                        "metadata_boundary",
                    )),
                }
            }
            if !crate::issue_rows::within_limit(
                &issues,
                crate::specification_check::MAX_ISSUE_BYTES,
            ) {
                return refused("environment");
            }
            Ok(issues)
        }
        EnvironmentFailure::Interrupted { error, .. } => {
            return file_application::resource_failure("environment", error)
        }
        EnvironmentFailure::Root(_) => {
            return file_application::rejected("environment", "environment_boundary")
        }
    };
    projected.unwrap_or_else(|_| refused("environment"))
}
pub fn check(specification: &str, environment: &str, host: Language) -> Failure {
    match crate::file_workflow::prepare(specification, environment, host) {
        Ok(graph) => crate::producer_check::prepared(&graph).unwrap_or_else(|_| refused("check")),
        Err(error) => {
            crate::producer_check::rejected(&error, host).unwrap_or_else(|error| match error {
                crate::producer_check::Error::Original(
                    crate::file_workflow::Error::Configuration(error),
                ) => match error {
                    crate::file_configuration::Error::Path(error) => {
                        file_application::resource_failure("prepare", error)
                    }
                    _ => file_application::rejected("prepare", "project_configuration"),
                },
                crate::producer_check::Error::Original(crate::file_workflow::Error::Schema(_)) => {
                    file_application::rejected("prepare", "shipped_schema")
                }
                crate::producer_check::Error::Original(error) => workflow_failure(error),
                crate::producer_check::Error::Projection(_) => refused("check"),
            })
        }
    }
}
fn workflow_failure(error: &crate::file_workflow::Error) -> Failure {
    use crate::{file_graph::Error as G, file_preparation::Error as D, file_workflow::Error as W};
    match error {
        W::Environment(environment) => match environment.error() {
            file_project::Error::Resource(error) => {
                file_application::resource_failure("environment", error)
            }
            file_project::Error::Environment(EnvironmentFailure::Interrupted { error, .. }) => {
                file_application::resource_failure("environment", error)
            }
            _ => file_application::rejected("environment", "metadata_boundary"),
        },
        W::Graph(graph) => match graph.error() {
            G::Resource(error) | G::Document(D::Resource(error)) => {
                file_application::resource_failure("prepare", error)
            }
            G::Graph(crate::producer_graph::Error::Graph(
                yamaa_core::producer_graph::Error::Compilation {
                    error: PrepareError::Limit(_),
                    ..
                },
            )) => file_application::rejected("check", "compilation_limit"),
            _ => file_application::rejected("check", "metadata_boundary"),
        },
        _ => file_application::rejected("prepare", "metadata_boundary"),
    }
}
