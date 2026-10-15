//! One planning capture per resolved dictionary identity, with original policy failures.
use crate::{
    inheritance::SourceError,
    inheritance_capture::{CapturePort, CaptureResource, DecodePort},
    inheritance_preparation::PreparedInheritance,
    project_preparation::Source,
};
use alloc::{boxed::Box, vec::Vec};
use yamaa_core::{
    resource::ResourceFailure,
    schema::{
        AdmittedMappingDictionary, MappingDictionaryReference, NormalizationBudget,
        NormalizationError, NormalizationLimits, SchemaStructure, SpecificationDocument,
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
    Limit(X),
}
#[derive(Debug)]
pub struct Error<E, X, R> {
    pub cause: Cause<E, X>,
    pub captures: Vec<Captured<R>>,
}
#[derive(Debug)]
pub struct Prepared<R> {
    pub model: SpecificationDocument,
    pub captures: Vec<Captured<R>>,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub captured_bytes: usize,
    pub identity_bytes: usize,
    pub normalization: NormalizationLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            captured_bytes: 16_777_216,
            identity_bytes: 65_536,
            normalization: Default::default(),
        }
    }
}
struct Run<'a, P: CapturePort, D: DecodePort> {
    port: &'a mut P,
    decoder: &'a mut D,
    seeds: &'a [Seed<'a, D::Raw>],
    captures: Vec<Captured<D::Raw>>,
    remaining_bytes: usize,
    remaining_identities: usize,
    budget: NormalizationBudget,
}
impl<P: CapturePort, D: DecodePort> Run<'_, P, D> {
    fn admit_seed(&mut self, source: &Source) -> Result<(), Cause<P::Error, D::Error>> {
        self.remaining_bytes = self
            .remaining_bytes
            .checked_sub(source.bytes.len())
            .ok_or_else(|| Cause::Limit(D::limit_error(CaptureResource::CapturedBytes)))?;
        self.remaining_identities = self
            .remaining_identities
            .checked_sub(source.identity.len())
            .ok_or_else(|| Cause::Limit(D::limit_error(CaptureResource::IdentityBytes)))?;
        Ok(())
    }
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
        // Compare every held identity only after its maximum text work is admitted.
        self.budget
            .work(
                identity
                    .identity
                    .len()
                    .saturating_mul(self.seeds.len().saturating_add(self.captures.len())),
            )
            .map_err(Cause::Materialize)?;
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
            self.remaining_identities = self
                .remaining_identities
                .checked_sub(identity.identity.len())
                .ok_or_else(|| Cause::Limit(D::limit_error(CaptureResource::IdentityBytes)))?;
            let bytes = match self.port.capture_resource(&identity, self.remaining_bytes) {
                Ok(bytes) => bytes,
                Err(error) => return Err(self.capture_failure(reference, error)),
            };
            self.remaining_bytes = self
                .remaining_bytes
                .checked_sub(bytes.len())
                .ok_or_else(|| Cause::Limit(D::limit_error(CaptureResource::CapturedBytes)))?;
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
        self.budget
            .text(
                reference
                    .path()
                    .len()
                    .saturating_add(reference.written().len()),
            )
            .map_err(Cause::Materialize)?;
        schema
            .admit_mapping_dictionary(reference.clone(), D::document(raw), &mut self.budget)
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
    limits: Limits,
) -> PreparationResult<P::Error, D::Error, D::Raw> {
    let mut run = Run {
        port,
        decoder,
        seeds,
        captures: Vec::new(),
        remaining_bytes: limits.captured_bytes,
        remaining_identities: limits.identity_bytes,
        budget: NormalizationBudget::new(limits.normalization),
    };
    let attempt = (|| {
        for (index, seed) in seeds.iter().enumerate() {
            run.budget
                .work(
                    index
                        .saturating_mul(seed.source.identity.len())
                        .saturating_add(1),
                )
                .map_err(Cause::Materialize)?;
            if !seeds[..index]
                .iter()
                .any(|prior| prior.source.identity == seed.source.identity)
            {
                run.admit_seed(seed.source)?;
            }
        }
        let references = schema
            .mapping_dictionary_references(model.document(), &mut run.budget)
            .map_err(Cause::Materialize)?;
        if references.is_empty() {
            return Ok(None);
        }
        let mut dictionaries = Vec::new();
        for reference in references {
            if let Origins::Inherited(prepared) = &origins {
                run.budget
                    .work(prepared.provenance().iter().fold(1usize, |work, origin| {
                        work.saturating_add(
                            origin.path.len().saturating_add(reference.path().len()),
                        )
                    }))
                    .map_err(Cause::Materialize)?;
            }
            let declaring = match origins.declaring(reference.path()) {
                Some(declaring) => declaring,
                None => return Err(Cause::MissingOrigin(reference)),
            };
            dictionaries.push(run.dictionary(schema, reference, declaring)?);
        }
        schema
            .materialize_mapping_dictionaries(model, &dictionaries, &mut run.budget)
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
