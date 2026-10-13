//! Closed name/type carriers for retained repository fixture binding checks.
use crate::{issue_rows::Issue, specification_run_view::RunView};
use yamaa_engine::source_metadata::{self as application, Kind, SourceSchema};
pub use yamaa_engine::source_metadata::{Budget, Limits};
pub use yamaa_engine::source_metadata::{InputMetadata, Profile};

pub fn inputs(run: &dyn RunView) -> Result<Vec<InputMetadata>, Error> {
    let origins = run
        .compiled()
        .sources()
        .iter()
        .map(|source| {
            let original = run
                .document()
                .written_input_origin(&source.name, "path")
                .ok_or(Error::Application(application::Error::SourceIdentity))?;
            Ok(application::Origin {
                dataset: &source.name,
                declaring_source: original.declaring_source,
                written: original.written,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    application::inputs(
        &run.document().source().identity,
        run.compiled(),
        &origins,
        Limits::default(),
    )
    .map_err(Error::Application)
}

pub struct Source {
    pub owner: Option<String>,
    pub dataset: String,
    pub columns: Vec<(String, String)>,
}
#[derive(Debug)]
pub enum Error {
    Carrier,
    Application(application::Error),
    Projection,
}
impl Error {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Carrier => "invalid source metadata type carrier",
            Self::Application(application::Error::Limit) => "source metadata limit",
            Self::Application(application::Error::SourceIdentity) => {
                "source metadata does not match its retained specification"
            }
            Self::Application(_) => "source metadata admission refused",
            Self::Projection => "source metadata diagnostic projection refused",
        }
    }
}

pub fn check(run: &dyn RunView, sources: &[Source]) -> Result<Vec<Issue>, Error> {
    if sources.iter().any(|source| {
        source
            .owner
            .as_deref()
            .is_some_and(|identity| identity != run.document().source().identity)
    }) {
        return Err(Error::Application(application::Error::SourceIdentity));
    }
    let sources = admit(sources)?;
    let findings = application::check(run.compiled(), &sources, Limits::default())
        .map_err(Error::Application)?;
    project(findings)
}

fn admit(sources: &[Source]) -> Result<Vec<SourceSchema>, Error> {
    let limits = Limits::default();
    let mut budget = Budget::new(sources.len(), limits).map_err(Error::Application)?;
    for source in sources {
        if let Some(owner) = &source.owner {
            budget.text(owner.len()).map_err(Error::Application)?;
        }
        budget
            .text(source.dataset.len())
            .map_err(Error::Application)?;
        budget
            .columns(source.columns.len())
            .map_err(Error::Application)?;
        for (name, kind) in &source.columns {
            budget.text(name.len()).map_err(Error::Application)?;
            budget.text(kind.len()).map_err(Error::Application)?;
            if !matches!(kind.as_str(), "str" | "int" | "float" | "date" | "datetime") {
                return Err(Error::Carrier);
            }
        }
    }
    let sources = sources
        .iter()
        .map(|source| {
            let columns = source
                .columns
                .iter()
                .map(|(name, kind)| yamaa_core::table::Column {
                    name: name.clone(),
                    kind: match kind.as_str() {
                        "str" => Kind::Str,
                        "int" => Kind::Int,
                        "float" => Kind::Float,
                        "date" => Kind::Date,
                        "datetime" => Kind::DateTime,
                        _ => unreachable!("admitted closed type carrier"),
                    },
                })
                .collect();
            SourceSchema::new(source.dataset.clone(), columns).map_err(Error::Application)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(sources)
}

fn project(findings: Vec<yamaa_core::diagnostic::Diagnostic>) -> Result<Vec<Issue>, Error> {
    let issues = findings
        .into_iter()
        .map(|finding| Issue::from_core(finding).map_err(|_| Error::Projection))
        .collect::<Result<Vec<_>, _>>()?;
    if !crate::issue_rows::within_limit(&issues, crate::specification_check::MAX_ISSUE_BYTES) {
        return Err(Error::Projection);
    }
    Ok(issues)
}

pub fn graph_inputs(graph: &crate::file_graph::FileGraph) -> Result<Vec<InputMetadata>, Error> {
    let prepared = graph.graph().checked().metadata();
    let mut origins = Vec::new();
    for (node, supplied) in prepared.nodes().iter().zip(graph.graph().nodes()) {
        for source in node.sources() {
            if node.producers().iter().any(|p| p.dataset() == source.name) {
                continue;
            }
            let original = supplied
                .document
                .written_input_origin(&source.name, "path")
                .ok_or(Error::Application(application::Error::SourceIdentity))?;
            origins.push(application::GraphOrigin {
                owner: node.identity(),
                origin: application::Origin {
                    dataset: &source.name,
                    declaring_source: original.declaring_source,
                    written: original.written,
                },
            });
        }
    }
    application::graph_inputs(prepared, &origins, Limits::default()).map_err(Error::Application)
}

pub fn graph_check(
    graph: &crate::file_graph::FileGraph,
    sources: &[Source],
) -> Result<Vec<Issue>, Error> {
    if sources.iter().any(|source| source.owner.is_none()) {
        return Err(Error::Application(application::Error::SourceIdentity));
    }
    let admitted = admit(sources)?;
    let external = sources
        .iter()
        .zip(admitted)
        .map(|(source, admitted)| application::GraphSourceSchema {
            owner: source
                .owner
                .as_ref()
                .expect("admitted graph identity")
                .clone(),
            source: admitted,
        })
        .collect::<Vec<_>>();
    let metadata = graph.graph().checked().metadata();
    let findings = application::graph_check(metadata, &external, Limits::default())
        .map_err(Error::Application)?;
    let findings = findings
        .into_iter()
        .map(|(index, mut finding)| {
            let identity = metadata.nodes()[index].identity();
            if index != metadata.root() {
                finding.context.insert(
                    "source_identity".into(),
                    yamaa_core::diagnostic::ContextValue::Scalar(yamaa_core::value::Value::Str(
                        identity.into(),
                    )),
                );
            }
            finding
        })
        .collect();
    project(findings)
}

pub fn selected_inputs(
    selected: &crate::file_workflow::Selected,
) -> Result<Vec<InputMetadata>, Error> {
    use yamaa_engine::build::Prepared;
    match selected {
        Prepared::Plain(owner) => inputs(owner.run()),
        Prepared::Ordinary(owner) => inputs(owner.run()),
        Prepared::Producer(owner) => graph_inputs(owner),
    }
}

pub fn selected_check(
    selected: &crate::file_workflow::Selected,
    sources: &[Source],
) -> Result<Vec<Issue>, Error> {
    use yamaa_engine::build::Prepared;
    match selected {
        Prepared::Plain(owner) => check(owner.run(), sources),
        Prepared::Ordinary(owner) => check(owner.run(), sources),
        Prepared::Producer(owner) => graph_check(owner, sources),
    }
}

pub(crate) fn capture(
    resources: &mut crate::file_resources::Resources,
    descriptors: &[InputMetadata],
    index: usize,
    maximum: usize,
) -> Result<std::sync::Arc<[u8]>, Vec<Issue>> {
    application::admit_capture(index, descriptors.len(), maximum)
        .map_err(|_| crate::file_application::rejected("source_metadata", "capture_admission"))?;
    let descriptor = &descriptors[index];
    resources
        .capture_from(&descriptor.declaring_source, &descriptor.written, maximum)
        .map(|(bytes, _)| bytes)
        .map_err(|error| {
            use yamaa_engine::specification_run::SourcePort;
            match resources.resource_failure(&error) {
                Some(failure) => vec![Issue::from_core(
                    failure.diagnostic(&descriptor.dataset, &descriptor.written),
                )
                .expect("native resource issue")],
                None => crate::file_application::resource_failure("source_metadata", &error),
            }
        })
}

pub fn selected_capture(
    selected: &mut crate::file_workflow::Selected,
    index: usize,
    maximum: usize,
) -> Result<std::sync::Arc<[u8]>, Vec<Issue>> {
    use yamaa_engine::build::Prepared;
    match selected {
        Prepared::Plain(owner) => owner.capture_input_metadata(index, maximum),
        Prepared::Ordinary(owner) => owner.capture_input_metadata(index, maximum),
        Prepared::Producer(owner) => owner.capture_input_metadata(index, maximum),
    }
}

pub fn selected_reads(selected: &crate::file_workflow::Selected) -> usize {
    use yamaa_engine::build::Prepared;
    match selected {
        Prepared::Plain(owner) => owner.capture_reads(),
        Prepared::Ordinary(owner) => owner.capture_reads(),
        Prepared::Producer(owner) => owner.capture_reads(),
    }
}

/// Human attribution over the original held normalization provenance. This
/// borrows only diagnostic paths/source identities and never captures a file.
pub fn rejected_origins(
    error: &crate::file_workflow::SelectionError,
) -> Result<Vec<(String, String)>, Error> {
    use crate::{file_graph, file_preparation, file_workflow};
    let preparation = match error {
        file_workflow::SelectionError::Preparation(file_workflow::Error::Admission(error)) => {
            match error.error() {
                file_graph::Error::Document(file_preparation::Error::Preparation(error)) => {
                    Some(error)
                }
                _ => None,
            }
        }
        file_workflow::SelectionError::Preparation(file_workflow::Error::Graph(error)) => {
            match error.error() {
                file_graph::Error::Document(file_preparation::Error::Preparation(error)) => {
                    Some(error)
                }
                _ => None,
            }
        }
        file_workflow::SelectionError::Plain(error) => match error.error() {
            file_preparation::Error::Preparation(error) => Some(error),
            _ => None,
        },
        _ => None,
    };
    let Some(crate::specification_source::InheritanceError::Preparation { error, .. }) =
        preparation
    else {
        return Ok(Vec::new());
    };
    let origins = yamaa_engine::inheritance_preparation::diagnostic_origins(error);
    if origins.is_empty() {
        return Ok(Vec::new());
    }
    let limits = Limits::default();
    let mut budget = Budget::new(origins.len(), limits).map_err(Error::Application)?;
    for (path, source) in &origins {
        budget.text(path.len()).map_err(Error::Application)?;
        budget.text(source.len()).map_err(Error::Application)?;
    }
    Ok(origins
        .into_iter()
        .map(|(path, source)| (path.into(), source.into()))
        .collect())
}

pub fn selected_issues(selected: &crate::file_workflow::Selected) -> Result<Vec<Issue>, Error> {
    use yamaa_engine::build::Prepared;
    match selected {
        Prepared::Plain(owner) => {
            crate::specification_check::issue_rows(owner.run()).map_err(|_| Error::Projection)
        }
        Prepared::Ordinary(owner) => {
            crate::project_check::prepared(owner.run()).map_err(|_| Error::Projection)
        }
        Prepared::Producer(owner) => {
            crate::producer_check::prepared(owner).map_err(|_| Error::Projection)
        }
    }
}
