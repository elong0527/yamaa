//! Explicit producer-first publication of bytes accepted by the actual build.
//! Construction is private to the build boundary; public attempts are evidence.
use crate::{producer_build::Attempt, specification_output::PreparedOutput};
use crate::{producer_graph::PreparedGraph, table::TableAccess};
use std::sync::Arc;

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
#[derive(Debug, thiserror::Error)]
pub enum Failure<E> {
    #[error("producer publication incomplete")]
    Incomplete,
    #[error("producer publication preflight: {0:?}")]
    Preflight(E),
    #[error("producer publication publish: node={node:?}, error={error:?}")]
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
    ) -> Result<Self, &'static str> {
        if attempt.outcome.is_err() || attempt.nodes.len() != graph.order().len() {
            return Err("producer_publication_incomplete");
        }
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
        }
        // Every retained artifact belongs to the actual completed node order.
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
        attempt: &mut PublicationAttempt<P::Error>,
    ) {
        *attempt = Default::default();
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
