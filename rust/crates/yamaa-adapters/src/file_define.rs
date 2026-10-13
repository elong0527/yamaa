//! Native capture, file geometry, codecs and publication for the engine Define service.
use crate::{
    file_configuration, file_preparation, file_project, file_publication,
    file_resources::{self, MetadataLimits, Resources},
    issue_rows::Issue,
    project_application,
    project_source::OwnedEnvironment,
    specification_source::PreparedDocument,
};
use std::path::Path;
use yamaa_core::{
    diagnostic::Diagnostic, project_function::Language, submission_composition::Finding,
};

use yamaa_engine::define::{self as service, Ports};
#[derive(Debug)]
pub enum Failure {
    Project(project_application::Error),
    Resource {
        path: String,
        written: String,
        error: file_resources::Error,
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
    issues: Vec<Issue>,
    host: Language,
}
#[derive(Debug)]
pub struct SaveError;
impl Define {
    pub fn prepare(
        environment: &str,
        standard: &str,
        created: Option<&str>,
        installed: &str,
        host: Language,
    ) -> Self {
        let mut ports = NativePorts {
            resources: None,
            host,
        };
        let attempt = service::Application::prepare(
            &mut ports,
            service::Request {
                environment,
                standard,
                created,
                installed,
            },
        );
        let mut result = Self {
            ports,
            application: attempt.application,
            failure: None,
            issues: Vec::new(),
            host,
        };
        if let Err(failure) = attempt.outcome {
            result.refuse(native_failure(failure));
        }
        result
    }
    fn refuse(&mut self, failure: Failure) {
        self.issues = issues(&failure, self.host);
        if !crate::issue_rows::within_limit(
            &self.issues,
            crate::specification_check::MAX_ISSUE_BYTES,
        ) {
            self.issues = crate::file_application::rejected("define", "issue_limit");
        }
        self.failure = Some(failure);
    }
    pub fn issues(&self) -> &[Issue] {
        &self.issues
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
    pub fn save(&mut self) -> Result<bool, SaveError> {
        let result = self.application.save(&mut self.ports);
        self.save_result(result)
    }
    #[cfg(test)]
    fn save_with_checkpoint(&mut self, checkpoint: impl FnMut(usize)) -> Result<bool, SaveError> {
        let result = self
            .application
            .save_with_checkpoint(&mut self.ports, checkpoint);
        self.save_result(result)
    }
    fn save_result(
        &mut self,
        result: Result<bool, service::SaveFailure<Failure, file_publication::Error>>,
    ) -> Result<bool, SaveError> {
        match result {
            Ok(saved) => Ok(saved),
            Err(service::SaveFailure::Refused) => Err(SaveError),
            Err(service::SaveFailure::Failure(failure)) => {
                self.refuse(native_failure(failure));
                Err(SaveError)
            }
        }
    }
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
    host: Language,
}
impl NativePorts {
    fn resources(&self) -> &Resources {
        self.resources.as_ref().expect("selected resource owner")
    }
    fn resources_mut(&mut self) -> &mut Resources {
        self.resources.as_mut().expect("selected resource owner")
    }
}
impl Ports for NativePorts {
    type Environment = OwnedEnvironment;
    type Specification = PreparedDocument;
    type Error = Failure;
    type PublicationError = file_publication::Error;
    type Bytes = std::sync::Arc<[u8]>;
    type Parquet = crate::arrow_table::ArrowTable;
    type ParquetError = crate::parquet_source::Error;
    type Target = file_publication::AnchoredTarget;
    type Publisher = file_publication::Publisher;
    fn utc_seconds(&mut self) -> Option<u64> {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs())
    }
    fn capture_environment(
        &mut self,
        written: &str,
        limits: service::MetadataLimits,
    ) -> Result<Self::Environment, Failure> {
        let (mut resources, entry) = file_configuration::resources(written)
            .map_err(|error| Failure::Project(project_application::Error::Configuration(error)))?;
        resources
            .admit_metadata_budget(MetadataLimits {
                bytes: limits.bytes,
                snapshots: limits.snapshots,
                text: limits.text,
                work: limits.work,
            })
            .map_err(|error| resource(written, written, error))?;
        self.resources = Some(resources);
        let schema = crate::shipped_schema::capture_environment()
            .map_err(|error| Failure::Project(project_application::Error::Schema(error)))?;
        let host = self.host;
        file_project::capture_environment(self.resources_mut(), &entry, host, schema)
            .map_err(|error| Failure::Project(project_application::Error::Project(error)))
    }
    fn environment(environment: &Self::Environment) -> service::EnvironmentView<'_> {
        service::EnvironmentView {
            document: &environment.root().normalized().document,
            catalogue: environment.environment().catalogue(),
            source: &environment.root().source().identity,
        }
    }
    fn capture_specification(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Self::Specification, Failure> {
        file_preparation::prepare_document_from(
            self.resources_mut(),
            Some(declaring),
            written,
            None,
        )
        .map_err(|error| {
            Failure::Project(project_application::Error::Project(
                file_project::Error::Document(error),
            ))
        })
    }
    fn specification(document: &Self::Specification) -> service::SpecificationView<'_> {
        service::SpecificationView {
            document: document.model().document(),
            source: &document.source().identity,
            output: document.written_output_origin(),
        }
    }
    fn location(&self, path: &str, declaring: &str, written: &str) -> Result<String, Failure> {
        self.resources()
            .location_from(declaring, written)
            .map_err(|error| resource(path, written, error))
    }
    fn json_written(&self, written: &str) -> String {
        Path::new(written)
            .with_extension("json")
            .to_string_lossy()
            .into_owned()
    }
    fn same_location(&self, left: &str, right: &str) -> bool {
        same_location(Path::new(left), Path::new(right))
    }
    fn same_parent(&self, left: &str, right: &str) -> bool {
        Path::new(left)
            .parent()
            .zip(Path::new(right).parent())
            .is_some_and(|(left, right)| same_location(left, right))
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
            .map_err(|(_, error)| resource(path, source, error))
    }
    fn finish_metadata(&mut self) {
        self.resources_mut().finish_metadata_budget();
    }
    fn capture_published(
        &mut self,
        path: &str,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<Self::Bytes, Failure> {
        self.resources_mut()
            .capture_from(declaring, written, maximum)
            .map(|(bytes, _)| bytes)
            .map_err(|error| resource(path, written, error))
    }
    fn decode_parquet(&mut self, bytes: &[u8]) -> Result<Self::Parquet, Self::ParquetError> {
        read_parquet(bytes)
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
    fn publish(
        &mut self,
        publisher: &mut Self::Publisher,
        written: &str,
        bytes: &[u8],
    ) -> Result<(), Self::PublicationError> {
        publisher.publish(written, bytes)
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
fn issues(failure: &Failure, host: Language) -> Vec<Issue> {
    match failure {
        Failure::Project(error) => project_application::failure(error, host),
        Failure::Metadata(findings) => findings
            .iter()
            .map(|f| {
                Issue::from_definition(f.definition(), vec![f.path.clone()], f.context.clone())
                    .expect("admitted shared submission finding")
            })
            .collect(),
        Failure::Diagnostic(findings) => findings
            .iter()
            .cloned()
            .map(|finding| Issue::from_core(finding).expect("admitted shared diagnostic"))
            .collect(),
        Failure::Resource {
            path,
            written,
            error,
        } => {
            use file_resources::Error as E;
            use service::ResourceObservation as R;
            let observation = match error {
                E::Missing => R::Missing,
                E::Unreadable => R::Unreadable,
                E::NotRegularFile => R::NotRegularFile,
                E::InvalidRoot => R::InvalidRoot,
                E::InvalidBase => R::InvalidBase,
                E::InvalidPath => R::InvalidPath,
                E::InvalidLock => R::InvalidLock,
                E::OutsideRoots => R::OutsideRoots,
                E::Symlink => R::Symlink,
                E::Changed => R::Changed,
                E::Limit => R::Limit,
            };
            vec![
                Issue::from_core(service::resource_diagnostic(path, written, observation))
                    .expect("admitted Define resource finding"),
            ]
        }
        Failure::TargetAdmission { index, error } => {
            vec![Issue::from_core(service::publication_diagnostic(
                *index,
                service::PublicationFailure::TargetAdmission,
                error.message(),
            ))
            .expect("admitted Define publication target finding")]
        }
        Failure::Publication { index, error } => {
            vec![Issue::from_core(service::publication_diagnostic(
                *index,
                service::PublicationFailure::Publication,
                error.message(),
            ))
            .expect("admitted Define publication finding")]
        }
    }
}
fn read_parquet(
    bytes: &[u8],
) -> Result<crate::arrow_table::ArrowTable, crate::parquet_source::Error> {
    use crate::parquet_source::{self, CompressionLimits, FramingLimits, MetadataLimits};
    let table_limits = crate::arrow_table::TableLimits {
        max_rows: 65_536,
        max_columns: 16_384,
        max_batches: 1024,
        max_cells: 16_777_216,
    };
    parquet_source::parse(
        bytes,
        parquet_source::Limits {
            framing: FramingLimits {
                source_bytes: 67_108_864,
                metadata_bytes: 2_097_152,
                metadata_nodes: 65_536,
                header_bytes: 65_536,
                header_nodes: 65_536,
                row_groups: 1024,
                columns: 16_384,
                rows: table_limits.max_rows,
                cells: table_limits.max_cells,
                pages: 65_536,
                page_bytes: 8_388_608,
                decoded_bytes: 67_108_864,
            },
            compression: CompressionLimits {
                page_bytes: 8_388_608,
                window_log: 24,
            },
            metadata: MetadataLimits {
                bytes: 2_097_152,
                tables: 65_536,
                depth: 64,
            },
            expanded_bytes: 67_108_864,
            retained_bytes: 67_108_864,
            array_elements: table_limits.max_cells,
            batches: table_limits.max_batches,
        },
        table_limits,
    )
}

#[cfg(test)]
#[path = "../tests/support/file_define_publication.rs"]
mod publication_failure_tests;
