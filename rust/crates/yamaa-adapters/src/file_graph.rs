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
impl FileGraph {
    /// Capture a canonical closure iteratively through the approved reader,
    /// then compile the whole graph once against the admitted environment.
    /// No activation, data, runtime or publisher port is accepted or exposed.
    pub fn prepare(
        mut resources: Resources,
        written: &str,
        environment: OwnedEnvironment,
        schema: Arc<CapturedSchema>,
        limits: Limits,
    ) -> Result<Self, Box<RejectedGraph>> {
        let (environment, captured) = environment.into_parts();
        let mut documents = Vec::new();
        let mut origin = None;
        let mut entry = None;
        let result = collect(
            &mut resources,
            written,
            &schema,
            limits,
            &mut documents,
            &mut origin,
            &mut entry,
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
                entry: entry.expect("successful collection owns its admitted entry"),
                environment: captured,
                resources,
            }),
            Err(error) => Err(Box::new(RejectedGraph {
                error,
                entry,
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
    resources
        .charge_metadata_text(document.source().identity.len())
        .map_err(Error::Resource)?;
    let index = documents.len();
    let document = Arc::new(document);
    identities.insert(document.source().identity.clone(), index);
    documents.push(Arc::clone(&document));
    Ok(document)
}
fn collect(
    resources: &mut Resources,
    written: &str,
    schema: &Arc<CapturedSchema>,
    limits: Limits,
    documents: &mut Vec<Arc<PreparedDocument>>,
    origin: &mut Option<Origin>,
    entry: &mut Option<String>,
) -> Result<Vec<SuppliedNode>, Error> {
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
        .admit_metadata_budget(MetadataLimits {
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
    *entry = Some(written.into());
    let mut identities = BTreeMap::new();
    let mut model_nodes = 0;
    let root =
        file_preparation::prepare_document_from(resources, None, written, Some(Arc::clone(schema)))
            .map_err(Error::Document)?;
    own_document(
        root,
        resources,
        documents,
        &mut identities,
        &mut model_nodes,
        limits,
    )?;
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
            if d.field(declaration, "schema").is_none() {
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
