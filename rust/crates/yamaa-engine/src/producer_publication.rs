//! Explicit producer-first publication of bytes accepted by the actual build.
//! Construction is private to the build boundary; public attempts are evidence.
use crate::{producer_build::Attempt, specification_output::PreparedOutput};
use alloc::{string::String, sync::Arc, vec::Vec};
use yamaa_core::{producer_graph::PreparedGraph, table::TableAccess};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub nodes: usize,
    pub path_bytes: usize,
    pub artifact_bytes: usize,
    pub work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 64,
            path_bytes: 1_048_576,
            artifact_bytes: 16_777_216,
            work: 67_108_864,
        }
    }
}
/// An immutable request retained from one accepted node. There is no public
/// constructor and no caller-supplied replacement path, byte owner or order.
pub struct Artifact {
    node: usize,
    path: String,
    bytes: Arc<[u8]>,
}
impl Artifact {
    pub fn node(&self) -> usize {
        self.node
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
pub trait PublicationPort {
    type Error;
    /// Validate the complete target set before the first write. This may open
    /// publishers but must not publish, render, capture or invoke functions.
    fn preflight(&mut self, artifacts: &[Artifact]) -> Result<(), Self::Error>;
    fn publish(&mut self, artifact: &Artifact) -> Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum Failure<E> {
    Incomplete,
    Limit(&'static str),
    Preflight(E),
    Publish { node: usize, error: E },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entered {
    pub node: usize,
    pub published: bool,
}
pub struct PublicationAttempt<E> {
    /// Actual entered calls only, recorded before crossing the publisher port.
    pub entered: Vec<Entered>,
    pub outcome: Result<(), Failure<E>>,
}
impl<E> Default for PublicationAttempt<E> {
    fn default() -> Self {
        Self {
            entered: Vec::new(),
            outcome: Err(Failure::Incomplete),
        }
    }
}
pub struct PreparedPublication {
    artifacts: Vec<Artifact>,
}
impl PreparedPublication {
    pub fn artifacts(&self) -> &[Artifact] {
        &self.artifacts
    }
    /// Only the engine's actual successful build may call this constructor.
    pub(crate) fn from_actual<C, D, T: TableAccess, R, RE, CE, A>(
        graph: &PreparedGraph,
        attempt: &Attempt<C, D, T, R, RE, CE, A>,
        limits: Limits,
    ) -> Result<Self, &'static str> {
        if attempt.outcome.is_err() || attempt.nodes.len() != graph.order().len() {
            return Err("producer_publication_incomplete");
        }
        let count = attempt.nodes.len();
        if count == 0 || count > limits.nodes || count > limits.work {
            return Err("producer_publication_nodes");
        }
        let mut paths = limits.path_bytes;
        let mut bytes = limits.artifact_bytes;
        let mut work = limits.work - count;
        for (entered, &expected) in attempt.nodes.iter().zip(graph.order()) {
            if entered.node != expected
                || entered
                    .dataset
                    .result
                    .as_ref()
                    .ok()
                    .is_none_or(|execution| execution.result.is_err())
            {
                return Err("producer_publication_incomplete");
            }
            let artifact = entered
                .output
                .as_ref()
                .ok()
                .and_then(Option::as_ref)
                .and_then(PreparedOutput::artifact)
                .ok_or("producer_publication_incomplete")?;
            paths = paths
                .checked_sub(artifact.path().len())
                .ok_or("producer_publication_paths")?;
            bytes = bytes
                .checked_sub(artifact.bytes().len())
                .ok_or("producer_publication_bytes")?;
            work = work
                .checked_sub(artifact.path().len())
                .and_then(|left| left.checked_sub(artifact.bytes().len()))
                .ok_or("producer_publication_work")?;
        }
        // Complete cumulative admission precedes vector/text ownership growth.
        let artifacts = attempt
            .nodes
            .iter()
            .map(|entered| {
                let artifact = entered
                    .output
                    .as_ref()
                    .ok()
                    .and_then(Option::as_ref)
                    .and_then(PreparedOutput::artifact)
                    .expect("complete actual artifact admission");
                Artifact {
                    node: entered.node,
                    path: artifact.path().into(),
                    bytes: artifact.retained_bytes(),
                }
            })
            .collect();
        Ok(Self { artifacts })
    }
    /// A repeat Save repeats only publication. Earlier writes are never rolled
    /// back; an error or unwind retains the actual entered prefix in the caller.
    pub fn save_into<P: PublicationPort>(
        &self,
        publisher: &mut P,
        limits: Limits,
        attempt: &mut PublicationAttempt<P::Error>,
    ) {
        *attempt = Default::default();
        let mut paths = limits.path_bytes;
        let mut bytes = limits.artifact_bytes;
        if self.artifacts.is_empty()
            || self.artifacts.len() > limits.nodes
            || self.artifacts.len() > limits.work
        {
            attempt.outcome = Err(Failure::Limit("producer_publication_nodes"));
            return;
        }
        let mut work = limits.work - self.artifacts.len();
        for artifact in &self.artifacts {
            let Some(left) = paths.checked_sub(artifact.path.len()) else {
                attempt.outcome = Err(Failure::Limit("producer_publication_paths"));
                return;
            };
            paths = left;
            let Some(left) = bytes.checked_sub(artifact.bytes.len()) else {
                attempt.outcome = Err(Failure::Limit("producer_publication_bytes"));
                return;
            };
            bytes = left;
            let Some(left) = work
                .checked_sub(artifact.path.len())
                .and_then(|left| left.checked_sub(artifact.bytes.len()))
            else {
                attempt.outcome = Err(Failure::Limit("producer_publication_work"));
                return;
            };
            work = left;
        }
        // Reserve all prefix evidence before crossing any port.
        attempt.entered.reserve(self.artifacts.len());
        if let Err(error) = publisher.preflight(&self.artifacts) {
            attempt.outcome = Err(Failure::Preflight(error));
            return;
        }
        for artifact in &self.artifacts {
            attempt.entered.push(Entered {
                node: artifact.node,
                published: false,
            });
            if let Err(error) = publisher.publish(artifact) {
                attempt.outcome = Err(Failure::Publish {
                    node: artifact.node,
                    error,
                });
                return;
            }
            attempt.entered.last_mut().unwrap().published = true;
        }
        attempt.outcome = Ok(());
    }
}
