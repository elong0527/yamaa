//! Native capture, file geometry, codecs, encoders and publication for the
//! engine Define service.
use crate::{
    file_configuration, file_preparation, file_project, file_publication,
    file_resources::{self, Resources},
    project_application,
    project_source::OwnedEnvironment,
    specification_source::PreparedDocument,
};
use std::path::Path;
use yamaa_core::{
    ast::Transport,
    diagnostic::Diagnostic,
    project_environment::Submission,
    project_function::Language,
    submission_composition::{Dataset, Finding, Study},
    table::TableAccess,
};

use yamaa_core::define::{self as service, Ports};
#[derive(Debug)]
pub enum Failure {
    Project(project_application::Error),
    Resource {
        path: String,
        written: String,
        error: file_resources::Error,
    },
    ClassifiedResource {
        error: file_resources::Error,
        diagnostic: Diagnostic,
    },
    Metadata(Vec<Finding>),
    Diagnostic(Vec<Diagnostic>),
    TargetAdmission {
        index: usize,
        error: file_publication::Error,
    },
    Publication {
        index: usize,
        error: file_publication::Error,
    },
}
pub use service::PublicationState;
/// Representation and original opaque failure owner for both public hosts.
pub struct Define {
    ports: NativePorts,
    application: service::Application<NativePorts>,
    failure: Option<Failure>,
    issues: Vec<Diagnostic>,
}
#[derive(Debug)]
pub struct SaveError;
impl Define {
    /// Compose `section`'s submission from the project file `environment`,
    /// reading it and everything it declares through `resolver`: the
    /// caller's, or the local file resolver without one. The caller selects
    /// a section the project declares (PROJECT-14).
    pub fn prepare(
        environment: &str,
        section: Submission,
        created: Option<&str>,
        installed: &str,
        host: Option<Language>,
        resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
    ) -> Self {
        let mut ports = NativePorts {
            resources: None,
            resolver,
            host,
            environment: None,
            environment_written: String::new(),
            specifications: Vec::new(),
        };
        let attempt = service::Application::prepare(
            &mut ports,
            service::Request {
                environment,
                section,
                created,
                installed,
            },
        );
        let mut result = Self {
            ports,
            application: attempt.application,
            failure: None,
            issues: Vec::new(),
        };
        if let Err(failure) = attempt.outcome {
            result.refuse(native_failure(failure));
        }
        result
    }
    fn refuse(&mut self, failure: Failure) {
        self.issues = crate::file_application::collect(issues(&failure));
        self.failure = Some(failure);
    }
    pub fn issues(&self) -> &[Diagnostic] {
        &self.issues
    }
    /// What locates the composition's issues (P-ISSUE-1): the project file
    /// and the files it reads, and each specification a section lists,
    /// linked from the listing that names it (`sdtm.specs[0]`).
    pub fn locator(&self) -> crate::issue_location::Locator {
        crate::issue_location::project(
            self.ports
                .environment
                .as_deref()
                .map(Into::into)
                .unwrap_or_default(),
            self.ports
                .specifications
                .iter()
                .map(|(path, _, document)| (path.as_str(), &**document)),
            &self
                .ports
                .resources
                .as_ref()
                .map(Resources::retain_metadata_sources)
                .unwrap_or_default(),
        )
    }
    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }
    pub fn artifacts(&self) -> impl ExactSizeIterator<Item = (&str, &[u8], PublicationState)> {
        self.application.artifacts()
    }
    pub fn capture_reads(&self) -> usize {
        self.ports
            .resources
            .as_ref()
            .map_or(0, Resources::capture_reads)
    }
    /// The files a save of this composition writes, in publication order:
    /// each transport file, then define.xml (JSON-14). `None` when the
    /// composition, or an earlier save of it, failed: it writes nothing, and
    /// the save stops at it (PROJECT-13).
    pub fn plan(&self) -> Option<Vec<crate::publication::File<'_>>> {
        if self.application.failed() {
            return None;
        }
        Some(
            self.application
                .files()
                .map(|(target, written, bytes)| crate::publication::File {
                    target,
                    written,
                    bytes,
                })
                .collect(),
        )
    }
    /// What the latest save did with this composition's files: `committed`
    /// replaced their targets, and, when the save stopped at one of them,
    /// which and why. A stop fails the composition, whose issues then say why;
    /// an earlier file that names the same target is this composition's own.
    pub fn record(
        &mut self,
        committed: usize,
        stopped: Option<(usize, crate::publication::Cause)>,
    ) {
        use crate::publication::Cause;
        let failure = stopped.map(|(index, cause)| match cause {
            Cause::Target(error) => service::Failure::TargetAdmission { index, error },
            Cause::Collision { earlier } => {
                let written = |position: usize| {
                    self.application
                        .artifacts()
                        .nth(position)
                        .map_or("", |(written, _, _)| written)
                };
                service::Failure::Metadata(vec![service::shared_target(
                    written(earlier),
                    written(index),
                )])
            }
            Cause::Stage(error) | Cause::Commit(error) => {
                service::Failure::Publication { index, error }
            }
            Cause::Unwind(_) => {
                service::Failure::Diagnostic(vec![service::publication_diagnostic(
                    index,
                    service::PublicationFailure::Publication,
                    "publication unwound".into(),
                )])
            }
        });
        self.application.record(committed, failure.as_ref());
        if let Some(failure) = failure {
            self.refuse(native_failure(failure));
        }
    }
    /// A save that stopped at this composition for `issues` of its own,
    /// before any of its files was written.
    pub fn refuse_save(&mut self, issues: Vec<Diagnostic>) {
        let failure = service::Failure::Diagnostic(issues);
        self.application.record(0, Some(&failure));
        self.refuse(native_failure(failure));
    }
    /// Save this composition alone, as an outcome of one member saves it.
    pub fn save(&mut self) -> Result<bool, SaveError> {
        self.save_with(|_| Ok(()))
    }
    /// [`Self::save`], calling `before` with each file's position before it
    /// is committed.
    pub(crate) fn save_with(
        &mut self,
        before: impl FnMut(usize) -> Result<(), file_publication::Error>,
    ) -> Result<bool, SaveError> {
        let saved = match self.plan() {
            Some(files) => {
                crate::publication::stage(&files).and_then(|staged| staged.commit_with(before))
            }
            None => return Err(SaveError),
        };
        match saved {
            Ok(paths) => {
                self.record(paths.len(), None);
                Ok(true)
            }
            Err(stop) => {
                self.record(stop.committed, Some((stop.file, stop.cause)));
                Err(SaveError)
            }
        }
    }
}
/// The current UTC time as a composition records it when `created` is
/// omitted (DEFINE-3), so that one submission can give every section the
/// same time; `None` when the system clock gives no time the calendar
/// admits, before 1970 or after the year 9999.
pub fn current_time() -> Option<String> {
    utc_seconds().and_then(yamaa_core::define::utc_time)
}

/// Whole seconds since the UTC epoch, from the system clock.
fn utc_seconds() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn native_failure(failure: service::Failure<Failure, file_publication::Error>) -> Failure {
    match failure {
        service::Failure::Port(failure) => failure,
        service::Failure::Metadata(findings) => Failure::Metadata(findings),
        service::Failure::Diagnostic(findings) => Failure::Diagnostic(findings),
        service::Failure::TargetAdmission { index, error } => {
            Failure::TargetAdmission { index, error }
        }
        service::Failure::Publication { index, error } => Failure::Publication { index, error },
    }
}
struct NativePorts {
    resources: Option<Resources>,
    /// The caller's resolver, which serves the project file under the name
    /// it is given; the local file resolver reads a caller path without one.
    resolver: Option<std::sync::Arc<dyn crate::host_resources::Resolver>>,
    /// The host language, or none for an engine that claims no host language.
    host: Option<Language>,
    environment: Option<std::sync::Arc<OwnedEnvironment>>,
    environment_written: String,
    specifications: Vec<(String, String, std::sync::Arc<PreparedDocument>)>,
}
impl NativePorts {
    fn resources(&self) -> &Resources {
        self.resources.as_ref().expect("selected resource owner")
    }
    fn resources_mut(&mut self) -> &mut Resources {
        self.resources.as_mut().expect("selected resource owner")
    }
    fn metadata_diagnostic(&self, identity: &str) -> Option<yamaa_core::diagnostic::Diagnostic> {
        use yamaa_core::resource::ResourceFailure as R;
        for (field, written, document) in &self.specifications {
            if let Some(diagnostic) = crate::changed_metadata::specification(
                field,
                written,
                document,
                identity,
                R::ContentChanged,
            ) {
                return Some(diagnostic);
            }
        }
        let environment = self.environment.as_ref()?;
        crate::changed_metadata::environment(
            &crate::changed_metadata::Environment {
                written: &self.environment_written,
                host: self.host,
                root: environment.root(),
                project: environment.project()?,
                environment_file: environment.environment_file(),
                lock: environment.lock(),
                captures: environment.captures(),
            },
            identity,
            R::ContentChanged,
        )
    }
}
impl Ports for NativePorts {
    type Environment = std::sync::Arc<OwnedEnvironment>;
    type Specification = std::sync::Arc<PreparedDocument>;
    type Error = Failure;
    type PublicationError = file_publication::Error;
    type Bytes = std::sync::Arc<[u8]>;
    type Parquet = crate::arrow_table::ArrowTable;
    type ParquetError = crate::parquet_source::Error;
    type Target = crate::publication::AnchoredTarget;
    type Publisher = crate::publication::Publisher;
    fn utc_seconds(&mut self) -> Option<u64> {
        utc_seconds()
    }
    fn capture_environment(&mut self, written: &str) -> Result<Self::Environment, Failure> {
        let (resources, entry) = match &self.resolver {
            Some(resolver) => (
                Resources::with_resolver(std::sync::Arc::clone(resolver), written),
                written.to_owned(),
            ),
            None => file_configuration::resources(written).map_err(|error| {
                Failure::Project(project_application::Error::Configuration(error))
            })?,
        };
        self.resources = Some(resources);
        self.environment_written = written.into();
        let schema = crate::shipped_schema::capture_project()
            .map_err(|error| Failure::Project(project_application::Error::Schema(error)))?;
        let host = self.host;
        let environment =
            file_project::capture_environment(self.resources_mut(), &entry, host, schema)
                .map_err(|error| Failure::Project(project_application::Error::Project(error)))?;
        let environment = std::sync::Arc::new(environment);
        self.environment = Some(std::sync::Arc::clone(&environment));
        Ok(environment)
    }
    fn environment(environment: &Self::Environment) -> service::EnvironmentView<'_> {
        service::EnvironmentView {
            project: environment
                .project()
                .expect("a captured environment holds its project"),
            catalogue: environment.environment().catalogue(),
            source: &environment.root().source().identity,
        }
    }
    fn capture_specification(
        &mut self,
        path: &str,
        declaring: &str,
        written: &str,
    ) -> Result<Self::Specification, Failure> {
        let document = file_preparation::prepare_document_from(
            self.resources_mut(),
            Some(declaring),
            written,
            None,
        )
        .map_err(|error| {
            Failure::Project(project_application::Error::Project(
                file_project::Error::Document(error.entry_field(path, written)),
            ))
        })?;
        let document = std::sync::Arc::new(document);
        self.specifications.push((
            path.into(),
            written.into(),
            std::sync::Arc::clone(&document),
        ));
        Ok(document)
    }
    fn specification(document: &Self::Specification) -> service::SpecificationView<'_> {
        use yamaa_core::schema::DocumentNode as N;
        let model = document.document();
        let inputs = model
            .field(model.root(), "input")
            .and_then(|node| match &model.nodes()[node] {
                N::Mapping(fields) => Some(fields),
                _ => None,
            })
            .into_iter()
            .flatten()
            .filter_map(|&(key, _)| match &model.nodes()[key] {
                N::Text(name) => Some(name.as_str()),
                _ => None,
            })
            .filter_map(|name| {
                document
                    .written_input_origin(name, "path")
                    .map(|origin| (name, origin))
            })
            .collect();
        service::SpecificationView {
            document: model,
            tree: document.tree(),
            source: &document.source().identity,
            output: document.written_output_origin(),
            report: document.written_output_field_origin("report"),
            inputs,
        }
    }
    fn location(&self, path: &str, declaring: &str, written: &str) -> Result<String, Failure> {
        self.resources()
            .location_from(declaring, written)
            .map_err(|error| resource(path, written, error))
    }
    fn transport_written(&self, define: &str, file: &str) -> String {
        // Written paths use `/` separators; the file replaces the final one.
        match define.rfind('/') {
            Some(separator) => format!("{}{file}", &define[..=separator]),
            None => file.into(),
        }
    }
    fn same_location(&self, left: &str, right: &str) -> bool {
        same_location(Path::new(left), Path::new(right))
    }
    fn package_reference(&self, target: &str, define: &str) -> Option<String> {
        package_reference(
            Path::new(target),
            Path::new(define).parent().expect("admitted file location"),
        )
    }
    fn define_name(&self, location: &str) -> String {
        Path::new(location)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("admitted define name")
            .into()
    }
    fn verify_metadata(&mut self, path: &str, source: &str) -> Result<(), Failure> {
        self.resources()
            .verify_captured()
            .map_err(|(identity, error)| {
                if error == file_resources::Error::Changed {
                    if let Some(diagnostic) = self.metadata_diagnostic(&identity) {
                        return Failure::ClassifiedResource { error, diagnostic };
                    }
                    // Keep the original cause opaque when no retained authored
                    // occurrence can supply its required diagnostic context.
                    return Failure::Project(project_application::Error::Project(
                        file_project::Error::Resource(error),
                    ));
                }
                resource(path, source, error)
            })
    }

    fn capture_published(
        &mut self,
        path: &str,
        declaring: &str,
        written: &str,
    ) -> Result<Self::Bytes, Failure> {
        self.resources_mut()
            .capture_from(declaring, written)
            .map(|(bytes, _)| bytes)
            .map_err(|error| resource(path, written, error))
    }
    fn decode_parquet(&mut self, bytes: &[u8]) -> Result<Self::Parquet, Self::ParquetError> {
        read_parquet(bytes)
    }
    fn read_report(
        &mut self,
        path: &str,
        declaring: &str,
        written: &str,
    ) -> Result<Option<yamaa_core::run_report::Recorded>, Failure> {
        let (bytes, _) = self
            .resources_mut()
            .capture_from(declaring, written)
            .map_err(|error| resource(path, written, error))?;
        Ok(crate::run_report::recorded(&bytes, written))
    }
    fn count_records(
        &mut self,
        source: &yamaa_core::specification::SourceDeclaration,
        bytes: &[u8],
    ) -> Option<usize> {
        crate::specification_run::decode_source(
            source,
            bytes,
            yamaa_core::specification::SourceInterpretation::Ordinary,
            None,
        )
        .ok()
        .map(|table| table.row_count())
    }
    fn render_define(&mut self, study: &Study) -> Result<Vec<u8>, Finding> {
        crate::submission_xml::render(study)
    }
    fn admit_transport<T: TableAccess>(
        &mut self,
        transport: Transport,
        study: &Study,
        dataset: &Dataset,
        table: &T,
    ) -> Result<(), Finding> {
        match transport {
            Transport::Xpt => crate::submission_xpt::admit(
                &crate::submission_xpt::document(study, dataset),
                dataset,
                table,
            ),
            // Dataset-JSON refuses no file as a whole; its writer bounds the
            // output bytes as it encodes.
            Transport::DatasetJson => Ok(()),
        }
    }
    fn render_transport<T: TableAccess>(
        &mut self,
        transport: Transport,
        study: &Study,
        dataset: &Dataset,
        table: &T,
        define: &str,
    ) -> Result<Vec<u8>, Finding> {
        match transport {
            Transport::Xpt => crate::submission_xpt::render(
                &crate::submission_xpt::document(study, dataset),
                table,
            ),
            Transport::DatasetJson => crate::submission_json::render(
                &crate::submission_json::document(study, dataset, define),
                table,
            ),
        }
    }
    fn target(&self, path: &str, declaring: &str, written: &str) -> Result<Self::Target, Failure> {
        self.resources()
            .publication_target_from(declaring, written)
            .map_err(|error| resource(path, written, error))
    }
    fn publisher(
        &mut self,
        target: &Self::Target,
        written: &str,
    ) -> Result<Self::Publisher, Self::PublicationError> {
        target.publisher(written)
    }
    fn same_target(
        &mut self,
        left: &Self::Publisher,
        right: &Self::Publisher,
    ) -> Result<bool, Self::PublicationError> {
        left.same_target(right)
    }
}
fn same_location(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_str()
            .zip(right.to_str())
            .is_some_and(|(left, right)| crate::windows_file::equal_name(left, right))
    }
    #[cfg(unix)]
    {
        left == right
    }
}
// Component comparison preserves literal report spellings while accepting the
// same Windows ordinal spelling policy as the descriptor-backed file adapter.
fn package_reference(target: &Path, parent: &Path) -> Option<String> {
    let mut components = target.components();
    for base in parent.components() {
        if !same_location(
            Path::new(components.next()?.as_os_str()),
            Path::new(base.as_os_str()),
        ) {
            return None;
        }
    }
    let relative = components.as_path().to_str()?;
    (!relative.is_empty()).then(|| relative.replace('\\', "/"))
}
fn resource(path: &str, written: &str, error: file_resources::Error) -> Failure {
    Failure::Resource {
        path: path.into(),
        written: written.into(),
        error,
    }
}
fn issues(failure: &Failure) -> Vec<Diagnostic> {
    match failure {
        Failure::Project(error) => project_application::failure(error),
        // Submission findings record no geometry of their own.
        Failure::Metadata(findings) => findings
            .iter()
            .map(|finding| Diagnostic {
                code: finding.code,
                spec_paths: vec![finding.path.clone()],
                context: finding.context.clone(),
                source_span: None,
                operand_route: None,
            })
            .collect(),
        Failure::Diagnostic(findings) => findings.clone(),
        Failure::ClassifiedResource { diagnostic, .. } => vec![diagnostic.clone()],
        Failure::Resource {
            path,
            written,
            error,
        } => {
            if let file_resources::Error::WrittenForm(failure) = error {
                return vec![failure.field_diagnostic(path, written)];
            }
            use file_resources::Error as E;
            use service::ResourceObservation as R;
            let observation = match error {
                E::Host(_) => R::Unreadable,
                E::Missing => R::Missing,
                E::Unreadable => R::Unreadable,
                E::NotRegularFile => R::NotRegularFile,
                E::InvalidRoot => R::InvalidRoot,
                E::InvalidBase => R::InvalidBase,
                E::InvalidPath | E::WrittenForm(_) => R::InvalidPath,
                E::InvalidLock => R::InvalidLock,
                E::OutsideRoots => R::OutsideRoots,
                E::Symlink => R::Symlink,
                E::Changed => R::Changed,
                E::Limit(error) => R::Limit(*error),
            };
            vec![service::resource_diagnostic(path, written, observation)]
        }
        Failure::TargetAdmission { index, error } => vec![service::publication_diagnostic(
            *index,
            service::PublicationFailure::TargetAdmission(
                crate::file_application::publication_fault(error),
            ),
            error.message(),
        )],
        Failure::Publication { index, error } => vec![service::publication_diagnostic(
            *index,
            service::PublicationFailure::Publication,
            error.message(),
        )],
    }
}
fn read_parquet(
    bytes: &[u8],
) -> Result<crate::arrow_table::ArrowTable, crate::parquet_source::Error> {
    crate::parquet_source::parse(bytes)
}

#[cfg(feature = "yaml")]
#[cfg(test)]
mod publication_failure_tests;
