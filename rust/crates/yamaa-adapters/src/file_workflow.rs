//! One original environment and recursive metadata closure under one native
//! capture scope. Preparation accepts no activation, data or publication port.
use crate::{
    file_graph::{FileGraph, RejectedGraph},
    file_project,
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
            Self::Graph(error) => f.debug_tuple("Graph").field(error).finish(),
        }
    }
}
pub fn prepare(specification: &str, environment: &str, host: Language) -> Result<FileGraph, Error> {
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
    prepare_with_resources(
        resources,
        &entry,
        &environment,
        host,
        specification_schema,
        environment_schema,
        Default::default(),
    )
}

pub fn prepare_with_resources(
    mut resources: Resources,
    entry: &str,
    environment: &str,
    host: Language,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
    limits: Limits,
) -> Result<FileGraph, Error> {
    let mut owned_entry = None;
    let mut owned_environment = None;
    let result = (|| {
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
        owned_entry = Some(entry.to_owned());
        owned_environment = Some(environment.to_owned());
        file_project::capture_environment(
            &mut resources,
            environment,
            host,
            Arc::clone(&environment_schema),
        )
    })();
    match result {
        Ok(environment) => {
            FileGraph::prepare(resources, entry, environment, specification_schema, limits)
                .map_err(Error::Graph)
        }
        Err(error) => Err(Error::Environment(Box::new(RejectedEnvironment {
            error,
            entry: owned_entry,
            environment: owned_environment,
            specification_schema,
            environment_schema,
            resources,
        }))),
    }
}
