//! The layers a specification's traversal captures besides its entry, its
//! parents and included fragments, each held with what it decoded to, and why
//! a layer could not be captured. Capture goes through the resolver port
//! ([`crate::resolver`]) and decoding through the decoder port
//! ([`crate::decoder`]).
use crate::project_preparation::Source;

#[derive(Debug)]
pub struct CapturedParent<R> {
    source: Source,
    raw: R,
}
impl<R> CapturedParent<R> {
    /// A layer captured as `source` and decoded as `raw`, as a 1.x chain's
    /// traversal records each parent and fragment it reads.
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
/// Why a layer a chain's traversal reads could not be captured: the host's
/// own failure, or the captured bytes' admission.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum SourceFailure<E, X> {
    #[error("inheritance capture host: {0:?}")]
    Host(E),
    #[error("inheritance capture admission: {0:?}")]
    Admission(X),
}
