//! Schema-directed planning dictionary references, without host YAML expansion.
use super::{
    scalar_diagnostic_label, validation::matches_member, Document, DocumentNode as N,
    NormalizationBudget, NormalizationError, NormalizationResource, SchemaAliasKind, SchemaField,
    SchemaShape, SchemaStructure, SpecificationDocument, TypeExpression, TypeNode, ValidationError,
};
use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

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

struct Run<'a, 'b> {
    schema: &'a SchemaStructure,
    input: &'a Document,
    budget: &'b mut NormalizationBudget,
    active: Vec<(usize, usize)>,
    references: Vec<MappingDictionaryReference>,
}
impl Run<'_, '_> {
    fn step(&mut self, depth: usize) -> Result<(), NormalizationError> {
        let limit = self.budget.limits.depth.min(128);
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        self.budget.work(1)
    }
    fn field(&mut self, value: usize, name: &str) -> Result<Option<usize>, NormalizationError> {
        let length = self.input.nodes()[value].length().unwrap_or(0);
        self.budget
            .work(length.saturating_mul(name.len().saturating_add(1)))?;
        Ok(self.input.field(value, name))
    }
    fn path(&mut self, parent: &str, suffix: &str) -> Result<String, NormalizationError> {
        self.budget
            .validation
            .text(parent.len().saturating_add(suffix.len()))?;
        Ok(format!("{parent}{suffix}"))
    }
    fn join(&mut self, parent: &str, name: &str) -> Result<String, NormalizationError> {
        let separator = if parent.is_empty() { "" } else { "." };
        self.budget.validation.text(name.len().saturating_add(1))?;
        self.path(parent, &format!("{separator}{name}"))
    }
    fn label(&mut self, node: usize) -> Result<String, NormalizationError> {
        let value = &self.input.nodes()[node];
        let maximum = match value {
            N::Text(text) | N::Integer(text) => text.len(),
            N::Boolean(_) => 5,
            N::Null => 4,
            N::Float(_) => 512,
            _ => return Err(ValidationError::InvalidDescriptor.into()),
        };
        self.budget.work(maximum)?;
        self.budget.validation.text(maximum)?;
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
            self.budget.work(1)?;
            let member = Member {
                expression,
                node: expression.root(),
            };
            if !single {
                self.budget.work(self.active.len())?;
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
                    &self.active,
                    &mut self.budget.validation,
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
            self.budget.work(1)?;
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
                self.budget.validation.text(written.len())?;
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
                        "column_class" => Some("name"),
                        "row_class" | "intermediate_class" => Some("id"),
                        _ => None,
                    };
                    for (index, &item) in items.iter().enumerate() {
                        self.budget.work(1)?;
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
                        self.budget.work(1)?;
                        let label = self.label(key)?;
                        let path = self.join(logical, &label)?;
                        self.walk(member.child(*inner), item, &path, depth + 1)?;
                    }
                }
            }
            TypeNode::Name(_) => {
                if let Some(class) = self.schema.class_named(kind) {
                    if kind == "handled_expression_class" {
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
                    self.budget.work(self.active.len())?;
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
                                        self.budget.work(
                                            registry
                                                .entries
                                                .len()
                                                .saturating_mul(name.len().saturating_add(1)),
                                        )?;
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
        budget: &mut NormalizationBudget,
    ) -> Result<Vec<MappingDictionaryReference>, NormalizationError> {
        if !matches!(input.nodes()[input.root()], N::Mapping(_)) {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut run = Run {
            schema: self,
            input,
            budget,
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
        budget: &mut NormalizationBudget,
    ) -> Result<AdmittedMappingDictionary, NormalizationError> {
        let types = self.parse_query_types(
            &["dict[str, literal_value]".into()],
            budget.validation_scope(),
        )?;
        let findings = self.validate_types(
            &types,
            input,
            input.root(),
            reference.path(),
            false,
            budget.validation_scope(),
        )?;
        if !findings.is_empty() {
            return Err(NormalizationError::Invalid(findings));
        }
        let normalized = self.normalize_types(&types, input, input.root(), false, budget)?;
        Ok(AdmittedMappingDictionary {
            reference,
            document: normalized.document,
        })
    }

    /// Derive a compiler-only model from admitted held content. The caller retains
    /// its original normalized model and origins; this result grants no source IO
    /// and must not be exposed as authored or resolved specification provenance.
    pub fn materialize_mapping_dictionaries(
        &self,
        input: &SpecificationDocument,
        dictionaries: &[AdmittedMappingDictionary],
        budget: &mut NormalizationBudget,
    ) -> Result<SpecificationDocument, NormalizationError> {
        let references = self.mapping_dictionary_references(input.document(), budget)?;
        if references.len() != dictionaries.len() {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut replacements = BTreeMap::new();
        for dictionary in dictionaries {
            budget.work(references.len())?;
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
            budget,
        };
        let original = input.document();
        let root = run.copy(original, original.root(), true, 0)?;
        let document = Document::new(run.nodes, root, run.budget.limits.storage)
            .map_err(NormalizationError::Document)?;
        SpecificationDocument::admit(document, run.budget.validation_scope())?
            .map_err(NormalizationError::Invalid)
    }
}
struct Materialize<'a, 'b> {
    replacements: BTreeMap<usize, &'a Document>,
    nodes: Vec<N>,
    budget: &'b mut NormalizationBudget,
}
impl Materialize<'_, '_> {
    fn copy(
        &mut self,
        input: &Document,
        value: usize,
        replace: bool,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        let limit = self.budget.limits.depth.min(128);
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        self.budget.work(1)?;
        if replace {
            if let Some(dictionary) = self.replacements.get(&value).copied() {
                return self.copy(dictionary, dictionary.root(), false, depth + 1);
            }
        }
        let node = match &input.nodes()[value] {
            N::Text(text) | N::Integer(text) => {
                self.budget.work(text.len())?;
                self.budget.reserve(text.len(), 0)?;
                input.nodes()[value].clone()
            }
            N::Sequence(items) => {
                self.budget.reserve(0, items.len())?;
                let mut copied = Vec::with_capacity(items.len());
                for &item in items {
                    copied.push(self.copy(input, item, replace, depth + 1)?);
                }
                N::Sequence(copied)
            }
            N::Mapping(items) => {
                self.budget.reserve(0, items.len().saturating_mul(2))?;
                let mut copied = Vec::with_capacity(items.len());
                for &(key, item) in items {
                    copied.push((
                        self.copy(input, key, replace, depth + 1)?,
                        self.copy(input, item, replace, depth + 1)?,
                    ));
                }
                N::Mapping(copied)
            }
            node => {
                self.budget.reserve(0, 0)?;
                node.clone()
            }
        };
        let id = self.nodes.len();
        self.nodes.push(node);
        Ok(id)
    }
}
