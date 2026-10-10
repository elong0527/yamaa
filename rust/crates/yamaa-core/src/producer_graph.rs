//! Complete, bounded producer metadata and sealed whole-graph build transfer.
//! Graph compilation owns one admitted environment and preserves every local
//! call's projection into the graph's unique activation selection.
use crate::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    producer_admission::{self, Candidate, Declaration},
    producer_contract::diagnostics::text,
    project_environment::ExecutionEnvironment,
    schema::{DocumentNode, SpecificationDocument},
    specification::{PrepareError, PreparedSpecification, UnsupportedFeature},
    table::TableSchema,
};
use alloc::{collections::BTreeMap, format, string::String, vec, vec::Vec};

/// The surrounding preparation boundary retains one document per canonical
/// identity. References must borrow that same admitted document, so a candidate
/// cannot silently replace a node's contract or compilation input.
pub struct SuppliedNode<'a> {
    pub identity: &'a str,
    pub document: &'a SpecificationDocument,
    pub producers: &'a [Candidate<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub nodes: usize,
    pub edges: usize,
    pub document_nodes: usize,
    pub text_bytes: usize,
    pub slots: usize,
    pub parameters: usize,
    pub functions: usize,
    pub work: usize,
    pub metadata: producer_admission::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 64,
            edges: 1_024,
            document_nodes: 1_048_576,
            text_bytes: 16_777_216,
            slots: 65_536,
            parameters: 65_536,
            functions: 1_024,
            work: 67_108_864,
            metadata: Default::default(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Limit(&'static str),
    Boundary {
        node: Option<usize>,
        reason: &'static str,
    },
    Admission {
        node: usize,
        error: producer_admission::Error,
    },
    Compilation {
        node: usize,
        error: PrepareError,
    },
    Cycle {
        node: usize,
        diagnostic: Diagnostic,
    },
}

#[derive(Debug)]
pub struct Node {
    identity: String,
    compiled: PreparedSpecification,
    producers: Vec<Declaration>,
    dependencies: Vec<usize>,
    activation_slots: Vec<usize>,
}
impl Node {
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn producers(&self) -> &[Declaration] {
        &self.producers
    }
    /// One entry per authored producer declaration, including shared aliases.
    pub fn dependencies(&self) -> &[usize] {
        &self.dependencies
    }
    pub fn output(&self) -> &TableSchema {
        self.compiled.output()
    }
    pub fn projection(&self) -> &[String] {
        self.compiled.projection()
    }
    pub fn called_functions(&self) -> &[usize] {
        self.compiled.called_functions()
    }
    /// Local call slot -> graph activation slot. Neither index is a host handle.
    pub fn activation_slots(&self) -> &[usize] {
        &self.activation_slots
    }
}

#[derive(Debug)]
pub struct PreparedGraph {
    nodes: Vec<Node>,
    root: usize,
    order: Vec<usize>,
    selected: Vec<usize>,
    environment: ExecutionEnvironment,
}
impl PreparedGraph {
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn root(&self) -> usize {
        self.root
    }
    /// Deterministic producer-first order, visiting each canonical node once.
    pub fn order(&self) -> &[usize] {
        &self.order
    }
    /// Unique called definitions in the retained environment's original order.
    pub fn called_functions(&self) -> &[usize] {
        &self.selected
    }
    pub fn environment(&self) -> &ExecutionEnvironment {
        &self.environment
    }
    /// Static findings borrow the complete sealed closure in canonical node
    /// order. No standalone compiled node or execution plan is exposed.
    pub fn check_findings(
        &self,
    ) -> impl Iterator<
        Item = (
            usize,
            crate::specification::VerificationDeclarationFinding<'_>,
        ),
    > {
        self.nodes.iter().enumerate().flat_map(|(index, node)| {
            node.compiled
                .verification_declaration_findings()
                .map(move |finding| (index, finding))
        })
    }
    /// Transfer the complete admitted closure, its plans and its one environment
    /// together. Metadata nodes never expose a standalone compiled producer.
    pub fn into_build(self) -> BuildGraph {
        BuildGraph { graph: self }
    }
    /// This static capability grants no data, activation, build or publication
    /// authority. #1741 must qualify the complete orchestration before release.
    pub fn execution_refusal(&self) -> PrepareError {
        let producers = &self.nodes[self.root].producers;
        if producers.is_empty() {
            // A single external-input node is a valid metadata graph. Keep its
            // denial nonempty and anchored at the admitted input declaration.
            return PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "producer_workflow".into(),
                path: "input".into(),
            }]);
        }
        PrepareError::Unsupported(
            producers
                .iter()
                .map(|p| UnsupportedFeature {
                    operation: "producer_workflow".into(),
                    path: format!("input.{}.schema", p.dataset()),
                })
                .collect(),
        )
    }
}

/// Whole-closure compilation ownership for the application engine. Construction
/// consumes a validated graph; plans can only be borrowed with that owner alive.
#[derive(Debug)]
pub struct BuildGraph {
    graph: PreparedGraph,
}
impl BuildGraph {
    pub fn metadata(&self) -> &PreparedGraph {
        &self.graph
    }
    /// Borrow the complete compiled collection in the metadata's node order.
    /// No node plan is moved out, substituted or compiled again at this boundary.
    pub fn plans(&self) -> impl ExactSizeIterator<Item = &PreparedSpecification> {
        self.graph.nodes.iter().map(|node| &node.compiled)
    }
}

fn boundary(node: Option<usize>, reason: &'static str) -> Error {
    Error::Boundary { node, reason }
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

/// Validate the supplied closure before compiling it. The signature has no IO,
/// runtime, activation or codec port; every returned graph remains metadata only.
pub fn prepare(
    root_identity: &str,
    supplied: &[SuppliedNode<'_>],
    environment: ExecutionEnvironment,
    limits: Limits,
) -> Result<PreparedGraph, Error> {
    if supplied.is_empty() || supplied.len() > limits.nodes {
        return Err(Error::Limit("producer_graph_nodes"));
    }
    let (mut edges, mut document_nodes, mut bytes) = (0usize, 0usize, 0usize);
    charge(
        &mut bytes,
        root_identity.len(),
        limits.text_bytes,
        "producer_graph_text_bytes",
    )?;
    for node in supplied {
        charge(
            &mut bytes,
            node.identity.len(),
            limits.text_bytes,
            "producer_graph_text_bytes",
        )?;
        charge(
            &mut edges,
            node.producers.len(),
            limits.edges,
            "producer_graph_edges",
        )?;
        charge(
            &mut document_nodes,
            node.document.document().nodes().len(),
            limits.document_nodes,
            "producer_graph_document_nodes",
        )?;
        for value in node.document.document().nodes() {
            if let DocumentNode::Text(s) | DocumentNode::Integer(s) = value {
                charge(
                    &mut bytes,
                    s.len(),
                    limits.text_bytes,
                    "producer_graph_text_bytes",
                )?;
            }
        }
        for candidate in node.producers {
            // Each link retains its own ordered contract. Charge repeated
            // contract scans/copies even when aliases share the same node.
            charge(
                &mut document_nodes,
                candidate.document.document().nodes().len(),
                limits.document_nodes,
                "producer_graph_document_nodes",
            )?;
            for value in candidate.document.document().nodes() {
                if let DocumentNode::Text(s) | DocumentNode::Integer(s) = value {
                    charge(
                        &mut bytes,
                        s.len(),
                        limits.text_bytes,
                        "producer_graph_text_bytes",
                    )?;
                }
            }
            for value in [
                candidate.dataset,
                candidate.schema_path,
                candidate.schema_identity,
                candidate.producer_identity,
                candidate.source_identity,
                candidate.output_identity,
                candidate.schema_origin.declaring_source,
                candidate.schema_origin.written,
                candidate.input_origin.declaring_source,
                candidate.input_origin.written,
                candidate.output_origin.declaring_source,
                candidate.output_origin.written,
            ] {
                charge(
                    &mut bytes,
                    value.len(),
                    limits.text_bytes,
                    "producer_graph_text_bytes",
                )?;
            }
        }
    }
    // Bound borrowed identity indexes and traversal before their ownership.
    // Contract validation and compilation keep their own existing quotas.
    let keys = supplied
        .len()
        .checked_add(edges)
        .ok_or(Error::Limit("producer_graph_work"))?;
    let factor = 16 * (usize::BITS as usize - keys.max(1).leading_zeros() as usize + 1);
    let work = document_nodes
        .checked_add(bytes)
        .and_then(|n| n.checked_add(edges))
        .and_then(|n| n.checked_mul(factor))
        .filter(|&n| n <= limits.work)
        .ok_or(Error::Limit("producer_graph_work"))?;
    let mut used_work = work;
    let mut identities = BTreeMap::new();
    for (index, node) in supplied.iter().enumerate() {
        if node.identity.is_empty() || identities.insert(node.identity, index).is_some() {
            return Err(boundary(Some(index), "duplicate_or_empty_identity"));
        }
    }
    let root = identities
        .get(root_identity)
        .copied()
        .ok_or_else(|| boundary(None, "missing_root"))?;
    for (index, node) in supplied.iter().enumerate() {
        for candidate in node.producers {
            let producer = identities
                .get(candidate.producer_identity)
                .copied()
                .ok_or_else(|| boundary(Some(index), "missing_producer_node"))?;
            if !core::ptr::eq(candidate.document, supplied[producer].document) {
                return Err(boundary(Some(index), "contradictory_producer_document"));
            }
        }
    }
    let mut declarations = Vec::with_capacity(supplied.len());
    let mut dependencies = Vec::with_capacity(supplied.len());
    for (index, node) in supplied.iter().enumerate() {
        let has_producer = node
            .document
            .document()
            .field(node.document.document().root(), "input")
            .and_then(|id| match &node.document.document().nodes()[id] {
                DocumentNode::Mapping(inputs) => Some(inputs.iter().any(|&(_, id)| {
                    node.document
                        .document()
                        .field(id, "schema")
                        .is_some_and(|id| {
                            !matches!(node.document.document().nodes()[id], DocumentNode::Null)
                        })
                })),
                _ => None,
            })
            .unwrap_or(false);
        let producers = if has_producer || !node.producers.is_empty() {
            producer_admission::prepare_declarations(
                node.identity,
                node.document,
                node.producers,
                limits.metadata,
                true,
            )
            .map_err(|error| Error::Admission { node: index, error })?
        } else {
            Vec::new()
        };
        dependencies.push(
            producers
                .iter()
                .map(|p| identities[p.producer_identity()])
                .collect::<Vec<_>>(),
        );
        declarations.push(producers);
    }
    // A shared predecessor has one output location and contract across all
    // branches, even when each consumer's metadata is individually consistent.
    let mut producer_metadata: BTreeMap<&str, &Declaration> = BTreeMap::new();
    for (node, producers) in declarations.iter().enumerate() {
        for producer in producers {
            if let Some(prior) = producer_metadata.insert(producer.producer_identity(), producer) {
                if prior.artifact_identity() != producer.artifact_identity()
                    || prior.contract() != producer.contract()
                    || prior.output_origin() != producer.output_origin()
                {
                    return Err(boundary(Some(node), "contradictory_producer_metadata"));
                }
            }
        }
    }
    let mut state = vec![0u8; supplied.len()];
    let mut stack = vec![(root, 0usize)];
    let mut order = Vec::with_capacity(supplied.len());
    state[root] = 1;
    while let Some(&(node, next)) = stack.last() {
        if next == dependencies[node].len() {
            state[node] = 2;
            order.push(node);
            stack.pop();
            continue;
        }
        stack.last_mut().expect("nonempty traversal").1 += 1;
        let producer = dependencies[node][next];
        match state[producer] {
            0 => {
                state[producer] = 1;
                stack.push((producer, 0));
            }
            1 => {
                let start = stack
                    .iter()
                    .position(|&(n, _)| n == producer)
                    .expect("active producer");
                let cycle = stack[start..]
                    .iter()
                    .map(|&(n, _)| text(supplied[n].identity))
                    .chain(core::iter::once(text(supplied[producer].identity)))
                    .collect();
                return Err(Error::Cycle {
                    node,
                    diagnostic: Diagnostic {
                        code: ConditionCode::ProducerWorkflowCycle,
                        spec_paths: vec![format!(
                            "input.{}.schema",
                            declarations[node][next].dataset()
                        )],
                        context: [("cycle".into(), ContextValue::Sequence(cycle))]
                            .into_iter()
                            .collect(),
                        source_span: None,
                        operand_route: None,
                    },
                });
            }
            _ => {}
        }
    }
    if let Some(node) = state.iter().position(|&s| s == 0) {
        return Err(boundary(Some(node), "unreachable_node"));
    }
    let mut nodes = Vec::with_capacity(supplied.len());
    let mut selected = Vec::new();
    let mut slots = 0usize;
    let mut parameters = 0usize;
    let slot_comparisons =
        16 * (usize::BITS as usize - limits.slots.max(1).leading_zeros() as usize + 1);
    for (index, ((node, producers), dependencies)) in supplied
        .iter()
        .zip(declarations)
        .zip(dependencies)
        .enumerate()
    {
        let compiled = if producers.is_empty() {
            PreparedSpecification::prepare_with_environment_limits(
                node.document,
                &environment,
                limits.metadata.compilation,
                Default::default(),
            )
        } else {
            PreparedSpecification::prepare_producer_metadata(
                node.document,
                limits.metadata.compilation,
                Some(&environment),
                &producers,
            )
        }
        .map_err(|error| Error::Compilation { node: index, error })?;
        charge(
            &mut slots,
            compiled.called_functions().len(),
            limits.slots,
            "producer_graph_slots",
        )?;
        charge(
            &mut used_work,
            compiled
                .called_functions()
                .len()
                .checked_mul(slot_comparisons)
                .ok_or(Error::Limit("producer_graph_work"))?,
            limits.work,
            "producer_graph_work",
        )?;
        // Compilation has its existing bounded temporary frame. Charge every
        // retained signature before adding that node to graph ownership;
        // callable/default text is not present in the specification's tree.
        if let Some(calls) = compiled.project_calls() {
            for plan in calls.plans() {
                for value in [&plan.identity().name, &plan.identity().call] {
                    charge(
                        &mut bytes,
                        value.len(),
                        limits.text_bytes,
                        "producer_graph_text_bytes",
                    )?;
                }
                charge(
                    &mut parameters,
                    plan.signature().parameters().len(),
                    limits.parameters,
                    "producer_graph_parameters",
                )?;
                charge(
                    &mut used_work,
                    plan.signature().parameters().len(),
                    limits.work,
                    "producer_graph_work",
                )?;
                for parameter in plan.signature().parameters() {
                    for value in [&parameter.name, &parameter.host_name] {
                        charge(
                            &mut bytes,
                            value.len(),
                            limits.text_bytes,
                            "producer_graph_text_bytes",
                        )?;
                    }
                    if let crate::function_signature::Presence::Optional(
                        crate::value::Value::Str(value),
                    ) = &parameter.presence
                    {
                        charge(
                            &mut bytes,
                            value.len(),
                            limits.text_bytes,
                            "producer_graph_text_bytes",
                        )?;
                    }
                }
            }
        }
        selected.extend_from_slice(compiled.called_functions());
        nodes.push(Node {
            identity: node.identity.into(),
            compiled,
            producers,
            dependencies,
            activation_slots: Vec::new(),
        });
    }
    selected.sort_unstable();
    selected.dedup();
    if selected.len() > limits.functions {
        return Err(Error::Limit("producer_graph_functions"));
    }
    for node in &mut nodes {
        node.activation_slots = node
            .compiled
            .called_functions()
            .iter()
            .map(|index| {
                selected
                    .binary_search(index)
                    .expect("selected local function")
            })
            .collect();
    }
    Ok(PreparedGraph {
        nodes,
        root,
        order,
        selected,
        environment,
    })
}
