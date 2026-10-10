//! One original environment and recursive metadata closure under one native
//! capture scope. Preparation accepts no activation, data or publication port.
use crate::{
    file_graph::{FileGraph, RejectedGraph},
    file_project::{self, FileProject, RejectedOrdinary},
    file_resources::{MetadataLimits, Resources},
    producer_graph::Limits,
    specification_source::CapturedSchema,
};
use std::sync::Arc;
use yamaa_core::project_function::Language;

pub enum Error {
    Configuration(crate::file_configuration::Error),
    Schema(crate::specification_source::Error),
    Environment(Box<RejectedEnvironment>),
    Admission(Box<RejectedAdmission>),
    Graph(Box<RejectedGraph>),
}
/// Even an early root decode or metadata transport failure retains the original
/// raw captures and both schema capabilities. No diagnostic refusal consumes it.
pub struct RejectedEnvironment {
    error: file_project::Error,
    entry: Option<String>,
    environment: Option<String>,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
    resources: Resources,
}
/// Known entry/schema refusal before the first environment read. Original
/// resource captures and both schema capabilities remain owned without a
/// fabricated environment capture or graph document.
pub struct RejectedAdmission {
    error: crate::file_graph::Error,
    entry: Option<String>,
    environment: Option<String>,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
    resources: Resources,
}
impl std::fmt::Debug for RejectedAdmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedAdmission")
            .field("error", &self.error)
            .field("entry", &self.entry)
            .field("environment", &self.environment)
            .field("capture_reads", &self.capture_reads())
            .finish_non_exhaustive()
    }
}
impl RejectedAdmission {
    pub fn error(&self) -> &crate::file_graph::Error {
        &self.error
    }
    pub fn entry(&self) -> Option<&str> {
        self.entry.as_deref()
    }
    pub fn environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }
    pub fn specification_schema(&self) -> &Arc<CapturedSchema> {
        &self.specification_schema
    }
    pub fn environment_schema(&self) -> &Arc<CapturedSchema> {
        &self.environment_schema
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    pub fn retained_sources(
        &self,
        aliases: usize,
        text: usize,
    ) -> Result<crate::file_resources::RetainedSources, crate::file_resources::RetentionFailure>
    {
        self.resources.retain_sources(aliases, text)
    }
}
impl RejectedEnvironment {
    pub fn error(&self) -> &file_project::Error {
        &self.error
    }
    pub fn entry(&self) -> Option<&str> {
        self.entry.as_deref()
    }
    pub fn environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }
    pub fn specification_schema(&self) -> &Arc<CapturedSchema> {
        &self.specification_schema
    }
    pub fn environment_schema(&self) -> &Arc<CapturedSchema> {
        &self.environment_schema
    }
    pub fn captured_sources(&self) -> impl Iterator<Item = (String, &[u8])> {
        self.resources.captured_sources()
    }
    /// Bounded owned aliases of held bytes; this performs no resource access.
    pub fn retained_sources(
        &self,
        aliases: usize,
        text: usize,
    ) -> Result<crate::file_resources::RetainedSources, crate::file_resources::RetentionFailure>
    {
        self.resources.retain_sources(aliases, text)
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
}
impl std::fmt::Debug for RejectedEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedEnvironment")
            .field("error", &self.error)
            .field("entry", &self.entry)
            .field("environment", &self.environment)
            .field("capture_reads", &self.capture_reads())
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration(error) => f.debug_tuple("Configuration").field(error).finish(),
            Self::Schema(error) => f.debug_tuple("Schema").field(error).finish(),
            Self::Environment(error) => f.debug_tuple("Environment").field(error).finish(),
            Self::Admission(error) => f.debug_tuple("Admission").field(error).finish(),
            Self::Graph(error) => f.debug_tuple("Graph").field(error).finish(),
        }
    }
}
struct Configured {
    resources: Resources,
    entry: String,
    environment: String,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
}
fn configure(specification: &str, environment: &str) -> Result<Configured, Error> {
    let (resources, entry) =
        crate::file_configuration::resources(specification).map_err(Error::Configuration)?;
    // Public caller paths retain the established absolute-path policy. This is
    // a lexical fact; the first environment file is opened only after admission.
    let specification_schema = crate::shipped_schema::capture().map_err(Error::Schema)?;
    let environment_schema = crate::shipped_schema::capture_environment().map_err(Error::Schema)?;
    let environment = match crate::project_application::environment_path(environment) {
        Ok(environment) => environment,
        Err(error) => {
            return Err(Error::Environment(Box::new(RejectedEnvironment {
                error: file_project::Error::Resource(error),
                entry: None,
                environment: None,
                specification_schema,
                environment_schema,
                resources,
            })))
        }
    };
    Ok(Configured {
        resources,
        entry,
        environment,
        specification_schema,
        environment_schema,
    })
}

pub fn prepare(specification: &str, environment: &str, host: Language) -> Result<FileGraph, Error> {
    let request = configure(specification, environment)?;
    prepare_with_resources(
        request.resources,
        &request.entry,
        &request.environment,
        host,
        request.specification_schema,
        request.environment_schema,
        Default::default(),
    )
}

pub fn prepare_with_resources(
    resources: Resources,
    entry: &str,
    environment: &str,
    host: Language,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
    limits: Limits,
) -> Result<FileGraph, Error> {
    let (resources, environment, entry) = capture_environment(
        resources,
        entry,
        environment,
        host,
        Arc::clone(&specification_schema),
        environment_schema,
        limits,
    )?;
    FileGraph::capture_admitted_entry(resources, entry, environment)
        .and_then(FileGraph::prepare_captured)
        .map_err(Error::Graph)
}

/// Explicit prepared capabilities from one original capture. Preparation takes
/// no activation, study-source or publication port. The ordinary capability keeps
/// existing build/Save behavior; the static producer graph grants no publisher.
/// Installed public dispatch remains a separate integration boundary.
pub enum Selected {
    Ordinary(Box<FileProject>),
    Producer(Box<FileGraph>),
}
#[derive(Debug)]
pub enum SelectionError {
    Preparation(Error),
    Ordinary(Box<RejectedOrdinary>),
}

pub fn select(
    specification: &str,
    environment: &str,
    host: Language,
) -> Result<Selected, SelectionError> {
    let request = configure(specification, environment).map_err(SelectionError::Preparation)?;
    select_with_resources(
        request.resources,
        &request.entry,
        &request.environment,
        host,
        request.specification_schema,
        request.environment_schema,
        Default::default(),
    )
}

pub fn select_with_resources(
    resources: Resources,
    entry: &str,
    environment: &str,
    host: Language,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
    limits: Limits,
) -> Result<Selected, SelectionError> {
    let (resources, environment, entry) = capture_environment(
        resources,
        entry,
        environment,
        host,
        Arc::clone(&specification_schema),
        environment_schema,
        limits,
    )
    .map_err(SelectionError::Preparation)?;
    let entry = FileGraph::capture_admitted_entry(resources, entry, environment)
        .map_err(|error| SelectionError::Preparation(Error::Graph(error)))?;
    if entry.is_producer() {
        FileGraph::prepare_captured(entry)
            .map(|graph| Selected::Producer(Box::new(graph)))
            .map_err(|error| SelectionError::Preparation(Error::Graph(error)))
    } else {
        FileProject::from_workflow(entry)
            .map(|project| Selected::Ordinary(Box::new(project)))
            .map_err(SelectionError::Ordinary)
    }
}

fn capture_environment(
    mut resources: Resources,
    entry: &str,
    environment: &str,
    host: Language,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
    limits: Limits,
) -> Result<
    (
        Resources,
        crate::project_source::OwnedEnvironment,
        crate::file_graph::AdmittedEntry,
    ),
    Error,
> {
    let paths = (|| {
        resources
            .ensure_metadata_budget(MetadataLimits {
                bytes: limits.captured_bytes,
                snapshots: limits.snapshots,
                text: limits.graph.text_bytes,
                work: limits.graph.work,
            })
            .map_err(file_project::Error::Resource)?;
        resources
            .charge_metadata_text(entry.len().checked_add(environment.len()).ok_or(
                file_project::Error::Resource(crate::file_resources::Error::Limit),
            )?)
            .map_err(file_project::Error::Resource)?;
        Ok((entry.to_owned(), environment.to_owned()))
    })();
    let (owned_entry, owned_environment) = match paths {
        Ok(paths) => paths,
        Err(error) => {
            return Err(Error::Environment(Box::new(RejectedEnvironment {
                error,
                entry: None,
                environment: None,
                specification_schema,
                environment_schema,
                resources,
            })))
        }
    };
    // Known root/schema limits and its written identity ownership are admitted
    // before crossing the first environment capture boundary. The sealed token
    // is consumed later; neither selection nor graph collection charges it again.
    let admission = match crate::file_graph::admit_entry(
        &mut resources,
        entry,
        Arc::clone(&specification_schema),
        limits,
    ) {
        Ok(admitted) => admitted,
        Err(error) => {
            return Err(Error::Admission(Box::new(RejectedAdmission {
                error,
                entry: Some(owned_entry),
                environment: Some(owned_environment),
                specification_schema,
                environment_schema,
                resources,
            })))
        }
    };
    match file_project::capture_environment(
        &mut resources,
        environment,
        host,
        Arc::clone(&environment_schema),
    ) {
        Ok(environment) => Ok((resources, environment, admission)),
        Err(error) => Err(Error::Environment(Box::new(RejectedEnvironment {
            error,
            entry: Some(owned_entry),
            environment: Some(owned_environment),
            specification_schema,
            environment_schema,
            resources,
        }))),
    }
}
