//! Schema-directed planning dictionary references, without host YAML expansion.
use super::{
    scalar_diagnostic_label, validation::matches_member, Document, DocumentNode as N,
    NormalizationError, NormalizationResource, SchemaAliasKind, SchemaField, SchemaShape,
    SchemaStructure, TypeExpression, TypeNode, ValidationError,
};
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
#[derive(Clone, Copy)]
struct Member<'a> {
    expression: &'a TypeExpression,
    node: usize,
}
impl<'a> Member<'a> {
    fn text(self) -> &'a str {
        self.expression.node_text(self.node).expect("admitted type")
    }
    fn child(self, node: usize) -> Self {
        Self { node, ..self }
    }
}

struct Run<'a> {
    schema: &'a SchemaStructure,
    input: &'a Document,
    active: Vec<(usize, usize)>,
    references: Vec<MappingDictionaryReference>,
}
impl Run<'_> {
    fn step(&mut self, depth: usize) -> Result<(), NormalizationError> {
        let limit = crate::limits::Limits::SEMANTIC_DEPTH;
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        Ok(())
    }
    fn field(&mut self, value: usize, name: &str) -> Result<Option<usize>, NormalizationError> {
        Ok(self.input.field(value, name))
    }
    fn path(&mut self, parent: &str, suffix: &str) -> Result<String, NormalizationError> {
        Ok(format!("{parent}{suffix}"))
    }
    fn join(&mut self, parent: &str, name: &str) -> Result<String, NormalizationError> {
        let separator = if parent.is_empty() { "" } else { "." };
        self.path(parent, &format!("{separator}{name}"))
    }
    fn label(&mut self, node: usize) -> Result<String, NormalizationError> {
        let value = &self.input.nodes()[node];

        scalar_diagnostic_label(value).ok_or_else(|| ValidationError::InvalidDescriptor.into())
    }
    fn descriptor(
        &mut self,
        descriptor: usize,
        value: usize,
        logical: &str,
        depth: usize,
    ) -> Result<(), NormalizationError> {
        self.step(depth)?;
        let located = &self.schema.descriptors()[descriptor];
        let module = &self.schema.modules()[located.module].document;
        let type_node = module
            .field(located.source_node, "type")
            .ok_or(ValidationError::InvalidDescriptor)?;
        let single = matches!(module.nodes()[type_node], N::Text(_));
        let members = located.descriptor.members();
        for expression in members {
            let member = Member {
                expression,
                node: expression.root(),
            };
            if !single {
                if let Some((alias, _)) = self.schema.alias_named(member.text()) {
                    if self.active.contains(&(value, alias)) {
                        continue;
                    }
                }
                if !matches_member(
                    self.schema,
                    self.input,
                    (expression, expression.root()),
                    value,
                    true,
                    super::validation::NullPolicy::Absent,
                    &self.active,
                )? {
                    continue;
                }
            }
            return self.walk(member, value, logical, depth + 1);
        }
        Ok(())
    }
    fn fields(
        &mut self,
        fields: &[SchemaField],
        value: usize,
        logical: &str,
        depth: usize,
    ) -> Result<(), NormalizationError> {
        self.step(depth)?;
        if !matches!(self.input.nodes()[value], N::Mapping(_)) {
            return Ok(());
        }
        for field in fields {
            if let Some(child) = self.field(value, &field.name)? {
                let path = self.join(logical, &field.name)?;
                self.descriptor(field.descriptor, child, &path, depth + 1)?;
            }
        }
        Ok(())
    }
    fn dictionary(&mut self, payload: usize, path: &str) -> Result<(), NormalizationError> {
        if let Some(node) = self.field(payload, "dict")? {
            if let N::Text(written) = &self.input.nodes()[node] {
                let path = self.join(path, "dict")?;
                self.references.push(MappingDictionaryReference {
                    node,
                    path,
                    written: written.clone(),
                });
            }
        }
        Ok(())
    }
    fn walk(
        &mut self,
        member: Member<'_>,
        value: usize,
        logical: &str,
        depth: usize,
    ) -> Result<(), NormalizationError> {
        self.step(depth)?;
        let kind = member.text();
        match &member.expression.nodes()[member.node] {
            TypeNode::List(inner) => {
                if let N::Sequence(items) = &self.input.nodes()[value] {
                    let child = member.child(*inner);
                    let identity = match child.text() {
                        "column_class" | "column_entry" => Some("name"),
                        "row_class" | "intermediate_class" => Some("id"),
                        _ => None,
                    };
                    for (index, &item) in items.iter().enumerate() {
                        let path = if let Some(identity) =
                            identity.filter(|_| matches!(self.input.nodes()[item], N::Mapping(_)))
                        {
                            let label = match self.field(item, identity)? {
                                Some(id) => self.label(id)?,
                                None => format!("{index}"),
                            };
                            self.join(logical, &label)?
                        } else {
                            self.path(logical, &format!("[{index}]"))?
                        };
                        self.walk(child, item, &path, depth + 1)?;
                    }
                }
            }
            TypeNode::Dictionary { value: inner, .. } => {
                if let N::Mapping(items) = &self.input.nodes()[value] {
                    for &(key, item) in items {
                        let label = self.label(key)?;
                        let path = self.join(logical, &label)?;
                        self.walk(member.child(*inner), item, &path, depth + 1)?;
                    }
                }
            }
            TypeNode::Name(_) => {
                if let Some(class) = self.schema.class_named(kind) {
                    if matches!(
                        kind,
                        "handled_expression_class" | "intermediate_expression_class"
                    ) {
                        if let Some(child) = self.field(value, "value")? {
                            let field = class
                                .fields
                                .iter()
                                .find(|field| field.name == "value")
                                .ok_or(ValidationError::InvalidDescriptor)?;
                            return self.descriptor(field.descriptor, child, logical, depth + 1);
                        }
                    }
                    return self.fields(&class.fields, value, logical, depth + 1);
                }
                if let Some((index, alias)) = self.schema.alias_named(kind) {
                    if self.active.contains(&(value, index)) {
                        return Ok(());
                    }
                    match alias.kind {
                        SchemaAliasKind::Descriptor(id) => {
                            self.active.push((value, index));
                            let result = self.descriptor(id, value, logical, depth + 1);
                            self.active.pop();
                            result?;
                        }
                        SchemaAliasKind::Registry(id) => {
                            if let N::Mapping(items) = &self.input.nodes()[value] {
                                if items.len() == 1 {
                                    let (key, payload) = items[0];
                                    if let N::Text(name) = &self.input.nodes()[key] {
                                        let registry = &self.schema.registries()[id];
                                        if let Some(entry) = registry
                                            .entries
                                            .iter()
                                            .find(|entry| entry.name == *name)
                                        {
                                            let path = self.join(logical, name)?;
                                            if registry.name == "expressions" && name == "mapping" {
                                                self.dictionary(payload, &path)?;
                                            }
                                            match &entry.shape {
                                                SchemaShape::Class(fields) => {
                                                    self.fields(fields, payload, &path, depth + 1)?
                                                }
                                                SchemaShape::Descriptor(id) => {
                                                    self.descriptor(*id, payload, &path, depth + 1)?
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
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
        if !matches!(input.nodes()[input.root()], N::Mapping(_)) {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut run = Run {
            schema: self,
            input,
            active: Vec::new(),
            references: Vec::new(),
        };
        run.fields(&self.root_class().fields, input.root(), "", 0)?;
        Ok(run.references)
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
        let limit = crate::limits::Limits::SEMANTIC_DEPTH;
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
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
