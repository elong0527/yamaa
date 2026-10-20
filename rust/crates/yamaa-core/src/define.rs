//! Define preparation and one-shot publication over retained native snapshots.
//! This service owns admission, composition, rendering and effect order. Ports
//! observe file geometry and codecs without interpreting submission declarations.
use crate::json::Value as JsonValue;
use crate::{
    application_issue::{Fault, Stage},
    ast::{self, Transport},
    diagnostic::{ConditionCode, Diagnostic},
    limits::Limits,
    project_terminology::Catalogue,
    resource::WrittenPath,
    schema::{Document, DocumentNode},
    submission_composition::{self as composition, Finding},
    submission_transport,
    table::{CellError, TableAccess, TableSchema, ValueRef},
};
use std::vec;

pub struct EnvironmentView<'a> {
    /// The project file, lowered.
    pub project: &'a ast::Project,
    pub catalogue: &'a Catalogue,
    pub source: &'a str,
}
pub struct SpecificationView<'a> {
    pub document: &'a Document,
    pub source: &'a str,
    pub output: Option<WrittenPath<'a>>,
    /// Every declared input dataset and the written origin of its path; a
    /// transport file never replaces one (JSON-2).
    pub inputs: Vec<(&'a str, WrittenPath<'a>)>,
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
    fn capture_environment(&mut self, written: &str) -> Result<Self::Environment, Self::Error>;
    fn environment(environment: &Self::Environment) -> EnvironmentView<'_>;
    fn capture_specification(
        &mut self,
        path: &str,
        declaring: &str,
        written: &str,
    ) -> Result<Self::Specification, Self::Error>;
    fn specification(document: &Self::Specification) -> SpecificationView<'_>;
    fn location(&self, path: &str, declaring: &str, written: &str) -> Result<String, Self::Error>;
    /// The written path of `file` in the directory of the written `define`
    /// target, relative to the same declaring environment (JSON-1).
    fn transport_written(&self, define: &str, file: &str) -> String;
    fn same_location(&self, left: &str, right: &str) -> bool;
    fn package_reference(&self, target: &str, define: &str) -> Option<String>;
    fn define_name(&self, location: &str) -> String;
    fn verify_metadata(&mut self, path: &str, source: &str) -> Result<(), Self::Error>;
    fn capture_published(
        &mut self,
        path: &str,
        declaring: &str,
        written: &str,
    ) -> Result<Self::Bytes, Self::Error>;
    fn decode_parquet(&mut self, bytes: &[u8]) -> Result<Self::Parquet, Self::ParquetError>;
    /// Encode one admitted Dataset-JSON document over its held table. Core
    /// owns admission and the typed document; the port owns only the bytes.
    fn render_dataset_json<T: TableAccess>(
        &mut self,
        document: &crate::submission_json::DatasetJsonDocument,
        table: &T,
    ) -> Result<Vec<u8>, Finding>;
    /// Encode one admitted SAS Transport version 5 document over its held
    /// table. Core owns admission and the typed document; the port owns bytes.
    fn render_xpt<T: TableAccess>(
        &mut self,
        document: &crate::submission_xpt::XptDocument,
        table: &T,
    ) -> Result<Vec<u8>, Finding>;
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
#[derive(Debug, thiserror::Error)]
pub enum Failure<E, PE> {
    #[error("define port: {0:?}")]
    Port(E),
    #[error("define metadata: {0:?}")]
    Metadata(Vec<Finding>),
    #[error("define diagnostic: {0:?}")]
    Diagnostic(Vec<Diagnostic>),
    #[error("define target admission: index={index:?}, error={error:?}")]
    TargetAdmission { index: usize, error: PE },
    #[error("define publication: index={index:?}, error={error:?}")]
    Publication { index: usize, error: PE },
}
fn published_source_failure<E, PE>(
    finding: Option<crate::diagnostic::SourceFinding<'_>>,
    fallback: Finding,
) -> Failure<E, PE> {
    let Some(finding) = finding else {
        return Failure::Metadata(vec![fallback]);
    };
    Failure::Diagnostic(vec![finding.diagnostic()])
}
fn published_boundary_failure<E, PE>(
    error: crate::limits::LimitExceeded,
    dataset: &str,
    written: &str,
) -> Failure<E, PE> {
    Failure::Diagnostic(vec![resource_diagnostic(
        &format!("datasets.{dataset}.artifact"),
        written,
        ResourceObservation::Limit(error),
    )])
}
fn published_csv_failure<E, PE>(
    error: &crate::csv_source::Error,
    dataset: &str,
    written: &str,
) -> Failure<E, PE> {
    match *error {
        crate::csv_source::Error::Limit {
            resource,
            limit,
            actual,
        } => published_boundary_failure(
            crate::limits::LimitExceeded {
                resource,
                limit,
                actual,
            },
            dataset,
            written,
        ),
        crate::csv_source::Error::Profile {
            cause,
            record,
            ref field,
        } => Failure::Diagnostic(vec![crate::diagnostic::SourceFinding::Csv {
            cause,
            record,
            field,
            dataset,
            path: written,
        }
        .diagnostic()]),
    }
}
fn published_typed_csv_failure<E, PE>(
    error: &crate::typed_csv::Error,
    dataset: &str,
    written: &str,
) -> Failure<E, PE> {
    if let crate::typed_csv::Error::Limit(error) = error {
        return published_boundary_failure(*error, dataset, written);
    }
    published_source_failure(
        error.finding(dataset),
        Finding::cause(
            ConditionCode::ProducerContractMismatch,
            written,
            "published CSV contract",
        ),
    )
}
/// One published artifact held for a transport, as its profile decoded it.
enum Held<C, Q> {
    Csv(C),
    Parquet(Q),
}
impl<C: TableAccess, Q: TableAccess> TableAccess for Held<C, Q> {
    type Error = ();
    fn schema(&self) -> &TableSchema {
        match self {
            Self::Csv(table) => table.schema(),
            Self::Parquet(table) => table.schema(),
        }
    }
    fn row_count(&self) -> usize {
        match self {
            Self::Csv(table) => table.row_count(),
            Self::Parquet(table) => table.row_count(),
        }
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<()>> {
        fn opaque<E>(error: CellError<E>) -> CellError<()> {
            match error {
                CellError::OutOfBounds { row, column } => CellError::OutOfBounds { row, column },
                CellError::Access(_) => CellError::Access(()),
            }
        }
        match self {
            Self::Csv(table) => table.cell(row, column).map_err(opaque),
            Self::Parquet(table) => table.cell(row, column).map_err(opaque),
        }
    }
}
/// The typed document a transport file encodes.
enum Rendered {
    Json(Box<crate::submission_json::DatasetJsonDocument>),
    Xpt(crate::submission_xpt::XptDocument),
}
/// A published artifact held for a transport, or the service failure.
type Published<P> = Result<
    Held<crate::typed_csv::TypedRows, <P as Ports>::Parquet>,
    Failure<<P as Ports>::Error, <P as Ports>::PublicationError>,
>;
/// Whether a resolved specification declares `submission.has_no_data: true`.
fn declares_no_data(document: &Document) -> bool {
    document
        .field(document.root(), "submission")
        .and_then(|submission| document.field(submission, "has_no_data"))
        .is_some_and(|node| document.nodes()[node] == DocumentNode::Boolean(true))
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
}
pub struct Attempt<P: Ports> {
    pub application: Application<P>,
    pub outcome: Result<(), Failure<P::Error, P::PublicationError>>,
}
#[derive(Debug)]
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
        let family = crate::define_diagnostics::standard_name(standard)
            .map_err(|finding| Failure::Metadata(vec![finding]))?;
        let created = created_time(created, || ports.utc_seconds())
            .map_err(|finding| Failure::Metadata(vec![finding]))?;
        self.environment = Some(
            ports
                .capture_environment(environment)
                .map_err(Failure::Port)?,
        );
        let environment = P::environment(self.environment.as_ref().expect("captured environment"));
        let declarations = environment.project.submission.as_ref().map_err(|_| {
            Failure::Metadata(vec![Finding::cause(
                ConditionCode::SubmissionMetadataMissing,
                standard,
                "invalid closed project submission declarations",
            )])
        })?;
        // A run needs only a section's specifications. Submission composition
        // additionally requires the study, the selected section and its guide
        // and target, and reports every one that is missing together.
        let required = composition::Required::admit(
            declarations.study.as_ref(),
            declarations
                .sections
                .iter()
                .find(|section| section.family == family),
            family,
            environment.project.header.surface,
        )
        .map_err(Failure::Metadata)?;
        let section = required.section;
        let define = required.define;
        let identity = environment.source;
        let define_location = ports
            .location(&format!("{standard}.define"), identity, &define.text)
            .map_err(Failure::Port)?;
        let transport = section.transport;
        let mut locations = Vec::new();
        let mut hrefs = Vec::new();
        let mut outputs = Vec::new();
        let mut transports = Vec::new();
        // Every file this generation writes, the document first (JSON-2).
        let mut written = vec![define_location.clone()];
        let mut inputs = Vec::new();
        for (index, spec) in section.specs.iter().enumerate() {
            let path = format!("{standard}.specs[{index}]");
            let document = ports
                .capture_specification(&path, identity, &spec.text)
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
                    origin.base_source,
                    origin.written,
                )
                .map_err(Failure::Port)?;
            if ports.same_location(&artifact, &define_location) {
                return Err(Failure::Metadata(vec![
                    crate::define_diagnostics::admit_relation(
                        crate::define_diagnostics::TargetRelation::ArtifactCollision,
                        &path,
                        "published dataset overlaps Define target",
                    )
                    .unwrap_err(),
                ]));
            }
            for (name, input) in &view.inputs {
                let input_path = format!("{path}.input.{name}.path");
                let location = ports
                    .location(&input_path, input.base_source, input.written)
                    .map_err(Failure::Port)?;
                inputs.push((input_path, location));
            }
            let document_root = view.document.root();
            let id = composition::text(view.document, document_root, "domain").unwrap_or_default();
            // A dataset declared to have no data has no transport file (JSON-3).
            let (href, target) = match transport {
                Some(transport) if !declares_no_data(view.document) => {
                    let file = format!("{}.{}", id.to_ascii_lowercase(), transport.extension());
                    let target_written = ports.transport_written(&define.text, &file);
                    let target = ports
                        .location(&format!("{path}.transport"), identity, &target_written)
                        .map_err(Failure::Port)?;
                    if let Some(conflict) = written
                        .iter()
                        .find(|other| ports.same_location(other, &target))
                    {
                        return Err(Failure::Metadata(vec![
                            crate::define_diagnostics::admit_relation(
                                crate::define_diagnostics::TargetRelation::TransportCollision,
                                &path,
                                &format!("{target} and {conflict} name one publication target"),
                            )
                            .unwrap_err(),
                        ]));
                    }
                    written.push(target);
                    (Some(file), Some(target_written))
                }
                Some(_) => (None, None),
                None => {
                    let href = ports
                        .package_reference(&artifact, &define_location)
                        .ok_or_else(|| {
                            Failure::Metadata(vec![crate::define_diagnostics::admit_relation(
                                crate::define_diagnostics::TargetRelation::ArtifactOutsidePackage,
                                &path,
                                "artifact must be inside the Define package",
                            )
                            .unwrap_err()])
                        })?;
                    (Some(href), None)
                }
            };
            locations.push(artifact);
            hrefs.push(href);
            outputs.push((origin.base_source.to_string(), origin.written.to_string()));
            transports.push(target.map(|target| (path, target)));
            self.specifications.push(document);
        }
        // The selected output set is admitted before any published artifact
        // read: no published artifact or declared input is a written target.
        for artifact in &locations {
            if written
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
        for (input_path, input) in &inputs {
            if let Some(target) = written
                .iter()
                .find(|target| ports.same_location(target, input))
            {
                return Err(Failure::Metadata(vec![
                    crate::define_diagnostics::admit_relation(
                        crate::define_diagnostics::TargetRelation::TransportCollision,
                        input_path,
                        &format!("declared input {input} is the written target {target}"),
                    )
                    .unwrap_err(),
                ]));
            }
        }
        // The dataset inventory (CT-11) and the predecessor columns (SUB-20)
        // span every submission section. The other sections' specifications
        // are read, never represented, when a composed specification needs them.
        let mut others = Vec::new();
        if self.specifications.iter().any(|document| {
            let document = P::specification(document).document;
            crate::submission_inventory::requested(document)
                || crate::submission_origin::names_predecessor(document)
        }) {
            for other in declarations
                .sections
                .iter()
                .filter(|other| other.family != family)
            {
                let key = composition::family_name(other.family);
                for (index, spec) in other.specs.iter().enumerate() {
                    others.push(
                        ports
                            .capture_specification(
                                &format!("{key}.specs[{index}]"),
                                identity,
                                &spec.text,
                            )
                            .map_err(Failure::Port)?,
                    );
                }
            }
        }
        let others: Vec<_> = others
            .iter()
            .map(|document| P::specification(document).document)
            .collect();
        let inputs: Vec<_> = self
            .specifications
            .iter()
            .zip(&locations)
            .zip(&hrefs)
            .map(|((document, artifact), href)| composition::Input {
                document: P::specification(document).document,
                source: P::specification(document).source,
                artifact,
                href: href.as_deref(),
            })
            .collect();
        let mut composed = composition::compose(
            required,
            environment.catalogue,
            &inputs,
            &others,
            installed,
            &created,
        )
        .map_err(Failure::Metadata)?;
        // Every document fault is found before a published artifact is read;
        // with a transport the document is rendered again once measured.
        let mut xml =
            crate::submission_xml::render(&composed).map_err(|f| Failure::Metadata(vec![f]))?;
        ports
            .verify_metadata(standard, identity)
            .map_err(Failure::Port)?;
        let mut tables = Vec::new();
        if transport.is_some() {
            for (dataset, (declaring, written)) in composed.datasets().iter().zip(&outputs) {
                if dataset.has_no_data() {
                    tables.push(None);
                    continue;
                }
                let table = Self::published(ports, dataset, declaring, written)?;
                submission_transport::admit(&composed, dataset, &table)
                    .map_err(|finding| Failure::Metadata(vec![finding]))?;
                tables.push(Some(table));
            }
            let held: Vec<_> = tables.iter().map(Option::as_ref).collect();
            submission_transport::measure(&mut composed, &held)
                .map_err(|finding| Failure::Metadata(vec![finding]))?;
            let mut documents = Vec::new();
            for (dataset, table) in composed.datasets().iter().zip(&tables) {
                let Some(table) = table else {
                    documents.push(None);
                    continue;
                };
                documents.push(Some(match transport {
                    Some(Transport::Xpt) => {
                        let document = crate::submission_xpt::document(&composed, dataset);
                        crate::submission_xpt::admit(&document, dataset, table)
                            .map_err(|finding| Failure::Metadata(vec![finding]))?;
                        Rendered::Xpt(document)
                    }
                    _ => Rendered::Json(Box::new(crate::submission_json::document(
                        &composed,
                        dataset,
                        &ports.define_name(&define_location),
                    ))),
                }));
            }
            xml =
                crate::submission_xml::render(&composed).map_err(|f| Failure::Metadata(vec![f]))?;
            for ((document, table), target) in documents.iter().zip(&tables).zip(&transports) {
                let (Some(document), Some(table), Some((path, target))) = (document, table, target)
                else {
                    continue;
                };
                let rendered = match document {
                    Rendered::Xpt(document) => ports.render_xpt(document, table),
                    Rendered::Json(document) => ports.render_dataset_json(document, table),
                }
                .map_err(|f| Failure::Metadata(vec![f]))?;
                let held_target = ports
                    .target(&format!("{path}.transport"), identity, target)
                    .map_err(Failure::Port)?;
                self.artifacts.push(Artifact {
                    written: target.clone(),
                    target: held_target,
                    bytes: rendered,
                    state: PublicationState::Ready,
                });
            }
        }
        let target = ports
            .target(&format!("{standard}.define"), identity, &define.text)
            .map_err(Failure::Port)?;
        self.artifacts.push(Artifact {
            written: define.text.clone(),
            target,
            bytes: xml,
            state: PublicationState::Ready,
        });
        // Preflight all outputs after complete admission/rendering. Recheck them
        // together on Save; no retained metadata or artifact is reopened there.
        self.preflight(ports)?;
        Ok(())
    }
    /// JSON-5: read one published artifact exactly as a consumer reads its
    /// producer's artifact, under the profile its extension selects.
    fn published(
        ports: &mut P,
        dataset: &composition::Dataset,
        declaring: &str,
        written: &str,
    ) -> Published<P> {
        let bytes = ports
            .capture_published(
                &format!("datasets.{}.artifact", dataset.id()),
                declaring,
                written,
            )
            .map_err(Failure::Port)?;
        if let Err(error) = Limits::source_bytes(bytes.as_ref().len()) {
            return Err(published_boundary_failure(error, dataset.id(), written));
        }
        let lower = written.to_ascii_lowercase();
        if lower.ends_with(".csv") {
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
            let source = crate::csv_source::parse(bytes.as_ref())
                .map_err(|error| published_csv_failure(&error, dataset.id(), written))?;
            submission_transport::admit_fields(dataset, source.names.iter().map(String::as_str))
                .map_err(|finding| Failure::Metadata(vec![finding]))?;
            let table = crate::typed_csv::PreparedTypes::new(&kinds)
                .and_then(|types| types.convert(source))
                .map_err(|error| published_typed_csv_failure(&error, dataset.id(), written))?;
            Ok(Held::Csv(table))
        } else if lower.ends_with(".parquet") {
            let table = ports.decode_parquet(bytes.as_ref()).map_err(|_| {
                Failure::Metadata(vec![Finding::cause(
                    ConditionCode::ProducerContractMismatch,
                    written,
                    "published Parquet profile",
                )])
            })?;
            Limits::table(table.row_count(), table.schema().columns().len())
                .map_err(|error| published_boundary_failure(error, dataset.id(), written))?;
            Ok(Held::Parquet(table))
        } else {
            Err(Failure::Metadata(vec![Finding::cause(
                ConditionCode::ApplicationUnsupported,
                written,
                "published_dataset_format",
            )]))
        }
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
                        crate::define_diagnostics::admit_relation(
                            crate::define_diagnostics::TargetRelation::TransportCollision,
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
    /// Each artifact's target with its written path, in publication order.
    pub fn targets(&self) -> impl ExactSizeIterator<Item = (&P::Target, &str)> {
        self.artifacts
            .iter()
            .map(|a| (&a.target, a.written.as_str()))
    }
    pub fn save(
        &mut self,
        ports: &mut P,
    ) -> Result<bool, SaveFailure<P::Error, P::PublicationError>> {
        if self.failed {
            return Err(SaveFailure::Refused);
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
        }
        Ok(true)
    }
}
fn created_time(
    created: Option<&str>,
    clock: impl FnOnce() -> Option<u64>,
) -> Result<String, Finding> {
    if let Some(created) = created {
        return crate::define_diagnostics::created_text(created).map(str::to_owned);
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
    Limit(crate::limits::LimitExceeded),
}
pub fn resource_diagnostic(
    path: &str,
    written: &str,
    observation: ResourceObservation,
) -> Diagnostic {
    use ResourceObservation as R;
    if let R::Limit(error) = observation {
        let mut diagnostic = crate::application_issue::resource_rejected(
            Stage::Define,
            Fault::ResourceLimit,
            error.resource,
            error.limit,
            error.actual,
            vec![path.into()],
        );
        diagnostic
            .context
            .insert("path".into(), JsonValue::String(written.into()));
        return diagnostic;
    }
    let code = match observation {
        R::Missing if path.ends_with(".artifact") => ConditionCode::ProducerContractMismatch,
        R::Missing => ConditionCode::ResourceMissing,
        R::NotRegularFile => ConditionCode::ResourceNotRegularFile,
        R::Changed => ConditionCode::ResourceContentChanged,
        _ => {
            let fault = match observation {
                R::Unreadable => Fault::ResourceUnreadable,
                R::InvalidRoot => Fault::ResourceRoot,
                R::InvalidBase => Fault::ResourceBase,
                R::InvalidPath => Fault::ResourcePath,
                R::InvalidLock => Fault::InvalidPackagingLock,
                R::OutsideRoots => Fault::ResourceOutsideRoots,
                R::Symlink => Fault::ResourceLink,
                R::Limit(_) => unreachable!("fixed boundary projected above"),
                R::Missing | R::NotRegularFile | R::Changed => {
                    unreachable!("normative resource cause selected")
                }
            };
            let mut diagnostic = crate::application_issue::rejected(Stage::Define, fault);
            diagnostic.spec_paths = vec![path.into()];
            diagnostic
                .context
                .insert("path".into(), JsonValue::String(written.into()));
            return diagnostic;
        }
    };
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context: [(String::from("path"), JsonValue::String(written.into()))].into(),
        source_span: None,
        operand_route: None,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PublicationFailure {
    /// The target was refused before any byte was written, for the reason
    /// the adapter classified from its typed cause.
    #[error("define target admission")]
    TargetAdmission(Fault),
    #[error("define publication")]
    Publication,
}
/// Project the entered publication stage and the adapter's typed admission fault,
/// never a classification inferred from an exception message.
/// The native original error remains held separately from its display text.
pub fn publication_diagnostic(
    index: usize,
    stage: PublicationFailure,
    cause: String,
) -> Diagnostic {
    let mut diagnostic = match stage {
        PublicationFailure::TargetAdmission(fault) => {
            crate::application_issue::rejected(Stage::PublicationTargetAdmission, fault)
        }
        PublicationFailure::Publication => Diagnostic {
            code: ConditionCode::OutputPublicationFailed,
            spec_paths: Vec::new(),
            context: Default::default(),
            source_span: None,
            operand_route: None,
        },
    };
    diagnostic.spec_paths = vec!["define".into()];
    for (key, value) in [
        ("artifact", JsonValue::from(index)),
        ("cause", JsonValue::String(cause)),
    ] {
        diagnostic.context.insert(key.into(), value);
    }
    diagnostic
}

impl<E: std::fmt::Debug, PE: std::fmt::Debug> std::fmt::Display for SaveFailure<E, PE> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "define: {self:?}")
    }
}
impl<E: std::fmt::Debug, PE: std::fmt::Debug> std::error::Error for SaveFailure<E, PE> {}

#[cfg(test)]
mod projection_tests {
    use super::*;

    #[test]
    fn published_boundaries_keep_structured_resource_facts_and_artifact_provenance() {
        let source_error = Limits::source_bytes(Limits::SOURCE_BYTES + 1).unwrap_err();
        let table_error = Limits::table(Limits::TABLE_ROWS + 1, 1).unwrap_err();
        let declarations = (0..=Limits::COLUMNS)
            .map(|index| (format!("C{index}"), crate::value::ColumnType::Str))
            .collect::<Vec<_>>();
        let typed_error = match crate::typed_csv::PreparedTypes::new(&declarations) {
            Err(error) => error,
            Ok(_) => panic!("oversized declarations must be refused"),
        };
        let failures: [Failure<(), ()>; 3] = [
            published_boundary_failure(source_error, "DM", "published.csv"),
            published_csv_failure(
                &crate::csv_source::Error::Limit {
                    resource: table_error.resource,
                    limit: table_error.limit,
                    actual: table_error.actual,
                },
                "DM",
                "published.csv",
            ),
            published_typed_csv_failure(&typed_error, "DM", "published.csv"),
        ];
        for (failure, resource, limit) in [
            (&failures[0], "source_bytes", Limits::SOURCE_BYTES),
            (&failures[1], "table_rows", Limits::TABLE_ROWS),
            (&failures[2], "columns", Limits::COLUMNS),
        ] {
            let Failure::Diagnostic(findings) = failure else {
                panic!("boundary refusal must retain its diagnostic owner");
            };
            let diagnostic = &findings[0];
            assert_eq!(diagnostic.code, ConditionCode::ApplicationRejected);
            assert_eq!(diagnostic.code.definition().rule, None);
            assert_eq!(diagnostic.spec_paths, ["datasets.DM.artifact"]);
            assert_eq!(diagnostic.context["stage"], "define");
            assert_eq!(diagnostic.context["code"], "resource_limit");
            assert_eq!(diagnostic.context["path"], "published.csv");
            assert_eq!(diagnostic.context["resource"], resource);
            assert_eq!(diagnostic.context["limit"], limit);
            assert_eq!(diagnostic.context["actual"], limit + 1);
        }
    }

    #[test]
    fn published_resource_and_csv_limits_retain_artifact_owner_and_size_facts() {
        let error = crate::limits::LimitExceeded {
            resource: "columns",
            limit: Limits::COLUMNS,
            actual: Some(Limits::COLUMNS + 1),
        };
        let failures: [Failure<(), ()>; 3] = [
            published_boundary_failure(error, "DM", "dm.csv"),
            published_csv_failure(
                &crate::csv_source::Error::Limit {
                    resource: error.resource,
                    limit: error.limit,
                    actual: error.actual,
                },
                "DM",
                "dm.csv",
            ),
            published_typed_csv_failure(&crate::typed_csv::Error::Limit(error), "DM", "dm.csv"),
        ];
        for failure in failures {
            let Failure::Diagnostic(findings) = failure else {
                panic!("resource boundary must retain structured facts");
            };
            assert_eq!(findings.len(), 1);
            let finding = &findings[0];
            assert_eq!(finding.code, ConditionCode::ApplicationRejected);
            assert_eq!(finding.spec_paths, ["datasets.DM.artifact"]);
            assert_eq!(finding.context["path"], "dm.csv");
            assert_eq!(finding.context["stage"], "define");
            assert_eq!(finding.context["code"], "resource_limit");
            assert_eq!(finding.context["resource"], "columns");
            assert_eq!(finding.context["limit"], Limits::COLUMNS);
            assert_eq!(finding.context["actual"], Limits::COLUMNS + 1);
        }
    }

    #[test]
    fn published_csv_failure_preserves_the_complete_value_and_its_language_owner() {
        use crate::diagnostic::SourceFinding;
        let value = "x".repeat(1_048_577);
        let failure: Failure<(), ()> = published_source_failure(
            Some(SourceFinding::FieldParse {
                dataset: "DM",
                field: "AGE",
                target: crate::value::ColumnType::Int,
                value: &value,
            }),
            Finding::cause(
                ConditionCode::ProducerContractMismatch,
                "sdtm.specs[0]",
                "published CSV contract",
            ),
        );
        let Failure::Diagnostic(findings) = failure else {
            panic!("source failure must retain its diagnostic owner");
        };
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, ConditionCode::SourceTypeFieldParse);
        assert_eq!(findings[0].spec_paths, ["input.DM.types.AGE"]);
        assert_eq!(findings[0].context["dataset"], "DM");
        assert_eq!(findings[0].context["field"], "AGE");
        assert_eq!(findings[0].context["type"], "int");
        assert_eq!(findings[0].context["value"], value);
    }
}
