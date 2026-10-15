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
    pub rows: usize,
    pub cells: usize,
    pub storage_bytes: usize,
    pub work_bytes: usize,
}
/// Actual modeled charges admitted by independently bounded decoder stages.
/// Reused stage allowances report their maximum; cumulative stages retain sums.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecodeUsage {
    pub storage_bytes: usize,
    pub work_bytes: usize,
}
pub trait DecodePort: SourceDecoder {
    /// The most recent bounded decode's admitted prefix, including failures.
    /// None keeps the complete reservation; an unknown prefix is never guessed.
    fn usage(&self) -> Option<DecodeUsage>;
    fn decode_bounded(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        contract: Option<&yamaa_core::producer_contract::Contract>,
        limits: DecodeLimits,
    ) -> Result<Self::Table, DecodeError<Self::Error>>;
    /// Carry the authored fixed-source interpretation through producer routing.
    /// Canonical ODM coordinates and identifiers are independently admitted
    /// by the engine.
    fn decode_bounded_interpreted(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        contract: Option<&yamaa_core::producer_contract::Contract>,
        limits: DecodeLimits,
        interpretation: yamaa_core::specification::SourceInterpretation,
    ) -> Result<Self::Table, DecodeError<Self::Error>>;
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
    /// A generic port retains equal reservations. A native port may pool this
    /// capacity only while admitting cumulative actual encoded lengths and
    /// retained projection charges across every entered node.
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

/// Legacy ports reserve source rows by input slots and other capacities by node.
/// Accountable native decoders opt into actual graph pools for admitted source
/// geometry, decoder bytes, retained outputs, text, identities and predicates.
/// Work, byte reads and serialized outputs retain one cumulative graph owner.
/// Grouping storage is transient within one entered node. Alias re-ingestion
/// consumes read/decode charges while borrowing the same artifact Arc.
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
        let budget = dataset::ExecutionBudget::longitudinal();
        Self {
            nodes: 64,
            source_bytes: budget.max_ipc_bytes,
            source_cells: budget.max_decoded_cells,
            source_storage_bytes: crate::capacity::SOURCE_STORAGE_BYTES,
            decode_work_bytes: crate::capacity::SOURCE_STORAGE_BYTES,
            output_bytes: budget.max_output_bytes,
            report_bytes: crate::capacity::REPORT_ENCODED_BYTES,
            routing_work: crate::capacity::EXECUTION_WORK,
            execution: budget.execution,
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
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
}
#[derive(Debug)]
pub enum Failure<E> {
    Incomplete,
    Limit(&'static str),
    Activation(project_activation::Failure<E>),
    Binding { node: usize, slot: usize },
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
        self.build_publication_with_observer_into(
            ports,
            limits,
            attempt,
            crate::application_observer::Scope::new(&crate::application_observer::Noop),
        )
    }

    pub fn build_publication_with_observer_into<A, P, D, R, C>(
        &self,
        ports: Ports<'_, A, P, D, R, C>,
        limits: Limits,
        attempt: &mut BuildAttempt<A, P, D, R, C>,
        scope: crate::application_observer::Scope<'_>,
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
        let source_rows = match plans
            .iter()
            .map(|plan| {
                row_allowance(
                    limits.execution.source_rows,
                    plan.sources().len(),
                    source_count,
                )
            })
            .collect::<Option<Vec<_>>>()
        {
            Some(rows) => rows,
            None => {
                attempt.outcome = Err(Failure::Limit("producer_source_rows"));
                return Err("producer_build_failed");
            }
        };
        let column_count = match plans.iter().try_fold(0usize, |sum, plan| {
            sum.checked_add(plan.output().columns().len())
        }) {
            Some(count) => count,
            _ => {
                attempt.outcome = Err(Failure::Limit("producer_output_cells"));
                return Err("producer_build_failed");
            }
        };
        let output_cells = match plans
            .iter()
            .map(|plan| {
                output_cell_allowance(
                    limits.execution.output_cells,
                    plan.output().columns().len(),
                    column_count,
                )
            })
            .collect::<Option<Vec<_>>>()
        {
            Some(cells) => cells,
            None => {
                attempt.outcome = Err(Failure::Limit("producer_output_cells"));
                return Err("producer_build_failed");
            }
        };
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
                environment.lock().expect("admitted called lock"),
                &selected,
                &mut scope.activation(ports.activation),
                limits.activation,
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
        let mut bytes_left = limits.source_bytes;
        let mut cells_left = limits.source_cells;
        let mut output_left = limits.output_bytes;
        let routing_left = Cell::new(limits.routing_work);
        let e = limits.execution;
        let execution_work = crate::execution_work::WorkBudget::pooled(e);
        // Every decode, including aliases, reserves the whole remaining graph
        // pool and gets back only what the port's usage receipt proves unused.
        // This bounds all retained/staged decoded ownership without deriving
        // an identity or trusting post-allocation size checks.
        let mut decode_pools = DecodePools {
            rows_limit: e.source_rows,
            rows_left: e.source_rows,
            storage_left: limits.source_storage_bytes,
            work_left: limits.decode_work_bytes,
        };
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
            let node_scope = scope.for_node(index, node.identity());
            let plan = plans[index];
            let node_limits = run::Limits {
                execution: dataset::Limits {
                    source_rows: source_rows[index],
                    output_cells: output_cells[index],
                    ..node_limits.execution
                },
                ..node_limits
            };
            let node_limits = if let Some(execution) = execution_work.node_limits() {
                run::Limits {
                    execution,
                    ..node_limits
                }
            } else {
                node_limits
            };
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
            run::execute_with_observer_and_work_into(
                plan,
                &mut sources,
                &mut NodeDecoder {
                    node,
                    decoder: ports.decoder,
                    cells_left: &mut cells_left,
                    cell_limit: limits.source_cells,
                    work_left: &routing_left,
                    pools: &mut decode_pools,
                },
                &mut functions,
                node_limits,
                &mut entered.dataset,
                node_scope,
                &execution_work,
            );
            node_scope.phase(crate::application_observer::Phase::Output);
            let output_result = (|| {
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
                Ok(())
            })();
            node_scope.exit(
                crate::application_observer::Phase::Output,
                crate::application_observer::Outcome::from_result(&output_result),
            );
            output_result?;
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
    cell_limit: usize,
    work_left: &'a Cell<usize>,
    pools: &'a mut DecodePools,
}
struct DecodePools {
    rows_limit: usize,
    rows_left: usize,
    storage_left: usize,
    work_left: usize,
}
impl<D: DecodePort> SourceDecoder for NodeDecoder<'_, D> {
    type Error = DecodeError<D::Error>;
    type Table = D::Table;
    fn continue_after(&self, error: &Self::Error) -> bool {
        match error {
            DecodeError::Codec(error) => self.decoder.continue_after(error),
            DecodeError::Metadata(_) => true,
            DecodeError::Contract(_)
            | DecodeError::Limit(_)
            | DecodeError::ResourceLimit { .. } => false,
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
        let bounded = DecodeLimits {
            rows: self.pools.rows_left,
            cells: *self.cells_left,
            storage_bytes: self.pools.storage_left,
            work_bytes: self.pools.work_left,
        };
        // An unwind or unobservable prefix cannot release this reservation.
        self.pools.storage_left = 0;
        self.pools.work_left = 0;
        let result = self.decoder.decode_bounded_interpreted(
            source,
            bytes,
            declaration.map(|p| p.contract()),
            bounded,
            interpretation,
        );
        if let Some(used) = self.decoder.usage() {
            match (
                bounded.storage_bytes.checked_sub(used.storage_bytes),
                bounded.work_bytes.checked_sub(used.work_bytes),
            ) {
                (Some(storage), Some(work)) => {
                    self.pools.storage_left = storage;
                    self.pools.work_left = work;
                }
                _ if result.is_ok() => return Err(DecodeError::Limit("producer_decode_receipt")),
                _ => (),
            }
        }
        let table = result?;
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
        let rows = table.row_count();
        self.pools.rows_left =
            self.pools
                .rows_left
                .checked_sub(rows)
                .ok_or(DecodeError::ResourceLimit {
                    resource: "rows",
                    limit: self.pools.rows_limit,
                    actual: (self.pools.rows_limit - self.pools.rows_left).checked_add(rows),
                })?;
        let cells = rows.checked_mul(schema.columns().len());
        let cells = match cells.filter(|cells| *cells <= *self.cells_left) {
            Some(cells) => cells,
            None => {
                return Err(DecodeError::ResourceLimit {
                    resource: "cells",
                    limit: self.cell_limit,
                    actual: cells
                        .and_then(|cells| (self.cell_limit - *self.cells_left).checked_add(cells)),
                })
            }
        };
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

// floor(total * slots / all), without overflowing the otherwise bounded product.
// All reservations sum to at most total; no source effects select the weights.
fn row_allowance(total: usize, slots: usize, all: usize) -> Option<usize> {
    if all == 0 || slots == 0 || slots > all {
        return None;
    }
    (total / all)
        .checked_mul(slots)?
        .checked_add((total % all).checked_mul(slots)? / all)
}
fn output_cell_allowance(total: usize, columns: usize, all: usize) -> Option<usize> {
    if columns == 0 {
        Some(0)
    } else {
        row_allowance(total, columns, all)
    }
}
#[cfg(test)]
mod output_cell_allowance_tests {
    use super::output_cell_allowance;
    #[test]
    fn actual_owned_columns_reserve_a_wider_root_without_expanding_the_total() {
        let widths = [2, 2, 2, 2, 2, 4];
        let total = 2_097_152;
        let allocated = widths.map(|width| output_cell_allowance(total, width, 14).unwrap());
        assert_eq!(allocated[5], 599_186);
        assert!(allocated[..5].iter().all(|&cells| cells >= 200_000));
        assert!(allocated.iter().sum::<usize>() <= total);
        assert_eq!(output_cell_allowance(total, 0, 0), Some(0));
        assert_eq!(
            output_cell_allowance(usize::MAX, 4, 14),
            Some(usize::MAX / 14 * 4 + (usize::MAX % 14) * 4 / 14)
        );
    }
}
#[cfg(test)]
mod row_allowance_tests {
    use super::row_allowance;
    #[test]
    fn input_slots_bound_the_total_and_preserve_the_wider_root() {
        let total = 1_048_576;
        let allocations = [1, 1, 1, 1, 1, 2].map(|slots| row_allowance(total, slots, 7).unwrap());
        assert_eq!(
            allocations,
            [149_796, 149_796, 149_796, 149_796, 149_796, 299_593]
        );
        assert_eq!(allocations.iter().sum::<usize>(), 1_048_573);
        for total in [0, 1, 6, 7, 700_000, usize::MAX] {
            let leaf = row_allowance(total, 1, 7).unwrap();
            let root = row_allowance(total, 2, 7).unwrap();
            assert!(leaf.checked_mul(5).unwrap().checked_add(root).unwrap() <= total);
        }
        assert_eq!(row_allowance(700_000, 2, 7), Some(200_000));
        assert_eq!(row_allowance(699_999, 2, 7), Some(199_999));
        assert_eq!(row_allowance(total, 0, 7), None);
        assert_eq!(row_allowance(total, 8, 7), None);
        assert_eq!(row_allowance(total, 1, 0), None);
    }
}
