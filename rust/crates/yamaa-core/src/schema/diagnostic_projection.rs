//! Resolve semantic findings only against their caller-held pass and schema arenas.
use super::{
    Document, DocumentKind as K, DocumentNode as N, SchemaContext as C, SchemaDiagnostic,
    SchemaStructure,
};
use alloc::{collections::BTreeSet, string::String};

/// Conservative escaped JSON admission shared across ordered captured reports.
pub const MAX_SCHEMA_DIAGNOSTIC_BYTES: usize = 16_777_216;

/// Malformed references and resource refusal are distinct from authored findings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaDiagnosticError {
    Limit,
    InvalidContext,
}

/// One projection attempt owns accounting; failed prefixes are never refunded.
pub struct SchemaDiagnosticBudget {
    used: usize,
    maximum: usize,
}
impl SchemaDiagnosticBudget {
    pub fn new() -> Self {
        Self::with_limit(MAX_SCHEMA_DIAGNOSTIC_BYTES)
    }
    pub fn with_limit(maximum: usize) -> Self {
        Self { used: 128, maximum }
    }
    pub fn bytes_used(&self) -> usize {
        self.used
    }
    /// Charge representation overhead without copying any caller text.
    pub fn charge(&mut self, bytes: usize) -> Result<(), SchemaDiagnosticError> {
        self.used = self
            .used
            .checked_add(bytes)
            .filter(|&n| n <= self.maximum)
            .ok_or(SchemaDiagnosticError::Limit)?;
        Ok(())
    }
    /// Each UTF-8 byte can expand to at most six JSON bytes.
    pub fn text(&mut self, text: &str) -> Result<(), SchemaDiagnosticError> {
        self.charge(
            text.len()
                .checked_mul(6)
                .and_then(|n| n.checked_add(2))
                .ok_or(SchemaDiagnosticError::Limit)?,
        )
    }
}
impl Default for SchemaDiagnosticBudget {
    fn default() -> Self {
        Self::new()
    }
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
fn admit_value(
    value: SchemaDiagnosticValue<'_>,
    budget: &mut SchemaDiagnosticBudget,
) -> Result<(), SchemaDiagnosticError> {
    use SchemaDiagnosticValue as V;
    match value {
        V::Text(text) => budget.text(text),
        V::Count(_) => budget.charge(20),
        V::Null => budget.charge(4),
        V::Boolean(_) => budget.charge(5),
        V::Integer(text) => budget.charge(text.len()),
        V::Float(_) => budget.charge(32),
        V::TextList(values) => {
            budget.charge(
                values
                    .len()
                    .checked_add(2)
                    .ok_or(SchemaDiagnosticError::Limit)?,
            )?;
            for text in values {
                budget.text(text)?;
            }
            Ok(())
        }
    }
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
                phase: "validation",
                path: &finding.path,
                condition: finding.condition,
                requirement: finding.requirement,
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
    budget: &mut SchemaDiagnosticBudget,
) -> Result<SchemaDiagnosticReport<'a>, SchemaDiagnosticError> {
    for finding in findings {
        budget.charge(128)?;
        budget.text(&finding.path)?;
        budget.text(finding.condition)?;
        if let Some(requirement) = finding.requirement {
            budget.text(requirement)?;
        }
        let mut names = BTreeSet::new();
        for (name, value) in &finding.context {
            budget.text(name)?;
            budget.charge(2)?;
            admit_value(resolve(schema, document, value)?, budget)?;
            if !names.insert(*name) {
                return Err(SchemaDiagnosticError::InvalidContext);
            }
        }
        for (name, value) in source_context {
            budget.text(name)?;
            budget.text(value)?;
            if !names.insert(*name) {
                return Err(SchemaDiagnosticError::InvalidContext);
            }
        }
    }
    Ok(SchemaDiagnosticReport {
        schema,
        document,
        findings,
        source_context,
    })
}
