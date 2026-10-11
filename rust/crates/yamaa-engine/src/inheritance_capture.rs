//! Raw inheritance capture admission and retained catalogs over explicit native ports.
//! YAML/schema representation and native path spelling remain decoder/host authority.
use crate::{
    inheritance as graph, inheritance_preparation as lifecycle, project_preparation::Source,
};
use alloc::{boxed::Box, string::String, vec::Vec};
use yamaa_core::schema::{Document, NormalizationBudget, NormalizationLimits, SchemaStructure};

/// Fail-fast inheritance authority. Unlike project metadata capture, an ordinary
/// failure stops the attempt; no independent-error collection or retry occurs.
pub trait CapturePort {
    type Error;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<graph::Source, graph::SourceError<Self::Error>>;
    fn capture(
        &mut self,
        source: &graph::Source,
        maximum_bytes: usize,
    ) -> Result<Vec<u8>, graph::SourceError<Self::Error>>;
    fn rebase(
        &mut self,
        layer: &graph::Source,
        entry: &graph::Source,
        written: &str,
        maximum_bytes: usize,
    ) -> Result<String, Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureResource {
    CapturedBytes,
    IdentityBytes,
}
/// Physical parsing preserves its own limits and original failure representation.
/// `limit_error` only constructs that representation for a closed engine refusal.
pub trait DecodePort {
    type Raw;
    type Error;
    fn decode(&mut self, identity: &str, bytes: &[u8]) -> Result<Self::Raw, Self::Error>;
    fn document(raw: &Self::Raw) -> &Document;
    fn limit_error(resource: CaptureResource) -> Self::Error;
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub captured_bytes: usize,
    pub identity_bytes: usize,
    pub normalization: NormalizationLimits,
    pub preparation: lifecycle::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            captured_bytes: 16_777_216,
            identity_bytes: 65_536,
            normalization: Default::default(),
            preparation: Default::default(),
        }
    }
}
#[derive(Debug)]
pub struct CapturedParent<R> {
    source: Source,
    raw: R,
}
impl<R> CapturedParent<R> {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &R {
        &self.raw
    }
}
#[derive(Debug)]
pub enum SourceFailure<E, X> {
    Host(E),
    Admission(X),
}
#[derive(Debug)]
pub enum Error<E, X, R> {
    Entry(X),
    Preparation {
        error: Box<lifecycle::Error<SourceFailure<E, X>>>,
        captured_parents: Vec<CapturedParent<R>>,
    },
}
#[derive(Debug)]
pub struct Prepared<R> {
    pub inheritance: lifecycle::PreparedInheritance,
    /// First successful physical-decode order, independently of layer postorder.
    pub parents: Vec<CapturedParent<R>>,
}
pub type PreparationResult<E, X, R> = Result<Prepared<R>, Error<E, X, R>>;
struct Application<'a, P, D: DecodePort> {
    host: &'a mut P,
    decoder: &'a mut D,
    remaining_bytes: usize,
    remaining_identities: usize,
    parents: Vec<CapturedParent<D::Raw>>,
}
fn host_error<E, X>(error: graph::SourceError<E>) -> graph::SourceError<SourceFailure<E, X>> {
    match error {
        graph::SourceError::Unavailable => graph::SourceError::Unavailable,
        graph::SourceError::Raised(error) => graph::SourceError::Raised(SourceFailure::Host(error)),
    }
}
impl<P: CapturePort, D: DecodePort> graph::SourcePort for Application<'_, P, D> {
    type Error = SourceFailure<P::Error, D::Error>;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
        self.host
            .canonicalize(declaring, written)
            .map_err(host_error)
    }
    fn read(
        &mut self,
        source: &graph::Source,
    ) -> Result<Document, graph::SourceError<Self::Error>> {
        let admission = |error| graph::SourceError::Raised(SourceFailure::Admission(error));
        self.remaining_identities = self
            .remaining_identities
            .checked_sub(source.identity.len())
            .ok_or_else(|| admission(D::limit_error(CaptureResource::IdentityBytes)))?;
        let bytes = self
            .host
            .capture(source, self.remaining_bytes)
            .map_err(host_error)?;
        self.remaining_bytes = self
            .remaining_bytes
            .checked_sub(bytes.len())
            .ok_or_else(|| admission(D::limit_error(CaptureResource::CapturedBytes)))?;
        let raw = self
            .decoder
            .decode(&source.identity, &bytes)
            .map_err(admission)?;
        let document = D::document(&raw).clone();
        self.parents.push(CapturedParent {
            source: Source {
                identity: source.identity.clone(),
                bytes,
            },
            raw,
        });
        Ok(document)
    }
}
impl<P: CapturePort, D: DecodePort> lifecycle::PathPort for Application<'_, P, D> {
    fn rebase(
        &mut self,
        layer: &graph::Source,
        entry: &graph::Source,
        written: &str,
        maximum_bytes: usize,
    ) -> Result<String, Self::Error> {
        self.host
            .rebase(layer, entry, written, maximum_bytes)
            .map_err(SourceFailure::Host)
    }
}
/// Continue from the exact already-decoded entry. Entry parsing occurs once in
/// the caller's codec; this application neither reopens nor decodes it again.
/// Root/capture, traversal, normalization and rebasing policies remain distinct.
pub fn prepare<P: CapturePort, D: DecodePort>(
    schema: &SchemaStructure,
    source: &Source,
    display_path: String,
    raw: &D::Raw,
    host: &mut P,
    decoder: &mut D,
    limits: Limits,
) -> PreparationResult<P::Error, D::Error, D::Raw> {
    let remaining_bytes = limits
        .captured_bytes
        .checked_sub(source.bytes.len())
        .ok_or_else(|| Error::Entry(D::limit_error(CaptureResource::CapturedBytes)))?;
    let remaining_identities = limits
        .identity_bytes
        .checked_sub(source.identity.len())
        .ok_or_else(|| Error::Entry(D::limit_error(CaptureResource::IdentityBytes)))?;
    let mut application = Application {
        host,
        decoder,
        remaining_bytes,
        remaining_identities,
        parents: Vec::new(),
    };
    let prepared = lifecycle::prepare(
        schema,
        graph::Source {
            identity: source.identity.clone(),
            display_path,
        },
        D::document(raw).clone(),
        &mut application,
        &mut NormalizationBudget::new(limits.normalization),
        limits.preparation,
    );
    match prepared {
        Ok(inheritance) => Ok(Prepared {
            inheritance,
            parents: application.parents,
        }),
        Err(error) => Err(Error::Preparation {
            error: Box::new(error),
            captured_parents: application.parents,
        }),
    }
}
