//! Schema-directed planning dictionary references, without host YAML expansion.
use super::{
    walk::{join, Visit},
    Document, DocumentNode as N, NormalizationError, SchemaStructure, ValidationError,
};
use crate::expression_table::Verb;
use std::collections::BTreeMap;

/// An admitted expression dictionary path and its exact normalized occurrence.
/// Construction is private; arbitrary metadata keys cannot invent a resource use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingDictionaryReference {
    node: usize,
    path: String,
    written: String,
}
impl MappingDictionaryReference {
    pub fn node(&self) -> usize {
        self.node
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn written(&self) -> &str {
        &self.written
    }
}
/// Collects the `dict` of each expression `mapping` entry the walk reaches.
struct Dictionaries<'a> {
    input: &'a Document,
    references: Vec<MappingDictionaryReference>,
}
impl Visit for Dictionaries<'_> {
    /// Literal values, handlers and unrelated metadata of an expression class stay opaque.
    fn value_only(&self, class: &str) -> bool {
        matches!(
            class,
            "handled_expression_class" | "intermediate_expression_class"
        )
    }
    fn entry(
        &mut self,
        registry: &str,
        name: &str,
        payload: usize,
        logical: &str,
    ) -> Result<(), NormalizationError> {
        if registry != "expressions" || Verb::from_name(name) != Some(Verb::Mapping) {
            return Ok(());
        }
        if let Some(node) = self.input.field(payload, "dict") {
            if let N::Text(written) = &self.input.nodes()[node] {
                self.references.push(MappingDictionaryReference {
                    node,
                    path: join(logical, "dict"),
                    written: written.clone(),
                });
            }
        }
        Ok(())
    }
}
impl SchemaStructure {
    /// Collect surviving expression dictionaries in schema traversal order.
    /// Literal values, handlers and unrelated metadata remain opaque values.
    pub fn mapping_dictionary_references(
        &self,
        input: &Document,
    ) -> Result<Vec<MappingDictionaryReference>, NormalizationError> {
        let mut dictionaries = Dictionaries {
            input,
            references: Vec::new(),
        };
        self.walk_document(input, &mut dictionaries)?;
        Ok(dictionaries.references)
    }
}

/// A separately captured dictionary admitted against the inline schema contract.
/// Its original raw arena remains the engine capture catalog's diagnostic source.
#[derive(Clone, Debug)]
pub struct AdmittedMappingDictionary {
    reference: MappingDictionaryReference,
    document: Document,
}
impl AdmittedMappingDictionary {
    pub fn reference(&self) -> &MappingDictionaryReference {
        &self.reference
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
}
impl SchemaStructure {
    /// Validate at the original expression use site before normalizing a held dictionary.
    /// Wrong roots, non-string keys and nonliteral values use ordinary schema findings.
    pub fn admit_mapping_dictionary(
        &self,
        reference: MappingDictionaryReference,
        input: &Document,
    ) -> Result<AdmittedMappingDictionary, NormalizationError> {
        let types = self.parse_query_types(&["dict[str, literal_value]".into()])?;
        let findings = self.validate_types(&types, input, input.root(), reference.path(), false)?;
        if !findings.is_empty() {
            return Err(NormalizationError::Invalid(findings));
        }
        let normalized = self.normalize_types(&types, input, input.root(), false)?;
        Ok(AdmittedMappingDictionary {
            reference,
            document: normalized.document,
        })
    }

    /// Derive a compiler-only copy of the resolved specification `input` from
    /// admitted held content. The caller retains its original document and
    /// origins; this result grants no source IO and must not be exposed as
    /// authored or resolved specification provenance. Beside the derived
    /// document, each of its nodes names the node of `input` it copies, or,
    /// for a node of a dictionary, the reference that read it.
    pub fn materialize_mapping_dictionary_document(
        &self,
        input: &Document,
        dictionaries: &[AdmittedMappingDictionary],
    ) -> Result<(Document, Vec<usize>), NormalizationError> {
        let references = self.mapping_dictionary_references(input)?;
        if references.len() != dictionaries.len() {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut replacements = BTreeMap::new();
        for dictionary in dictionaries {
            if !references
                .iter()
                .any(|reference| reference == dictionary.reference())
                || replacements
                    .insert(dictionary.reference.node, &dictionary.document)
                    .is_some()
            {
                return Err(ValidationError::InvalidDescriptor.into());
            }
        }
        let mut run = Materialize {
            replacements,
            nodes: Vec::new(),
            sources: Vec::new(),
        };
        let root = run.copy(input, input.root(), None, 0)?;
        let document = Document::new(run.nodes, root).map_err(NormalizationError::Document)?;
        Ok((document, run.sources))
    }
}
struct Materialize<'a> {
    replacements: BTreeMap<usize, &'a Document>,
    nodes: Vec<N>,
    /// Parallel to `nodes`: the input node each copies, or the reference
    /// node whose dictionary holds it.
    sources: Vec<usize>,
}
impl Materialize<'_> {
    /// Copy `value` of `input`, the specification when `reference` is none
    /// and otherwise the dictionary that reference node reads.
    fn copy(
        &mut self,
        input: &Document,
        value: usize,
        reference: Option<usize>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        NormalizationError::check_depth(depth)?;
        if reference.is_none() {
            if let Some(dictionary) = self.replacements.get(&value).copied() {
                return self.copy(dictionary, dictionary.root(), Some(value), depth + 1);
            }
        }
        let node = match &input.nodes()[value] {
            N::Text(_) | N::Integer(_) => input.nodes()[value].clone(),
            N::Sequence(items) => {
                let mut copied = Vec::with_capacity(items.len());
                for &item in items {
                    copied.push(self.copy(input, item, reference, depth + 1)?);
                }
                N::Sequence(copied)
            }
            N::Mapping(items) => {
                let mut copied = Vec::with_capacity(items.len());
                for &(key, item) in items {
                    copied.push((
                        self.copy(input, key, reference, depth + 1)?,
                        self.copy(input, item, reference, depth + 1)?,
                    ));
                }
                N::Mapping(copied)
            }
            node => node.clone(),
        };
        let id = self.nodes.len();
        self.nodes.push(node);
        self.sources.push(reference.unwrap_or(value));
        Ok(id)
    }
}
