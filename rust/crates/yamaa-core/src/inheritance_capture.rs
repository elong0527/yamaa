//! Raw inheritance capture admission and retained catalogs over explicit native ports.
//! YAML/schema representation and native path spelling remain decoder/host authority.
use crate::schema::{Document, SchemaStructure};
use crate::{
    inheritance as graph, inheritance_preparation as lifecycle, project_preparation::Source,
};

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
    ) -> Result<Vec<u8>, graph::SourceError<Self::Error>>;
    /// Planning resources retain their own path failures rather than parent-unavailability semantics.
    fn canonicalize_resource(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
        self.canonicalize(declaring, written)
    }
    fn capture_resource(
        &mut self,
        source: &graph::Source,
    ) -> Result<Vec<u8>, graph::SourceError<Self::Error>> {
        self.capture(source)
    }
    fn resource_failure(&self, _error: &Self::Error) -> Option<crate::resource::ResourceFailure> {
        None
    }
    fn rebase(
        &mut self,
        layer: &graph::Source,
        entry: &graph::Source,
        written: &str,
    ) -> Result<String, Self::Error>;
}
/// A physical decoder retains original source and occurrence authority.
pub trait DecodePort {
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
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &R {
        &self.raw
    }
}
#[derive(Debug, thiserror::Error)]
pub enum SourceFailure<E, X> {
    #[error("inheritance capture host: {0:?}")]
    Host(E),
    #[error("inheritance capture admission: {0:?}")]
    Admission(X),
}
#[derive(Debug, thiserror::Error)]
pub enum Error<E, X, R> {
    #[error(
        "inheritance capture preparation: error={error:?}, captured_parents={captured_parents:?}"
    )]
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
        let bytes = self.host.capture(source).map_err(host_error)?;
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
    ) -> Result<String, Self::Error> {
        self.host
            .rebase(layer, entry, written)
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
) -> PreparationResult<P::Error, D::Error, D::Raw> {
    let mut application = Application {
        host,
        decoder,
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
