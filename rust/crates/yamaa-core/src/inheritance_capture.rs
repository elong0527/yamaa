//! Raw inheritance capture admission and retained catalogs over the resolver
//! port. YAML/schema representation and native path spelling remain decoder
//! and host authority.
use crate::schema::{Document, SchemaStructure};
use crate::{
    inheritance as graph, inheritance_preparation as lifecycle,
    project_preparation::Source,
    resolver::{self, File, Name, Resolver},
};

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
struct Application<'a, P, D: InheritanceDecodePort> {
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
impl<P: Resolver, D: InheritanceDecodePort> graph::InheritanceSourcePort for Application<'_, P, D> {
    type Error = SourceFailure<P::Error, D::Error>;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
        let name = Name::Written {
            file: File::Parent,
            declaring,
            written,
        };
        self.host
            .inspect(name)
            .map_err(|error| host_error(resolver::source_error(self.host, name, error)))
    }
    fn read(
        &mut self,
        source: &graph::Source,
    ) -> Result<Document, graph::SourceError<Self::Error>> {
        let admission = |error| graph::SourceError::Raised(SourceFailure::Admission(error));
        let name = Name::Located {
            file: File::Parent,
            source,
        };
        let bytes = self
            .host
            .capture(name)
            .map(resolver::bytes)
            .map_err(|error| host_error(resolver::source_error(self.host, name, error)))?;
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
impl<P: Resolver, D: InheritanceDecodePort> lifecycle::PathPort for Application<'_, P, D> {
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
pub fn prepare<P: Resolver, D: InheritanceDecodePort>(
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
