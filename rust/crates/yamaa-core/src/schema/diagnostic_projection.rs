//! Resolve semantic findings only against their caller-held pass and schema arenas.
use super::{
    Document, DocumentKind as K, DocumentNode as N, SchemaContext as C, SchemaDiagnostic,
    SchemaStructure,
};
use std::collections::BTreeSet;

/// Malformed references are distinct from authored findings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SchemaDiagnosticError {
    #[error("schema diagnostic projection invalid context")]
    InvalidContext,
}

/// Borrowed semantic value. Integer spelling retains mathematical identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SchemaDiagnosticValue<'a> {
    Text(&'a str),
    Count(usize),
    Null,
    Boolean(bool),
    Integer(&'a str),
    Float(f64),
    TextList(&'a [String]),
}

/// Descriptor-value and layer-shape findings preserve their qualified vocabularies.
pub fn schema_diagnostic_kind_label(context: &C) -> Option<&'static str> {
    let (kind, layer) = match context {
        C::ValueKind(kind) => (*kind, false),
        C::LayerKind(kind) => (*kind, true),
        _ => return None,
    };
    Some(match kind {
        K::Null if layer => "NoneType",
        K::Null => "null",
        K::Boolean => "bool",
        K::Integer => "int",
        K::Float => "float",
        K::Text => "str",
        K::Sequence if layer => "list",
        K::Sequence => "sequence",
        K::Mapping if layer => "dict",
        K::Mapping => "mapping",
    })
}

fn resolve<'a>(
    schema: &'a SchemaStructure,
    document: Option<&'a Document>,
    value: &'a C,
) -> Result<SchemaDiagnosticValue<'a>, SchemaDiagnosticError> {
    use SchemaDiagnosticError::InvalidContext as Invalid;
    use SchemaDiagnosticValue as V;
    Ok(match value {
        C::Text(text) => V::Text(text),
        C::Boolean(value) => V::Boolean(*value),
        C::ValueKind(_) | C::LayerKind(_) => {
            V::Text(schema_diagnostic_kind_label(value).ok_or(Invalid)?)
        }
        C::Count(count) => V::Count(*count),
        C::Null => V::Null,
        C::InputValue(node) => match document.and_then(|d| d.nodes().get(*node)).ok_or(Invalid)? {
            N::Null => V::Null,
            N::Boolean(value) => V::Boolean(*value),
            N::Integer(text) => V::Integer(text),
            N::Float(value) => V::Float(*value),
            N::Text(text) => V::Text(text),
            N::Sequence(_) | N::Mapping(_) => return Err(Invalid),
        },
        C::DescriptorValues(id)
        | C::DescriptorPattern(id)
        | C::DescriptorMinimum(id)
        | C::DescriptorSize(id) => {
            let descriptor = &schema.descriptors().get(*id).ok_or(Invalid)?.descriptor;
            match value {
                C::DescriptorValues(_) => V::TextList(descriptor.permitted().ok_or(Invalid)?),
                C::DescriptorPattern(_) => V::Text(descriptor.pattern().ok_or(Invalid)?.0),
                C::DescriptorMinimum(_) => V::Integer(descriptor.minimum().ok_or(Invalid)?),
                C::DescriptorSize(_) => V::Integer(descriptor.size().ok_or(Invalid)?),
                _ => unreachable!(),
            }
        }
    })
}
/// Immutable report witness: all records are admitted before representation copies.
/// `document` must be the exact arena used by the failed pass, including defaults
/// from a descriptor's declaring module. Paths cannot establish that authority.
pub struct SchemaDiagnosticReport<'a> {
    schema: &'a SchemaStructure,
    document: Option<&'a Document>,
    findings: &'a [SchemaDiagnostic],
    source_context: &'a [(&'a str, &'a str)],
}
pub struct ResolvedSchemaDiagnostic<'a> {
    pub phase: &'static str,
    pub path: &'a str,
    pub condition: &'static str,
    pub requirement: Option<&'static str>,
    pub context: SchemaDiagnosticContext<'a>,
}
pub struct SchemaDiagnosticContext<'a> {
    schema: &'a SchemaStructure,
    document: Option<&'a Document>,
    finding: &'a SchemaDiagnostic,
    source_context: &'a [(&'a str, &'a str)],
    index: usize,
}
impl<'a> Iterator for SchemaDiagnosticContext<'a> {
    type Item = (&'a str, SchemaDiagnosticValue<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        let result = if let Some((name, value)) = self.finding.context.get(self.index) {
            Some((
                *name,
                resolve(self.schema, self.document, value)
                    .expect("immutable report admitted all references"),
            ))
        } else {
            self.source_context
                .get(self.index.checked_sub(self.finding.context.len())?)
                .map(|(name, value)| (*name, SchemaDiagnosticValue::Text(value)))
        };
        if result.is_some() {
            self.index += 1;
        }
        result
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.finding.context.len() + self.source_context.len() - self.index;
        (size, Some(size))
    }
}
impl ExactSizeIterator for SchemaDiagnosticContext<'_> {}
impl<'a> SchemaDiagnosticReport<'a> {
    pub fn schema(&self) -> &'a SchemaStructure {
        self.schema
    }
    pub fn document(&self) -> Option<&'a Document> {
        self.document
    }
    pub fn findings(&self) -> &'a [SchemaDiagnostic] {
        self.findings
    }
    pub fn source_context(&self) -> &'a [(&'a str, &'a str)] {
        self.source_context
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = ResolvedSchemaDiagnostic<'a>> + 'a {
        let schema = self.schema;
        let document = self.document;
        let source_context = self.source_context;
        self.findings
            .iter()
            .map(move |finding| ResolvedSchemaDiagnostic {
                phase: finding.definition().phase,
                path: &finding.path,
                condition: finding.definition().condition,
                requirement: finding.definition().requirement,
                context: SchemaDiagnosticContext {
                    schema,
                    document,
                    source_context,
                    finding,
                    index: 0,
                },
            })
    }
}

/// Preflight the whole ordered report without copying document, schema or context.
/// Duplicate context/provenance names refuse rather than replace authored fields.
pub fn admit_schema_diagnostics<'a>(
    schema: &'a SchemaStructure,
    document: Option<&'a Document>,
    findings: &'a [SchemaDiagnostic],
    source_context: &'a [(&'a str, &'a str)],
) -> Result<SchemaDiagnosticReport<'a>, SchemaDiagnosticError> {
    for finding in findings {
        let mut names = BTreeSet::new();
        for (name, value) in &finding.context {
            resolve(schema, document, value)?;
            if !names.insert(*name) {
                return Err(SchemaDiagnosticError::InvalidContext);
            }
        }
        for (name, _) in source_context {
            if !names.insert(*name) {
                return Err(SchemaDiagnosticError::InvalidContext);
            }
        }
    }
    Ok(SchemaDiagnosticReport {
        schema,
        document,
        findings: &findings[..findings.len().min(crate::limits::Limits::FINDINGS)],
        source_context,
    })
}
