//! The Define service over retained native snapshots. It owns admission and
//! composition, decides when each file is rendered, and holds the artifacts
//! in the order a save publishes them; the save itself is the one save's
//! (`crate::publication`). It reads everything through [`Reads`], renders
//! with the Define-XML, Dataset-JSON and XPT encoders, and admits every
//! target through the publisher port before the composition is held.
use super::{Failure, Reads};
use crate::{
    column_table::ColumnTable,
    project_source::OwnedEnvironment,
    publisher::{AnchoredTarget, Publisher},
    specification_source::PreparedDocument,
};
use std::sync::Arc;
use yamaa_core::{
    ast::{self, Transport},
    cdisc_vocabulary::DefineContext,
    define::{self, ResourceObservation},
    diagnostic::ConditionCode,
    limits::Limits,
    project_environment::Submission,
    project_terminology::Catalogue,
    resource::WrittenPath,
    schema::{Document, DocumentNode},
    specification::SourceDeclaration,
    specification_submission_document::SubmissionMode,
    submission_composition::{self as composition, Dataset, Finding, Study},
    submission_staleness::{self as staleness, Reason},
    submission_transport,
    table::{CellError, TableAccess, TableSchema, ValueRef},
};

pub(super) struct EnvironmentView<'a> {
    /// The project file, lowered.
    pub project: &'a ast::Project,
    pub catalogue: &'a Catalogue,
    pub source: &'a str,
}
pub(super) struct SpecificationView<'a> {
    pub document: &'a Document,
    /// The specification lowered into the AST, in the language it is written
    /// in, whose derivations composition reads (SUB-19, SUB-20).
    pub tree: &'a ast::Spec,
    pub source: &'a str,
    pub output: Option<WrittenPath<'a>>,
    /// The written origin of `output.report`, when the specification
    /// declares one: the run report the staleness guard reads (PROJECT-15).
    pub report: Option<WrittenPath<'a>>,
    /// Every declared input dataset and the written origin of its path; a
    /// transport file never replaces one (JSON-2).
    pub inputs: Vec<(&'a str, WrittenPath<'a>)>,
}
impl SpecificationView<'_> {
    fn historical(&self) -> bool {
        self.tree
            .submission
            .as_ref()
            .is_some_and(|metadata| metadata.mode == SubmissionMode::Historical)
    }
}
fn published_source_failure(
    finding: Option<yamaa_core::diagnostic::SourceFinding<'_>>,
    fallback: Finding,
) -> Failure {
    let Some(finding) = finding else {
        return Failure::Metadata(vec![fallback]);
    };
    Failure::Diagnostic(vec![finding.diagnostic()])
}
fn published_boundary_failure(
    error: yamaa_core::limits::LimitExceeded,
    dataset: &str,
    written: &str,
) -> Failure {
    Failure::Diagnostic(vec![define::resource_diagnostic(
        &format!("datasets.{dataset}.artifact"),
        written,
        ResourceObservation::Limit(error),
    )])
}
fn published_csv_failure(
    error: &yamaa_core::csv_source::Error,
    dataset: &str,
    written: &str,
) -> Failure {
    match *error {
        yamaa_core::csv_source::Error::Limit {
            resource,
            limit,
            actual,
        } => published_boundary_failure(
            yamaa_core::limits::LimitExceeded {
                resource,
                limit,
                actual,
            },
            dataset,
            written,
        ),
        yamaa_core::csv_source::Error::Profile {
            cause,
            record,
            ref field,
        } => Failure::Diagnostic(vec![yamaa_core::diagnostic::SourceFinding::Csv {
            cause,
            record,
            field,
            dataset,
            path: written,
        }
        .diagnostic()]),
    }
}
/// A published artifact of `dataset` the Parquet profile does not read.
fn published_parquet_failure(dataset: &str, written: &str) -> Failure {
    Failure::Metadata(vec![Finding::cause(
        ConditionCode::ProducerContractMismatch,
        written,
        "published Parquet profile",
    )
    .with("dataset", dataset)])
}
/// A published artifact whose extension selects no dataset profile.
fn published_format_failure(written: &str) -> Failure {
    Failure::Metadata(vec![Finding::unsupported(
        written,
        "published_dataset_format",
    )])
}
fn published_typed_csv_failure(
    error: &yamaa_core::typed_csv::Error,
    dataset: &str,
    written: &str,
) -> Failure {
    if let yamaa_core::typed_csv::Error::Limit(error) = error {
        return published_boundary_failure(*error, dataset, written);
    }
    published_source_failure(
        error.finding(dataset),
        Finding::cause(
            ConditionCode::ProducerContractMismatch,
            written,
            "published CSV contract",
        )
        .with("dataset", dataset),
    )
}
/// One published artifact held for a transport, as its profile decoded it.
enum Held {
    Csv(yamaa_core::typed_csv::TypedRows),
    Parquet(ColumnTable),
}
impl TableAccess for Held {
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
/// Whether a resolved specification declares `submission.has_no_data: true`.
fn declares_no_data(document: &Document) -> bool {
    document
        .field(document.root(), "submission")
        .and_then(|submission| document.field(submission, "has_no_data"))
        .is_some_and(|node| document.nodes()[node] == DocumentNode::Boolean(true))
}
/// Refuse one dataset's transport file over its held table for what the file
/// as a whole cannot carry, such as XPT-10's size and blank last row. Every
/// value was admitted first (`submission_transport::admit`) and the lengths
/// measured; this runs for every dataset with data before the document is
/// rendered again.
fn admit_transport<T: TableAccess>(
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
        Transport::DatasetJson | Transport::DatasetNdjson => Ok(()),
    }
}
/// Encode one admitted transport file over its held table; `define` is the
/// file name of the document that describes it.
fn render_transport<T: TableAccess>(
    transport: Transport,
    study: &Study,
    dataset: &Dataset,
    table: &T,
    define: &str,
) -> Result<Vec<u8>, Finding> {
    match transport {
        Transport::Xpt => {
            crate::submission_xpt::render(&crate::submission_xpt::document(study, dataset), table)
        }
        Transport::DatasetJson | Transport::DatasetNdjson => crate::submission_json::render(
            &crate::submission_json::document(study, dataset, define),
            table,
            if transport == Transport::DatasetNdjson {
                crate::submission_json::Layout::Lines
            } else {
                crate::submission_json::Layout::Object
            },
        ),
    }
}
/// The records `source` decodes from `bytes` under its profile, as a run
/// counts an input's records (CHECK-24); `None` when the bytes do not decode.
fn count_records(source: &SourceDeclaration, bytes: &[u8]) -> Option<usize> {
    crate::source_decoding::decode_source(
        source,
        bytes,
        yamaa_core::specification::SourceInterpretation::Ordinary,
        None,
    )
    .ok()
    .map(|table| table.row_count())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationState {
    Ready,
    Published,
    Failed,
    Stopped,
}
struct Artifact {
    written: String,
    target: AnchoredTarget,
    bytes: Vec<u8>,
    state: PublicationState,
}
pub(super) struct Request<'a> {
    pub environment: &'a str,
    /// The section to compose, one the caller found the project declares
    /// (PROJECT-14).
    pub section: Submission,
    pub created: Option<&'a str>,
    pub installed: &'a str,
}
/// The service retains the actual snapshot owners and admitted artifact prefix.
/// Callers may inspect bytes and state, but cannot replace documents or targets.
pub(super) struct Application {
    environment: Option<Arc<OwnedEnvironment>>,
    specifications: Vec<Arc<PreparedDocument>>,
    artifacts: Vec<Artifact>,
    failed: bool,
}
pub(super) struct Attempt {
    pub application: Application,
    pub outcome: Result<(), Failure>,
}
impl Application {
    pub(super) fn prepare(reads: &mut Reads, request: Request<'_>) -> Attempt {
        let mut application = Self {
            environment: None,
            specifications: Vec::new(),
            artifacts: Vec::new(),
            failed: false,
        };
        let outcome = application.prepare_inner(
            reads,
            request.environment,
            request.section,
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
        reads: &mut Reads,
        environment: &str,
        family: Submission,
        created: Option<&str>,
        installed: &str,
    ) -> Result<(), Failure> {
        // The section's project key, which begins every path reported below.
        let standard = composition::family_name(family);
        let created = define::created_time(created, super::utc_seconds)
            .map_err(|finding| Failure::Metadata(vec![finding]))?;
        self.environment = Some(reads.capture_environment(environment)?);
        let environment =
            Reads::environment(self.environment.as_ref().expect("captured environment"));
        let declarations = environment.project.submission.as_ref().map_err(|_| {
            Failure::Metadata(vec![Finding::cause(
                ConditionCode::SubmissionMetadataMissing,
                standard,
                "invalid closed project submission declarations",
            )
            .with("field", "submission")])
        })?;
        // The caller selected a section the project declares. A project file
        // that no longer declares it when read here is refused as any
        // undeclared section is (PROJECT-14).
        let Some(declared) = declarations
            .sections
            .iter()
            .find(|section| section.family == family)
        else {
            let sections: Vec<_> = declarations
                .sections
                .iter()
                .map(|section| composition::family_name(section.family))
                .collect();
            return Err(Failure::Diagnostic(vec![
                yamaa_core::project_scope::unknown_section(standard, &sections),
            ]));
        };
        // A run needs only a section's specifications. Submission composition
        // additionally requires the study and the section's guide and target,
        // and reports every one that is missing together.
        let required = composition::Required::admit(declarations.study.as_ref(), declared)
            .map_err(Failure::Metadata)?;
        let section = required.section;
        let define = required.define;
        let identity = environment.source;
        let define_location =
            reads.location(&format!("{standard}.define"), identity, &define.text)?;
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
            let document = reads.capture_specification(&path, identity, &spec.text)?;
            let view = Reads::specification(&document);
            let origin = view.output.ok_or_else(|| {
                Failure::Metadata(vec![Finding::cause(
                    ConditionCode::SubmissionMetadataMissing,
                    &path,
                    "output.path origin",
                )
                .with("field", "output.path")])
            })?;
            let artifact = reads.location(
                &format!("{path}.output.path"),
                origin.base_source,
                origin.written,
            )?;
            if reads.same_location(&artifact, &define_location) {
                return Err(Failure::Metadata(vec![
                    yamaa_core::define_diagnostics::admit_relation(
                        yamaa_core::define_diagnostics::TargetRelation::ArtifactCollision,
                        &path,
                        origin.written,
                        "published dataset overlaps Define target",
                    )
                    .unwrap_err(),
                ]));
            }
            for (name, input) in &view.inputs {
                let input_path = format!("{path}.input.{name}.path");
                let location = reads.location(&input_path, input.base_source, input.written)?;
                inputs.push((input_path, location, input.written.to_string()));
            }
            let document_root = view.document.root();
            let id = composition::text(view.document, document_root, "domain").unwrap_or_default();
            // A dataset declared to have no data has no transport file (JSON-3).
            let (href, target) = match transport {
                Some(transport) if !declares_no_data(view.document) => {
                    let file = format!("{}.{}", id.to_ascii_lowercase(), transport.extension());
                    let target_written = reads.transport_written(&define.text, &file);
                    let target =
                        reads.location(&format!("{path}.transport"), identity, &target_written)?;
                    if let Some(conflict) = written
                        .iter()
                        .find(|other| reads.same_location(other, &target))
                    {
                        return Err(Failure::Metadata(vec![
                            yamaa_core::define_diagnostics::admit_relation(
                                yamaa_core::define_diagnostics::TargetRelation::TransportCollision,
                                &path,
                                &target_written,
                                &format!("{target} and {conflict} name one publication target"),
                            )
                            .unwrap_err(),
                        ]));
                    }
                    written.push(target);
                    (Some(file), Some(target_written))
                }
                Some(_) => (None, None),
                // Historical links come only from the received declaration,
                // never from the current executable output (DEFINE-30).
                None if view.historical() => (None, None),
                // Composition refuses a `Submission` section without a
                // transport (DEFINE-4), so no artifact link is resolved for it.
                None if section.context == DefineContext::Submission => (None, None),
                None => {
                    let href = reads
                        .package_reference(&artifact, &define_location)
                        .ok_or_else(|| {
                            Failure::Metadata(vec![
                                yamaa_core::define_diagnostics::artifact_outside_document(
                                    &path,
                                    &id,
                                    origin.written,
                                    &define.text,
                                ),
                            ])
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
        Self::admit_targets(reads, standard, &written, &locations, &outputs, &inputs)?;
        // The dataset inventory (CT-11) and the predecessor columns (SUB-20)
        // span every submission section. The other sections' specifications
        // are read, never represented, when a composed specification needs them.
        let mut others = Vec::new();
        if self.specifications.iter().any(|document| {
            let view = Reads::specification(document);
            !view.historical() && composition::reads_other_sections(view.document)
        }) {
            for other in declarations
                .sections
                .iter()
                .filter(|other| other.family != family)
            {
                let key = composition::family_name(other.family);
                for (index, spec) in other.specs.iter().enumerate() {
                    others.push(reads.capture_specification(
                        &format!("{key}.specs[{index}]"),
                        identity,
                        &spec.text,
                    )?);
                }
            }
        }
        let others: Vec<_> = others
            .iter()
            .map(|document| Reads::specification(document).document)
            .collect();
        let inputs: Vec<_> = self
            .specifications
            .iter()
            .zip(&locations)
            .zip(&hrefs)
            .map(|((document, artifact), href)| composition::Input {
                document: Reads::specification(document).document,
                tree: Reads::specification(document).tree,
                source: Reads::specification(document).source,
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
        let mut xml = crate::submission_xml::render(&composed)
            .map_err(|finding| Failure::Metadata(vec![finding]))?;
        reads.verify_metadata(standard, identity)?;
        // A specification that declares a run report composes only with the
        // artifact and the inputs that report records (PROJECT-15).
        self.guard(reads, standard, &composed, &outputs)?;
        let mut tables = Vec::new();
        if let Some(transport) = transport {
            for (dataset, (declaring, written)) in composed.datasets().iter().zip(&outputs) {
                if dataset.has_no_data() {
                    tables.push(None);
                    continue;
                }
                let table = Self::published(reads, dataset, declaring, written)?;
                submission_transport::admit(&composed, dataset, &table)
                    .map_err(|finding| Failure::Metadata(vec![finding]))?;
                tables.push(Some(table));
            }
            let held: Vec<_> = tables.iter().map(Option::as_ref).collect();
            submission_transport::measure(&mut composed, &held)
                .map_err(|finding| Failure::Metadata(vec![finding]))?;
            // Every file-level refusal precedes the measured document, and
            // both precede any transport byte.
            for (dataset, table) in composed.datasets().iter().zip(&tables) {
                if let Some(table) = table {
                    admit_transport(transport, &composed, dataset, table)
                        .map_err(|finding| Failure::Metadata(vec![finding]))?;
                }
            }
            xml = crate::submission_xml::render(&composed)
                .map_err(|finding| Failure::Metadata(vec![finding]))?;
            let define_name = reads.define_name(&define_location);
            for ((dataset, table), target) in
                composed.datasets().iter().zip(&tables).zip(&transports)
            {
                let (Some(table), Some((path, target))) = (table, target) else {
                    continue;
                };
                let rendered = render_transport(transport, &composed, dataset, table, &define_name)
                    .map_err(|finding| Failure::Metadata(vec![finding]))?;
                let held_target = reads.target(&format!("{path}.transport"), identity, target)?;
                self.artifacts.push(Artifact {
                    written: target.clone(),
                    target: held_target,
                    bytes: rendered,
                    state: PublicationState::Ready,
                });
            }
        }
        let target = reads.target(&format!("{standard}.define"), identity, &define.text)?;
        self.artifacts.push(Artifact {
            written: define.text.clone(),
            target,
            bytes: xml,
            state: PublicationState::Ready,
        });
        // Preflight all outputs after complete admission/rendering. Recheck them
        // together on Save; no retained metadata or artifact is reopened there.
        self.preflight()?;
        Ok(())
    }
    /// The selected output set is admitted before any published artifact is
    /// read: no published artifact or declared input is a written target.
    fn admit_targets(
        reads: &Reads,
        standard: &str,
        written: &[String],
        locations: &[String],
        outputs: &[(String, String)],
        inputs: &[(String, String, String)],
    ) -> Result<(), Failure> {
        for (artifact, (_, artifact_written)) in locations.iter().zip(outputs) {
            if written
                .iter()
                .any(|target| reads.same_location(target, artifact))
            {
                return Err(Failure::Metadata(vec![Finding::cause(
                    ConditionCode::DefineArtifactCollision,
                    standard,
                    "published dataset overlaps Define targets",
                )
                .with("path", artifact_written.as_str())]));
            }
        }
        for (input_path, input, input_written) in inputs {
            if let Some(target) = written
                .iter()
                .find(|target| reads.same_location(target, input))
            {
                return Err(Failure::Metadata(vec![
                    yamaa_core::define_diagnostics::admit_relation(
                        yamaa_core::define_diagnostics::TargetRelation::TransportCollision,
                        input_path,
                        input_written,
                        &format!("declared input {input} is the written target {target}"),
                    )
                    .unwrap_err(),
                ]));
            }
        }
        Ok(())
    }
    /// The staleness guard (PROJECT-15): in entry order, each listed
    /// specification that declares `output.report` composes only with the
    /// artifact and the inputs its saved run report records. Its `run` row
    /// records a successful run of the declared `output.path` and the rows the
    /// artifact holds, `submission.has_no_data` agrees with those rows, and
    /// each declared input holds the byte length and records its `input` row
    /// records. The first mismatch fails with `stale_artifact`; a report, an
    /// artifact or an input that cannot be read fails as it does elsewhere.
    fn guard(
        &self,
        reads: &mut Reads,
        standard: &str,
        composed: &Study,
        outputs: &[(String, String)],
    ) -> Result<(), Failure> {
        fn stale(finding: Finding) -> Failure {
            Failure::Metadata(vec![finding])
        }
        for (index, ((document, dataset), (declaring, written))) in self
            .specifications
            .iter()
            .zip(composed.datasets())
            .zip(outputs)
            .enumerate()
        {
            let view = Reads::specification(document);
            if view.historical() {
                continue;
            }
            let Some(report) = view.report else {
                continue;
            };
            let listing = format!("{standard}.specs[{index}]");
            let guard = staleness::Guard {
                path: format!("{listing}.output.report"),
                dataset: dataset.id(),
                report: report.written,
            };
            let recorded = reads
                .read_report(&guard.path, report.base_source, report.written)?
                .ok_or_else(|| stale(guard.stale(Reason::ReportInvalid, [])))?;
            // The run records the lowered entry-relative path, including
            // inherited paths respelled under LAYER-10.
            let rows = guard
                .run(&recorded, &view.tree.output.path.text)
                .map_err(stale)?;
            let actual = Self::published_rows(reads, dataset, declaring, written)?;
            guard
                .artifact(rows, actual, dataset.has_no_data())
                .map_err(stale)?;
            for input in &view.tree.inputs {
                let name = input.name.name.as_str();
                let row = guard.input(&recorded, name).map_err(stale)?;
                let Some((_, origin)) = view.inputs.iter().find(|(declared, _)| *declared == name)
                else {
                    return Err(stale(
                        Finding::cause(
                            ConditionCode::SubmissionMetadataMissing,
                            &listing,
                            "input path origin",
                        )
                        .with("field", format!("input.{name}.path")),
                    ));
                };
                let bytes = reads.capture_published(
                    &format!("{listing}.input.{name}.path"),
                    origin.base_source,
                    origin.written,
                )?;
                guard.input_bytes(row, bytes.len()).map_err(stale)?;
                let records =
                    staleness::counted(input).and_then(|source| count_records(&source, &bytes));
                guard.input_records(row, records).map_err(stale)?;
            }
        }
        Ok(())
    }
    /// One published artifact's bytes, within the source byte limit.
    fn artifact_bytes(
        reads: &mut Reads,
        dataset: &composition::Dataset,
        declaring: &str,
        written: &str,
    ) -> Result<Arc<[u8]>, Failure> {
        let bytes = reads.capture_published(
            &format!("datasets.{}.artifact", dataset.id()),
            declaring,
            written,
        )?;
        if let Err(error) = Limits::source_bytes(bytes.len()) {
            return Err(published_boundary_failure(error, dataset.id(), written));
        }
        Ok(bytes)
    }
    /// The rows one published artifact holds, read under the profile its
    /// extension selects without typing its values (PROJECT-15).
    fn published_rows(
        reads: &mut Reads,
        dataset: &composition::Dataset,
        declaring: &str,
        written: &str,
    ) -> Result<usize, Failure> {
        let bytes = Self::artifact_bytes(reads, dataset, declaring, written)?;
        let lower = written.to_ascii_lowercase();
        if lower.ends_with(".csv") {
            crate::csv_source::parse(&bytes)
                .map(|source| source.records.len())
                .map_err(|error| published_csv_failure(&error, dataset.id(), written))
        } else if lower.ends_with(".parquet") {
            crate::parquet_source::parse(&bytes)
                .map(|table| table.row_count())
                .map_err(|_| published_parquet_failure(dataset.id(), written))
        } else {
            Err(published_format_failure(written))
        }
    }
    /// JSON-5: read one published artifact exactly as a consumer reads its
    /// producer's artifact, under the profile its extension selects.
    fn published(
        reads: &mut Reads,
        dataset: &composition::Dataset,
        declaring: &str,
        written: &str,
    ) -> Result<Held, Failure> {
        let bytes = Self::artifact_bytes(reads, dataset, declaring, written)?;
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
            let source = crate::csv_source::parse(&bytes)
                .map_err(|error| published_csv_failure(&error, dataset.id(), written))?;
            submission_transport::admit_fields(dataset, source.names.iter().map(String::as_str))
                .map_err(|finding| Failure::Metadata(vec![finding]))?;
            let table = yamaa_core::typed_csv::PreparedTypes::new(&kinds)
                .and_then(|types| types.convert(source))
                .map_err(|error| published_typed_csv_failure(&error, dataset.id(), written))?;
            Ok(Held::Csv(table))
        } else if lower.ends_with(".parquet") {
            let table = crate::parquet_source::parse(&bytes)
                .map_err(|_| published_parquet_failure(dataset.id(), written))?;
            Limits::table(table.row_count(), table.schema().columns().len())
                .map_err(|error| published_boundary_failure(error, dataset.id(), written))?;
            Ok(Held::Parquet(table))
        } else {
            Err(published_format_failure(written))
        }
    }
    /// Admit every artifact's target through the publisher port, and refuse
    /// two artifacts whose targets are one physical target (JSON-2).
    fn preflight(&self) -> Result<Vec<Publisher>, Failure> {
        let mut publishers: Vec<Publisher> = Vec::with_capacity(self.artifacts.len());
        for (index, artifact) in self.artifacts.iter().enumerate() {
            let publisher = artifact
                .target
                .publisher(&artifact.written)
                .map_err(|error| Failure::TargetAdmission { index, error })?;
            for (previous_index, previous) in publishers.iter().enumerate() {
                if publisher
                    .same_target(previous)
                    .map_err(|error| Failure::TargetAdmission { index, error })?
                {
                    return Err(Failure::Metadata(vec![define::shared_target(
                        &self.artifacts[previous_index].written,
                        &artifact.written,
                    )]));
                }
            }
            publishers.push(publisher);
        }
        Ok(publishers)
    }
    fn refuse(&mut self, failure: &Failure) {
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
    pub(super) fn artifacts(
        &self,
    ) -> impl ExactSizeIterator<Item = (&str, &[u8], PublicationState)> {
        self.artifacts
            .iter()
            .map(|a| (a.written.as_str(), a.bytes.as_slice(), a.state))
    }
    /// Whether the composition, or an earlier save of it, failed; it then
    /// publishes nothing.
    pub(super) fn failed(&self) -> bool {
        self.failed
    }
    /// Each artifact's target, written path and bytes, in publication order:
    /// the transport files in entry order, then the document (JSON-14).
    pub(super) fn files(&self) -> impl ExactSizeIterator<Item = (&AnchoredTarget, &str, &[u8])> {
        self.artifacts
            .iter()
            .map(|a| (&a.target, a.written.as_str(), a.bytes.as_slice()))
    }
    /// What a save did with the artifacts: the first `published` replaced
    /// their targets, and a `failure` at one of them fails the composition,
    /// so it publishes nothing again.
    pub(super) fn record(&mut self, published: usize, failure: Option<&Failure>) {
        for artifact in self.artifacts.iter_mut().take(published) {
            artifact.state = PublicationState::Published;
        }
        if let Some(failure) = failure {
            self.refuse(failure);
        }
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;

    #[test]
    fn published_boundaries_keep_structured_resource_facts_and_artifact_provenance() {
        let source_error = Limits::source_bytes(Limits::SOURCE_BYTES + 1).unwrap_err();
        let table_error = Limits::table(Limits::TABLE_ROWS + 1, 1).unwrap_err();
        let declarations = (0..=Limits::COLUMNS)
            .map(|index| (format!("C{index}"), yamaa_core::value::ColumnType::Str))
            .collect::<Vec<_>>();
        let typed_error = match yamaa_core::typed_csv::PreparedTypes::new(&declarations) {
            Err(error) => error,
            Ok(_) => panic!("oversized declarations must be refused"),
        };
        let failures: [Failure; 3] = [
            published_boundary_failure(source_error, "DM", "published.csv"),
            published_csv_failure(
                &yamaa_core::csv_source::Error::Limit {
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
            assert_eq!(diagnostic.context["step"], "define");
            assert_eq!(diagnostic.context["code"], "resource_limit");
            assert_eq!(diagnostic.context["path"], "published.csv");
            assert_eq!(diagnostic.context["resource"], resource);
            assert_eq!(diagnostic.context["limit"], limit);
            assert_eq!(diagnostic.context["actual"], limit + 1);
        }
    }

    #[test]
    fn published_resource_and_csv_limits_retain_artifact_owner_and_size_facts() {
        let error = yamaa_core::limits::LimitExceeded {
            resource: "columns",
            limit: Limits::COLUMNS,
            actual: Some(Limits::COLUMNS + 1),
        };
        let failures: [Failure; 3] = [
            published_boundary_failure(error, "DM", "dm.csv"),
            published_csv_failure(
                &yamaa_core::csv_source::Error::Limit {
                    resource: error.resource,
                    limit: error.limit,
                    actual: error.actual,
                },
                "DM",
                "dm.csv",
            ),
            published_typed_csv_failure(
                &yamaa_core::typed_csv::Error::Limit(error),
                "DM",
                "dm.csv",
            ),
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
            assert_eq!(finding.context["step"], "define");
            assert_eq!(finding.context["code"], "resource_limit");
            assert_eq!(finding.context["resource"], "columns");
            assert_eq!(finding.context["limit"], Limits::COLUMNS);
            assert_eq!(finding.context["actual"], Limits::COLUMNS + 1);
        }
    }

    #[test]
    fn published_csv_failure_preserves_the_complete_value_and_its_language_owner() {
        use yamaa_core::diagnostic::SourceFinding;
        let value = "x".repeat(1_048_577);
        let failure = published_source_failure(
            Some(SourceFinding::FieldParse {
                dataset: "DM",
                field: "AGE",
                target: yamaa_core::value::ColumnType::Int,
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
