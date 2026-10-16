//! Whole-graph activation and producer-first builds over retained artifact bytes.
//! Ports grant study capture, decoding and bounded output preparation separately;
//! this service accepts no publisher and never reads prospective output files.
use crate::{
    dataset::{self, Execution},
    project_activation::{self, ActivationPort, Bindings, Observations},
    specification_output::{self as output, ArtifactEncoder, OutputReport, PreparedOutput},
    specification_run::{self as run, CapturedAttempt, SourceDecoder, SourcePort},
};
use crate::{
    producer_graph::{Node, PreparedGraph},
    resource::ResourceFailure,
    specification::{PreparedSpecification, SourceDeclaration},
    table::TableAccess,
};
use std::convert::Infallible;
use std::sync::Arc;

/// External study requests retain their canonical document context. Generated
/// producer sources never reach this port, even for location inspection.
pub trait StudyPort {
    type Error;
    fn owned_table(
        &self,
        _node: &Node,
        _source: &SourceDeclaration,
    ) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        None
    }
    fn resource_failure(&self, error: &Self::Error) -> Option<ResourceFailure>;
    fn inspect(&mut self, node: &Node, source: &SourceDeclaration) -> Result<(), Self::Error>;
    fn capture_reads(&self) -> usize;
    fn capture(
        &mut self,
        node: &Node,
        source: &SourceDeclaration,
    ) -> Result<Arc<[u8]>, Self::Error>;
}
/// Encode through the exact node's admitted profile/precision. The engine enters
/// this port only after execution and ordinary output gates have passed.
pub trait CodecPort {
    type Error;
    fn encode(
        &mut self,
        plan: &PreparedSpecification,
        dataset: &dataset::Dataset,
        projection: &[usize],
    ) -> Result<Vec<u8>, Self::Error>;
}
/// Prepare observations from the retained attempt without granting publication.
pub trait ReportPort: OutputReport {
    fn prepare_diagnostics<C, D, T: TableAccess>(
        &mut self,
        _node: &Node,
        _attempt: &CapturedAttempt<C, D, T>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn select<C, D, T: TableAccess>(
        &mut self,
        node: &Node,
        attempt: &CapturedAttempt<C, D, T>,
    ) -> Result<(), Self::Error>;
}
pub struct Ports<'a, A, P, D, R, C> {
    pub activation: &'a mut A,
    pub study: &'a mut P,
    pub decoder: &'a mut D,
    pub report: &'a mut R,
    pub codec: &'a mut C,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError<E> {
    #[error("producer build external: {0:?}")]
    External(E),
    #[error("producer build producer unavailable: producer={producer:?}")]
    ProducerUnavailable { producer: usize },
}
#[derive(Debug, thiserror::Error)]
pub enum DecodeError<E> {
    #[error("producer build codec: {0:?}")]
    Codec(E),
    #[error("producer build contract: {0:?}")]
    Contract(crate::producer_contract::Error),
    #[error("producer build metadata: {0:?}")]
    Metadata(crate::diagnostic::Diagnostic),
    #[error("producer build source ordinal: {0}")]
    Ordinal(crate::source_ordinal::Error),
}
#[derive(Debug, thiserror::Error)]
pub enum Failure<E> {
    #[error("producer build incomplete")]
    Incomplete,
    #[error("producer build activation: {0:?}")]
    Activation(project_activation::Failure<E>),
    #[error("producer build binding: node={node:?}, slot={slot:?}")]
    Binding { node: usize, slot: usize },
    #[error("producer build node: node={node:?}")]
    Node { node: usize },
}
pub(crate) type OutputFailure<R, C> = output::CompleteError<R, C, Infallible>;
type PreparedNode<R, C> = Result<
    PreparedOutput<<R as OutputReport>::Report>,
    OutputFailure<<R as OutputReport>::Error, <C as CodecPort>::Error>,
>;
type NodePrefix<'a, P, D, T, R, RE, CE> =
    &'a [NodeAttempt<<P as StudyPort>::Error, D, T, R, RE, CE>];
pub struct NodeAttempt<C, D, T: TableAccess, R, RE, CE> {
    pub node: usize,
    pub dataset: CapturedAttempt<CaptureError<C>, DecodeError<D>, T>,
    /// None means output preparation was never entered or was interrupted.
    pub output: Result<Option<PreparedOutput<R>>, OutputFailure<RE, CE>>,
}
pub struct Attempt<C, D, T: TableAccess, R, RE, CE, A> {
    pub activation: Observations,
    /// Entered nodes only, in actual producer-first execution order.
    pub nodes: Vec<NodeAttempt<C, D, T, R, RE, CE>>,
    pub outcome: Result<(), Failure<A>>,
}
impl<C, D, T: TableAccess, R, RE, CE, A> Default for Attempt<C, D, T, R, RE, CE, A> {
    fn default() -> Self {
        Self {
            activation: Default::default(),
            nodes: Vec::new(),
            outcome: Err(Failure::Incomplete),
        }
    }
}
pub type BuildAttempt<A, P, D, R, C> = Attempt<
    <P as StudyPort>::Error,
    <D as SourceDecoder>::Error,
    <D as SourceDecoder>::Table,
    <R as OutputReport>::Report,
    <R as OutputReport>::Error,
    <C as CodecPort>::Error,
    <A as ActivationPort>::Error,
>;

/// Own the complete precompiled closure and environment; callers cannot replace
/// node plans, dependencies, local slots or activation definitions at build.
#[derive(Debug)]
pub struct PreparedBuild {
    graph: PreparedGraph,
}
impl PreparedBuild {
    pub(crate) fn new(graph: PreparedGraph) -> Self {
        Self { graph }
    }
    pub fn metadata(&self) -> &PreparedGraph {
        &self.graph
    }
    /// Borrow coherent reporting facts from the complete consumed closure.
    /// Metadata nodes alone still expose no compiled plan; the whole immutable
    /// owner must remain alive for every returned borrow.
    pub fn report_contexts(
        &self,
    ) -> impl ExactSizeIterator<Item = (&Node, &PreparedSpecification)> {
        self.metadata().nodes().iter().zip(self.graph.plans())
    }
    /// Execute the complete closure once and seal its actual publication authority.
    /// Caller-held evidence retains every entered activation/node across unwinds.
    pub fn build_into<A, P, D, R, C>(
        &self,
        ports: Ports<'_, A, P, D, R, C>,
        attempt: &mut BuildAttempt<A, P, D, R, C>,
        scope: crate::application_observer::Scope<'_>,
    ) -> Result<crate::producer_publication::PreparedPublication, &'static str>
    where
        A: ActivationPort,
        P: StudyPort,
        D: SourceDecoder,
        D::Table: TableAccess<Error = A::Error>,
        R: ReportPort,
        C: CodecPort,
    {
        *attempt = Attempt::default();
        let graph = self.metadata();
        let plans = self.graph.plans().collect::<Vec<_>>();
        let environment = graph.environment();
        let selected = graph
            .called_functions()
            .iter()
            .map(|&index| &environment.functions()[index])
            .collect::<Vec<_>>();
        let activated = if selected.is_empty() {
            Vec::new()
        } else {
            scope.phase(crate::application_observer::Phase::Activation);
            let result = project_activation::activate_references_observed(
                environment.language().expect("admitted called language"),
                environment.lock(),
                &selected,
                &mut scope.activation(ports.activation),
                &mut attempt.activation,
            );
            scope.exit(
                crate::application_observer::Phase::Activation,
                crate::application_observer::Outcome::from_result(&result),
            );
            match result {
                Ok(functions) => functions,
                Err(error) => {
                    attempt.outcome = Err(Failure::Activation(error));
                    return Err("producer_build_failed");
                }
            }
        };
        // Check every node before the first study request, including nodes which
        // would otherwise only be reached after earlier producer effects.
        for (node, (metadata, plan)) in graph.nodes().iter().zip(&plans).enumerate() {
            let calls = plan.project_calls().map_or(&[][..], |calls| calls.plans());
            if let Err(slot) = Bindings::projected(
                &activated,
                metadata.activation_slots(),
                calls,
                ports.activation,
            ) {
                attempt.outcome = Err(Failure::Binding { node, slot });
                return Err("producer_build_failed");
            }
        }
        for &index in graph.order() {
            let node = &graph.nodes()[index];
            let node_scope = scope.for_node(index, node.identity());
            let plan = plans[index];
            attempt.nodes.push(NodeAttempt {
                node: index,
                dataset: CapturedAttempt::new(plan.source()),
                output: Ok(None),
            });
            let last = attempt.nodes.len() - 1;
            let (prefix, entered) = attempt.nodes.split_at_mut(last);
            let entered = &mut entered[0];
            let mut sources = Sources {
                node,
                prefix,
                external: ports.study,
            };
            let calls = plan.project_calls().map_or(&[][..], |calls| calls.plans());
            let mut functions =
                Bindings::projected(&activated, node.activation_slots(), calls, ports.activation)
                    .expect("complete graph projections preflighted before sources");
            run::execute_into(
                plan,
                &mut sources,
                &mut NodeDecoder {
                    node,
                    decoder: ports.decoder,
                },
                &mut functions,
                &mut entered.dataset,
                node_scope,
            );
            node_scope.phase(crate::application_observer::Phase::Output);
            let output_result = (|| {
                if let Err(error) = ports.report.prepare_diagnostics(node, &entered.dataset) {
                    entered.output = Err(output::CompleteError::Report(error));
                    attempt.outcome = Err(Failure::Node { node: index });
                    return Err("producer_build_failed");
                }
                let execution = entered
                    .dataset
                    .result
                    .as_ref()
                    .ok()
                    .and_then(|execution| execution.result.as_ref().ok());
                let Some(execution) = execution else {
                    attempt.outcome = Err(Failure::Node { node: index });
                    return Err("producer_build_failed");
                };
                let prepared = prepare_output(
                    node,
                    plan,
                    &entered.dataset,
                    execution,
                    ports.report,
                    ports.codec,
                );
                entered.output = prepared.map(Some);
                let artifact = entered
                    .output
                    .as_ref()
                    .ok()
                    .and_then(Option::as_ref)
                    .and_then(PreparedOutput::artifact);
                let Some(_) = artifact else {
                    attempt.outcome = Err(Failure::Node { node: index });
                    return Err("producer_build_failed");
                };
                Ok(())
            })();
            node_scope.exit(
                crate::application_observer::Phase::Output,
                crate::application_observer::Outcome::from_result(&output_result),
            );
            output_result?;
        }
        attempt.outcome = Ok(());
        crate::producer_publication::PreparedPublication::from_actual(graph, attempt)
    }
}

struct NodeDecoder<'a, D> {
    node: &'a Node,
    decoder: &'a mut D,
}
impl<D: SourceDecoder> SourceDecoder for NodeDecoder<'_, D> {
    type Error = DecodeError<D::Error>;
    type Table = D::Table;
    fn decode_owned(
        &mut self,
        source: &SourceDeclaration,
        table: &(dyn std::any::Any + Send + Sync),
        interpretation: crate::specification::SourceInterpretation,
        contract: Option<&crate::producer_contract::Contract>,
    ) -> Option<Result<Self::Table, Self::Error>> {
        self.decoder
            .decode_owned(source, table, interpretation, contract)
            .map(|result| result.map_err(DecodeError::Codec))
    }
    fn continue_after(&self, error: &Self::Error) -> bool {
        match error {
            DecodeError::Codec(error) => self.decoder.continue_after(error),
            DecodeError::Metadata(_) => true,
            DecodeError::Contract(_) | DecodeError::Ordinal(_) => false,
        }
    }
    fn decode(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: crate::specification::SourceInterpretation,
        contract: Option<&crate::producer_contract::Contract>,
    ) -> Result<D::Table, Self::Error> {
        let declaration = self
            .node
            .producers()
            .iter()
            .find(|producer| producer.dataset() == source.name);
        let table = self
            .decoder
            .decode(
                source,
                bytes,
                interpretation,
                declaration.map(|producer| producer.contract()).or(contract),
            )
            .map_err(DecodeError::Codec)?;
        if let Some(declaration) = declaration {
            let stored =
                crate::source_ordinal::stored_fields(table.schema(), source.ordinal.as_deref())
                    .map_err(DecodeError::Ordinal)?;
            let actual = stored
                .iter()
                .map(|field| (field.name.as_str(), field.kind))
                .collect::<Vec<_>>();
            if let Some(diagnostic) = declaration
                .contract()
                .validate_typed_fields(&source.name, &actual)
                .map_err(DecodeError::Contract)?
            {
                return Err(DecodeError::Metadata(diagnostic));
            }
        }
        Ok(table)
    }
}
fn prepare_output<SC, SD, T: TableAccess, R: ReportPort, C: CodecPort>(
    node: &Node,
    plan: &PreparedSpecification,
    attempt: &CapturedAttempt<SC, SD, T>,
    execution: &Execution,
    report: &mut R,
    codec: &mut C,
) -> PreparedNode<R, C> {
    report
        .select(node, attempt)
        .map_err(output::CompleteError::Report)?;
    output::prepare(
        plan,
        Some(execution),
        report,
        &mut NodeCodec { plan, codec },
    )
}

struct NodeCodec<'a, C> {
    plan: &'a PreparedSpecification,
    codec: &'a mut C,
}
impl<C: CodecPort> ArtifactEncoder for NodeCodec<'_, C> {
    type Error = C::Error;
    fn encode(
        &mut self,
        dataset: &dataset::Dataset,
        projection: &[usize],
    ) -> Result<Vec<u8>, Self::Error> {
        self.codec.encode(self.plan, dataset, projection)
    }
}

struct Sources<'a, P: StudyPort, D, T: TableAccess, R, RE, CE> {
    node: &'a Node,
    prefix: NodePrefix<'a, P, D, T, R, RE, CE>,
    external: &'a mut P,
}
impl<P: StudyPort, D, T: TableAccess, R, RE, CE> Sources<'_, P, D, T, R, RE, CE> {
    fn producer(&self, source: &SourceDeclaration) -> Option<usize> {
        self.node
            .producers()
            .iter()
            .zip(self.node.dependencies())
            .find_map(|(declaration, &dependency)| {
                (declaration.dataset() == source.name).then_some(dependency)
            })
    }
    fn artifact(&self, producer: usize) -> Result<&output::Artifact, CaptureError<P::Error>> {
        self.prefix
            .iter()
            .find(|attempt| attempt.node == producer)
            .and_then(|attempt| attempt.output.as_ref().ok())
            .and_then(Option::as_ref)
            .and_then(PreparedOutput::artifact)
            .ok_or(CaptureError::ProducerUnavailable { producer })
    }
}
impl<P: StudyPort, D, T: TableAccess, R, RE, CE> SourcePort for Sources<'_, P, D, T, R, RE, CE> {
    type Error = CaptureError<P::Error>;
    fn owned_table(
        &self,
        source: &SourceDeclaration,
    ) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        if self.producer(source).is_some() {
            None
        } else {
            self.external.owned_table(self.node, source)
        }
    }
    fn resource_failure(&self, error: &Self::Error) -> Option<ResourceFailure> {
        match error {
            CaptureError::External(error) => self.external.resource_failure(error),
            CaptureError::ProducerUnavailable { .. } => None,
        }
    }
    fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Self::Error> {
        match self.producer(source) {
            Some(producer) => self.artifact(producer).map(|_| ()),
            None => self
                .external
                .inspect(self.node, source)
                .map_err(CaptureError::External),
        }
    }
    fn capture_reads(&self) -> usize {
        self.external.capture_reads()
    }
    fn capture(
        &mut self,
        source: &SourceDeclaration,
        origin: &mut Option<run::SourceOrigin>,
    ) -> Result<Arc<[u8]>, Self::Error> {
        match self.producer(source) {
            Some(producer) => {
                *origin = Some(run::SourceOrigin::Generated);
                self.artifact(producer)
                    .map(output::Artifact::retained_bytes)
            }
            None => {
                *origin = Some(run::SourceOrigin::Resource);
                self.external
                    .capture(self.node, source)
                    .map_err(CaptureError::External)
            }
        }
    }
}
