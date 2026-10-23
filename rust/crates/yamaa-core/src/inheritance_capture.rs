//! The native ports a specification's layers and resources are captured
//! through, and the parents a chain's traversal records. YAML/schema
//! representation and native path spelling remain decoder/host authority.
use crate::project_preparation::Source;
use crate::schema::Document;

/// A physical decoder retains original source and occurrence authority.
pub trait InheritanceDecodePort {
    type Raw;
    type Error;
    fn decode(&mut self, identity: &str, bytes: &[u8]) -> Result<Self::Raw, Self::Error>;
    fn document(raw: &Self::Raw) -> &Document;
}
#[derive(Debug)]
pub struct CapturedParent<R> {
    source: Source,
    raw: R,
}
impl<R> CapturedParent<R> {
    /// A parent captured as `source` and decoded as `raw`, as a 1.x chain's
    /// traversal records each parent it reads.
    pub fn new(source: Source, raw: R) -> Self {
        Self { source, raw }
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &R {
        &self.raw
    }
}
/// Why a parent a chain's traversal reads could not be captured: the host's
/// own failure, or the captured bytes' admission.
#[derive(Debug, thiserror::Error)]
pub enum SourceFailure<E, X> {
    #[error("inheritance capture host: {0:?}")]
    Host(E),
    #[error("inheritance capture admission: {0:?}")]
    Admission(X),
}
