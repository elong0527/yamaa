//! Admission of a complete captured schema closure over an explicit physical codec.
//! Include resolution, descriptors and default authority remain in the shared core.
use crate::project_preparation::Source;
use crate::schema::{BundleError, Document, SchemaModule, SchemaStructure};

/// Discovery grants read authority only to names accepted by the existing core
/// include grammar. Unsafe authored names remain for final bundle diagnostics.
pub fn include_names(input: &Document) -> Vec<&str> {
    use crate::schema::DocumentNode;
    let Some(index) = input.field(input.root(), "includes") else {
        return Vec::new();
    };
    let DocumentNode::Sequence(entries) = &input.nodes()[index] else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|&entry| match &input.nodes()[entry] {
            DocumentNode::Text(value) if crate::schema::schema_include_name(value) => {
                Some(value.as_str())
            }
            _ => None,
        })
        .collect()
}

/// Read-only queries borrow an already admitted closure.
/// Physical codecs, host query carriers and result serialization stay in adapters.
pub fn validate_query(
    schema: &SchemaStructure,
    input: &Document,
    types: &[String],
    fragment: bool,
) -> Result<Vec<crate::schema::SchemaDiagnostic>, crate::schema::ValidationError> {
    let selected = schema.parse_query_types(types)?;
    schema.validate_types(&selected, input, input.root(), "", fragment)
}

pub fn normalize_query(
    schema: &SchemaStructure,
    input: &Document,
    types: &[String],
    fragment: bool,
) -> Result<crate::schema::NormalizedDocument, crate::schema::NormalizationError> {
    let selected = schema.parse_query_types(types)?;
    schema.normalize_types(&selected, input, input.root(), fragment)
}

pub fn default_findings(
    schema: &SchemaStructure,
) -> Result<Vec<crate::schema::DefaultDiagnostics>, crate::schema::ValidationError> {
    schema.validate_defaults()
}

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
/// Select the structural root without changing its admitted schema authority.
#[derive(Clone, Copy, Debug)]
pub enum RootSelection<'a> {
    Default,
    Named(&'a str),
}
#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("schema capture decode: identity={identity:?}, error={error:?}")]
    Decode { identity: String, error: E },
    #[error("schema capture bundle: {0:?}")]
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

/// Physical decoding of each file completes in
/// supplied order before bundle semantic admission, including entry/name errors.
/// Core defaults are not validated early or promoted to execution qualification.
pub fn admit<P: DecodePort>(
    sources: Vec<Source>,
    entry: usize,
    root: RootSelection<'_>,
    decoder: &mut P,
) -> Result<CapturedClosure<P::Locations>, Error<P::Error>> {
    let root_class = match root {
        RootSelection::Default => "root_class",
        RootSelection::Named(name) => name,
    };
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
    let structure = SchemaStructure::admit(modules, entry, root_class).map_err(Error::Bundle)?;
    Ok(CapturedClosure {
        sources,
        locations,
        structure,
    })
}
