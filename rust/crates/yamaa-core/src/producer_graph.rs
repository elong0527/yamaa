//! Complete, bounded producer metadata and sealed whole-graph build transfer.
//! Graph compilation owns one admitted environment and preserves every local
//! call's projection into the graph's unique activation selection.
use crate::{
    diagnostic::{ConditionCode, Diagnostic},
    producer_admission::{self, Candidate, Declaration},
    producer_contract::diagnostics::text,
    project_environment::ExecutionEnvironment,
    schema::{DocumentNode, SpecificationDocument},
    specification::{PrepareError, PreparedSpecification},
    table::TableSchema,
};
use serde_json::Value as JsonValue;
use std::{collections::BTreeMap, vec};

/// The surrounding preparation boundary retains one document per canonical
/// identity. References must borrow that same admitted document, so a candidate
/// cannot silently replace a node's contract or compilation input.
pub struct SuppliedNode<'a> {
    pub identity: &'a str,
    pub document: &'a SpecificationDocument,
    pub producers: &'a [Candidate<'a>],
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("producer graph boundary: node={node:?}, reason={reason:?}")]
    Boundary {
        node: Option<usize>,
        reason: &'static str,
    },
    #[error("producer graph admission: node={node:?}, error={error:?}")]
    Admission {
        node: usize,
        error: producer_admission::Error,
    },
    #[error("producer graph compilation: node={node:?}, error={error:?}")]
    Compilation { node: usize, error: PrepareError },
    #[error("producer graph cycle: node={node:?}, diagnostic={diagnostic:?}")]
    Cycle { node: usize, diagnostic: Diagnostic },
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
    /// Authored source metadata remains tied to this sealed graph node.
    pub fn sources(&self) -> &[crate::specification::SourceDeclaration] {
        self.compiled.sources()
    }
    pub fn source_interpretation(
        &self,
        index: usize,
    ) -> Option<crate::specification::SourceInterpretation> {
        self.compiled.source_interpretation(index)
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
    /// Header/type-only binding over the complete retained graph. This never
    /// releases a node compilation owner or accepts data or execution ports.
    pub fn check_source_schemas(
        &self,
        schemas: &[Vec<TableSchema>],
    ) -> Result<Vec<(usize, crate::diagnostic::Diagnostic)>, MetadataBindingError> {
        if schemas.len() != self.nodes.len()
            || schemas
                .iter()
                .zip(&self.nodes)
                .any(|(schemas, node)| schemas.len() != node.compiled.sources().len())
        {
            return Err(MetadataBindingError::Identity);
        }
        let mut diagnostics = Vec::new();
        for (index, (schemas, node)) in schemas.iter().zip(&self.nodes).enumerate() {
            let schemas = schemas.iter().collect::<Vec<_>>();
            let findings = node
                .compiled
                .metadata_source_findings(&schemas)
                .map_err(|error| MetadataBindingError::Binding { node: index, error })?;
            let remaining = crate::limits::Limits::FINDINGS.saturating_sub(diagnostics.len());
            diagnostics.extend(
                findings
                    .into_iter()
                    .take(remaining)
                    .map(|finding| (index, finding)),
            );
        }
        Ok(diagnostics)
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
                .check_declaration_findings()
                .map(move |finding| (index, finding))
        })
    }
    /// Transfer the complete admitted closure, its plans and its one environment
    /// together. Metadata nodes never expose a standalone compiled producer.
    pub fn into_build(self) -> crate::producer_build::PreparedBuild {
        crate::producer_build::PreparedBuild::new(self)
    }
    pub(crate) fn plans(&self) -> impl ExactSizeIterator<Item = &PreparedSpecification> {
        self.nodes.iter().map(|node| &node.compiled)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataBindingError {
    #[error("producer graph identity")]
    Identity,
    #[error("producer graph projection")]
    Projection,
    #[error("producer graph binding: node={node:?}, error={error:?}")]
    Binding {
        node: usize,
        error: crate::specification::BindError,
    },
}

fn boundary(node: Option<usize>, reason: &'static str) -> Error {
    Error::Boundary { node, reason }
}
/// Validate the supplied closure before compiling it. The signature has no IO,
/// runtime, activation or codec port; every returned graph remains metadata only.
pub fn prepare(
    root_identity: &str,
    supplied: &[SuppliedNode<'_>],
    environment: ExecutionEnvironment,
) -> Result<PreparedGraph, Error> {
    if supplied.is_empty() {
        return Err(boundary(None, "empty_graph"));
    }
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
            if !std::ptr::eq(candidate.document, supplied[producer].document) {
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
                    .chain(std::iter::once(text(supplied[producer].identity)))
                    .collect();
                return Err(Error::Cycle {
                    node,
                    diagnostic: Diagnostic {
                        code: ConditionCode::ProducerWorkflowCycle,
                        spec_paths: vec![format!(
                            "input.{}.schema",
                            declarations[node][next].dataset()
                        )],
                        context: [("cycle".into(), JsonValue::Array(cycle))]
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
    for (index, ((node, producers), dependencies)) in supplied
        .iter()
        .zip(declarations)
        .zip(dependencies)
        .enumerate()
    {
        let compiled = if producers.is_empty() {
            PreparedSpecification::prepare_with_environment(node.document, &environment)
        } else {
            PreparedSpecification::prepare_producer_metadata(
                node.document,
                Some(&environment),
                &producers,
            )
        }
        .map_err(|error| Error::Compilation { node: index, error })?;
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
