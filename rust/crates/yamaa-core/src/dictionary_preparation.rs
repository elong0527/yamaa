//! One planning capture per resolved dictionary identity, with original policy failures.
use crate::{
    inheritance::SourceError,
    inheritance_capture::{CapturePort, DecodePort},
    inheritance_preparation::PreparedInheritance,
    project_preparation::Source,
};
use crate::{
    resource::ResourceFailure,
    schema::{
        AdmittedMappingDictionary, MappingDictionaryReference, NormalizationError, SchemaStructure,
        SpecificationDocument,
    },
};

/// Only the retained declaring contribution can authorize a relative resource path.
pub enum Origins<'a> {
    Standalone(&'a str),
    Inherited(&'a PreparedInheritance),
}
impl Origins<'_> {
    fn declaring(&self, path: &str) -> Option<&str> {
        match self {
            Self::Standalone(source) => Some(source),
            Self::Inherited(prepared) => {
                let contribution = prepared
                    .provenance()
                    .iter()
                    .filter(|origin| {
                        path == origin.path
                            || path
                                .strip_prefix(&origin.path)
                                .is_some_and(|suffix| suffix.starts_with(['.', '[']))
                    })
                    .max_by_key(|origin| origin.path.len())?;
                Some(&prepared.layers().get(contribution.layer)?.source.identity)
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
struct Run<'a, P: CapturePort, D: DecodePort> {
    port: &'a mut P,
    decoder: &'a mut D,
    seeds: &'a [Seed<'a, D::Raw>],
    captures: Vec<Captured<D::Raw>>,
}
impl<P: CapturePort, D: DecodePort> Run<'_, P, D> {
    fn capture_failure(
        &self,
        reference: MappingDictionaryReference,
        error: SourceError<P::Error>,
    ) -> Cause<P::Error, D::Error> {
        let failure = match &error {
            SourceError::Raised(error) => self.port.resource_failure(error),
            _ => None,
        };
        Cause::Capture {
            reference,
            error,
            failure,
        }
    }
    fn dictionary(
        &mut self,
        schema: &SchemaStructure,
        reference: MappingDictionaryReference,
        declaring: &str,
    ) -> Result<AdmittedMappingDictionary, Cause<P::Error, D::Error>> {
        let identity = match self
            .port
            .canonicalize_resource(declaring, reference.written())
        {
            Ok(identity) => identity,
            Err(error) => return Err(self.capture_failure(reference, error)),
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
            let bytes = match self.port.capture_resource(&identity) {
                Ok(bytes) => bytes,
                Err(error) => return Err(self.capture_failure(reference, error)),
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
pub fn prepare<P: CapturePort, D: DecodePort>(
    schema: &SchemaStructure,
    model: &SpecificationDocument,
    origins: Origins<'_>,
    seeds: &[Seed<'_, D::Raw>],
    port: &mut P,
    decoder: &mut D,
) -> PreparationResult<P::Error, D::Error, D::Raw> {
    let mut run = Run {
        port,
        decoder,
        seeds,
        captures: Vec::new(),
    };
    let attempt = (|| {
        let references = schema
            .mapping_dictionary_references(model.document())
            .map_err(Cause::Materialize)?;
        if references.is_empty() {
            return Ok(None);
        }
        let mut dictionaries = Vec::new();
        for reference in references {
            if let Origins::Inherited(prepared) = &origins {}
            let declaring = match origins.declaring(reference.path()) {
                Some(declaring) => declaring,
                None => return Err(Cause::MissingOrigin(reference)),
            };
            dictionaries.push(run.dictionary(schema, reference, declaring)?);
        }
        schema
            .materialize_mapping_dictionaries(model, &dictionaries)
            .map(Some)
            .map_err(Cause::Materialize)
    })();
    match attempt {
        Ok(None) => Ok(None),
        Ok(Some(model)) => Ok(Some(Prepared {
            model,
            captures: run.captures,
        })),
        Err(cause) => Err(Box::new(Error {
            cause,
            captures: run.captures,
        })),
    }
}
