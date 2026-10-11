//! Owned shared Define lifecycle. Metadata capture never acquires execution ports.
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
    diagnostic::{ConditionCode, Diagnostic},
    project_environment::Submission,
    project_function::Language,
    submission_composition::{self as composition, Finding},
    submission_document,
};

const METADATA_LIMITS: MetadataLimits = MetadataLimits {
    bytes: 67_108_864,
    snapshots: 1024,
    text: 16_777_216,
    work: 268_435_456,
};
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationState {
    Ready,
    Published,
    Failed,
    Stopped,
}
struct Artifact {
    written: String,
    target: file_publication::AnchoredTarget,
    bytes: Vec<u8>,
    state: PublicationState,
}
/// No host may replace captured documents, rendered bytes or targets. Both hosts
/// view the same issue rows and borrow this original failure on Save refusal.
pub struct Define {
    resources: Option<Resources>,
    environment: Option<OwnedEnvironment>,
    specifications: Vec<PreparedDocument>,
    artifacts: Vec<Artifact>,
    failure: Option<Failure>,
    issues: Vec<Issue>,
    saved: bool,
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
        let mut result = Self {
            resources: None,
            environment: None,
            specifications: Vec::new(),
            artifacts: Vec::new(),
            failure: None,
            issues: Vec::new(),
            saved: false,
            host,
        };
        if let Err(failure) = result.prepare_inner(environment, standard, created, installed) {
            result.refuse(failure);
        }
        result
    }
    fn prepare_inner(
        &mut self,
        environment: &str,
        standard: &str,
        created: Option<&str>,
        installed: &str,
    ) -> Result<(), Failure> {
        let family = match standard {
            "sdtm" => Submission::Sdtm,
            "adam" => Submission::Adam,
            "send" => Submission::Send,
            _ => {
                return Err(Failure::Metadata(vec![Finding::new(
                    "value_not_permitted",
                    "REQ-1074",
                    "standard",
                    standard,
                )]))
            }
        };
        let created = created_time(created).map_err(|finding| Failure::Metadata(vec![finding]))?;
        let (mut resources, entry) = file_configuration::resources(environment)
            .map_err(|error| Failure::Project(project_application::Error::Configuration(error)))?;
        resources
            .admit_metadata_budget(METADATA_LIMITS)
            .map_err(|error| resource(environment, environment, error))?;
        self.resources = Some(resources);
        let resources = self.resources.as_mut().expect("selected resource owner");
        let schema = crate::shipped_schema::capture_environment()
            .map_err(|e| Failure::Project(project_application::Error::Schema(e)))?;
        self.environment = Some(
            file_project::capture_environment(resources, &entry, self.host, schema)
                .map_err(|error| Failure::Project(project_application::Error::Project(error)))?,
        );
        let environment = self.environment.as_ref().expect("captured environment");
        let captured = environment.root();
        let root = &captured.normalized().document;
        let declarations = submission_document::decode(root, root.root(), Default::default())
            .map_err(|_| {
                Failure::Metadata(vec![Finding::new(
                    "submission_metadata_missing",
                    "REQ-1015",
                    standard,
                    "invalid closed environment submission declarations",
                )])
            })?;
        let study = declarations.study().ok_or_else(|| {
            Failure::Metadata(vec![Finding::new(
                "missing_required_field",
                "REQ-1015",
                "study",
                "study",
            )])
        })?;
        let section = declarations
            .sections()
            .iter()
            .find(|s| s.family == family)
            .ok_or_else(|| {
                Failure::Metadata(vec![Finding::new(
                    "missing_required_field",
                    "REQ-1015",
                    standard,
                    standard,
                )])
            })?;
        let identity = &captured.source().identity;
        let define_location = resources
            .location_from(identity, &section.define.value)
            .map_err(|e| resource(&format!("{standard}.define"), &section.define.value, e))?;
        let define_parent = Path::new(&define_location)
            .parent()
            .expect("admitted file location");
        let mut locations = Vec::new();
        let mut hrefs = Vec::new();
        let mut outputs = Vec::new();
        let mut targets = vec![define_location.clone()];
        for (index, spec) in section.specs.value.iter().enumerate() {
            let path = format!("{standard}.specs[{index}]");
            let document = file_preparation::prepare_document_from(
                resources,
                Some(identity),
                &spec.value,
                None,
            )
            .map_err(|error| {
                Failure::Project(project_application::Error::Project(
                    file_project::Error::Document(error),
                ))
            })?;
            let origin = document.written_output_origin().ok_or_else(|| {
                Failure::Metadata(vec![Finding::new(
                    "submission_metadata_missing",
                    "REQ-1015",
                    &path,
                    "output.path origin",
                )])
            })?;
            let artifact = resources
                .location_from(origin.declaring_source, origin.written)
                .map_err(|e| resource(&format!("{path}.output.path"), origin.written, e))?;
            let written_json = Path::new(origin.written)
                .with_extension("json")
                .to_string_lossy()
                .into_owned();
            let reference = if section.dataset_json.value {
                resources
                    .location_from(origin.declaring_source, &written_json)
                    .map_err(|e| resource(&format!("{path}.dataset_json"), &written_json, e))?
            } else {
                artifact.clone()
            };
            if section.dataset_json.value
                && !Path::new(&reference)
                    .parent()
                    .is_some_and(|parent| same_location(parent, define_parent))
            {
                return Err(Failure::Metadata(vec![Finding::new(
                    "artifact_outside_document",
                    "REQ-1229",
                    &path,
                    &format!("Dataset-JSON {reference} and Define {define_location} require parent traversal in their mutual references"),
                )]));
            }
            if same_location(Path::new(&artifact), Path::new(&define_location)) {
                return Err(Failure::Metadata(vec![Finding::new(
                    "artifact_path_collision",
                    "REQ-1195",
                    &path,
                    "published dataset overlaps Define target",
                )]));
            }
            if section.dataset_json.value {
                if let Some(conflict) = targets
                    .iter()
                    .find(|target| same_location(Path::new(target), Path::new(&reference)))
                    .or_else(|| {
                        same_location(Path::new(&reference), Path::new(&artifact))
                            .then_some(&artifact)
                    })
                {
                    return Err(Failure::Metadata(vec![Finding::new(
                        "dataset_json_path_collision",
                        "REQ-1226",
                        &path,
                        &format!("{reference} and {conflict} name one publication target"),
                    )]));
                }
            }
            if section.dataset_json.value {
                targets.push(reference.clone());
            }
            let href =
                package_reference(Path::new(&reference), define_parent).ok_or_else(|| {
                    Failure::Metadata(vec![Finding::new(
                        "artifact_outside_document",
                        "REQ-1021",
                        &path,
                        "artifact must be inside the Define package",
                    )])
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
                .any(|target| same_location(Path::new(target), Path::new(artifact)))
            {
                return Err(Failure::Metadata(vec![Finding::new(
                    "artifact_path_collision",
                    "REQ-1195",
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
                document: document.model().document(),
                source: &document.source().identity,
                artifact,
                href,
            })
            .collect();
        let composed = composition::compose(
            study,
            section,
            environment.environment().catalogue(),
            &inputs,
            installed,
            &created,
        )
        .map_err(Failure::Metadata)?;
        let xml = yamaa_core::submission_xml::render(&composed)
            .map_err(|f| Failure::Metadata(vec![f]))?;
        resources.verify_captured().map_err(|(_, error)| {
            resource(
                standard,
                environment.root().source().identity.as_str(),
                error,
            )
        })?;
        resources.finish_metadata_budget();
        if section.dataset_json.value {
            let mut rendered_bytes = xml.len();
            for ((dataset, (declaring, written, json)), location) in
                composed.datasets().iter().zip(&outputs).zip(&locations)
            {
                let (bytes, _) = resources
                    .capture_from(declaring, written, 67_108_864)
                    .map_err(|e| {
                        resource(&format!("datasets.{}.artifact", dataset.id()), written, e)
                    })?;
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
                    let source = yamaa_core::csv_source::parse(&bytes, Default::default())
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
                        Path::new(&define_location)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .expect("admitted define name"),
                    )
                } else if location.to_ascii_lowercase().ends_with(".parquet") {
                    let table = read_parquet(&bytes).map_err(|_| {
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
                        Path::new(&define_location)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .expect("admitted define name"),
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
                    .filter(|&n| n <= 67_108_864)
                    .ok_or_else(|| {
                        Failure::Metadata(vec![Finding::cause(
                            ConditionCode::ApplicationRejected,
                            standard,
                            "complete_publication_byte_limit",
                        )])
                    })?;
                let target = resources
                    .publication_target_from(declaring, json)
                    .map_err(|e| resource(json, json, e))?;
                self.artifacts.push(Artifact {
                    written: json.clone(),
                    target,
                    bytes: rendered,
                    state: PublicationState::Ready,
                });
            }
        }
        let target = resources
            .publication_target_from(identity, &section.define.value)
            .map_err(|e| resource(&format!("{standard}.define"), &section.define.value, e))?;
        self.artifacts.push(Artifact {
            written: section.define.value.clone(),
            target,
            bytes: xml,
            state: PublicationState::Ready,
        });
        // Preflight all outputs after complete admission/rendering. Recheck them
        // together on Save; no retained metadata or artifact is reopened there.
        self.preflight()?;
        Ok(())
    }
    fn preflight(&self) -> Result<Vec<file_publication::Publisher>, Failure> {
        let mut publishers: Vec<file_publication::Publisher> =
            Vec::with_capacity(self.artifacts.len());
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
                    return Err(Failure::Metadata(vec![Finding::new(
                        "dataset_json_path_collision",
                        "REQ-1226",
                        "define.targets",
                        &format!(
                            "{} and {} name one physical publication target",
                            self.artifacts[previous_index].written, artifact.written
                        ),
                    )]));
                }
            }
            publishers.push(publisher);
        }
        Ok(publishers)
    }
    fn refuse(&mut self, failure: Failure) {
        self.issues = issues(&failure, self.host);
        if !crate::issue_rows::within_limit(
            &self.issues,
            crate::specification_check::MAX_ISSUE_BYTES,
        ) {
            self.issues = crate::file_application::rejected("define", "issue_limit");
        }
        let failed = match &failure {
            Failure::TargetAdmission { index, .. } | Failure::Publication { index, .. } => {
                Some(*index)
            }
            _ => None,
        };
        for (i, artifact) in self.artifacts.iter_mut().enumerate() {
            if artifact.state != PublicationState::Published {
                artifact.state = if failed == Some(i) {
                    PublicationState::Failed
                } else {
                    PublicationState::Stopped
                };
            }
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
        self.artifacts
            .iter()
            .map(|a| (a.written.as_str(), a.bytes.as_slice(), a.state))
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.as_ref().map_or(0, Resources::capture_reads)
    }
    pub fn save(&mut self) -> Result<bool, SaveError> {
        self.save_with_checkpoint(|_| {})
    }
    // A private checkpoint permits deterministic operational-failure witnesses;
    // hosts receive no callback or replacement-publication capability.
    fn save_with_checkpoint(
        &mut self,
        mut checkpoint: impl FnMut(usize),
    ) -> Result<bool, SaveError> {
        if self.failure.is_some() {
            return Err(SaveError);
        }
        if self.saved {
            return Ok(false);
        }
        let mut publishers = match self.preflight() {
            Ok(publishers) => publishers,
            Err(failure) => {
                self.refuse(failure);
                return Err(SaveError);
            }
        };
        for (index, (publisher, artifact)) in
            publishers.iter_mut().zip(&mut self.artifacts).enumerate()
        {
            if let Err(error) = publisher.publish(&artifact.written, &artifact.bytes) {
                self.refuse(Failure::Publication { index, error });
                return Err(SaveError);
            }
            artifact.state = PublicationState::Published;
            checkpoint(index);
        }
        self.saved = true;
        Ok(true)
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
    let issue = |phase: &str,
                 condition: &str,
                 requirement: Option<&str>,
                 path: &str,
                 context: serde_json::Value| Issue {
        phase: phase.into(),
        condition: condition.into(),
        requirement: requirement.map(str::to_owned),
        spec_paths: vec![path.into()],
        context: context.to_string(),
    };
    match failure {
        Failure::Project(error) => project_application::failure(error, host),
        Failure::Metadata(findings) => findings
            .iter()
            .map(|f| {
                Issue::from_definition(f.definition, vec![f.path.clone()], f.context.clone())
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
            let code = match error {
                E::Missing if path.ends_with(".artifact") => {
                    Some(ConditionCode::ProducerContractMismatch)
                }
                E::Missing => Some(ConditionCode::ResourceMissing),
                E::NotRegularFile => Some(ConditionCode::ResourceNotRegularFile),
                _ => None,
            };
            if let Some(code) = code {
                let definition = code.definition();
                vec![issue(
                    definition.phase,
                    definition.condition,
                    definition.requirement,
                    path,
                    serde_json::json!({"path":written}),
                )]
            } else {
                let mut rows = crate::file_application::resource_failure("define", error);
                for row in &mut rows {
                    row.spec_paths = vec![path.clone()];
                    let mut context: serde_json::Value =
                        serde_json::from_str(&row.context).expect("shared resource issue context");
                    context["path"] = serde_json::json!(written);
                    row.context = context.to_string();
                }
                rows
            }
        }
        Failure::TargetAdmission { index, error } => {
            let definition = ConditionCode::ApplicationRejected.definition();
            vec![issue(
                definition.phase,
                definition.condition,
                definition.requirement,
                "define",
                serde_json::json!({"artifact":index,"stage":"publication_target_admission","cause":error.message()}),
            )]
        }
        Failure::Publication { index, error } => vec![issue(
            "output",
            "publication_failed",
            Some("REQ-0764"),
            "define",
            serde_json::json!({"artifact":index,"cause":error.message()}),
        )],
    }
}
fn created_time(created: Option<&str>) -> Result<String, Finding> {
    if let Some(created) = created {
        if created.len() == 19 && created.parse::<yamaa_core::temporal::DateTime>().is_ok() {
            return Ok(created.into());
        }
        return Err(Finding::new(
            "value_not_permitted",
            "REQ-1072",
            "created",
            "expected whole-second civil datetime",
        ));
    }
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| {
            Finding::new(
                "value_not_permitted",
                "REQ-1072",
                "created",
                "UTC clock before epoch",
            )
        })?
        .as_secs();
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
        return Err(Finding::new(
            "value_not_permitted",
            "REQ-1072",
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
mod publication_failure_tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    #[test]
    fn later_publication_failure_preserves_prior_bytes_original_cause_and_stopped_remainder() {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-define-failure-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        let original =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/sdtm-dm-metadata");
        let environment = fs::read_to_string(original.join("environment.yaml"))
            .unwrap()
            .replace("  - spec.yaml", "  - spec.yaml\n  - second.yaml");
        let spec = fs::read_to_string(original.join("spec.yaml")).unwrap();
        fs::write(directory.join("environment.yaml"), environment).unwrap();
        fs::write(directory.join("spec.yaml"), &spec).unwrap();
        fs::write(
            directory.join("second.yaml"),
            spec.replace("domain: DM", "domain: DM2")
                .replace("path: dm.csv", "path: second.csv"),
        )
        .unwrap();
        for name in ["dm.csv", "second.csv"] {
            fs::copy(original.join("expected/dm.csv"), directory.join(name)).unwrap();
        }
        fs::write(directory.join("define.xml"), b"prior document").unwrap();
        let mut result = Define::prepare(
            directory.join("environment.yaml").to_str().unwrap(),
            "sdtm",
            Some("2026-09-11T00:00:00"),
            "0.3.0",
            Language::Python,
        );
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        let held = result.artifacts().next().unwrap().1.to_vec();
        let reads = result.capture_reads();
        assert!(result
            .save_with_checkpoint(|index| {
                if index == 0 {
                    fs::create_dir(directory.join("second.json")).unwrap();
                }
            })
            .is_err());
        assert_eq!(fs::read(directory.join("dm.json")).unwrap(), held);
        assert_eq!(
            fs::read(directory.join("define.xml")).unwrap(),
            b"prior document"
        );
        assert_eq!(
            result
                .artifacts()
                .map(|(_, _, state)| state)
                .collect::<Vec<_>>(),
            [
                PublicationState::Published,
                PublicationState::Failed,
                PublicationState::Stopped
            ]
        );
        let Some(Failure::Publication { index, error }) = result.failure() else {
            panic!("lost original publication failure")
        };
        assert_eq!(*index, 1);
        assert!(matches!(error, file_publication::Error::InvalidTarget));
        assert_eq!(result.issues()[0].phase, "output");
        assert_eq!(result.issues()[0].condition, "publication_failed");
        assert_eq!(result.issues()[0].requirement.as_deref(), Some("REQ-0764"));
        assert!(result.issues()[0].context.contains(&error.message()));
        assert_eq!(result.capture_reads(), reads);
        assert!(result.save().is_err());
        assert_eq!(
            fs::read_dir(&directory)
                .unwrap()
                .filter(|entry| entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".yamaa-"))
                .count(),
            0
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
