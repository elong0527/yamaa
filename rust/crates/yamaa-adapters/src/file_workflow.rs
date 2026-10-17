//! One original environment, entry and linked producing specifications under one
//! native capture scope. Preparation accepts no activation, data or publication port.
use crate::{
    file_entry::RejectedEntry,
    file_project::{self, FileProject, RejectedOrdinary},
    file_resources::Resources,
    specification_source::CapturedSchema,
};
use std::sync::Arc;
use yamaa_core::project_function::Language;

/// Repository metadata preparation uses the public application's declaring-only
/// resource policy. Explicit extra roots remain a separate private development
/// capability; the repository consumer supplies none and never retries a root.
pub fn repository_resources(
    project_root: &str,
    base_directory: &str,
    extra_roots: &[String],
) -> Result<Resources, crate::file_resources::Error> {
    if extra_roots.is_empty() {
        Resources::for_entry(base_directory)
    } else {
        Resources::new(project_root, base_directory, extra_roots)
    }
}

pub enum Error {
    Configuration(crate::file_configuration::Error),
    Schema(crate::specification_source::Error),
    Environment(Box<RejectedEnvironment>),
    Entry(Box<RejectedEntry>),
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
    /// Bounded owned aliases of held bytes; this performs no resource access.
    pub fn retained_sources(&self) -> crate::file_resources::RetainedSources {
        self.resources.retain_sources()
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
            Self::Entry(error) => f.debug_tuple("Entry").field(error).finish(),
        }
    }
}
struct Configured {
    resources: Resources,
    entry: String,
    environment: Option<String>,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
}
fn configure(specification: &str, environment: Option<&str>) -> Result<Configured, Error> {
    let (resources, entry) =
        crate::file_configuration::resources(specification).map_err(Error::Configuration)?;
    // Public caller paths retain the established absolute-path policy. This is
    // a lexical fact; the first environment file is opened only after admission.
    let specification_schema = crate::shipped_schema::capture().map_err(Error::Schema)?;
    let environment_schema = crate::shipped_schema::capture_environment().map_err(Error::Schema)?;
    let environment = match environment
        .map(crate::project_application::environment_path)
        .transpose()
    {
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

/// Explicit prepared capabilities from one original capture. Preparation takes
/// no activation, study-source or publication port; each capability keeps the
/// existing build/Save behavior.
pub type Selected =
    yamaa_core::build::Prepared<crate::file_preparation::FileSpecification, Box<FileProject>>;
#[derive(Debug)]
pub enum SelectionError {
    Preparation(Error),
    Ordinary(Box<RejectedOrdinary>),
    Plain(Box<crate::file_preparation::RejectedPlain>),
}
impl SelectionError {
    /// Bounded aliases of originally retained bytes; no file access or replay.
    pub fn retained_sources(&self) -> crate::file_resources::RetainedSources {
        match self {
            Self::Preparation(Error::Environment(error)) => error.retained_sources(),
            Self::Preparation(Error::Entry(error)) => error.retained_sources(),
            Self::Ordinary(error) => error.retained_sources(),
            Self::Plain(error) => error.retained_sources(),
            Self::Preparation(Error::Configuration(_) | Error::Schema(_)) => Vec::new(),
        }
    }
}

pub fn select(
    specification: &str,
    environment: &str,
    host: Language,
) -> Result<Selected, SelectionError> {
    select_request(
        yamaa_core::build::Request {
            specification,
            environment: Some(environment),
        },
        host,
    )
}

pub fn select_request(
    request: yamaa_core::build::Request<'_>,
    host: Language,
) -> Result<Selected, SelectionError> {
    yamaa_core::build::prepare(request, &mut Metadata { host })
}

/// Host authority replaces physical lookup while all metadata preparation and
/// plain/project selection use the same engine ports.
pub fn select_with_resolver(
    request: yamaa_core::build::Request<'_>,
    host: Language,
    resolver: Arc<dyn crate::host_resources::Resolver>,
) -> Result<Selected, SelectionError> {
    let specification_schema = crate::shipped_schema::capture()
        .map_err(|error| SelectionError::Preparation(Error::Schema(error)))?;
    let environment_schema = crate::shipped_schema::capture_environment()
        .map_err(|error| SelectionError::Preparation(Error::Schema(error)))?;
    select_with_optional_resources(
        Resources::with_resolver(resolver, request.specification),
        request.specification,
        request.environment,
        host,
        specification_schema,
        environment_schema,
    )
}

struct Metadata {
    host: Language,
}
impl yamaa_core::build::PreparationPort for Metadata {
    type Entry = crate::file_entry::CapturedEntry;
    type Plain = crate::file_preparation::FileSpecification;
    type Project = Box<FileProject>;
    type Failure = SelectionError;
    fn capture(
        &mut self,
        request: yamaa_core::build::Request<'_>,
    ) -> Result<Self::Entry, SelectionError> {
        let request = configure(request.specification, request.environment)
            .map_err(SelectionError::Preparation)?;
        let (resources, environment) = capture_environment(
            request.resources,
            &request.entry,
            request.environment.as_deref(),
            self.host,
            Arc::clone(&request.specification_schema),
            request.environment_schema,
        )
        .map_err(SelectionError::Preparation)?;
        crate::file_entry::capture(
            resources,
            &request.entry,
            environment,
            request.specification_schema,
        )
        .map_err(|error| SelectionError::Preparation(Error::Entry(error)))
    }
    fn plain(&mut self, entry: Self::Entry) -> Result<Self::Plain, SelectionError> {
        crate::file_preparation::FileSpecification::from_workflow(entry)
            .map_err(SelectionError::Plain)
    }
    fn project(&mut self, entry: Self::Entry) -> Result<Self::Project, SelectionError> {
        FileProject::from_workflow(entry)
            .map(Box::new)
            .map_err(SelectionError::Ordinary)
    }
}

pub fn select_with_resources(
    resources: Resources,
    entry: &str,
    environment: &str,
    host: Language,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
) -> Result<Selected, SelectionError> {
    select_with_optional_resources(
        resources,
        entry,
        Some(environment),
        host,
        specification_schema,
        environment_schema,
    )
}

pub fn select_with_optional_resources(
    resources: Resources,
    entry: &str,
    environment: Option<&str>,
    host: Language,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
) -> Result<Selected, SelectionError> {
    let has_environment = environment.is_some();
    let (resources, environment) = capture_environment(
        resources,
        entry,
        environment,
        host,
        Arc::clone(&specification_schema),
        environment_schema,
    )
    .map_err(SelectionError::Preparation)?;
    let entry = crate::file_entry::capture(resources, entry, environment, specification_schema)
        .map_err(|error| SelectionError::Preparation(Error::Entry(error)))?;
    yamaa_core::build::select(entry, has_environment, &mut Metadata { host })
}

fn capture_environment(
    mut resources: Resources,
    entry: &str,
    environment: Option<&str>,
    host: Language,
    specification_schema: Arc<CapturedSchema>,
    environment_schema: Arc<CapturedSchema>,
) -> Result<(Resources, crate::project_source::OwnedEnvironment), Error> {
    let owned_entry = entry.to_owned();
    let owned_environment = environment.map(str::to_owned);
    let Some(environment) = environment else {
        return Ok((
            resources,
            crate::project_source::OwnedEnvironment::without_environment(host),
        ));
    };
    match file_project::capture_environment(
        &mut resources,
        environment,
        host,
        Arc::clone(&environment_schema),
    ) {
        Ok(environment) => Ok((resources, environment)),
        Err(error) => Err(Error::Environment(Box::new(RejectedEnvironment {
            error,
            entry: Some(owned_entry),
            environment: owned_environment,
            specification_schema,
            environment_schema,
            resources,
        }))),
    }
}

#[cfg(all(test, feature = "yaml"))]
mod root_read_tests {
    use super::*;
    #[test]
    fn classified_unreadable_root_projects_without_new_captures_or_losing_its_owner() {
        let root =
            std::env::temp_dir().join(format!("yamaa-env-read-facts-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let _cleanup = Cleanup(root.clone());
        let entry = root
            .join("absent-entry.yaml")
            .to_string_lossy()
            .into_owned();
        let environment = root.join("environment.yaml").to_string_lossy().into_owned();
        let configured = configure(&entry, Some(&environment)).unwrap();
        let path = configured.environment.clone().unwrap();
        let rejection = Error::Environment(Box::new(RejectedEnvironment {
            error: file_project::Error::Resource(crate::file_resources::Error::Unreadable),
            entry: Some(configured.entry),
            environment: configured.environment,
            specification_schema: configured.specification_schema,
            environment_schema: configured.environment_schema,
            resources: configured.resources,
        }));
        let Error::Environment(owner) = &rejection else {
            unreachable!()
        };
        let original = owner.error() as *const file_project::Error;
        let issues =
            crate::project_application::preparation_issues(&rejection, Language::Python).unwrap();
        assert_eq!(issues.len(), 1);
        let definition =
            yamaa_core::diagnostic::ConditionCode::ProjectEnvironmentMissing.definition();
        assert_eq!(issues[0].condition, definition.condition);
        assert_eq!(issues[0].requirement.as_deref(), definition.requirement);
        assert_eq!(
            issues[0].context,
            crate::json!({"cause":"unreadable", "path":path})
        );

        assert_eq!(owner.error() as *const file_project::Error, original);
        assert_eq!(owner.capture_reads(), 0);
        assert_eq!(owner.captured_sources().count(), 0);
    }
}
