//! Separate whole-graph native builds, with no public host dispatch or publisher.
//! Original metadata and attempts remain owned after this resource owner drops.
use crate::{
    file_resources::{
        Error as ResourceError, MetadataLimits, Resources, RetainedSources, RetentionFailure,
    },
    project_source::CapturedEnvironment,
    project_source_decoder::{Decoder, Snapshot},
    specification_source::PreparedDocument,
};
use std::{
    any::Any,
    convert::Infallible,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};
use yamaa_core::{
    producer_graph::{Node, PreparedGraph},
    specification::{PreparedSpecification, SourceDeclaration},
};
use yamaa_engine::{
    dataset::Dataset,
    producer_build::{self as engine, CodecPort, PreparedBuild, ReportPort, StudyPort},
    project_activation::ActivationPort,
};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub engine: engine::Limits,
    pub resource_bytes: usize,
    pub resource_text: usize,
    pub resource_work: usize,
    pub snapshots: usize,
    pub evidence_text: usize,
    pub evidence_aliases: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            engine: Default::default(),
            resource_bytes: 67_108_864,
            resource_text: 16_777_216,
            resource_work: 67_108_864,
            snapshots: 1024,
            evidence_text: 16_777_216,
            evidence_aliases: 1024,
        }
    }
}
pub struct Provenance {
    build: PreparedBuild,
    documents: Vec<Arc<PreparedDocument>>,
    entry: String,
    environment: CapturedEnvironment,
}
/// A read-only report view borrowed from the exact consumed whole graph. Its
/// constructor is private: callers cannot pair an unrelated document and plan.
pub struct NodeReport<'a> {
    node: &'a Node,
    document: &'a PreparedDocument,
    compiled: &'a PreparedSpecification,
}
impl NodeReport<'_> {
    pub fn node(&self) -> &Node {
        self.node
    }
    pub(crate) fn document(&self) -> &PreparedDocument {
        self.document
    }
    pub(crate) fn compiled(&self) -> &PreparedSpecification {
        self.compiled
    }
}
impl Provenance {
    pub fn metadata(&self) -> &PreparedGraph {
        self.build.metadata()
    }
    pub fn documents(&self) -> &[Arc<PreparedDocument>] {
        &self.documents
    }
    pub fn entry(&self) -> &str {
        &self.entry
    }
    pub fn environment(&self) -> &CapturedEnvironment {
        &self.environment
    }
    /// The original document order matches the sealed canonical node order.
    /// Borrowing this iterator neither recompiles nor enters any host/resource
    /// authority, and it cannot replace the retained environment or node plans.
    pub fn report_views(&self) -> impl ExactSizeIterator<Item = NodeReport<'_>> {
        self.build
            .report_contexts()
            .zip(&self.documents)
            .map(|((node, compiled), document)| NodeReport {
                node,
                document,
                compiled,
            })
    }
}
pub enum BoundaryFailure {
    Resource {
        identity: Option<String>,
        error: ResourceError,
    },
    Unwind(Box<dyn Any + Send>),
}
impl std::fmt::Debug for BoundaryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource { identity, error } => f
                .debug_struct("Resource")
                .field("identity", identity)
                .field("error", error)
                .finish(),
            Self::Unwind(_) => f.write_str("Unwind(..)"),
        }
    }
}
#[derive(Debug)]
pub enum CodecError {
    Csv(crate::csv_artifact::Error<Infallible>),
    Parquet(crate::parquet_artifact::Error<Infallible>),
    Profile,
}
pub type GraphAttempt<E, R, RE> = engine::Attempt<
    ResourceError,
    crate::specification_run::Error,
    Snapshot<E>,
    R,
    RE,
    CodecError,
    E,
>;
pub struct Attempt<E, R, RE> {
    pub provenance: Arc<Provenance>,
    pub boundary: Result<(), BoundaryFailure>,
    pub graph: GraphAttempt<E, R, RE>,
    /// Raw metadata and cached study bytes, including originals after Changed.
    /// A retention quota refusal keeps its own prefix beside the original run.
    pub resources: Result<RetainedSources, RetentionFailure>,
}
impl<E, R, RE> Attempt<E, R, RE> {
    pub fn accepted(&self) -> bool {
        self.boundary.is_ok() && self.resources.is_ok() && self.graph.outcome.is_ok()
    }
}
pub struct FileBuild {
    provenance: Arc<Provenance>,
    resources: Resources,
    metadata_snapshots: usize,
}
impl FileBuild {
    pub(crate) fn new(
        build: PreparedBuild,
        documents: Vec<Arc<PreparedDocument>>,
        entry: String,
        environment: CapturedEnvironment,
        resources: Resources,
        metadata_snapshots: usize,
    ) -> Self {
        Self {
            provenance: Arc::new(Provenance {
                build,
                documents,
                entry,
                environment,
            }),
            resources,
            metadata_snapshots,
        }
    }
    pub fn provenance(&self) -> &Arc<Provenance> {
        &self.provenance
    }
    /// Use complete native original-document reports as the ordinary output
    /// gate. Returned attempts own every report and all original typed failures.
    pub fn build_reported<A: ActivationPort>(
        &mut self,
        activation: &mut A,
        id: crate::specification_report::Identity<'_>,
        limits: Limits,
    ) -> Attempt<A::Error, serde_json::Value, crate::specification_report::Error> {
        let provenance = Arc::clone(&self.provenance);
        let mut report = crate::producer_report::NativeReport::new(&provenance, id);
        self.build(activation, &mut report, limits)
    }
    /// Fresh native scope and union activation on every attempt. Only the initial
    /// metadata prefix is verified before activation; cached study snapshots are
    /// inspected/reread by declared study requests after whole-graph activation.
    pub fn build<A: ActivationPort, R: ReportPort>(
        &mut self,
        activation: &mut A,
        report: &mut R,
        limits: Limits,
    ) -> Attempt<A::Error, R::Report, R::Error> {
        let mut attempt = Attempt {
            provenance: Arc::clone(&self.provenance),
            boundary: Ok(()),
            graph: Default::default(),
            resources: Ok(Vec::new()),
        };
        let ready = self
            .resources
            .admit_metadata_budget(MetadataLimits {
                bytes: limits.resource_bytes,
                snapshots: limits.snapshots,
                text: limits.resource_text,
                work: limits.resource_work,
            })
            .map_err(|error| BoundaryFailure::Resource {
                identity: None,
                error,
            })
            .and_then(|()| {
                self.resources
                    .verify_prefix(self.metadata_snapshots)
                    .map_err(|(identity, error)| BoundaryFailure::Resource {
                        identity: Some(identity),
                        error,
                    })
            });
        attempt.boundary = match ready {
            Err(error) => Err(error),
            Ok(()) => match catch_unwind(AssertUnwindSafe(|| {
                self.provenance.build.build_into(
                    engine::Ports {
                        activation,
                        study: &mut Study {
                            resources: &mut self.resources,
                        },
                        decoder: &mut Decoder::<A::Error>::default(),
                        report,
                        codec: &mut Codec,
                    },
                    limits.engine,
                    &mut attempt.graph,
                );
            })) {
                Ok(()) => Ok(()),
                Err(error) => Err(BoundaryFailure::Unwind(error)),
            },
        };
        self.resources.finish_metadata_budget();
        attempt.resources = self
            .resources
            .retain_sources(limits.evidence_aliases, limits.evidence_text);
        attempt
    }
}
struct Study<'a> {
    resources: &'a mut Resources,
}
impl StudyPort for Study<'_> {
    type Error = ResourceError;
    fn resource_failure(
        &self,
        error: &Self::Error,
    ) -> Option<yamaa_core::resource::ResourceFailure> {
        use yamaa_engine::specification_run::SourcePort;
        self.resources.resource_failure(error)
    }
    fn inspect(&mut self, node: &Node, source: &SourceDeclaration) -> Result<(), Self::Error> {
        self.resources.inspect_from(node.identity(), &source.path)
    }
    fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    fn capture(
        &mut self,
        node: &Node,
        source: &SourceDeclaration,
        maximum: usize,
    ) -> Result<Arc<[u8]>, Self::Error> {
        self.resources
            .capture_from(node.identity(), &source.path, maximum)
            .map(|(bytes, _)| bytes)
    }
}
struct Codec;
impl CodecPort for Codec {
    type Error = CodecError;
    fn encode(
        &mut self,
        plan: &PreparedSpecification,
        dataset: &Dataset,
        projection: &[usize],
        maximum: usize,
    ) -> Result<Vec<u8>, CodecError> {
        match plan.output_profile() {
            Some("csv") => crate::csv_artifact::render_with_decimals(
                dataset,
                projection,
                plan.output_decimals(),
                maximum,
            )
            .map_err(CodecError::Csv),
            Some("parquet") => crate::parquet_artifact::render(
                dataset,
                projection,
                crate::parquet_artifact::Limits {
                    output_bytes: maximum,
                    staged_bytes: 8_388_608,
                    cells: 1_048_576,
                    columns: 4096,
                },
            )
            .map_err(CodecError::Parquet),
            _ => Err(CodecError::Profile),
        }
    }
}
