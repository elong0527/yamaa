//! Captured raw YAML entry point. Hosts supply bytes and source identities only.
//! Filesystem authority, include discovery and publication remain explicit host ports.
use crate::yaml_decode::{decode_yaml, DecodeFailure, DecodeLimits, DecodedYaml, SourcePosition};
use std::sync::Arc;
use yamaa_core::schema::{
    BundleError, BundleLimits, NormalizationBudget, NormalizationError, NormalizationLimits,
    SchemaDiagnostic, SchemaModule, SchemaOrigin, SchemaStructure, SpecificationDocument,
    ValidationError, WindowReference,
};

/// The host's retained identity and byte snapshot. Names are not content digests.
#[derive(Clone, Debug)]
pub struct Source {
    pub identity: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub captured_bytes: usize,
    pub identity_bytes: usize,
    pub decode: DecodeLimits,
    pub bundle: BundleLimits,
    pub normalization: NormalizationLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            captured_bytes: 16_777_216,
            identity_bytes: 65_536,
            decode: Default::default(),
            bundle: Default::default(),
            normalization: Default::default(),
        }
    }
}
#[derive(Debug)]
pub enum Error {
    Limit(&'static str),
    Decode {
        identity: String,
        error: DecodeFailure,
    },
    Bundle(BundleError),
    Normalize(NormalizationError),
    Validation(ValidationError),
    Model(Vec<SchemaDiagnostic>),
    /// This entry point cannot silently treat an inherited layer as standalone.
    InheritanceRequired,
}

/// One immutable admitted schema closure, reusable across separate specification runs.
#[derive(Debug)]
pub struct CapturedSchema {
    sources: Vec<Source>,
    locations: Vec<Vec<SourcePosition>>,
    structure: SchemaStructure,
    limits: Limits,
}
impl CapturedSchema {
    /// Decode the complete host-captured closure using the shared YAML semantics.
    /// Cumulative byte/count admission happens before any source is decoded.
    pub fn admit(sources: Vec<Source>, entry: usize, limits: Limits) -> Result<Arc<Self>, Error> {
        if sources.len() > limits.bundle.modules {
            return Err(Error::Limit("schema_modules"));
        }
        let mut bytes = 0usize;
        let mut identities = 0usize;
        for source in &sources {
            bytes = bytes
                .checked_add(source.bytes.len())
                .filter(|&n| n <= limits.captured_bytes)
                .ok_or(Error::Limit("captured_bytes"))?;
            identities = identities
                .checked_add(source.identity.len())
                .filter(|&n| n <= limits.identity_bytes)
                .ok_or(Error::Limit("identity_bytes"))?;
        }
        let mut locations = Vec::with_capacity(sources.len());
        let modules = sources
            .iter()
            .map(|source| {
                let decoded =
                    decode_yaml(&source.bytes, limits.decode).map_err(|error| Error::Decode {
                        identity: source.identity.clone(),
                        error,
                    })?;
                locations.push(decoded.locations);
                Ok(SchemaModule {
                    name: source.identity.clone(),
                    document: decoded.document,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let structure = SchemaStructure::admit(modules, entry, "root_class", limits.bundle)
            .map_err(Error::Bundle)?;
        Ok(Arc::new(Self {
            sources,
            locations,
            structure,
            limits,
        }))
    }
    /// Preserve the exact captured bytes for provenance and direct equality checks.
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }
    pub fn locations(&self) -> &[Vec<SourcePosition>] {
        &self.locations
    }
    pub fn structure(&self) -> &SchemaStructure {
        &self.structure
    }

    /// Structural preparation only: no source data, executable plan or effects.
    /// Presence of `parents`, including null, must use the inheritance lifecycle.
    pub fn prepare_standalone(self: &Arc<Self>, source: Source) -> Result<PreparedDocument, Error> {
        if source.bytes.len() > self.limits.captured_bytes {
            return Err(Error::Limit("captured_bytes"));
        }
        if source.identity.len() > self.limits.identity_bytes {
            return Err(Error::Limit("identity_bytes"));
        }
        let raw =
            decode_yaml(&source.bytes, self.limits.decode).map_err(|error| Error::Decode {
                identity: source.identity.clone(),
                error,
            })?;
        if raw.document.field(raw.document.root(), "parents").is_some() {
            return Err(Error::InheritanceRequired);
        }
        let mut budget = NormalizationBudget::new(self.limits.normalization);
        let normalized = self
            .structure
            .normalize_document(&raw.document, &mut budget)
            .map_err(Error::Normalize)?;
        let expanded = self
            .structure
            .expand_named_windows(&normalized.document, true, &mut budget)
            .map_err(Error::Normalize)?;
        let origins = expanded
            .origins
            .iter()
            .map(|&id| normalized.origins[id])
            .collect();
        let model = SpecificationDocument::admit(expanded.document, budget.validation_scope())
            .map_err(Error::Validation)?
            .map_err(Error::Model)?;
        Ok(PreparedDocument {
            schema: Arc::clone(self),
            source,
            raw,
            model,
            origins,
            windows: expanded.references,
        })
    }
}

/// Retains every final occurrence's raw/schema-default origin and schema snapshot.
/// Construction is private; structural admission is never execution qualification.
#[derive(Debug)]
pub struct PreparedDocument {
    schema: Arc<CapturedSchema>,
    source: Source,
    raw: DecodedYaml,
    model: SpecificationDocument,
    origins: Vec<SchemaOrigin>,
    windows: Vec<WindowReference>,
}
impl PreparedDocument {
    pub fn schema(&self) -> &CapturedSchema {
        &self.schema
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &DecodedYaml {
        &self.raw
    }
    pub fn model(&self) -> &SpecificationDocument {
        &self.model
    }
    pub fn origins(&self) -> &[SchemaOrigin] {
        &self.origins
    }
    pub fn windows(&self) -> &[WindowReference] {
        &self.windows
    }
}
