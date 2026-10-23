//! One planning capture per resolved dictionary identity, with original policy failures.
use crate::{
    inheritance::SourceError,
    inheritance_capture::InheritanceDecodePort,
    inheritance_preparation::PreparedInheritance,
    project_preparation::Source,
    resolver::{self, File, Name, Resolver},
};
use crate::{
    resource::ResourceFailure,
    schema::{
        AdmittedMappingDictionary, Document, LayerProvenance, MappingDictionaryReference,
        NormalizationError, SchemaStructure, SpecificationDocument,
    },
};

/// Only the retained declaring contribution can authorize a relative resource path.
pub enum Origins<'a> {
    Standalone(&'a str),
    Inherited(&'a PreparedInheritance),
    /// A resolved 1.x specification: the layer that contributed each
    /// written path, and its layers in contribution order.
    Versioned {
        provenance: &'a [LayerProvenance],
        layers: &'a [crate::inheritance::VersionedLayer],
    },
}
/// The layer, numbered in contribution order, that contributed `path` or
/// the nearest written path holding it.
fn contribution(provenance: &[LayerProvenance], path: &str) -> Option<usize> {
    provenance
        .iter()
        .filter(|origin| {
            path == origin.path
                || path
                    .strip_prefix(&origin.path)
                    .is_some_and(|suffix| suffix.starts_with(['.', '[']))
        })
        .max_by_key(|origin| origin.path.len())
        .map(|origin| origin.layer)
}
impl Origins<'_> {
    fn declaring(&self, path: &str) -> Option<&str> {
        match self {
            Self::Standalone(source) => Some(source),
            Self::Inherited(prepared) => {
                let layer = contribution(prepared.provenance(), path)?;
                Some(&prepared.layers().get(layer)?.source.identity)
            }
            Self::Versioned { provenance, layers } => {
                let layer = contribution(provenance, path)?;
                Some(&layers.get(layer)?.source.identity)
            }
        }
    }
}
/// Entry and parents have already been captured and physically decoded.
pub struct Seed<'a, R> {
    pub source: &'a Source,
    pub raw: &'a R,
}
#[derive(Debug)]
pub struct Captured<R> {
    source: Source,
    raw: R,
}
impl<R> Captured<R> {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &R {
        &self.raw
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arena {
    Seed(usize),
    Captured(usize),
}
#[derive(Debug)]
pub enum Cause<E, X> {
    MissingOrigin(MappingDictionaryReference),
    Capture {
        reference: MappingDictionaryReference,
        error: SourceError<E>,
        failure: Option<ResourceFailure>,
    },
    Decode {
        reference: MappingDictionaryReference,
        source: Source,
        error: X,
    },
    Admission {
        reference: MappingDictionaryReference,
        arena: Arena,
        error: NormalizationError,
    },
    Materialize(NormalizationError),
}
#[derive(Debug, thiserror::Error)]
#[error("dictionary preparation: cause={cause:?}, captures={captures:?}")]
pub struct Error<E, X, R> {
    pub cause: Cause<E, X>,
    pub captures: Vec<Captured<R>>,
}
#[derive(Debug)]
pub struct Prepared<R> {
    pub model: SpecificationDocument,
    pub captures: Vec<Captured<R>>,
}
struct Run<'a, P: Resolver, D: InheritanceDecodePort> {
    port: &'a mut P,
    decoder: &'a mut D,
    seeds: &'a [Seed<'a, D::Raw>],
    captures: Vec<Captured<D::Raw>>,
}
impl<P: Resolver, D: InheritanceDecodePort> Run<'_, P, D> {
    fn capture_failure(
        &self,
        reference: MappingDictionaryReference,
        name: Name<'_>,
        error: P::Error,
    ) -> Cause<P::Error, D::Error> {
        let failure = self
            .port
            .failure(name, &error)
            .and_then(resolver::Failure::resource);
        Cause::Capture {
            reference,
            error: SourceError::Raised(error),
            failure,
        }
    }
    fn dictionary(
        &mut self,
        schema: &SchemaStructure,
        reference: MappingDictionaryReference,
        declaring: &str,
    ) -> Result<AdmittedMappingDictionary, Cause<P::Error, D::Error>> {
        let written = Name::Written {
            file: File::Resource,
            declaring,
            written: reference.written(),
        };
        let identity = match self.port.inspect(written) {
            Ok(identity) => identity,
            Err(error) => return Err(self.capture_failure(reference.clone(), written, error)),
        };

        let arena = if let Some(index) = self
            .seeds
            .iter()
            .position(|seed| seed.source.identity == identity.identity)
        {
            Arena::Seed(index)
        } else if let Some(index) = self
            .captures
            .iter()
            .position(|capture| capture.source.identity == identity.identity)
        {
            Arena::Captured(index)
        } else {
            let located = Name::Located {
                file: File::Resource,
                source: &identity,
            };
            let bytes = match self.port.capture(located) {
                Ok(captured) => resolver::bytes(captured),
                Err(error) => return Err(self.capture_failure(reference, located, error)),
            };
            let source = Source {
                identity: identity.identity,
                bytes,
            };
            let raw = match self.decoder.decode(&source.identity, &source.bytes) {
                Ok(raw) => raw,
                Err(error) => {
                    return Err(Cause::Decode {
                        reference,
                        source,
                        error,
                    })
                }
            };
            let index = self.captures.len();
            self.captures.push(Captured { source, raw });
            Arena::Captured(index)
        };
        let raw = match arena {
            Arena::Seed(index) => self.seeds[index].raw,
            Arena::Captured(index) => &self.captures[index].raw,
        };

        schema
            .admit_mapping_dictionary(reference.clone(), D::document(raw))
            .map_err(|error| Cause::Admission {
                reference,
                arena,
                error,
            })
    }
}
pub type PreparationResult<E, X, R> = Result<Option<Prepared<R>>, Box<Error<E, X, R>>>;
/// Metadata only. Failed parsing/admission keeps the entered capture prefix and
/// original host error; no study source, callback or publication authority is entered.
pub fn prepare<P: Resolver, D: InheritanceDecodePort>(
    schema: &SchemaStructure,
    model: &SpecificationDocument,
    origins: Origins<'_>,
    seeds: &[Seed<'_, D::Raw>],
    port: &mut P,
    decoder: &mut D,
) -> PreparationResult<P::Error, D::Error, D::Raw> {
    materialized(
        schema,
        model.document(),
        origins,
        seeds,
        port,
        decoder,
        |dictionaries| schema.materialize_mapping_dictionaries(model, dictionaries),
    )
    .map(|prepared| prepared.map(|(model, captures)| Prepared { model, captures }))
}

/// The dictionaries a resolved 1.x specification reads, materialized into a
/// compiler-only copy of it, as [`prepare`] materializes a 0.5 model's.
#[derive(Debug)]
pub struct PreparedDocument<R> {
    /// The specification with each dictionary file's admitted content in
    /// place of its path. It grants no source IO and is never shown as the
    /// resolved specification.
    pub document: Document,
    /// Parallel to `document`'s nodes: the node of the resolved
    /// specification each copies, or the `dict` path whose file holds it.
    pub sources: Vec<usize>,
    pub captures: Vec<Captured<R>>,
}
pub type DocumentPreparationResult<E, X, R> =
    Result<Option<PreparedDocument<R>>, Box<Error<E, X, R>>>;
/// [`prepare`] for a resolved 1.x specification held as `document`: each
/// dictionary file it names is captured from the layer that wrote its path,
/// admitted, and materialized; `None` when it names none.
pub fn prepare_document<P: Resolver, D: InheritanceDecodePort>(
    schema: &SchemaStructure,
    document: &Document,
    origins: Origins<'_>,
    seeds: &[Seed<'_, D::Raw>],
    port: &mut P,
    decoder: &mut D,
) -> DocumentPreparationResult<P::Error, D::Error, D::Raw> {
    materialized(
        schema,
        document,
        origins,
        seeds,
        port,
        decoder,
        |dictionaries| schema.materialize_mapping_dictionary_document(document, dictionaries),
    )
    .map(|prepared| {
        prepared.map(|((document, sources), captures)| PreparedDocument {
            document,
            sources,
            captures,
        })
    })
}

/// Capture and admit every dictionary file `document` names, then
/// `materialize` them.
#[allow(clippy::type_complexity)]
fn materialized<P: Resolver, D: InheritanceDecodePort, T>(
    schema: &SchemaStructure,
    document: &Document,
    origins: Origins<'_>,
    seeds: &[Seed<'_, D::Raw>],
    port: &mut P,
    decoder: &mut D,
    materialize: impl FnOnce(&[AdmittedMappingDictionary]) -> Result<T, NormalizationError>,
) -> Result<Option<(T, Vec<Captured<D::Raw>>)>, Box<Error<P::Error, D::Error, D::Raw>>> {
    let mut run = Run {
        port,
        decoder,
        seeds,
        captures: Vec::new(),
    };
    let attempt = (|| {
        let references = schema
            .mapping_dictionary_references(document)
            .map_err(Cause::Materialize)?;
        if references.is_empty() {
            return Ok(None);
        }
        let mut dictionaries = Vec::new();
        for reference in references {
            let declaring = match origins.declaring(reference.path()) {
                Some(declaring) => declaring,
                None => return Err(Cause::MissingOrigin(reference)),
            };
            dictionaries.push(run.dictionary(schema, reference, declaring)?);
        }
        materialize(&dictionaries)
            .map(Some)
            .map_err(Cause::Materialize)
    })();
    match attempt {
        Ok(None) => Ok(None),
        Ok(Some(materialized)) => Ok(Some((materialized, run.captures))),
        Err(cause) => Err(Box::new(Error {
            cause,
            captures: run.captures,
        })),
    }
}
