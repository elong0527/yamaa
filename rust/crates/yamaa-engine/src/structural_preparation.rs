//! Live structural root and versionless class preparation over captured metadata.
//! The codec owns physical source limits and exact occurrence-location authority.
use crate::project_preparation::Source;
use alloc::{boxed::Box, string::String, vec::Vec};
use yamaa_core::schema::{
    Document, NormalizationBudget, NormalizationError, NormalizationLimits, NormalizedDocument,
    SchemaDiagnostic, SchemaStructure, ValidationError,
};

/// A physical codec decodes this exact held source once. Its locations must
/// address the returned arena; equal strings cannot establish replacement authority.
pub trait DecodePort {
    type Error;
    type Locations;
    fn decode(&mut self, source: &Source) -> Result<Decoded<Self::Locations>, Self::Error>;
}
pub struct Decoded<L> {
    pub document: Document,
    pub locations: L,
}
#[derive(Clone, Copy, Debug)]
pub enum Selection<'a> {
    StructuralRoot,
    NamedClass(&'a str),
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Existing identity-byte policy applies only to explicit named selection.
    pub class_name_bytes: usize,
    /// Fresh per-document scope, shared by type-query binding and normalization.
    pub normalization: NormalizationLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            class_name_bytes: 65_536,
            normalization: Default::default(),
        }
    }
}
#[derive(Debug)]
pub enum Error<E> {
    ClassNameLimit,
    UnknownClass(String),
    VersionedRootRequired,
    Decode(E),
    /// Type-query failure is distinct from normalization policy failure.
    Validation(ValidationError),
    Normalize(NormalizationError),
    Findings(Box<Findings>),
}

/// Complete semantic findings retain their original source and raw pass arena.
#[derive(Debug)]
pub struct Findings {
    source: Source,
    document: Document,
    findings: Vec<SchemaDiagnostic>,
}
impl Findings {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn findings(&self) -> &[SchemaDiagnostic] {
        &self.findings
    }
    pub fn into_parts(self) -> (Source, Document, Vec<SchemaDiagnostic>) {
        (self.source, self.document, self.findings)
    }
}

/// Structural admission grants no specification, data or execution authority.
/// The caller must retain the exact admitted schema for Default origin references.
#[derive(Debug)]
pub struct Prepared<L> {
    source: Source,
    raw: Document,
    locations: L,
    class_name: String,
    normalized: NormalizedDocument,
}
impl<L> Prepared<L> {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &Document {
        &self.raw
    }
    pub fn locations(&self) -> &L {
        &self.locations
    }
    pub fn class_name(&self) -> &str {
        &self.class_name
    }
    pub fn normalized(&self) -> &NormalizedDocument {
        &self.normalized
    }
    pub fn into_parts(self) -> (Source, Document, L, String, NormalizedDocument) {
        (
            self.source,
            self.raw,
            self.locations,
            self.class_name,
            self.normalized,
        )
    }
}

/// Named text, closed-class lookup and any declared schema_version guard precede
/// physical decoding. Structural root has no separate named-selection guard.
/// One successful decode precedes the existing default/type/value checkpoints.
/// No source is captured, replayed, queried as a model or granted host authority.
pub fn prepare<P: DecodePort>(
    schema: &SchemaStructure,
    source: Source,
    selection: Selection<'_>,
    limits: Limits,
    decoder: &mut P,
) -> Result<Prepared<P::Locations>, Error<P::Error>> {
    let class_name = match selection {
        Selection::StructuralRoot => &schema.root_class().name,
        Selection::NamedClass(name) => {
            if name.len() > limits.class_name_bytes {
                return Err(Error::ClassNameLimit);
            }
            let class = schema
                .class_named(name)
                .ok_or_else(|| Error::UnknownClass(name.into()))?;
            if class
                .fields
                .iter()
                .any(|field| field.name == "schema_version")
            {
                return Err(Error::VersionedRootRequired);
            }
            name
        }
    };
    let Decoded {
        document,
        locations,
    } = decoder.decode(&source).map_err(Error::Decode)?;
    // Preserve the existing fresh per-document policy. Project metadata's
    // cumulative source/node/semantic quotas remain the outer application's scope.
    let mut budget = NormalizationBudget::new(limits.normalization);
    let normalized = match selection {
        Selection::StructuralRoot => schema.normalize_document(&document, &mut budget),
        Selection::NamedClass(_) => {
            let types = schema
                .parse_query_types(&[class_name.into()], budget.validation_scope())
                .map_err(Error::Validation)?;
            schema.normalize_types(&types, &document, document.root(), false, &mut budget)
        }
    };
    let normalized = match normalized {
        Ok(normalized) => normalized,
        Err(NormalizationError::Invalid(findings)) => {
            return Err(Error::Findings(Box::new(Findings {
                source,
                document,
                findings,
            })));
        }
        Err(error) => return Err(Error::Normalize(error)),
    };
    Ok(Prepared {
        source,
        raw: document,
        locations,
        class_name: class_name.into(),
        normalized,
    })
}
