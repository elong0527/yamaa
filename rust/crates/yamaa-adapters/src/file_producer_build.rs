//! Whole-graph native builds retain original evidence and explicit Save authority.
//! Original metadata and attempts remain owned after this resource owner drops.
use crate::{
    file_resources::{Error as ResourceError, Resources, RetainedSources},
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
    dataset::Dataset,
    producer_build::{self as engine, CodecPort, PreparedBuild, ReportPort, StudyPort},
    project_activation::ActivationPort,
};
use yamaa_core::{
    producer_graph::{Node, PreparedGraph},
    specification::{PreparedSpecification, SourceDeclaration},
};

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
    pub resources: RetainedSources,
}
type ActualBuild<E, R, RE> = (
    Attempt<E, R, RE>,
    Result<yamaa_core::producer_publication::PreparedPublication, &'static str>,
);
impl<E, R, RE> Attempt<E, R, RE> {
    pub fn accepted(&self) -> bool {
        self.boundary.is_ok() && self.graph.outcome.is_ok()
    }
}
/// Immutable evidence and publication authority produced by the actual native
/// build. Arbitrary public Attempt records cannot be promoted to this owner.
pub struct OwnedAttempt<E> {
    attempt: crate::producer_report::NativeAttempt<E>,
    report_publication: Option<Arc<crate::producer_report_publication::Publication>>,
    publication: Option<Arc<crate::producer_publication::Publication>>,
    publication_refusal: Option<&'static str>,
}
impl<E> std::ops::Deref for OwnedAttempt<E> {
    type Target = crate::producer_report::NativeAttempt<E>;
    fn deref(&self) -> &Self::Target {
        &self.attempt
    }
}
impl<E> OwnedAttempt<E> {
    /// Report and primary authority, sealed when any node declares
    /// `output.report`.
    pub(crate) fn report_publication(
        &self,
    ) -> Option<&Arc<crate::producer_report_publication::Publication>> {
        self.report_publication.as_ref()
    }
    pub(crate) fn publication(&self) -> Option<&Arc<crate::producer_publication::Publication>> {
        self.publication.as_ref()
    }
    pub(crate) fn publication_refusal(&self) -> Option<&'static str> {
        self.publication_refusal
    }
}
/// Serialized artifacts a project run has already built, by producer. A saved
/// producer link whose producer and artifact location match a held artifact
/// reads those bytes instead of the saved file, still under the producer's
/// output contract.
#[derive(Clone, Debug, Default)]
pub struct HeldArtifacts {
    artifacts: std::collections::BTreeMap<String, (String, Arc<[u8]>)>,
}
impl HeldArtifacts {
    pub fn new() -> Self {
        Self::default()
    }
    /// Hold the artifact the producer with canonical identity `producer`
    /// built for its `output.path` location `location`.
    pub fn insert(&mut self, producer: String, location: String, bytes: Arc<[u8]>) {
        self.artifacts.insert(producer, (location, bytes));
    }
    /// The held artifact of `producer`, when it was built for `location`.
    pub fn get(&self, producer: &str, location: &str) -> Option<Arc<[u8]>> {
        self.artifacts
            .get(producer)
            .filter(|(held, _)| crate::producer_publication::same_location(held, location))
            .map(|(_, bytes)| Arc::clone(bytes))
    }
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }
}
pub struct FileBuild {
    provenance: Arc<Provenance>,
    resources: Resources,
    metadata_snapshots: usize,
    held: Option<Arc<HeldArtifacts>>,
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
            held: None,
        }
    }
    pub fn provenance(&self) -> &Arc<Provenance> {
        &self.provenance
    }
    /// Offer artifacts the caller already built to this graph's saved links.
    /// Links without a held artifact read the saved file their path names.
    pub fn hold(&mut self, held: Arc<HeldArtifacts>) {
        self.held = Some(held);
    }
    /// Use complete native original-document reports as the ordinary output
    /// gate. Returned attempts own every report and all original typed failures.
    pub fn build_reported<A: ActivationPort>(
        &mut self,
        activation: &mut A,
        id: crate::specification_report::Identity<'_>,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Attempt<A::Error, crate::report_storage::StoredReport, crate::specification_report::Error>
    {
        let provenance = Arc::clone(&self.provenance);
        let mut report = crate::producer_report::NativeReport::new(&provenance, id);
        self.build(activation, &mut report, scope)
    }
    /// Seal only the actual boundary. Failed graphs retain their original
    /// evidence and the report authority of every node that ran, with primary
    /// authority revoked.
    pub fn build_owned_reported<A: ActivationPort>(
        &mut self,
        activation: &mut A,
        id: crate::specification_report::Identity<'_>,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> OwnedAttempt<A::Error> {
        let provenance = Arc::clone(&self.provenance);
        let mut report = crate::producer_report::NativeReport::new(&provenance, id);
        let (attempt, publication) = self.build_actual(activation, &mut report, scope);
        let reported = self
            .provenance
            .report_views()
            .any(|view| view.compiled().output_report().is_some());
        let (report_publication, publication, publication_refusal) = if reported {
            (
                Some(Arc::new(
                    crate::producer_report_publication::Publication::from_build(
                        &attempt,
                        publication,
                        &self.resources,
                    ),
                )),
                None,
                None,
            )
        } else if attempt.accepted() {
            match publication {
                Ok(prepared) => (
                    None,
                    Some(Arc::new(
                        crate::producer_publication::Publication::from_build(
                            prepared,
                            &self.provenance,
                            &self.resources,
                        ),
                    )),
                    None,
                ),
                Err(code) => (None, None, Some(code)),
            }
        } else {
            (None, None, None)
        };
        OwnedAttempt {
            attempt,
            report_publication,
            publication,
            publication_refusal,
        }
    }
    /// Fresh native scope and union activation on every attempt. Only the initial
    /// metadata prefix is verified before activation; cached study snapshots are
    /// inspected/reread by declared study requests after whole-graph activation.
    pub fn build<A: ActivationPort, R: ReportPort>(
        &mut self,
        activation: &mut A,
        report: &mut R,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Attempt<A::Error, R::Report, R::Error> {
        self.build_actual(activation, report, scope).0
    }
    fn build_actual<A: ActivationPort, R: ReportPort>(
        &mut self,
        activation: &mut A,
        report: &mut R,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> ActualBuild<A::Error, R::Report, R::Error> {
        let mut attempt = Attempt {
            provenance: Arc::clone(&self.provenance),
            boundary: Ok(()),
            graph: Default::default(),
            resources: Vec::new(),
        };
        let ready = self
            .resources
            .verify_prefix(self.metadata_snapshots)
            .map_err(|(identity, error)| BoundaryFailure::Resource {
                identity: Some(identity),
                error,
            });
        let mut publication = Err("producer_build_failed");
        attempt.boundary = match ready {
            Err(error) => Err(error),
            Ok(()) => match catch_unwind(AssertUnwindSafe(|| {
                publication = self.provenance.build.build_into(
                    engine::Ports {
                        activation,
                        study: &mut Study {
                            resources: &mut self.resources,
                            held: self.held.as_deref(),
                        },
                        decoder: &mut Decoder::<A::Error>::default(),
                        report,
                        codec: &mut Codec,
                    },
                    &mut attempt.graph,
                    scope,
                );
            })) {
                Ok(()) => Ok(()),
                Err(error) => Err(BoundaryFailure::Unwind(error)),
            },
        };
        attempt.resources = self.resources.retain_sources();
        (attempt, publication)
    }
}
struct Study<'a> {
    resources: &'a mut Resources,
    held: Option<&'a HeldArtifacts>,
}
impl StudyPort for Study<'_> {
    type Error = ResourceError;
    fn owned_table(
        &self,
        node: &Node,
        source: &SourceDeclaration,
    ) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        self.resources
            .owned_table_from(node.identity(), &source.path)
    }
    fn held(
        &self,
        _node: &Node,
        _source: &SourceDeclaration,
        producer: &yamaa_core::producer_admission::Declaration,
    ) -> Option<Arc<[u8]>> {
        self.held?
            .get(producer.producer_identity(), producer.artifact_identity())
    }
    fn resource_failure(
        &self,
        error: &Self::Error,
    ) -> Option<yamaa_core::resource::ResourceFailure> {
        use yamaa_core::specification_run::SpecificationSourcePort;
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
    ) -> Result<Arc<[u8]>, Self::Error> {
        self.resources
            .capture_from(node.identity(), &source.path)
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
    ) -> Result<Vec<u8>, CodecError> {
        match plan.output_profile() {
            Some("csv") => crate::csv_artifact::render(dataset, projection, plan.output_decimals())
                .map_err(CodecError::Csv),
            Some("parquet") => {
                crate::parquet_artifact::render(dataset, projection).map_err(CodecError::Parquet)
            }
            _ => Err(CodecError::Profile),
        }
    }
}
