//! Retained complete graph metadata from supplied prepared documents. No IO or
//! host port exists here; native graph resolution and execution remain #1741.
use crate::{
    producer_admission::{self, SuppliedProducer},
    specification_source::PreparedDocument,
};
use std::{collections::BTreeMap, sync::Arc};
use yamaa_core::{producer_graph as core, project_environment::ExecutionEnvironment};
use yamaa_engine::producer_graph::{self as engine, CheckedProducerGraph};

pub struct SuppliedNode {
    pub document: Arc<PreparedDocument>,
    pub producers: Vec<SuppliedProducer>,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub captured_bytes: usize,
    pub schema_bytes: usize,
    pub schema_modules: usize,
    pub snapshots: usize,
    pub graph: core::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            captured_bytes: 16_777_216,
            schema_bytes: 16_777_216,
            schema_modules: 128,
            snapshots: 1_024,
            graph: Default::default(),
        }
    }
}
#[derive(Debug)]
pub enum Error {
    Limit(&'static str),
    Boundary {
        node: usize,
        reason: &'static str,
    },
    Metadata {
        node: usize,
        error: producer_admission::Error,
    },
    Graph(core::Error),
}
pub struct PreparedGraph {
    nodes: Vec<SuppliedNode>,
    checked: CheckedProducerGraph,
}
impl PreparedGraph {
    pub fn nodes(&self) -> &[SuppliedNode] {
        &self.nodes
    }
    pub fn checked(&self) -> &CheckedProducerGraph {
        &self.checked
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

/// Own original schema/entry/parent bytes beside the sealed graph. References
/// must share its one canonical prepared owner; equality of text identities is
/// not a resource capability. The surrounding native resolver will supply it.
pub fn prepare(
    root_identity: &str,
    nodes: Vec<SuppliedNode>,
    environment: ExecutionEnvironment,
    limits: Limits,
) -> Result<PreparedGraph, Error> {
    if nodes.is_empty() || nodes.len() > limits.graph.nodes {
        return Err(Error::Limit("producer_graph_nodes"));
    }
    let (mut bytes, mut schema_bytes, mut modules, mut snapshots, mut text, mut edges) =
        (0, 0, 0, 0, 0, 0);
    for node in &nodes {
        if node.producers.len() > limits.graph.metadata.candidates {
            return Err(Error::Limit("producer_candidates"));
        }
        charge(
            &mut edges,
            node.producers.len(),
            limits.graph.edges,
            "producer_graph_edges",
        )?;
        charge(
            &mut modules,
            node.document.schema().sources().len(),
            limits.schema_modules,
            "producer_graph_schema_modules",
        )?;
        for schema in node.document.schema().sources() {
            charge(
                &mut schema_bytes,
                schema.bytes.len(),
                limits.schema_bytes,
                "producer_graph_schema_bytes",
            )?;
            charge(
                &mut text,
                schema.identity.len(),
                limits.graph.text_bytes,
                "producer_graph_identity_bytes",
            )?;
        }
        for source in std::iter::once(node.document.source())
            .chain(node.document.parents().iter().map(|p| p.source()))
        {
            charge(
                &mut snapshots,
                1,
                limits.snapshots,
                "producer_graph_snapshots",
            )?;
            charge(
                &mut bytes,
                source.bytes.len(),
                limits.captured_bytes,
                "producer_graph_captured_bytes",
            )?;
            charge(
                &mut text,
                source.identity.len(),
                limits.graph.text_bytes,
                "producer_graph_identity_bytes",
            )?;
        }
    }
    // Ordered schema scans and snapshot indexing precede candidate allocation.
    // Share the aggregate text/work budget with the subsequent core admission.
    let comparisons = modules
        .checked_add(snapshots)
        .and_then(|n| n.checked_add(nodes.len()))
        .ok_or(Error::Limit("producer_graph_work"))?;
    let work = text
        .checked_mul(comparisons)
        .and_then(|n| n.checked_add(schema_bytes))
        .and_then(|n| n.checked_add(bytes))
        .filter(|&n| n <= limits.graph.work)
        .ok_or(Error::Limit("producer_graph_work"))?;
    let mut graph_limits = limits.graph;
    graph_limits.text_bytes -= text;
    graph_limits.work -= work;
    let mut documents = BTreeMap::new();
    for (index, node) in nodes.iter().enumerate() {
        if documents
            .insert(node.document.source().identity.as_str(), index)
            .is_some()
        {
            return Err(Error::Boundary {
                node: index,
                reason: "duplicate_identity",
            });
        }
    }
    let expected = nodes[0].document.schema();
    let mut held = BTreeMap::new();
    for (index, node) in nodes.iter().enumerate() {
        let actual = node.document.schema();
        if expected.structure().root_class().name != actual.structure().root_class().name
            || expected.sources().len() != actual.sources().len()
            || expected.sources().iter().any(|source| {
                !actual
                    .sources()
                    .iter()
                    .any(|other| source.identity == other.identity && source.bytes == other.bytes)
            })
        {
            return Err(Error::Boundary {
                node: index,
                reason: "schema_mismatch",
            });
        }
        for source in std::iter::once(node.document.source())
            .chain(node.document.parents().iter().map(|p| p.source()))
        {
            if held
                .insert(source.identity.as_str(), source.bytes.as_slice())
                .is_some_and(|prior| prior != source.bytes.as_slice())
            {
                return Err(Error::Boundary {
                    node: index,
                    reason: "contradictory_snapshot",
                });
            }
        }
        for producer in &node.producers {
            let canonical = documents
                .get(producer.document.source().identity.as_str())
                .ok_or(Error::Boundary {
                    node: index,
                    reason: "missing_producer_node",
                })?;
            if !Arc::ptr_eq(&producer.document, &nodes[*canonical].document) {
                return Err(Error::Boundary {
                    node: index,
                    reason: "contradictory_producer_document",
                });
            }
        }
    }
    let metadata_limits = producer_admission::Limits {
        captured_bytes: limits.captured_bytes,
        schema_bytes: limits.schema_bytes,
        schema_modules: limits.schema_modules,
        snapshots: limits.snapshots,
        metadata: limits.graph.metadata,
    };
    let candidates = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            producer_admission::candidates(&node.document, &node.producers, metadata_limits)
                .map_err(|error| Error::Metadata { node: index, error })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let supplied = nodes
        .iter()
        .zip(&candidates)
        .map(|(node, producers)| core::SuppliedNode {
            identity: &node.document.source().identity,
            document: node.document.model(),
            producers,
        })
        .collect::<Vec<_>>();
    let checked =
        engine::check(root_identity, &supplied, environment, graph_limits).map_err(Error::Graph)?;
    Ok(PreparedGraph { nodes, checked })
}
