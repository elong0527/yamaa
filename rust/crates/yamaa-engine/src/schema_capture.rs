//! Admission of a complete captured schema closure over an explicit physical codec.
//! Include resolution, descriptors and default authority remain in the shared core.
use crate::project_preparation::Source;
use alloc::{string::String, vec::Vec};
use yamaa_core::schema::{BundleError, BundleLimits, Document, SchemaModule, SchemaStructure};

/// A codec keeps its own physical parsing limits and occurrence locations.
/// Locations must address this exact decoded document; the engine cannot infer
/// their authority from equal text or independently reconstructed arenas.
pub trait DecodePort {
    type Error;
    type Locations;
    fn decode(
        &mut self,
        identity: &str,
        bytes: &[u8],
    ) -> Result<DecodedModule<Self::Locations>, Self::Error>;
}
pub struct DecodedModule<L> {
    pub document: Document,
    pub locations: L,
}
/// The default route has no separate root-name text charge, as in the existing
/// public adapter. Named roots check that text before every other admission.
#[derive(Clone, Copy, Debug)]
pub enum RootSelection<'a> {
    Default,
    Named(&'a str),
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub captured_bytes: usize,
    pub identity_bytes: usize,
    pub bundle: BundleLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            captured_bytes: 16_777_216,
            identity_bytes: 65_536,
            bundle: Default::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    RootClass,
    SchemaModules,
    CapturedBytes,
    IdentityBytes,
}
#[derive(Debug)]
pub enum Error<E> {
    Limit(Resource),
    Decode { identity: String, error: E },
    Bundle(BundleError),
}
/// The original supplied source order and allocations survive admission.
#[derive(Debug)]
pub struct CapturedClosure<L> {
    sources: Vec<Source>,
    locations: Vec<L>,
    structure: SchemaStructure,
}
impl<L> CapturedClosure<L> {
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }
    pub fn locations(&self) -> &[L] {
        &self.locations
    }
    pub fn structure(&self) -> &SchemaStructure {
        &self.structure
    }
    pub fn into_parts(self) -> (Vec<Source>, Vec<L>, SchemaStructure) {
        (self.sources, self.locations, self.structure)
    }
}

/// Admit the whole captured closure before the first physical decode. Each
/// source charges bytes before identity text. Physical decoding completes in
/// supplied order before bundle semantic admission, including entry/name errors.
/// Core defaults are not validated early or promoted to execution qualification.
pub fn admit<P: DecodePort>(
    sources: Vec<Source>,
    entry: usize,
    root: RootSelection<'_>,
    limits: Limits,
    decoder: &mut P,
) -> Result<CapturedClosure<P::Locations>, Error<P::Error>> {
    let root_class = match root {
        RootSelection::Default => "root_class",
        RootSelection::Named(name) => {
            if name.len() > limits.identity_bytes {
                return Err(Error::Limit(Resource::RootClass));
            }
            name
        }
    };
    if sources.len() > limits.bundle.modules {
        return Err(Error::Limit(Resource::SchemaModules));
    }
    let (mut bytes, mut identities) = (0usize, 0usize);
    for source in &sources {
        bytes = bytes
            .checked_add(source.bytes.len())
            .filter(|&n| n <= limits.captured_bytes)
            .ok_or(Error::Limit(Resource::CapturedBytes))?;
        identities = identities
            .checked_add(source.identity.len())
            .filter(|&n| n <= limits.identity_bytes)
            .ok_or(Error::Limit(Resource::IdentityBytes))?;
    }
    let mut locations = Vec::with_capacity(sources.len());
    let mut modules = Vec::with_capacity(sources.len());
    for source in &sources {
        let decoded = decoder
            .decode(&source.identity, &source.bytes)
            .map_err(|error| Error::Decode {
                identity: source.identity.clone(),
                error,
            })?;
        locations.push(decoded.locations);
        modules.push(SchemaModule {
            name: source.identity.clone(),
            document: decoded.document,
        });
    }
    let structure =
        SchemaStructure::admit(modules, entry, root_class, limits.bundle).map_err(Error::Bundle)?;
    Ok(CapturedClosure {
        sources,
        locations,
        structure,
    })
}
