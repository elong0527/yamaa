//! Decode captured inheritance sources; shared application code owns traversal and preparation.
use super::*;
use yamaa_core::schema::Document;
use yamaa_engine::{inheritance as graph, inheritance_preparation as lifecycle};

pub trait InheritancePort {
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

#[derive(Debug)]
pub struct CapturedParent {
    source: Source,
    raw: DecodedYaml,
}
impl CapturedParent {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &DecodedYaml {
        &self.raw
    }
}
#[derive(Debug)]
pub enum SourceFailure<E> {
    Host(E),
    Admission(Error),
}
#[derive(Debug)]
pub enum InheritanceError<E> {
    Entry(Error),
    Preparation {
        error: Box<lifecycle::Error<SourceFailure<E>>>,
        captured_parents: Vec<CapturedParent>,
    },
}

struct Decoder<'a, P> {
    host: &'a mut P,
    limits: Limits,
    remaining_bytes: usize,
    remaining_identities: usize,
    parents: Vec<CapturedParent>,
}
fn raised<E>(error: Error) -> graph::SourceError<SourceFailure<E>> {
    graph::SourceError::Raised(SourceFailure::Admission(error))
}
fn host_error<E>(error: graph::SourceError<E>) -> graph::SourceError<SourceFailure<E>> {
    match error {
        graph::SourceError::Unavailable => graph::SourceError::Unavailable,
        graph::SourceError::Raised(error) => graph::SourceError::Raised(SourceFailure::Host(error)),
    }
}
impl<P: InheritancePort> graph::SourcePort for Decoder<'_, P> {
    type Error = SourceFailure<P::Error>;
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
        self.remaining_identities = self
            .remaining_identities
            .checked_sub(source.identity.len())
            .ok_or_else(|| raised(Error::Limit("identity_bytes")))?;
        let bytes = self
            .host
            .capture(source, self.remaining_bytes)
            .map_err(host_error)?;
        self.remaining_bytes = self
            .remaining_bytes
            .checked_sub(bytes.len())
            .ok_or_else(|| raised(Error::Limit("captured_bytes")))?;
        let raw = decode_yaml(&bytes, self.limits.decode).map_err(|error| {
            raised(Error::Decode {
                identity: source.identity.clone(),
                error,
            })
        })?;
        let document = raw.document.clone();
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
impl<P: InheritancePort> lifecycle::PathPort for Decoder<'_, P> {
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
impl CapturedSchema {
    /// Preserve raw YAML captures while the engine prepares the inherited model.
    pub fn prepare_inherited<P: InheritancePort>(
        self: &Arc<Self>,
        source: Source,
        display_path: String,
        port: &mut P,
    ) -> Result<PreparedDocument, InheritanceError<P::Error>> {
        let remaining_bytes = self
            .limits
            .captured_bytes
            .checked_sub(source.bytes.len())
            .ok_or(InheritanceError::Entry(Error::Limit("captured_bytes")))?;
        let remaining_identities = self
            .limits
            .identity_bytes
            .checked_sub(source.identity.len())
            .ok_or(InheritanceError::Entry(Error::Limit("identity_bytes")))?;
        let raw = decode_yaml(&source.bytes, self.limits.decode).map_err(|error| {
            InheritanceError::Entry(Error::Decode {
                identity: source.identity.clone(),
                error,
            })
        })?;
        let mut decoder = Decoder {
            host: port,
            limits: self.limits,
            remaining_bytes,
            remaining_identities,
            parents: Vec::new(),
        };
        let prepared = lifecycle::prepare(
            &self.structure,
            graph::Source {
                identity: source.identity.clone(),
                display_path,
            },
            raw.document.clone(),
            &mut decoder,
            &mut NormalizationBudget::new(self.limits.normalization),
            Default::default(),
        );
        match prepared {
            Ok(prepared) => Ok(PreparedDocument {
                schema: Arc::clone(self),
                source,
                raw,
                content: PreparedContent::Inherited(Box::new(prepared)),
                parents: decoder.parents,
            }),
            Err(error) => Err(InheritanceError::Preparation {
                error: Box::new(error),
                captured_parents: decoder.parents,
            }),
        }
    }
}
