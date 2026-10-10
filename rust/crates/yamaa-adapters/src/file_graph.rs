//! Native, original-document producer capture. This owner retains metadata and
//! descriptor witnesses only; complete workflow execution remains unqualified.
use crate::{
    file_preparation,
    file_resources::{Error as ResourceError, MetadataLimits, Resources},
    producer_admission::SuppliedProducer,
    producer_graph::{self, Limits, PreparedGraph, SuppliedNode},
    project_source::{CapturedEnvironment, OwnedEnvironment},
    specification_source::{CapturedSchema, PreparedDocument},
};
use std::{collections::BTreeMap, sync::Arc};
use yamaa_core::schema::DocumentNode as N;

#[derive(Debug)]
pub enum Error {
    Resource(ResourceError),
    Snapshot {
        identity: String,
        error: ResourceError,
    },
    Document(file_preparation::Error),
    Graph(producer_graph::Error),
    Limit(&'static str),
    Boundary(&'static str),
}
/// Identifies the original declaration being captured when a native failure
/// occurs. Graph diagnostics retain their own node and authored field paths.
#[derive(Debug)]
pub struct Origin {
    pub document: usize,
    pub dataset: String,
}
pub struct RejectedGraph {
    error: Error,
    entry: Option<String>,
    origin: Option<Origin>,
    documents: Vec<Arc<PreparedDocument>>,
    schema: Arc<CapturedSchema>,
    environment: CapturedEnvironment,
    resources: Resources,
}
impl RejectedGraph {
    pub fn error(&self) -> &Error {
        &self.error
    }
    /// Original root request, retained after its ownership quota is admitted.
    pub fn entry(&self) -> Option<&str> {
        self.entry.as_deref()
    }
    pub fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }
    pub fn documents(&self) -> &[Arc<PreparedDocument>] {
        &self.documents
    }
    pub fn schema(&self) -> &Arc<CapturedSchema> {
        &self.schema
    }
    pub fn environment(&self) -> &CapturedEnvironment {
        &self.environment
    }
    /// Raw captures include failed entry/parent decoding, independently of the
    /// successfully prepared documents. These held bytes require no reread.
    pub fn captured_sources(&self) -> impl Iterator<Item = (String, &[u8])> {
        self.resources.captured_sources()
    }
    /// Bounded raw rejection evidence. No preparation or IO is retried.
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
impl std::fmt::Debug for RejectedGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedGraph")
            .field("error", &self.error)
            .field("origin", &self.origin)
            .field("documents", &self.documents)
            .field("capture_reads", &self.capture_reads())
            .finish_non_exhaustive()
    }
}

pub struct FileGraph {
    graph: PreparedGraph,
    entry: String,
    environment: CapturedEnvironment,
    resources: Resources,
}
/// Constructed only by the approved original native capture boundary. Moving
/// this whole owner never supplies an arbitrary document, plan or environment.
pub(crate) struct CapturedEntry {
    resources: Resources,
    written: String,
    environment: OwnedEnvironment,
    schema: Arc<CapturedSchema>,
    document: PreparedDocument,
    limits: Limits,
    producer: bool,
}
impl CapturedEntry {
    pub(crate) fn is_producer(&self) -> bool {
        self.producer
    }
    pub(crate) fn into_ordinary_parts(
        self,
    ) -> (Resources, String, PreparedDocument, OwnedEnvironment) {
        (
            self.resources,
            self.written,
            self.document,
            self.environment,
        )
    }
}
impl FileGraph {
    /// Capture a canonical closure iteratively through the approved reader,
    /// then compile the whole graph once against the admitted environment.
    /// No activation, data, runtime or publisher port is accepted or exposed.
    pub fn prepare(
        resources: Resources,
        written: &str,
        environment: OwnedEnvironment,
        schema: Arc<CapturedSchema>,
        limits: Limits,
    ) -> Result<Self, Box<RejectedGraph>> {
        Self::capture_entry(resources, written, environment, schema, limits)
            .and_then(Self::prepare_captured)
    }
    pub(crate) fn capture_entry(
        mut resources: Resources,
        written: &str,
        environment: OwnedEnvironment,
        schema: Arc<CapturedSchema>,
        limits: Limits,
    ) -> Result<CapturedEntry, Box<RejectedGraph>> {
        let mut entry = None;
        let mut document = None;
        let result = (|| {
            admit_entry(&mut resources, written, &schema, limits)?;
            entry = Some(written.to_owned());
            document = Some(
                file_preparation::prepare_document_from(
                    &mut resources,
                    None,
                    written,
                    Some(Arc::clone(&schema)),
                )
                .map_err(Error::Document)?,
            );
            let root = document.as_ref().expect("captured entry");
            let d = root.normalization_input();
            // Document is a bounded tree. One whole arena reservation bounds
            // all mapping lookups, including inherited declaration fields.
            resources
                .charge_work(d.nodes().len())
                .map_err(Error::Resource)?;
            if root.model().document().nodes().len() > limits.graph.document_nodes {
                return Err(Error::Limit("producer_graph_document_nodes"));
            }
            // Defaults cannot invent authored producer declarations. Missing or
            // nonmapping original input remains for the selected compiler's
            // typed admission; shipped-schema failures have already rejected.
            let mut producer = false;
            if let Some(input) = d.field(d.root(), "input") {
                if let N::Mapping(entries) = &d.nodes()[input] {
                    // Inspect the whole collection, including after a match.
                    for &(_, declaration) in entries {
                        producer |=
                            yamaa_core::producer_admission::is_producer_input(d, declaration);
                    }
                }
            }
            resources
                .verify_captured()
                .map_err(|(identity, error)| Error::Snapshot { identity, error })?;
            Ok(producer)
        })();
        match result {
            Ok(producer) => Ok(CapturedEntry {
                resources,
                written: entry.expect("admitted entry"),
                environment,
                schema,
                document: document.expect("captured entry"),
                limits,
                producer,
            }),
            Err(error) => {
                let (_, environment) = environment.into_parts();
                Err(Box::new(RejectedGraph {
                    error,
                    entry,
                    origin: None,
                    documents: document.into_iter().map(Arc::new).collect(),
                    schema,
                    environment,
                    resources,
                }))
            }
        }
    }
    pub(crate) fn prepare_captured(entry: CapturedEntry) -> Result<Self, Box<RejectedGraph>> {
        let CapturedEntry {
            mut resources,
            written,
            environment,
            schema,
            document,
            limits,
            ..
        } = entry;
        let (environment, captured) = environment.into_parts();
        let mut documents = Vec::new();
        let mut origin = None;
        let entry = written;
        let result = collect(
            &mut resources,
            document,
            &schema,
            limits,
            &mut documents,
            &mut origin,
        )
        .and_then(|nodes| {
            let root = documents[0].source().identity.as_str();
            // Resource witnesses and native edge/index ownership share the
            // core's text/work ceilings, rather than receiving a new quota.
            let (_, text, work) = resources.metadata_usage();
            let mut remaining = limits;
            remaining.graph.text_bytes = limits
                .graph
                .text_bytes
                .checked_sub(text)
                .ok_or(Error::Limit("producer_graph_text_bytes"))?;
            remaining.graph.work = limits
                .graph
                .work
                .checked_sub(work)
                .ok_or(Error::Limit("producer_graph_work"))?;
            producer_graph::prepare(root, nodes, environment, remaining).map_err(Error::Graph)
        });
        match result {
            Ok(graph) => Ok(Self {
                graph,
                entry,
                environment: captured,
                resources,
            }),
            Err(error) => Err(Box::new(RejectedGraph {
                error,
                entry: Some(entry),
                origin,
                documents,
                schema,
                environment: captured,
                resources,
            })),
        }
    }
    pub fn graph(&self) -> &PreparedGraph {
        &self.graph
    }
    pub fn entry(&self) -> &str {
        &self.entry
    }
    pub fn environment(&self) -> &CapturedEnvironment {
        &self.environment
    }
    pub fn captured_sources(&self) -> impl Iterator<Item = (String, &[u8])> {
        self.resources.captured_sources()
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    /// Consume the complete native graph into the distinct build owner. Static
    /// preparation never accepts ports or exposes a standalone producer plan.
    pub fn into_build(mut self) -> crate::file_producer_build::FileBuild {
        let metadata_snapshots = self.resources.snapshot_count();
        self.resources.finish_metadata_budget();
        let (documents, build) = self.graph.into_build();
        crate::file_producer_build::FileBuild::new(
            build,
            documents,
            self.entry,
            self.environment,
            self.resources,
            metadata_snapshots,
        )
    }
}

fn charge(
    used: &mut usize,
    amount: usize,
    maximum: usize,
    name: &'static str,
) -> Result<(), Error> {
    *used = used
        .checked_add(amount)
        .filter(|&n| n <= maximum)
        .ok_or(Error::Limit(name))?;
    Ok(())
}
fn own_document(
    document: PreparedDocument,
    resources: &Resources,
    documents: &mut Vec<Arc<PreparedDocument>>,
    identities: &mut BTreeMap<String, usize>,
    model_nodes: &mut usize,
    limits: Limits,
) -> Result<Arc<PreparedDocument>, Error> {
    charge(
        model_nodes,
        document.model().document().nodes().len(),
        limits.graph.document_nodes,
        "producer_graph_document_nodes",
    )?;
    let index = documents.len();
    let document = Arc::new(document);
    // The admitted node owner also retains this original document on a later
    // identity-index quota refusal. The index copy receives separate text
    // admission; failure grants no edge, compilation or execution authority.
    documents.push(Arc::clone(&document));
    resources
        .charge_metadata_text(document.source().identity.len())
        .map_err(Error::Resource)?;
    identities.insert(document.source().identity.clone(), index);
    Ok(document)
}
fn collect(
    resources: &mut Resources,
    root: PreparedDocument,
    schema: &Arc<CapturedSchema>,
    limits: Limits,
    documents: &mut Vec<Arc<PreparedDocument>>,
    origin: &mut Option<Origin>,
) -> Result<Vec<SuppliedNode>, Error> {
    let mut identities = BTreeMap::new();
    let mut model_nodes = 0;
    own_document(
        root,
        resources,
        documents,
        &mut identities,
        &mut model_nodes,
        limits,
    )?;
    collect_nodes(
        resources,
        limits,
        documents,
        origin,
        identities,
        model_nodes,
        schema,
    )
}
fn admit_entry(
    resources: &mut Resources,
    written: &str,
    schema: &Arc<CapturedSchema>,
    limits: Limits,
) -> Result<(), Error> {
    if limits.graph.nodes == 0 {
        return Err(Error::Limit("producer_graph_nodes"));
    }
    if schema.sources().len() > limits.schema_modules {
        return Err(Error::Limit("producer_graph_schema_modules"));
    }
    let mut schema_bytes = 0;
    for source in schema.sources() {
        charge(
            &mut schema_bytes,
            source.bytes.len(),
            limits.schema_bytes,
            "producer_graph_schema_bytes",
        )?;
    }
    resources
        .ensure_metadata_budget(MetadataLimits {
            bytes: limits.captured_bytes,
            snapshots: limits.snapshots,
            text: limits.graph.text_bytes,
            work: limits.graph.work,
        })
        .map_err(Error::Resource)?;
    if written.len() > crate::specification_source::Limits::default().identity_bytes {
        return Err(Error::Limit("producer_graph_identity_bytes"));
    }
    resources
        .charge_metadata_text(written.len())
        .map_err(Error::Resource)?;
    Ok(())
}
fn collect_nodes(
    resources: &mut Resources,
    limits: Limits,
    documents: &mut Vec<Arc<PreparedDocument>>,
    origin: &mut Option<Origin>,
    mut identities: BTreeMap<String, usize>,
    mut model_nodes: usize,
    schema: &Arc<CapturedSchema>,
) -> Result<Vec<SuppliedNode>, Error> {
    let mut nodes = Vec::new();
    let mut edges = 0;
    let mut index = 0;
    // New documents append to the frontier. Alias edges verify their original
    // spelling and snapshots but reuse the one canonical prepared Arc.
    while index < documents.len() {
        *origin = None;
        let document = Arc::clone(&documents[index]);
        let d = document.model().document();
        resources
            .charge_work(d.nodes().len())
            .map_err(Error::Resource)?;
        let input = d.field(d.root(), "input").ok_or(Error::Boundary("input"))?;
        let N::Mapping(entries) = &d.nodes()[input] else {
            return Err(Error::Boundary("input"));
        };
        let mut producers = Vec::new();
        for &(name, declaration) in entries {
            if !yamaa_core::producer_admission::is_producer_input(d, declaration) {
                continue;
            }
            *origin = None;
            let N::Text(dataset) = &d.nodes()[name] else {
                return Err(Error::Boundary("dataset"));
            };
            resources
                .charge_metadata_text(dataset.len())
                .map_err(Error::Resource)?;
            *origin = Some(Origin {
                document: index,
                dataset: dataset.clone(),
            });
            charge(&mut edges, 1, limits.graph.edges, "producer_graph_edges")?;
            if producers.len() >= limits.graph.metadata.candidates {
                return Err(Error::Limit("producer_candidates"));
            }
            let schema_origin = document
                .written_input_origin(dataset, "schema")
                .ok_or(Error::Boundary("schema_origin"))?;
            let input_origin = document
                .written_input_origin(dataset, "path")
                .ok_or(Error::Boundary("input_origin"))?;
            let identity = resources
                .resolve_from(schema_origin.declaring_source, schema_origin.written)
                .map_err(Error::Resource)?;
            let producer = if let Some(&known) = identities.get(&identity) {
                let prior = &documents[known];
                resources
                    .capture_from(
                        schema_origin.declaring_source,
                        schema_origin.written,
                        prior.source().bytes.len(),
                    )
                    .map_err(Error::Resource)?;
                Arc::clone(prior)
            } else {
                if documents.len() >= limits.graph.nodes {
                    return Err(Error::Limit("producer_graph_nodes"));
                }
                let prepared = file_preparation::prepare_document_from(
                    resources,
                    Some(schema_origin.declaring_source),
                    schema_origin.written,
                    Some(Arc::clone(schema)),
                )
                .map_err(Error::Document)?;
                if prepared.source().identity != identity {
                    return Err(Error::Resource(ResourceError::Changed));
                }
                own_document(
                    prepared,
                    resources,
                    documents,
                    &mut identities,
                    &mut model_nodes,
                    limits,
                )?
            };
            let output = producer
                .written_output_origin()
                .ok_or(Error::Boundary("output_origin"))?;
            let source_identity = resources
                .location_from(input_origin.declaring_source, input_origin.written)
                .map_err(Error::Resource)?;
            let output_identity = resources
                .location_from(output.declaring_source, output.written)
                .map_err(Error::Resource)?;
            let text = [
                dataset.len(),
                identity.len(),
                source_identity.len(),
                output_identity.len(),
            ]
            .into_iter()
            .try_fold(0usize, usize::checked_add)
            .ok_or(Error::Limit("producer_graph_text_bytes"))?;
            resources
                .charge_metadata_text(text)
                .map_err(Error::Resource)?;
            producers.push(SuppliedProducer {
                dataset: dataset.clone(),
                schema_identity: identity,
                source_identity,
                output_identity,
                document: producer,
            });
        }
        nodes.push(SuppliedNode {
            document,
            producers,
        });
        index += 1;
    }
    *origin = None;
    resources
        .verify_captured()
        .map_err(|(identity, error)| Error::Snapshot { identity, error })?;
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Files(std::path::PathBuf);
    impl Drop for Files {
        fn drop(&mut self) {
            if self.0.exists() {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
    }
    fn captured(root: &str, limits: Limits) -> CapturedEntry {
        let mut resources = Resources::new(root, root, &[]).unwrap();
        resources
            .ensure_metadata_budget(MetadataLimits {
                bytes: limits.captured_bytes,
                snapshots: limits.snapshots,
                text: limits.graph.text_bytes,
                work: limits.graph.work,
            })
            .unwrap();
        let environment = crate::file_project::capture_environment(
            &mut resources,
            "environment.yaml",
            yamaa_core::project_function::Language::Python,
            crate::shipped_schema::capture_environment().unwrap(),
        )
        .unwrap();
        FileGraph::capture_entry(
            resources,
            "root.yaml",
            environment,
            crate::shipped_schema::capture().unwrap(),
            limits,
        )
        .unwrap()
    }
    #[test]
    fn retained_original_entry_survives_refusal_to_own_the_graph_identity_index() {
        let path = std::env::temp_dir().join(format!(
            "yamaa-entry-retention-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let files = Files(fs::canonicalize(path).unwrap());
        let root = crate::file_resources::caller_path(&files.0).unwrap();
        let original = b"schema_version: '1.0'\ndomain: TEST\nbase: RAW\nkeys: [ID]\ninput: {RAW: never-read.csv}\ncolumns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]\noutput: {path: never-written.csv, columns: [ID]}\n";
        fs::write(files.0.join("root.yaml"), original).unwrap();
        fs::write(files.0.join("environment.yaml"), b"schema_version: '1.0'\n").unwrap();
        let baseline = captured(&root, Default::default());
        let text = baseline.resources.metadata_usage().1;
        drop(baseline);
        let limited = captured(
            &root,
            Limits {
                graph: yamaa_core::producer_graph::Limits {
                    text_bytes: text,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let rejected = match FileGraph::prepare_captured(limited) {
            Err(rejected) => rejected,
            Ok(_) => panic!("identity index must receive separate text admission"),
        };
        assert!(matches!(
            rejected.error(),
            Error::Resource(ResourceError::Limit)
        ));
        assert_eq!(rejected.capture_reads(), 2);
        assert_eq!(rejected.documents().len(), 1);
        drop(files);
        assert_eq!(rejected.documents()[0].source().bytes, original);
        assert_eq!(
            rejected.documents()[0].written_source_path("RAW"),
            Some("never-read.csv")
        );
        assert_eq!(
            rejected.environment().root().source().bytes,
            b"schema_version: '1.0'\n"
        );
        assert_eq!(rejected.retained_sources(16, 65536).unwrap().len(), 2);
    }
}
