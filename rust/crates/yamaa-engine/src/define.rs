//! Define preparation and one-shot publication over retained native snapshots.
//! This service owns admission, composition, rendering and effect order. Ports
//! observe file geometry and codecs without interpreting submission declarations.
use alloc::{
    borrow::ToOwned,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    producer_admission::WrittenPath,
    project_terminology::Catalogue,
    schema::Document,
    submission_composition::{self as composition, Finding},
    submission_document,
    table::TableAccess,
    value::Value,
};

pub const MAX_ARTIFACT_BYTES: usize = 67_108_864;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetadataLimits {
    pub bytes: usize,
    pub snapshots: usize,
    pub text: usize,
    pub work: usize,
}
pub const METADATA_LIMITS: MetadataLimits = MetadataLimits {
    bytes: 67_108_864,
    snapshots: 1024,
    text: 16_777_216,
    work: 268_435_456,
};
pub struct EnvironmentView<'a> {
    pub document: &'a Document,
    pub catalogue: &'a Catalogue,
    pub source: &'a str,
}
pub struct SpecificationView<'a> {
    pub document: &'a Document,
    pub source: &'a str,
    pub output: Option<WrittenPath<'a>>,
}
/// Capture errors are moved once into the attempt. No clone, retry, thread or
/// serialization requirement is imposed on the native payload.
pub trait Ports {
    type Environment;
    type Specification;
    type Error;
    type PublicationError;
    type Bytes: AsRef<[u8]>;
    type Parquet: TableAccess;
    type ParquetError;
    type Target;
    type Publisher;
    fn utc_seconds(&mut self) -> Option<u64>;
    fn capture_environment(
        &mut self,
        written: &str,
        limits: MetadataLimits,
    ) -> Result<Self::Environment, Self::Error>;
    fn environment(environment: &Self::Environment) -> EnvironmentView<'_>;
    fn capture_specification(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Self::Specification, Self::Error>;
    fn specification(document: &Self::Specification) -> SpecificationView<'_>;
    fn location(&self, path: &str, declaring: &str, written: &str) -> Result<String, Self::Error>;
    fn json_written(&self, written: &str) -> String;
    fn same_location(&self, left: &str, right: &str) -> bool;
    fn same_parent(&self, left: &str, right: &str) -> bool;
    fn package_reference(&self, target: &str, define: &str) -> Option<String>;
    fn define_name(&self, location: &str) -> String;
    fn verify_metadata(&mut self, path: &str, source: &str) -> Result<(), Self::Error>;
    fn finish_metadata(&mut self);
    fn capture_published(
        &mut self,
        path: &str,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<Self::Bytes, Self::Error>;
    fn decode_parquet(&mut self, bytes: &[u8]) -> Result<Self::Parquet, Self::ParquetError>;
    fn target(
        &self,
        path: &str,
        declaring: &str,
        written: &str,
    ) -> Result<Self::Target, Self::Error>;
    fn publisher(
        &mut self,
        target: &Self::Target,
        written: &str,
    ) -> Result<Self::Publisher, Self::PublicationError>;
    fn same_target(
        &mut self,
        left: &Self::Publisher,
        right: &Self::Publisher,
    ) -> Result<bool, Self::PublicationError>;
    fn publish(
        &mut self,
        publisher: &mut Self::Publisher,
        written: &str,
        bytes: &[u8],
    ) -> Result<(), Self::PublicationError>;
}
#[derive(Debug)]
pub enum Failure<E, PE> {
    Port(E),
    Metadata(Vec<Finding>),
    Diagnostic(Vec<Diagnostic>),
    TargetAdmission { index: usize, error: PE },
    Publication { index: usize, error: PE },
}
type PreflightResult<P> = Result<
    Vec<<P as Ports>::Publisher>,
    Failure<<P as Ports>::Error, <P as Ports>::PublicationError>,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationState {
    Ready,
    Published,
    Failed,
    Stopped,
}
struct Artifact<T> {
    written: String,
    target: T,
    bytes: Vec<u8>,
    state: PublicationState,
}
pub struct Request<'a> {
    pub environment: &'a str,
    pub standard: &'a str,
    pub created: Option<&'a str>,
    pub installed: &'a str,
}
/// The service retains the actual snapshot owners and admitted artifact prefix.
/// Callers may inspect bytes and state, but cannot replace documents or targets.
pub struct Application<P: Ports> {
    environment: Option<P::Environment>,
    specifications: Vec<P::Specification>,
    artifacts: Vec<Artifact<P::Target>>,
    failed: bool,
    saved: bool,
}
pub struct Attempt<P: Ports> {
    pub application: Application<P>,
    pub outcome: Result<(), Failure<P::Error, P::PublicationError>>,
}
pub enum SaveFailure<E, PE> {
    Refused,
    Failure(Failure<E, PE>),
}
impl<P: Ports> Application<P> {
    pub fn prepare(ports: &mut P, request: Request<'_>) -> Attempt<P> {
        let mut application = Self {
            environment: None,
            specifications: Vec::new(),
            artifacts: Vec::new(),
            failed: false,
            saved: false,
        };
        let outcome = application.prepare_inner(
            ports,
            request.environment,
            request.standard,
            request.created,
            request.installed,
        );
        if let Err(failure) = &outcome {
            application.refuse(failure);
        }
        Attempt {
            application,
            outcome,
        }
    }
    fn prepare_inner(
        &mut self,
        ports: &mut P,
        environment: &str,
        standard: &str,
        created: Option<&str>,
        installed: &str,
    ) -> Result<(), Failure<P::Error, P::PublicationError>> {
        let family = yamaa_core::define_diagnostics::standard_name(standard)
            .map_err(|finding| Failure::Metadata(vec![finding]))?;
        let created = created_time(created, || ports.utc_seconds())
            .map_err(|finding| Failure::Metadata(vec![finding]))?;
        self.environment = Some(
            ports
                .capture_environment(environment, METADATA_LIMITS)
                .map_err(Failure::Port)?,
        );
        let environment = P::environment(self.environment.as_ref().expect("captured environment"));
        let root = environment.document;
        let declarations = submission_document::decode(root, root.root(), Default::default())
            .map_err(|_| {
                Failure::Metadata(vec![Finding::cause(
                    ConditionCode::SubmissionMetadataMissing,
                    standard,
                    "invalid closed environment submission declarations",
                )])
            })?;
        let study =
            yamaa_core::define_diagnostics::required(declarations.study(), "study", "study")
                .map_err(|finding| Failure::Metadata(vec![finding]))?;
        let section = yamaa_core::define_diagnostics::required(
            declarations
                .sections()
                .iter()
                .find(|section| section.family == family),
            standard,
            standard,
        )
        .map_err(|finding| Failure::Metadata(vec![finding]))?;
        let identity = environment.source;
        let define_location = ports
            .location(
                &format!("{standard}.define"),
                identity,
                &section.define.value,
            )
            .map_err(Failure::Port)?;
        let mut locations = Vec::new();
        let mut hrefs = Vec::new();
        let mut outputs = Vec::new();
        let mut targets = vec![define_location.clone()];
        for (index, spec) in section.specs.value.iter().enumerate() {
            let path = format!("{standard}.specs[{index}]");
            let document = ports
                .capture_specification(identity, &spec.value)
                .map_err(Failure::Port)?;
            let view = P::specification(&document);
            let origin = view.output.ok_or_else(|| {
                Failure::Metadata(vec![Finding::cause(
                    ConditionCode::SubmissionMetadataMissing,
                    &path,
                    "output.path origin",
                )])
            })?;
            let artifact = ports
                .location(
                    &format!("{path}.output.path"),
                    origin.declaring_source,
                    origin.written,
                )
                .map_err(Failure::Port)?;
            let written_json = ports.json_written(origin.written);
            let reference = if section.dataset_json.value {
                ports
                    .location(
                        &format!("{path}.dataset_json"),
                        origin.declaring_source,
                        &written_json,
                    )
                    .map_err(Failure::Port)?
            } else {
                artifact.clone()
            };
            if section.dataset_json.value && !ports.same_parent(&reference, &define_location) {
                return Err(Failure::Metadata(vec![yamaa_core::define_diagnostics::admit_relation(
                    yamaa_core::define_diagnostics::TargetRelation::DatasetJsonOutsidePackage,
                    &path, &format!("Dataset-JSON {reference} and Define {define_location} require parent traversal in their mutual references"),
                ).unwrap_err()]));
            }
            if ports.same_location(&artifact, &define_location) {
                return Err(Failure::Metadata(vec![
                    yamaa_core::define_diagnostics::admit_relation(
                        yamaa_core::define_diagnostics::TargetRelation::ArtifactCollision,
                        &path,
                        "published dataset overlaps Define target",
                    )
                    .unwrap_err(),
                ]));
            }
            if section.dataset_json.value {
                if let Some(conflict) = targets
                    .iter()
                    .find(|target| ports.same_location(target, &reference))
                    .or_else(|| {
                        ports
                            .same_location(&reference, &artifact)
                            .then_some(&artifact)
                    })
                {
                    return Err(Failure::Metadata(vec![
                        yamaa_core::define_diagnostics::admit_relation(
                            yamaa_core::define_diagnostics::TargetRelation::DatasetJsonCollision,
                            &path,
                            &format!("{reference} and {conflict} name one publication target"),
                        )
                        .unwrap_err(),
                    ]));
                }
            }
            if section.dataset_json.value {
                targets.push(reference.clone());
            }
            let href = ports
                .package_reference(&reference, &define_location)
                .ok_or_else(|| {
                    Failure::Metadata(vec![yamaa_core::define_diagnostics::admit_relation(
                        yamaa_core::define_diagnostics::TargetRelation::ArtifactOutsidePackage,
                        &path,
                        "artifact must be inside the Define package",
                    )
                    .unwrap_err()])
                })?;
            locations.push(artifact);
            hrefs.push(href);
            outputs.push((
                origin.declaring_source.to_string(),
                origin.written.to_string(),
                written_json,
            ));
            self.specifications.push(document);
        }
        // The selected output set is admitted before any published artifact read.
        for artifact in &locations {
            if targets
                .iter()
                .any(|target| ports.same_location(target, artifact))
            {
                return Err(Failure::Metadata(vec![Finding::cause(
                    ConditionCode::DefineArtifactCollision,
                    standard,
                    "published dataset overlaps Define targets",
                )]));
            }
        }
        let inputs: Vec<_> = self
            .specifications
            .iter()
            .zip(&locations)
            .zip(&hrefs)
            .map(|((document, artifact), href)| composition::Input {
                document: P::specification(document).document,
                source: P::specification(document).source,
                artifact,
                href,
            })
            .collect();
        let composed = composition::compose(
            study,
            section,
            environment.catalogue,
            &inputs,
            installed,
            &created,
        )
        .map_err(Failure::Metadata)?;
        let xml = yamaa_core::submission_xml::render(&composed)
            .map_err(|f| Failure::Metadata(vec![f]))?;
        ports
            .verify_metadata(standard, identity)
            .map_err(Failure::Port)?;
        ports.finish_metadata();
        if section.dataset_json.value {
            let mut rendered_bytes = xml.len();
            for ((dataset, (declaring, written, json)), location) in
                composed.datasets().iter().zip(&outputs).zip(&locations)
            {
                let bytes = ports
                    .capture_published(
                        &format!("datasets.{}.artifact", dataset.id()),
                        declaring,
                        written,
                        MAX_ARTIFACT_BYTES,
                    )
                    .map_err(Failure::Port)?;
                if bytes.as_ref().len() > MAX_ARTIFACT_BYTES {
                    return Err(Failure::Metadata(vec![Finding::cause(
                        ConditionCode::ApplicationRejected,
                        written,
                        "published artifact byte limit",
                    )]));
                }
                let kinds: Vec<_> = dataset
                    .columns()
                    .iter()
                    .map(|column| {
                        (
                            column.declaration().name.value.clone(),
                            column.runtime_type(),
                        )
                    })
                    .collect();
                let rendered = if location.to_ascii_lowercase().ends_with(".csv") {
                    let source = yamaa_core::csv_source::parse(bytes.as_ref(), Default::default())
                        .map_err(|error| {
                            error
                                .diagnostic(dataset.id(), written)
                                .map(|diagnostic| Failure::Diagnostic(vec![diagnostic]))
                                .unwrap_or_else(|| {
                                    Failure::Metadata(vec![Finding::cause(
                                        ConditionCode::ApplicationRejected,
                                        written,
                                        "published CSV admission",
                                    )])
                                })
                        })?;
                    yamaa_core::submission_json::admit_fields(
                        dataset,
                        source.names.iter().map(String::as_str),
                    )
                    .map_err(|finding| Failure::Metadata(vec![finding]))?;
                    let table = yamaa_core::typed_csv::PreparedTypes::new(&kinds, 16_384)
                        .and_then(|types| types.convert(source))
                        .map_err(|error| {
                            error
                                .diagnostic(dataset.id())
                                .map(|diagnostic| Failure::Diagnostic(vec![diagnostic]))
                                .unwrap_or_else(|| {
                                    Failure::Metadata(vec![Finding::cause(
                                        ConditionCode::ProducerContractMismatch,
                                        written,
                                        "published CSV contract",
                                    )])
                                })
                        })?;
                    yamaa_core::submission_json::render(
                        &composed,
                        dataset,
                        &table,
                        &ports.define_name(&define_location),
                    )
                } else if location.to_ascii_lowercase().ends_with(".parquet") {
                    let table = ports.decode_parquet(bytes.as_ref()).map_err(|_| {
                        Failure::Metadata(vec![Finding::cause(
                            ConditionCode::ProducerContractMismatch,
                            written,
                            "published Parquet profile",
                        )])
                    })?;
                    yamaa_core::submission_json::render(
                        &composed,
                        dataset,
                        &table,
                        &ports.define_name(&define_location),
                    )
                } else {
                    return Err(Failure::Metadata(vec![Finding::cause(
                        ConditionCode::ApplicationUnsupported,
                        written,
                        "published_dataset_format",
                    )]));
                }
                .map_err(|f| Failure::Metadata(vec![f]))?;
                rendered_bytes = rendered_bytes
                    .checked_add(rendered.len())
                    .filter(|&n| n <= MAX_ARTIFACT_BYTES)
                    .ok_or_else(|| {
                        Failure::Metadata(vec![Finding::cause(
                            ConditionCode::ApplicationRejected,
                            standard,
                            "complete_publication_byte_limit",
                        )])
                    })?;
                let target = ports.target(json, declaring, json).map_err(Failure::Port)?;
                self.artifacts.push(Artifact {
                    written: json.clone(),
                    target,
                    bytes: rendered,
                    state: PublicationState::Ready,
                });
            }
        }
        let target = ports
            .target(
                &format!("{standard}.define"),
                identity,
                &section.define.value,
            )
            .map_err(Failure::Port)?;
        self.artifacts.push(Artifact {
            written: section.define.value.clone(),
            target,
            bytes: xml,
            state: PublicationState::Ready,
        });
        // Preflight all outputs after complete admission/rendering. Recheck them
        // together on Save; no retained metadata or artifact is reopened there.
        self.preflight(ports)?;
        Ok(())
    }
    fn preflight(&self, ports: &mut P) -> PreflightResult<P> {
        let mut publishers: Vec<P::Publisher> = Vec::with_capacity(self.artifacts.len());
        for (index, artifact) in self.artifacts.iter().enumerate() {
            let publisher = ports
                .publisher(&artifact.target, &artifact.written)
                .map_err(|error| Failure::TargetAdmission { index, error })?;
            for (previous_index, previous) in publishers.iter().enumerate() {
                if ports
                    .same_target(&publisher, previous)
                    .map_err(|error| Failure::TargetAdmission { index, error })?
                {
                    return Err(Failure::Metadata(vec![
                        yamaa_core::define_diagnostics::admit_relation(
                            yamaa_core::define_diagnostics::TargetRelation::DatasetJsonCollision,
                            "define.targets",
                            &format!(
                                "{} and {} name one physical publication target",
                                self.artifacts[previous_index].written, artifact.written
                            ),
                        )
                        .unwrap_err(),
                    ]));
                }
            }
            publishers.push(publisher);
        }
        Ok(publishers)
    }
    fn refuse(&mut self, failure: &Failure<P::Error, P::PublicationError>) {
        let failed = match failure {
            Failure::TargetAdmission { index, .. } | Failure::Publication { index, .. } => {
                Some(*index)
            }
            _ => None,
        };
        for (index, artifact) in self.artifacts.iter_mut().enumerate() {
            if artifact.state != PublicationState::Published {
                artifact.state = if failed == Some(index) {
                    PublicationState::Failed
                } else {
                    PublicationState::Stopped
                };
            }
        }
        self.failed = true;
    }
    pub fn artifacts(&self) -> impl ExactSizeIterator<Item = (&str, &[u8], PublicationState)> {
        self.artifacts
            .iter()
            .map(|a| (a.written.as_str(), a.bytes.as_slice(), a.state))
    }
    pub fn save(
        &mut self,
        ports: &mut P,
    ) -> Result<bool, SaveFailure<P::Error, P::PublicationError>> {
        self.save_inner(ports, |_| {})
    }
    /// Test-only operational checkpoint; production hosts receive no callback.
    #[cfg(any(test, feature = "test-support"))]
    pub fn save_with_checkpoint(
        &mut self,
        ports: &mut P,
        checkpoint: impl FnMut(usize),
    ) -> Result<bool, SaveFailure<P::Error, P::PublicationError>> {
        self.save_inner(ports, checkpoint)
    }
    fn save_inner(
        &mut self,
        ports: &mut P,
        mut checkpoint: impl FnMut(usize),
    ) -> Result<bool, SaveFailure<P::Error, P::PublicationError>> {
        if self.failed {
            return Err(SaveFailure::Refused);
        }
        if self.saved {
            return Ok(false);
        }
        let mut publishers = match self.preflight(ports) {
            Ok(publishers) => publishers,
            Err(failure) => {
                self.refuse(&failure);
                return Err(SaveFailure::Failure(failure));
            }
        };
        for (index, (publisher, artifact)) in
            publishers.iter_mut().zip(&mut self.artifacts).enumerate()
        {
            if let Err(error) = ports.publish(publisher, &artifact.written, &artifact.bytes) {
                let failure = Failure::Publication { index, error };
                self.refuse(&failure);
                return Err(SaveFailure::Failure(failure));
            }
            artifact.state = PublicationState::Published;
            checkpoint(index);
        }
        self.saved = true;
        Ok(true)
    }
}
fn created_time(
    created: Option<&str>,
    clock: impl FnOnce() -> Option<u64>,
) -> Result<String, Finding> {
    if let Some(created) = created {
        return yamaa_core::define_diagnostics::created_text(created).map(str::to_owned);
    }
    let seconds = clock().ok_or_else(|| {
        Finding::cause(
            ConditionCode::DefineCreatedInvalid,
            "created",
            "UTC clock before epoch",
        )
    })?;
    // Gregorian civil date from elapsed UTC days; clock admission is host-free.
    let z = seconds / 86400 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    year += u64::from(month <= 2);
    if year > 9999 {
        return Err(Finding::cause(
            ConditionCode::DefineCreatedInvalid,
            "created",
            "UTC clock outside supported calendar",
        ));
    }
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        seconds % 86400 / 3600,
        seconds % 3600 / 60,
        seconds % 60
    ))
}

/// Infrastructure observation only; the use case owns its retained wire cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceObservation {
    Missing,
    Unreadable,
    NotRegularFile,
    InvalidRoot,
    InvalidBase,
    InvalidPath,
    InvalidLock,
    OutsideRoots,
    Symlink,
    Changed,
    Limit,
}
pub fn resource_diagnostic(
    path: &str,
    written: &str,
    observation: ResourceObservation,
) -> Diagnostic {
    use ResourceObservation as R;
    let code = match observation {
        R::Missing if path.ends_with(".artifact") => ConditionCode::ProducerContractMismatch,
        R::Missing => ConditionCode::ResourceMissing,
        R::NotRegularFile => ConditionCode::ResourceNotRegularFile,
        _ => ConditionCode::ApplicationRejected,
    };
    let mut context: yamaa_core::diagnostic::Context = [(
        String::from("path"),
        ContextValue::Scalar(Value::Str(written.into())),
    )]
    .into();
    if code == ConditionCode::ApplicationRejected {
        let classification = match observation {
            R::Unreadable => "resource_unreadable",
            R::InvalidRoot => "resource_root",
            R::InvalidBase => "resource_base",
            R::InvalidPath => "resource_path",
            R::InvalidLock => "invalid_packaging_lock",
            R::OutsideRoots => "resource_outside_roots",
            R::Symlink => "resource_link",
            R::Changed => "resource_changed",
            R::Limit => "resource_limit",
            R::Missing | R::NotRegularFile => unreachable!("normative resource cause selected"),
        };
        context.insert(
            "stage".into(),
            ContextValue::Scalar(Value::Str("define".into())),
        );
        context.insert(
            "code".into(),
            ContextValue::Scalar(Value::Str(classification.into())),
        );
    }
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context,
        source_span: None,
        operand_route: None,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationFailure {
    TargetAdmission,
    Publication,
}
/// Project the entered publication stage, not an inferred exception message.
/// The native original error remains held separately from its display text.
pub fn publication_diagnostic(
    index: usize,
    stage: PublicationFailure,
    cause: String,
) -> Diagnostic {
    let code = match stage {
        PublicationFailure::TargetAdmission => ConditionCode::ApplicationRejected,
        PublicationFailure::Publication => ConditionCode::OutputPublicationFailed,
    };
    let mut context: yamaa_core::diagnostic::Context = [
        (
            String::from("artifact"),
            ContextValue::Integer(index.to_string()),
        ),
        (
            String::from("cause"),
            ContextValue::Scalar(Value::Str(cause)),
        ),
    ]
    .into();
    if stage == PublicationFailure::TargetAdmission {
        context.insert(
            "stage".into(),
            ContextValue::Scalar(Value::Str("publication_target_admission".into())),
        );
    }
    Diagnostic {
        code,
        spec_paths: vec!["define".into()],
        context,
        source_span: None,
        operand_route: None,
    }
}
