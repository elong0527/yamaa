//! Whole-graph activation and producer-first builds over retained artifact bytes.
//! Ports grant study capture, decoding and bounded output preparation separately;
//! this service accepts no publisher and never reads prospective output files.
use crate::{
    dataset::{self, Execution},
    project_activation::{self, ActivationPort, Bindings, Observations},
    specification_output::{self as output, ArtifactEncoder, OutputReport, PreparedOutput},
    specification_run::{self as run, CapturedAttempt, SourceDecoder, SourcePort},
};
use alloc::{sync::Arc, vec::Vec};
use core::{cell::Cell, convert::Infallible};
use yamaa_core::{
    producer_graph::{BuildGraph, Node, PreparedGraph},
    resource::ResourceFailure,
    specification::{PreparedSpecification, SourceDeclaration},
    table::TableAccess,
};

/// External study requests retain their canonical document context. Generated
/// producer sources never reach this port, even for location inspection.
pub trait StudyPort {
    type Error;
    fn resource_failure(&self, _error: &Self::Error) -> Option<ResourceFailure> {
        None
    }
    fn inspect(&mut self, node: &Node, source: &SourceDeclaration) -> Result<(), Self::Error>;
    fn capture_reads(&self) -> usize;
    fn capture(
        &mut self,
        node: &Node,
        source: &SourceDeclaration,
        byte_limit: usize,
    ) -> Result<Arc<[u8]>, Self::Error>;
}
/// Reserve graph-wide codec ownership and decoding work before constructing a
/// table. Ports must enforce these capacities before their storage grows; the
/// engine independently admits the resulting complete shape before retaining it.
#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub cells: usize,
    pub storage_bytes: usize,
    pub work_bytes: usize,
}
pub trait DecodePort: SourceDecoder {
    fn decode_bounded(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        contract: Option<&yamaa_core::producer_contract::Contract>,
        limits: DecodeLimits,
    ) -> Result<Self::Table, DecodeError<Self::Error>>;
    /// Carry the authored fixed-source interpretation through producer routing.
    /// Trusted legacy ports retain their explicit table profile; canonical ODM
    /// coordinates and identifiers are independently admitted by the engine.
    fn decode_bounded_interpreted(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        contract: Option<&yamaa_core::producer_contract::Contract>,
        limits: DecodeLimits,
        _interpretation: yamaa_core::specification::SourceInterpretation,
    ) -> Result<Self::Table, DecodeError<Self::Error>> {
        self.decode_bounded(source, bytes, contract, limits)
    }
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
        byte_limit: usize,
    ) -> Result<Vec<u8>, Self::Error>;
}
/// Select the original node context and enforce this report's complete byte
/// budget. Reports own observations only; no method grants publication authority.
pub trait ReportPort: OutputReport {
    /// Default report ports retain the conservative equal share. A native port
    /// may pool this capacity only while admitting cumulative actual encoded
    /// lengths and retained storage before making any artifact available.
    fn report_allowance(&mut self, total: usize, nodes: usize) -> usize {
        total / nodes
    }
    /// Prepare bounded diagnostic output from this actual entered attempt before
    /// its primary can become available to any dependent. This also runs after
    /// an execution/source failure, preserving the original attempt unchanged.
    /// The port must admit allocation against both supplied capacities and keep
    /// separately completed diagnostic evidence when later preparation fails.
    /// Returning a byte count consumes the graph's shared held-output budget;
    /// it never grants publication authority or replaces an original failure.
    fn prepare_diagnostics<C, D, T: TableAccess>(
        &mut self,
        _node: &Node,
        _attempt: &CapturedAttempt<C, D, T>,
        _output_bytes: usize,
        _report_bytes: usize,
    ) -> Result<usize, Self::Error> {
        Ok(0)
    }
    fn select<C, D, T: TableAccess>(
        &mut self,
        node: &Node,
        attempt: &CapturedAttempt<C, D, T>,
        byte_limit: usize,
    ) -> Result<(), Self::Error>;
}
pub struct Ports<'a, A, P, D, R, C> {
    pub activation: &'a mut A,
    pub study: &'a mut P,
    pub decoder: &'a mut D,
    pub report: &'a mut R,
    pub codec: &'a mut C,
}

/// Execution/cell/text capacities reserve a conservative equal share per node
/// before host entry. Byte reads and serialized output share cumulative budgets;
/// alias re-ingestion consumes read/work budget while borrowing the same Arc.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub nodes: usize,
    pub source_bytes: usize,
    pub source_cells: usize,
    pub source_storage_bytes: usize,
    pub decode_work_bytes: usize,
    pub output_bytes: usize,
    pub report_bytes: usize,
    pub routing_work: usize,
    pub execution: dataset::Limits,
    pub activation: project_activation::Limits,
    pub publication: crate::producer_publication::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 64,
            source_bytes: 16_777_216,
            source_cells: 1_048_576,
            source_storage_bytes: 67_108_864,
            decode_work_bytes: 268_435_456,
            output_bytes: 16_777_216,
            report_bytes: 16_777_216,
            routing_work: 67_108_864,
            execution: dataset::Limits {
                source_rows: 1_048_576,
                output_rows: 1_048_576,
                output_cells: 1_048_576,
                key_cells: 1_048_576,
                work_cells: 67_108_864,
                scalar_text_bytes: 16_777_216,
                output_text_bytes: 16_777_216,
                identity_cells: 1_048_576,
                identity_text_bytes: 16_777_216,
            },
            activation: Default::default(),
            publication: Default::default(),
        }
    }
}
#[derive(Debug)]
pub enum CaptureError<E> {
    External(E),
    ProducerUnavailable { producer: usize },
    Limit(&'static str),
}
#[derive(Debug)]
pub enum DecodeError<E> {
    Codec(E),
    Contract(yamaa_core::producer_contract::Error),
    Metadata(yamaa_core::diagnostic::Diagnostic),
    Limit(&'static str),
}
#[derive(Debug)]
pub enum Failure<E> {
    Incomplete,
    Limit(&'static str),
    Activation(project_activation::Failure<E>),
    Binding { node: usize, slot: usize },
    Node { node: usize },
}
pub type OutputFailure<R, C> = output::CompleteError<R, C, Infallible>;
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
    graph: BuildGraph,
}
impl PreparedBuild {
    pub(crate) fn new(graph: BuildGraph) -> Self {
        Self { graph }
    }
    pub fn metadata(&self) -> &PreparedGraph {
        self.graph.metadata()
    }
    /// Borrow coherent reporting facts from the complete consumed closure.
    /// Metadata nodes alone still expose no compiled plan; the whole immutable
    /// owner must remain alive for every returned borrow.
    pub fn report_contexts(
        &self,
    ) -> impl ExactSizeIterator<Item = (&Node, &PreparedSpecification)> {
        self.metadata().nodes().iter().zip(self.graph.plans())
    }
    /// Fill caller-owned evidence, so an adapter panic fence keeps the original
    /// activation/node prefix. Every call discards earlier attempt state and
    /// performs fresh activation, reads, execution and output preparation.
    pub fn build_into<A, P, D, R, C>(
        &self,
        ports: Ports<'_, A, P, D, R, C>,
        limits: Limits,
        attempt: &mut BuildAttempt<A, P, D, R, C>,
    ) where
        A: ActivationPort,
        P: StudyPort,
        D: DecodePort,
        D::Table: TableAccess<Error = A::Error>,
        R: ReportPort,
        C: CodecPort,
    {
        let _ = self.build_publication_into(ports, limits, attempt);
    }
    /// Seal publication authority only inside this actual build. Supplied
    /// evidence is reset before activation; a caller-authored success is ignored.
    pub fn build_publication_into<A, P, D, R, C>(
        &self,
        ports: Ports<'_, A, P, D, R, C>,
        limits: Limits,
        attempt: &mut BuildAttempt<A, P, D, R, C>,
    ) -> Result<crate::producer_publication::PreparedPublication, &'static str>
    where
        A: ActivationPort,
        P: StudyPort,
        D: DecodePort,
        D::Table: TableAccess<Error = A::Error>,
        R: ReportPort,
        C: CodecPort,
    {
        *attempt = Attempt::default();
        let graph = self.metadata();
        let count = graph.nodes().len();
        if count == 0 || count > limits.nodes {
            attempt.outcome = Err(Failure::Limit("producer_build_nodes"));
            return Err("producer_build_failed");
        }
        if graph.called_functions().len() > limits.activation.functions {
            attempt.outcome = Err(Failure::Activation(project_activation::Failure::Limit(
                project_activation::Resource::Functions,
            )));
            return Err("producer_build_failed");
        }
        let plans = self.graph.plans().collect::<Vec<_>>();
        let source_count = match plans
            .iter()
            .try_fold(0usize, |sum, p| sum.checked_add(p.sources().len()))
        {
            Some(count) if count > 0 => count,
            _ => {
                attempt.outcome = Err(Failure::Limit("producer_build_sources"));
                return Err("producer_build_failed");
            }
        };
        // Every declared source, including aliases, reserves a codec share. This
        // conservative reservation bounds all retained/staged decoded ownership
        // without deriving an identity or trusting post-allocation size checks.
        let decode_storage = limits.source_storage_bytes / source_count;
        let decode_work = limits.decode_work_bytes / source_count;
        let environment = graph.environment();
        let selected = graph
            .called_functions()
            .iter()
            .map(|&index| &environment.functions()[index])
            .collect::<Vec<_>>();
        let activated = if selected.is_empty() {
            Vec::new()
        } else {
            match project_activation::activate_references_observed(
                environment.language().expect("admitted called language"),
                environment.lock().expect("admitted called lock"),
                &selected,
                ports.activation,
                limits.activation,
                &mut attempt.activation,
            ) {
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
        let mut bytes_left = limits.source_bytes;
        let mut cells_left = limits.source_cells;
        let mut output_left = limits.output_bytes;
        let routing_left = Cell::new(limits.routing_work);
        let e = limits.execution;
        let node_limits = run::Limits {
            source_bytes: limits.source_bytes,
            source_cells: limits.source_cells,
            execution: dataset::Limits {
                source_rows: e.source_rows / count,
                output_rows: e.output_rows / count,
                output_cells: e.output_cells / count,
                key_cells: e.key_cells / count,
                work_cells: e.work_cells / count,
                scalar_text_bytes: e.scalar_text_bytes / count,
                output_text_bytes: e.output_text_bytes / count,
                identity_cells: e.identity_cells / count,
                identity_text_bytes: e.identity_text_bytes / count,
            },
        };
        for &index in graph.order() {
            let node = &graph.nodes()[index];
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
                bytes_left: &mut bytes_left,
                work_left: &routing_left,
            };
            let calls = plan.project_calls().map_or(&[][..], |calls| calls.plans());
            let mut functions =
                Bindings::projected(&activated, node.activation_slots(), calls, ports.activation)
                    .expect("complete graph projections preflighted before sources");
            run::execute_with_functions_into(
                plan,
                &mut sources,
                &mut NodeDecoder {
                    node,
                    decoder: ports.decoder,
                    cells_left: &mut cells_left,
                    storage_bytes: decode_storage,
                    work_bytes: decode_work,
                    work_left: &routing_left,
                },
                &mut functions,
                node_limits,
                &mut entered.dataset,
            );
            let report_allowance = ports.report.report_allowance(limits.report_bytes, count);
            let diagnostics = ports.report.prepare_diagnostics(
                node,
                &entered.dataset,
                output_left,
                report_allowance,
            );
            match diagnostics {
                Ok(bytes) if bytes <= output_left => output_left -= bytes,
                Ok(_) => {
                    entered.output = Err(output::CompleteError::OutputLimit);
                    attempt.outcome = Err(Failure::Node { node: index });
                    return Err("producer_build_failed");
                }
                Err(error) => {
                    entered.output = Err(output::CompleteError::Report(error));
                    attempt.outcome = Err(Failure::Node { node: index });
                    return Err("producer_build_failed");
                }
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
            let report_allowance = ports.report.report_allowance(limits.report_bytes, count);
            let prepared = prepare_output(
                node,
                plan,
                &entered.dataset,
                execution,
                (output_left, report_allowance),
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
            let Some(artifact) = artifact else {
                attempt.outcome = Err(Failure::Node { node: index });
                return Err("producer_build_failed");
            };
            output_left -= artifact.bytes().len(); // output::prepare enforced remaining quota.
        }
        attempt.outcome = Ok(());
        crate::producer_publication::PreparedPublication::from_actual(
            graph,
            attempt,
            limits.publication,
        )
    }
}

struct NodeDecoder<'a, D> {
    node: &'a Node,
    decoder: &'a mut D,
    cells_left: &'a mut usize,
    storage_bytes: usize,
    work_bytes: usize,
    work_left: &'a Cell<usize>,
}
impl<D: DecodePort> SourceDecoder for NodeDecoder<'_, D> {
    type Error = DecodeError<D::Error>;
    type Table = D::Table;
    fn continue_after(&self, error: &Self::Error) -> bool {
        match error {
            DecodeError::Codec(error) => self.decoder.continue_after(error),
            DecodeError::Metadata(_) => true,
            DecodeError::Contract(_) | DecodeError::Limit(_) => false,
        }
    }
    fn decode(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
    ) -> Result<D::Table, Self::Error> {
        self.decode_with_interpretation(
            source,
            bytes,
            yamaa_core::specification::SourceInterpretation::Ordinary,
        )
    }
    fn decode_with_interpretation(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: yamaa_core::specification::SourceInterpretation,
    ) -> Result<D::Table, Self::Error> {
        let mut declaration = None;
        for producer in self.node.producers() {
            charge_routing(self.work_left).map_err(DecodeError::Limit)?;
            if producer.dataset() == source.name {
                declaration = Some(producer);
                break;
            }
        }
        let table = self.decoder.decode_bounded_interpreted(
            source,
            bytes,
            declaration.map(|p| p.contract()),
            DecodeLimits {
                cells: *self.cells_left,
                storage_bytes: self.storage_bytes,
                work_bytes: self.work_bytes,
            },
            interpretation,
        )?;
        let schema = table.schema();
        if let Some(declaration) = declaration {
            // Stored order and native logical schema only: no cardinality/cell
            // observation or value-based inference establishes this contract.
            let limits = yamaa_core::producer_contract::Limits::default();
            let stored = crate::source_ingestion::stored_fields(schema, source.ordinal.as_deref())
                .map_err(|_| DecodeError::Limit("source_ordinal_schema"))?;
            if stored.len() > limits.fields {
                return Err(DecodeError::Limit("producer_source_fields"));
            }
            let actual = stored
                .iter()
                .map(|c| (c.name.as_str(), c.kind))
                .collect::<Vec<_>>();
            if let Some(diagnostic) = declaration
                .contract()
                .validate_typed_fields(&source.name, &actual, limits)
                .map_err(DecodeError::Contract)?
            {
                return Err(DecodeError::Metadata(diagnostic));
            }
        }
        let cells = table
            .row_count()
            .checked_mul(schema.columns().len())
            .filter(|cells| *cells <= *self.cells_left)
            .ok_or(DecodeError::Limit("producer_source_cells"))?;
        *self.cells_left -= cells;
        Ok(table)
    }
}
fn prepare_output<SC, SD, T: TableAccess, R: ReportPort, C: CodecPort>(
    node: &Node,
    plan: &PreparedSpecification,
    attempt: &CapturedAttempt<SC, SD, T>,
    execution: &Execution,
    capacities: (usize, usize),
    report: &mut R,
    codec: &mut C,
) -> PreparedNode<R, C> {
    report
        .select(node, attempt, capacities.1)
        .map_err(output::CompleteError::Report)?;
    output::prepare(
        plan,
        Some(execution),
        capacities.0,
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
        limit: usize,
    ) -> Result<Vec<u8>, Self::Error> {
        self.codec.encode(self.plan, dataset, projection, limit)
    }
}

struct Sources<'a, P: StudyPort, D, T: TableAccess, R, RE, CE> {
    node: &'a Node,
    prefix: NodePrefix<'a, P, D, T, R, RE, CE>,
    external: &'a mut P,
    bytes_left: &'a mut usize,
    work_left: &'a Cell<usize>,
}
impl<P: StudyPort, D, T: TableAccess, R, RE, CE> Sources<'_, P, D, T, R, RE, CE> {
    fn producer(
        &mut self,
        source: &SourceDeclaration,
    ) -> Result<Option<usize>, CaptureError<P::Error>> {
        for (declaration, &dependency) in self.node.producers().iter().zip(self.node.dependencies())
        {
            charge_routing(self.work_left).map_err(CaptureError::Limit)?;
            if declaration.dataset() == source.name {
                return Ok(Some(dependency));
            }
        }
        Ok(None)
    }
    fn artifact(&mut self, producer: usize) -> Result<&output::Artifact, CaptureError<P::Error>> {
        let mut found = None;
        for attempt in self.prefix {
            charge_routing(self.work_left).map_err(CaptureError::Limit)?;
            if attempt.node == producer {
                found = Some(attempt);
                break;
            }
        }
        found
            .and_then(|attempt| attempt.output.as_ref().ok())
            .and_then(Option::as_ref)
            .and_then(PreparedOutput::artifact)
            .ok_or(CaptureError::ProducerUnavailable { producer })
    }
}
impl<P: StudyPort, D, T: TableAccess, R, RE, CE> SourcePort for Sources<'_, P, D, T, R, RE, CE> {
    type Error = CaptureError<P::Error>;
    fn resource_failure(&self, error: &Self::Error) -> Option<ResourceFailure> {
        match error {
            CaptureError::External(error) => self.external.resource_failure(error),
            _ => None,
        }
    }
    fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Self::Error> {
        match self.producer(source)? {
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
        maximum: usize,
    ) -> Result<Arc<[u8]>, Self::Error> {
        self.capture_observed(source, maximum, &mut None)
    }
    fn capture_observed(
        &mut self,
        source: &SourceDeclaration,
        maximum: usize,
        origin: &mut Option<run::SourceOrigin>,
    ) -> Result<Arc<[u8]>, Self::Error> {
        let maximum = maximum.min(*self.bytes_left);
        let bytes = match self.producer(source)? {
            Some(producer) => {
                *origin = Some(run::SourceOrigin::Generated);
                let artifact = self.artifact(producer)?;
                if artifact.bytes().len() > maximum {
                    return Err(CaptureError::Limit("producer_source_bytes"));
                }
                artifact.retained_bytes()
            }
            None => {
                *origin = Some(run::SourceOrigin::Resource);
                self.external
                    .capture(self.node, source, maximum)
                    .map_err(CaptureError::External)?
            }
        };
        if bytes.len() > maximum {
            return Err(CaptureError::Limit("producer_source_bytes"));
        }
        *self.bytes_left -= bytes.len();
        Ok(bytes)
    }
}

fn charge_routing(remaining: &Cell<usize>) -> Result<(), &'static str> {
    remaining.set(
        remaining
            .get()
            .checked_sub(1)
            .ok_or("producer_routing_work")?,
    );
    Ok(())
}
