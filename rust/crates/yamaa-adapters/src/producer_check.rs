//! Complete static graph diagnostics borrowed from sealed metadata ownership.
//! Projection refusal retains the exact original preparation error beside it.
use crate::{file_graph::FileGraph, file_workflow, issue_rows::Issue};
use yamaa_core::json::Value as JsonValue;
use yamaa_core::{diagnostic::Diagnostic, specification::PrepareError};

#[derive(Debug)]
pub enum Error<'a> {
    Original(&'a file_workflow::Error),
    Projection(crate::specification_check::Error),
    ResourceLimit(crate::resource_diagnostics::Refusal),
}

fn invalid() -> Error<'static> {
    Error::Projection(crate::specification_check::Error::InvalidContext)
}
fn issue(
    mut diagnostic: Diagnostic,
    source: Option<(&str, &str)>,
) -> Result<Issue, Error<'static>> {
    if let Some((source, entry)) = source {
        for (key, text) in [("source", source), ("entry", entry)] {
            if diagnostic
                .context
                .insert(key.into(), JsonValue::String(text.into()))
                .is_some()
            {
                return Err(invalid());
            }
        }
    }
    Issue::from_core(diagnostic).map_err(|_| invalid())
}

pub fn prepared(graph: &FileGraph) -> Result<Vec<Issue>, Error<'_>> {
    let checked = graph.graph().metadata();
    let entry = graph.graph().nodes()[checked.root()]
        .document
        .source()
        .identity
        .as_str();
    let multiple = checked.nodes().len() > 1;
    checked
        .check_findings()
        .map(|(index, finding)| {
            let mut diagnostic = finding.diagnostic();
            let document = &graph.graph().nodes()[index].document;
            if multiple && document.inheritance().is_some() {
                let origins = diagnostic
                    .spec_paths
                    .iter()
                    .map(|path| match document.declaring_source(path) {
                        Some(source) => JsonValue::String(source.into()),
                        None => JsonValue::Null,
                    })
                    .collect();
                if diagnostic
                    .context
                    .insert("declaring_sources".into(), JsonValue::Array(origins))
                    .is_some()
                {
                    return Err(invalid());
                }
            }
            issue(
                diagnostic,
                multiple.then(|| {
                    (
                        graph.graph().nodes()[index]
                            .document
                            .source()
                            .identity
                            .as_str(),
                        entry,
                    )
                }),
            )
        })
        .collect()
}

enum Rejection<'a> {
    Diagnostics(&'a [Diagnostic]),
    Compile(&'a PrepareError),
}
fn admission(error: &yamaa_core::producer_admission::Error) -> Option<Rejection<'_>> {
    match error {
        yamaa_core::producer_admission::Error::Invalid(diagnostics) => {
            Some(Rejection::Diagnostics(diagnostics))
        }
        yamaa_core::producer_admission::Error::Compilation(error) => {
            Some(Rejection::Compile(error))
        }
        _ => None,
    }
}
fn graph_error(error: &crate::producer_graph::Error) -> Option<(usize, Rejection<'_>)> {
    use crate::producer_graph::Error as A;
    use yamaa_core::producer_graph::Error as C;
    match error {
        A::Graph(C::Admission { node, error }) => Some((*node, admission(error)?)),
        A::Graph(C::Compilation { node, error }) => Some((*node, Rejection::Compile(error))),
        A::Graph(C::Cycle { node, diagnostic }) => Some((
            *node,
            Rejection::Diagnostics(std::slice::from_ref(diagnostic)),
        )),
        A::Metadata {
            node,
            error: crate::producer_admission::Error::Admission(error),
        } => Some((*node, admission(error)?)),
        _ => None,
    }
}
pub fn rejected(
    error: &file_workflow::Error,
    host: yamaa_core::project_function::Language,
) -> Result<Vec<Issue>, Error<'_>> {
    if let file_workflow::Error::Environment(environment) = error {
        return environment_issues(error, environment.error(), host);
    }
    let file_workflow::Error::Graph(graph) = error else {
        return Err(Error::Original(error));
    };
    if let crate::file_graph::Error::Document(crate::file_preparation::Error::Preparation(cause)) =
        graph.error()
    {
        return inheritance_issues(error, cause);
    }
    let crate::file_graph::Error::Graph(cause) = graph.error() else {
        return Err(Error::Original(error));
    };
    let Some((index, rejection)) = graph_error(cause) else {
        return Err(Error::Original(error));
    };
    let source = graph
        .documents()
        .get(index)
        .ok_or_else(invalid)?
        .source()
        .identity
        .as_str();
    let entry = graph
        .documents()
        .first()
        .ok_or_else(invalid)?
        .source()
        .identity
        .as_str();
    let provenance = (graph.documents().len() > 1).then_some((source, entry));
    match rejection {
        Rejection::Diagnostics(diagnostics) => {
            if diagnostics.is_empty() {
                return Err(invalid());
            }
            diagnostics
                .iter()
                .map(|diagnostic| issue(diagnostic.clone(), provenance))
                .collect()
        }
        Rejection::Compile(PrepareError::Invalid(findings)) => {
            if findings.is_empty() {
                return Err(invalid());
            }
            findings
                .iter()
                .map(|finding| issue(finding.diagnostic(), provenance))
                .collect()
        }
        Rejection::Compile(PrepareError::Unsupported(features)) => {
            if features.is_empty() {
                return Err(invalid());
            }
            features
                .iter()
                .map(|feature| {
                    issue(
                        yamaa_core::application_issue::unsupported(
                            &feature.operation,
                            Some(&feature.path),
                        ),
                        provenance,
                    )
                })
                .collect()
        }
        _ => Err(Error::Original(error)),
    }
}

fn environment_issues<'a>(
    owner: &'a file_workflow::Error,
    cause: &crate::file_project::Error,
    host: yamaa_core::project_function::Language,
) -> Result<Vec<Issue>, Error<'a>> {
    use crate::project_environment_diagnostics as D;
    use crate::project_source::Failure as F;
    use crate::specification_source::Error as S;
    fn projection(error: D::Error) -> Error<'static> {
        match error {
            D::Error::ResourceLimit(refusal) => Error::ResourceLimit(refusal),
            _ => invalid(),
        }
    }
    if let crate::file_project::Error::Resource(resource) = cause {
        use yamaa_core::project_environment_diagnostics::ReadFailure;
        let file_workflow::Error::Environment(environment) = owner else {
            return Err(invalid());
        };
        let path = environment.environment().ok_or_else(invalid)?;
        let failure = match resource {
            crate::file_resources::Error::Missing => ReadFailure::Missing,
            crate::file_resources::Error::Unreadable => ReadFailure::Unreadable,
            _ => return D::root_resource(resource, path).map_err(projection),
        };
        return Ok(vec![issue(failure.diagnostic(path), None)?]);
    }
    let crate::file_project::Error::Environment(cause) = cause else {
        return Err(Error::Original(owner));
    };
    let mut issues = Vec::new();
    match cause {
        F::Root(S::Findings(captured)) => {
            issues.extend(D::schema_root(captured).map_err(|_| invalid())?)
        }
        F::Root(error) => {
            let file_workflow::Error::Environment(environment) = owner else {
                return Err(invalid());
            };
            let written = environment.environment().ok_or_else(invalid)?;
            issues.extend(D::root_boundary(error, written).map_err(projection)?);
        }
        F::RootScalar { root, findings } => {
            issues.extend(D::scalar_root_with_host(root, findings, host).map_err(|_| invalid())?)
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
        _ => return Err(Error::Original(owner)),
    }
    Ok(issues)
}

fn inheritance_issues<'a>(
    owner: &'a file_workflow::Error,
    cause: &crate::specification_source::InheritanceError<crate::file_resources::Error>,
) -> Result<Vec<Issue>, Error<'a>> {
    use crate::specification_source::{Error as S, InheritanceError as I};
    use crate::yaml_decode::DecodeFailure;
    use yamaa_core::{
        dictionary_preparation::Cause,
        inheritance::Error as G,
        inheritance_preparation::Error as E,
        schema::{
            InheritanceDependencyError as D, InheritanceReferenceError as R,
            NormalizationError as N,
        },
    };
    let supported = match cause {
        I::Entry(S::Findings(_)) => true,
        I::Entry(S::Decode { error, .. }) => matches!(
            error,
            DecodeFailure::NonAscii(_)
                | DecodeFailure::InvalidYaml { .. }
                | DecodeFailure::InvalidText(_)
        ),
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
        I::Preparation { error, .. } => match error.as_ref() {
            E::Traversal(
                G::Unavailable { .. }
                | G::InvalidParent { .. }
                | G::Cycle { .. }
                | G::Version { .. },
            ) => true,
            E::Traversal(G::Layer {
                error: N::Invalid(_),
                ..
            }) => true,
            E::Normalize(failure) => matches!(&failure.error, N::Invalid(_)),
            E::Model(_) => true,
            E::Composition(failure) => matches!(&failure.error, N::Invalid(_)),
            E::Dependencies(failure) => matches!(
                &failure.error,
                D::Normalization(N::Invalid(_))
                    | D::Reference(R::Normalization(N::Invalid(_)))
                    | D::Invalid(_)
            ),
            _ => false,
        },
        _ => false,
    };
    if !supported {
        return Err(Error::Original(owner));
    }
    let encoded = crate::specification_diagnostics::inheritance_failure_ref(cause)
        .map_err(|_| Error::Original(owner))?;
    // Typed cause selection precedes projection; this protocol check validates
    // completeness, never chooses a preparation/execution route from error text.
    let value: yamaa_core::json::Value = serde_json::from_str(&encoded).map_err(|_| invalid())?;
    let outcome = &value["outcome"];
    if outcome["status"] != "invalid" {
        return Err(Error::Original(owner));
    }
    let rows = outcome["diagnostics"].as_array().ok_or_else(invalid)?;
    if rows.is_empty() {
        return Err(invalid());
    }
    rows.iter()
        .cloned()
        .map(|row| Issue::from_diagnostic(row).map_err(|_| invalid()))
        .collect()
}
