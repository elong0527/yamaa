//! Live structural root and versionless class preparation over captured metadata.
//! The codec owns physical source limits and exact occurrence-location authority.
use crate::project_preparation::Source;
use crate::schema::{
    Document, NormalizationError, NormalizedDocument, SchemaDiagnostic, SchemaStructure,
    ValidationError,
};

/// A physical codec decodes this exact held source once. Its locations must
/// address the returned arena; equal strings cannot establish replacement authority.
pub trait StructuralDecodePort {
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
#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("structural preparation unknown class: {0:?}")]
    UnknownClass(String),
    #[error("structural preparation versioned root required")]
    VersionedRootRequired,
    #[error("structural preparation decode: {0:?}")]
    Decode(E),
    /// Type-query failure is distinct from normalization policy failure.
    #[error("structural preparation validation: {0:?}")]
    Validation(ValidationError),
    #[error("structural preparation normalize: {0:?}")]
    Normalize(NormalizationError),
    #[error("structural preparation findings: {0:?}")]
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
/// One successful decode precedes the existing default/type/value checkpoints;
/// a root that declares the 1.x header is refused first (SCHEMA-2).
/// No source is captured, replayed, queried as a model or granted host authority.
pub fn prepare<P: StructuralDecodePort>(
    schema: &SchemaStructure,
    source: Source,
    selection: Selection<'_>,
    decoder: &mut P,
) -> Result<Prepared<P::Locations>, Error<P::Error>> {
    let class_name = match selection {
        Selection::StructuralRoot => &schema.root_class().name,
        Selection::NamedClass(name) => {
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
    // A root that declares the 1.x header is refused before the 0.5 closure
    // reads it (SCHEMA-2, #3030 D76).
    if let Some(fault) = crate::language::refusal(&document) {
        return Err(Error::Findings(Box::new(Findings {
            source,
            document,
            findings: vec![fault.finding()],
        })));
    }
    let normalized = match selection {
        Selection::StructuralRoot => schema.normalize_document(&document),
        Selection::NamedClass(_) => {
            let types = schema
                .parse_query_types(&[class_name.into()])
                .map_err(Error::Validation)?;
            schema.normalize_types(&types, &document, document.root(), false)
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
