//! Live structural root and versionless class preparation over captured
//! metadata. The decoder port decodes the held source once, keeping its own
//! physical limits and where each node was written.
use crate::decoder::Decoder;
use crate::project_preparation::Source;
use crate::schema::{
    Document, NormalizationError, NormalizedDocument, SchemaDiagnostic, SchemaStructure,
    ValidationError,
};
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
/// The caller must retain the exact admitted schema for Default origin
/// references. `R` is the source as the decoder decoded it.
#[derive(Debug)]
pub struct Prepared<R> {
    source: Source,
    raw: R,
    class_name: String,
    normalized: NormalizedDocument,
}
impl<R> Prepared<R> {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &R {
        &self.raw
    }
    pub fn class_name(&self) -> &str {
        &self.class_name
    }
    pub fn normalized(&self) -> &NormalizedDocument {
        &self.normalized
    }
    pub fn into_parts(self) -> (Source, R, String, NormalizedDocument) {
        (self.source, self.raw, self.class_name, self.normalized)
    }
}

/// Named text, closed-class lookup and any declared schema_version guard precede
/// physical decoding. Structural root has no separate named-selection guard.
/// One successful decode precedes the existing default/type/value checkpoints;
/// a root that declares the 1.x header is refused first (SCHEMA-2).
/// No source is captured, replayed, queried as a model or granted host authority.
pub fn prepare<D: Decoder>(
    schema: &SchemaStructure,
    source: Source,
    selection: Selection<'_>,
    decoder: &mut D,
) -> Result<Prepared<D::Raw>, Error<D::DocumentError>> {
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
    let raw = decoder
        .raw(&source.identity, &source.bytes)
        .map_err(Error::Decode)?;
    let document = D::raw_document(&raw);
    // The 0.5 closure reads every root it is given under the rules at the
    // `yamaa-v0.5.0` tag (#3030 D75): a caller routes a root that declares the
    // 1.x header to the 1.0 closures first.
    let normalized = match selection {
        Selection::StructuralRoot => schema.normalize_document(document),
        Selection::NamedClass(_) => {
            let types = schema
                .parse_query_types(&[class_name.into()])
                .map_err(Error::Validation)?;
            schema.normalize_types(&types, document, document.root(), false)
        }
    };
    let normalized = match normalized {
        Ok(normalized) => normalized,
        Err(NormalizationError::Invalid(findings)) => {
            return Err(Error::Findings(Box::new(Findings {
                source,
                document: document.clone(),
                findings,
            })));
        }
        Err(error) => return Err(Error::Normalize(error)),
    };
    Ok(Prepared {
        source,
        raw,
        class_name: class_name.into(),
        normalized,
    })
}
