//! Retained complete graph metadata from supplied prepared documents.
//! Graph preparation uses supplied documents; Build owns execution and host ports.
use crate::{
    producer_admission::{self, SuppliedProducer},
    specification_source::PreparedDocument,
};
use std::{collections::BTreeMap, sync::Arc};
use yamaa_core::{producer_graph as core, project_environment::ExecutionEnvironment};

pub struct SuppliedNode {
    pub document: Arc<PreparedDocument>,
    pub producers: Vec<SuppliedProducer>,
}

#[derive(Debug)]
pub enum Error {
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
    metadata: core::PreparedGraph,
}
impl PreparedGraph {
    pub fn nodes(&self) -> &[SuppliedNode] {
        &self.nodes
    }
    pub fn metadata(&self) -> &core::PreparedGraph {
        &self.metadata
    }
    pub(crate) fn into_build(
        self,
    ) -> (
        Vec<Arc<PreparedDocument>>,
        yamaa_core::producer_build::PreparedBuild,
    ) {
        (
            self.nodes.into_iter().map(|node| node.document).collect(),
            self.metadata.into_build(),
        )
    }
}

/// Own original schema/entry/parent bytes beside the sealed graph. References
/// must share its one canonical prepared owner; equality of text identities is
/// not a resource capability. The surrounding native resolver will supply it.
pub fn prepare(
    root_identity: &str,
    nodes: Vec<SuppliedNode>,
    environment: ExecutionEnvironment,
) -> Result<PreparedGraph, Error> {
    if nodes.is_empty() {
        return Err(Error::Boundary {
            node: 0,
            reason: "empty_graph",
        });
    }
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
            .chain(node.document.dictionary_sources())
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
    let candidates = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            producer_admission::candidates(&node.document, &node.producers)
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
    let metadata = core::prepare(root_identity, &supplied, environment).map_err(Error::Graph)?;
    Ok(PreparedGraph { nodes, metadata })
}
