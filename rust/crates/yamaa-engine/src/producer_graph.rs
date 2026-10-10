//! A complete graph's checked metadata still grants no execution capability.
use yamaa_core::{
    producer_graph::{self, Error, Limits, PreparedGraph, SuppliedNode},
    project_environment::ExecutionEnvironment,
    specification::PrepareError,
};

#[derive(Debug)]
pub struct CheckedProducerGraph {
    metadata: PreparedGraph,
}
pub fn check(
    root_identity: &str,
    nodes: &[SuppliedNode<'_>],
    environment: ExecutionEnvironment,
    limits: Limits,
) -> Result<CheckedProducerGraph, Error> {
    Ok(CheckedProducerGraph {
        metadata: producer_graph::prepare(root_identity, nodes, environment, limits)?,
    })
}
impl CheckedProducerGraph {
    pub fn metadata(&self) -> &PreparedGraph {
        &self.metadata
    }
    /// No source, activation, build or publication port is accepted by check.
    /// #1741 must qualify graph orchestration before this capability can execute.
    pub fn execution_capability(
        &self,
    ) -> Result<&crate::domain::CheckedSpecification, PrepareError> {
        Err(self.metadata.execution_refusal())
    }
}
