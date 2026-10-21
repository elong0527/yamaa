//! One original environment, entry and linked producing specifications under one
//! native capture scope. Preparation accepts no activation, data or publication port.
use crate::{
    file_entry::RejectedEntry,
    file_preparation::{FileSpecification, Rejected},
    file_project,
    file_resources::Resources,
    project_source::ProjectSchemas,
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
    /// A path the caller passed, the specification's or the project file's,
    /// that cannot be read: `path` as given, and why (FILE-7, FILE-8,
    /// FILE-11). Nothing was captured before it.
    Caller {
        path: String,
        error: crate::file_resources::Error,
    },
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
    project_schemas: ProjectSchemas,
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
    pub fn project_schemas(&self) -> &ProjectSchemas {
        &self.project_schemas
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
            Self::Caller { path, error } => f
                .debug_struct("Caller")
                .field("path", path)
                .field("error", error)
                .finish(),
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
    project_schemas: ProjectSchemas,
}
/// The refusal of the caller's own path `given`.
fn caller(given: &str, error: crate::file_resources::Error) -> Error {
    Error::Caller {
        path: given.to_owned(),
        error,
    }
}
fn configure(specification: &str, environment: Option<&str>) -> Result<Configured, Error> {
    let (mut resources, entry) = crate::file_configuration::resources(specification)
        .map_err(|crate::file_configuration::Error::Path(error)| caller(specification, error))?;
    // Public caller paths retain the established absolute-path policy. This is
    // a lexical fact; the first environment file is opened only after admission.
    let specification_schema = crate::shipped_schema::capture().map_err(Error::Schema)?;
    let project_schemas = crate::shipped_schema::capture_project().map_err(Error::Schema)?;
    let environment = environment
        .map(|given| {
            crate::project_application::environment_path(given)
                .map_err(|error| caller(given, error))
        })
        .transpose()?;
    if let Some(environment) = environment.as_deref() {
        resources.select_caller_parent(environment);
    }
    Ok(Configured {
        resources,
        entry,
        environment,
        specification_schema,
        project_schemas,
    })
}

/// One project file captured for project operations: the resources that read
/// it, its admitted project and environment documents, and the specification
/// schema that prepares the specifications its sections list.
pub struct CapturedProject {
    pub(crate) resources: Resources,
    pub(crate) environment: crate::project_source::OwnedEnvironment,
    pub(crate) schema: Arc<CapturedSchema>,
}

/// Capture a project file with the path policy, schemas and environment
/// admission that a specification build applies to the same project file.
/// A caller's resolver serves the project file and everything it declares
/// under the names it is given; without one, the local file resolver reads
/// the caller path. Without a host language, no `environments` entry is read.
pub fn capture_project(
    project: &str,
    host: Option<Language>,
    resolver: Option<Arc<dyn crate::host_resources::Resolver>>,
) -> Result<CapturedProject, Error> {
    let request = match resolver {
        Some(resolver) => Configured {
            resources: Resources::with_resolver(resolver, project),
            entry: project.to_owned(),
            environment: Some(project.to_owned()),
            specification_schema: crate::shipped_schema::capture().map_err(Error::Schema)?,
            project_schemas: crate::shipped_schema::capture_project().map_err(Error::Schema)?,
        },
        None => configure(project, Some(project))?,
    };
    let (resources, environment) = capture_environment(
        request.resources,
        &request.entry,
        request
            .environment
            .as_deref()
            .map(|resolved| (project, resolved)),
        host,
        Arc::clone(&request.specification_schema),
        request.project_schemas,
    )?;
    Ok(CapturedProject {
        resources,
        environment,
        schema: request.specification_schema,
    })
}

/// The one prepared capability from one original capture, with or without a
/// project. Preparation takes no activation, study-source or publication
/// port; the capability keeps the existing build/Save behavior.
pub type Selected = FileSpecification;
#[derive(Debug)]
pub enum SelectionError {
    Preparation(Error),
    Compile(Box<Rejected>),
}
impl SelectionError {
    /// Bounded aliases of originally retained bytes; no file access or replay.
    pub fn retained_sources(&self) -> crate::file_resources::RetainedSources {
        match self {
            Self::Preparation(Error::Environment(error)) => error.retained_sources(),
            Self::Preparation(Error::Entry(error)) => error.retained_sources(),
            Self::Compile(error) => error.retained_sources(),
            Self::Preparation(Error::Caller { .. } | Error::Schema(_)) => Vec::new(),
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
        Some(host),
    )
}

/// Capture the request's project, when it names one, and its entry through
/// the local file resolver, then compile. Without a host language, no
/// `environments` entry is read.
pub fn select_request(
    request: yamaa_core::build::Request<'_>,
    host: Option<Language>,
) -> Result<Selected, SelectionError> {
    let given = request;
    let request =
        configure(given.specification, given.environment).map_err(SelectionError::Preparation)?;
    let (resources, environment) = capture_environment(
        request.resources,
        &request.entry,
        given.environment.zip(request.environment.as_deref()),
        host,
        Arc::clone(&request.specification_schema),
        request.project_schemas,
    )
    .map_err(SelectionError::Preparation)?;
    compile(
        resources,
        given.specification,
        &request.entry,
        environment,
        request.specification_schema,
    )
}

/// Host authority replaces physical lookup while all metadata preparation
/// uses the same engine ports.
pub fn select_with_resolver(
    request: yamaa_core::build::Request<'_>,
    host: Option<Language>,
    resolver: Arc<dyn crate::host_resources::Resolver>,
) -> Result<Selected, SelectionError> {
    let specification_schema = crate::shipped_schema::capture()
        .map_err(|error| SelectionError::Preparation(Error::Schema(error)))?;
    let project_schemas = crate::shipped_schema::capture_project()
        .map_err(|error| SelectionError::Preparation(Error::Schema(error)))?;
    select_with_optional_resources(
        Resources::with_resolver(resolver, request.specification),
        request.specification,
        request.environment,
        host,
        specification_schema,
        project_schemas,
    )
}

/// Capture the entry and the producing specifications its links name under
/// the captured environment, then compile. `entry` names the entry to
/// `resources`, and `given` is the path the caller passed.
fn compile(
    resources: Resources,
    given: &str,
    entry: &str,
    environment: crate::project_source::OwnedEnvironment,
    specification_schema: Arc<CapturedSchema>,
) -> Result<Selected, SelectionError> {
    let entry =
        crate::file_entry::capture(resources, given, entry, environment, specification_schema)
            .map_err(|error| SelectionError::Preparation(Error::Entry(error)))?;
    FileSpecification::from_workflow(entry).map_err(SelectionError::Compile)
}

pub fn select_with_resources(
    resources: Resources,
    entry: &str,
    environment: &str,
    host: Language,
    specification_schema: Arc<CapturedSchema>,
    project_schemas: ProjectSchemas,
) -> Result<Selected, SelectionError> {
    select_with_optional_resources(
        resources,
        entry,
        Some(environment),
        Some(host),
        specification_schema,
        project_schemas,
    )
}

pub fn select_with_optional_resources(
    resources: Resources,
    entry: &str,
    environment: Option<&str>,
    host: Option<Language>,
    specification_schema: Arc<CapturedSchema>,
    project_schemas: ProjectSchemas,
) -> Result<Selected, SelectionError> {
    let (resources, environment) = capture_environment(
        resources,
        entry,
        environment.map(|environment| (environment, environment)),
        host,
        Arc::clone(&specification_schema),
        project_schemas,
    )
    .map_err(SelectionError::Preparation)?;
    compile(resources, entry, entry, environment, specification_schema)
}

/// Capture the project file `environment` names, as the path the caller gave
/// and as `resources` name it, and what it declares. A project file that
/// cannot be read is the caller's own path (FILE-7, FILE-8, FILE-11).
fn capture_environment(
    mut resources: Resources,
    entry: &str,
    environment: Option<(&str, &str)>,
    host: Option<Language>,
    specification_schema: Arc<CapturedSchema>,
    project_schemas: ProjectSchemas,
) -> Result<(Resources, crate::project_source::OwnedEnvironment), Error> {
    let owned_entry = entry.to_owned();
    let Some((given, environment)) = environment else {
        return Ok((
            resources,
            crate::project_source::OwnedEnvironment::without_environment(host),
        ));
    };
    match file_project::capture_environment(
        &mut resources,
        environment,
        host,
        project_schemas.clone(),
    ) {
        Ok(environment) => Ok((resources, environment)),
        Err(file_project::Error::Resource(error)) => Err(caller(given, error)),
        Err(error) => Err(Error::Environment(Box::new(RejectedEnvironment {
            error,
            entry: Some(owned_entry),
            environment: Some(environment.to_owned()),
            specification_schema,
            project_schemas,
            resources,
        }))),
    }
}

#[cfg(all(test, feature = "yaml"))]
mod root_read_tests {
    use super::*;
    use crate::host_resources::{Failure, Reply, Request, ResourceKind};

    /// A resolver that serves nothing, reporting every name as `failure`.
    struct Refusing(Failure);
    impl crate::host_resources::Resolver for Refusing {
        fn request(&self, _: Request<'_>) -> Result<Reply, Failure> {
            Err(self.0)
        }
    }

    /// A project file that cannot be read is the caller's own path: named as
    /// given, before anything is captured, with its file rule's condition
    /// when the resolver reports a kind and the opaque refusal otherwise.
    #[test]
    fn an_unreadable_project_file_is_the_callers_own_path_before_any_capture() {
        for (failure, expected) in [
            (
                Failure::Missing,
                crate::json!({"path": "study/project.yaml"}),
            ),
            (
                Failure::Kind(ResourceKind::Junction),
                crate::json!({"path": "study/project.yaml"}),
            ),
            (
                Failure::Unreadable,
                crate::json!({"stage": "prepare", "code": "resource_unreadable"}),
            ),
        ] {
            let rejection = select_with_resolver(
                yamaa_core::build::Request {
                    specification: "study/dm.yaml",
                    environment: Some("study/project.yaml"),
                },
                Some(Language::Python),
                Arc::new(Refusing(failure)),
            )
            .err()
            .unwrap();
            let SelectionError::Preparation(Error::Caller { path, .. }) = &rejection else {
                panic!("the caller's own path: {rejection:?}")
            };
            assert_eq!(path, "study/project.yaml");
            assert!(rejection.retained_sources().is_empty());
            let issues = crate::project_application::selection_failure(&rejection);
            assert_eq!(issues.len(), 1);
            assert!(issues[0].spec_paths.is_empty());
            assert_eq!(
                yamaa_core::json::Value::Object(issues[0].context.clone()),
                expected,
                "{failure:?}"
            );
        }
    }
}
