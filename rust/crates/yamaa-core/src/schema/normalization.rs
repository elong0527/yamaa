//! Validated schema normalization with request-wide storage and traversal policies.

use super::{
    validation::matches_member, DefaultDiagnostics, Document, DocumentError, DocumentLimits,
    DocumentNode as N, SchemaAliasKind, SchemaDiagnostic, SchemaField, SchemaShape,
    SchemaStructure, TypeExpression, TypeNode, ValidationBudget, ValidationError, ValidationLimits,
};
use alloc::{vec, vec::Vec};

/// Original decoded bytes remain in the caller's input or admitted schema module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaSource {
    Input,
    Default { module: usize, descriptor: usize },
}

/// Every output occurrence retains its source, including generated shorthand containers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaOrigin {
    pub source: SchemaSource,
    pub node: usize,
    pub generated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedDocument {
    pub document: Document,
    /// Parallel to document.nodes(); no content hashes or host object identities.
    pub origins: Vec<SchemaOrigin>,
}

#[derive(Clone, Copy, Debug)]
pub struct NormalizationLimits {
    pub validation: ValidationLimits,
    /// Cumulative storage includes temporary derivation shorthand documents.
    pub storage: DocumentLimits,
    /// Bounds interpretation independently of the resulting document depth.
    pub depth: usize,
}
impl Default for NormalizationLimits {
    fn default() -> Self {
        Self {
            validation: ValidationLimits::default(),
            storage: DocumentLimits::default(),
            depth: 128,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalizationResource {
    Nodes,
    TextBytes,
    Edges,
    Depth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NormalizationError {
    Validation(ValidationError),
    Invalid(Vec<SchemaDiagnostic>),
    Defaults(Vec<DefaultDiagnostics>),
    Limit {
        resource: NormalizationResource,
        limit: usize,
    },
    Document(DocumentError),
}
impl From<ValidationError> for NormalizationError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error)
    }
}

/// Reuse across related fragment/document operations; failed attempts keep their charges.
pub struct NormalizationBudget {
    limits: NormalizationLimits,
    validation: ValidationBudget,
    nodes: usize,
    bytes: usize,
    edges: usize,
}
impl NormalizationBudget {
    pub fn new(limits: NormalizationLimits) -> Self {
        Self {
            validation: ValidationBudget::new(limits.validation),
            limits,
            nodes: 0,
            bytes: 0,
            edges: 0,
        }
    }
    pub fn storage_used(&self) -> (usize, usize, usize) {
        (self.nodes, self.bytes, self.edges)
    }
    pub fn work_used(&self) -> usize {
        self.validation.work_used()
    }
    /// Mixed validation/normalization batches retain one request's validation accounting.
    pub fn validation_scope(&mut self) -> &mut ValidationBudget {
        &mut self.validation
    }
    fn work(&mut self, amount: usize) -> Result<(), NormalizationError> {
        self.validation.work(amount).map_err(Into::into)
    }
    fn charge(
        used: &mut usize,
        amount: usize,
        limit: usize,
        resource: NormalizationResource,
    ) -> Result<(), NormalizationError> {
        *used = used
            .checked_add(amount)
            .filter(|n| *n <= limit)
            .ok_or(NormalizationError::Limit { resource, limit })?;
        Ok(())
    }
    fn reserve(&mut self, bytes: usize, edges: usize) -> Result<(), NormalizationError> {
        Self::charge(
            &mut self.nodes,
            1,
            self.limits.storage.nodes,
            NormalizationResource::Nodes,
        )?;
        Self::charge(
            &mut self.bytes,
            bytes,
            self.limits.storage.text_bytes,
            NormalizationResource::TextBytes,
        )?;
        self.edges(edges)
    }
    fn edges(&mut self, count: usize) -> Result<(), NormalizationError> {
        Self::charge(
            &mut self.edges,
            count,
            self.limits.storage.edges,
            NormalizationResource::Edges,
        )
    }
}

#[derive(Clone, Copy)]
enum Provenance {
    Direct(SchemaSource),
    Generated(SchemaOrigin),
}
#[derive(Clone, Copy)]
struct Site<'a> {
    input: &'a Document,
    value: usize,
    provenance: Provenance,
    scope: usize,
    depth: usize,
    fragment: bool,
}
impl Site<'_> {
    fn origin(self) -> SchemaOrigin {
        match self.provenance {
            Provenance::Direct(source) => SchemaOrigin {
                source,
                node: self.value,
                generated: false,
            },
            Provenance::Generated(origin) => SchemaOrigin {
                generated: true,
                ..origin
            },
        }
    }
    fn child(self, value: usize) -> Self {
        Self {
            value,
            depth: self.depth + 1,
            ..self
        }
    }
    fn deeper(self) -> Self {
        self.child(self.value)
    }
}

#[derive(Clone, Copy)]
struct Member<'a> {
    expression: &'a TypeExpression,
    node: usize,
}
impl<'a> Member<'a> {
    fn root(expression: &'a TypeExpression) -> Self {
        Self {
            expression,
            node: expression.root(),
        }
    }
    fn text(self) -> &'a str {
        self.expression
            .node_text(self.node)
            .expect("admitted type node")
    }
    fn child(self, node: usize) -> Self {
        Self { node, ..self }
    }
}

struct Run<'a, 'b> {
    schema: &'a SchemaStructure,
    budget: &'b mut NormalizationBudget,
    nodes: Vec<N>,
    origins: Vec<SchemaOrigin>,
    active: Vec<(usize, usize, usize)>,
    next_scope: usize,
}
impl Run<'_, '_> {
    fn step(&mut self, site: Site<'_>) -> Result<(), NormalizationError> {
        let limit = self.budget.limits.depth.min(128);
        if site.depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        self.budget.work(1)
    }
    fn emit(&mut self, node: N, origin: SchemaOrigin) -> usize {
        let id = self.nodes.len();
        self.nodes.push(node);
        self.origins.push(origin);
        id
    }
    fn scope(&mut self) -> Result<usize, NormalizationError> {
        self.budget.work(1)?;
        self.next_scope = self
            .next_scope
            .checked_add(1)
            .ok_or(NormalizationError::Limit {
                resource: NormalizationResource::Nodes,
                limit: self.budget.limits.storage.nodes,
            })?;
        Ok(self.next_scope)
    }
    fn key(&mut self, name: &str, origin: SchemaOrigin) -> Result<usize, NormalizationError> {
        self.budget.work(name.len())?;
        self.budget.reserve(name.len(), 0)?;
        Ok(self.emit(
            N::Text(name.into()),
            SchemaOrigin {
                generated: true,
                ..origin
            },
        ))
    }
    fn copy(&mut self, site: Site<'_>) -> Result<usize, NormalizationError> {
        self.step(site)?;
        let node = match &site.input.nodes()[site.value] {
            N::Text(text) | N::Integer(text) => {
                self.budget.work(text.len())?;
                self.budget.reserve(text.len(), 0)?;
                site.input.nodes()[site.value].clone()
            }
            N::Sequence(items) => {
                self.budget.reserve(0, items.len())?;
                let mut result = Vec::with_capacity(items.len());
                for &value in items {
                    result.push(self.copy(site.child(value))?);
                }
                N::Sequence(result)
            }
            N::Mapping(items) => {
                self.budget.reserve(0, items.len().saturating_mul(2))?;
                let mut result = Vec::with_capacity(items.len());
                for &(key, value) in items {
                    result.push((self.copy(site.child(key))?, self.copy(site.child(value))?));
                }
                N::Mapping(result)
            }
            node => {
                self.budget.reserve(0, 0)?;
                node.clone()
            }
        };
        Ok(self.emit(node, site.origin()))
    }
    fn matches(&mut self, member: Member<'_>, site: Site<'_>) -> Result<bool, NormalizationError> {
        self.step(site)?;
        self.budget.work(self.active.len())?;
        let active: Vec<_> = self
            .active
            .iter()
            .filter(|(scope, _, _)| *scope == site.scope)
            .map(|&(_, value, alias)| (value, alias))
            .collect();
        Ok(matches_member(
            self.schema,
            site.input,
            (member.expression, member.node),
            site.value,
            site.fragment,
            &active,
            &mut self.budget.validation,
        )?)
    }
    fn descriptor(
        &mut self,
        descriptor: usize,
        site: Site<'_>,
    ) -> Result<usize, NormalizationError> {
        self.step(site)?;
        let members = self.schema.descriptors()[descriptor].descriptor.members();
        self.budget.work(members.len())?;
        self.members(
            &members.iter().map(Member::root).collect::<Vec<_>>(),
            site.deeper(),
        )
    }
    fn default(&mut self, descriptor: usize, site: Site<'_>) -> Result<usize, NormalizationError> {
        let schema = self.schema;
        let located = &schema.descriptors()[descriptor];
        let value = located.descriptor.default_node().expect("declared default");
        // A default materialization is a fresh occurrence, even when it points to
        // the same schema node. Reusing a scope here would hide recursive defaults.
        let scope = self.scope()?;
        self.descriptor(
            descriptor,
            Site {
                input: &schema.modules()[located.module].document,
                value,
                scope,
                provenance: Provenance::Direct(SchemaSource::Default {
                    module: located.module,
                    descriptor,
                }),
                depth: site.depth + 1,
                fragment: false,
            },
        )
    }
    fn class(
        &mut self,
        fields: &[SchemaField],
        site: Site<'_>,
    ) -> Result<usize, NormalizationError> {
        self.step(site)?;
        let N::Mapping(items) = &site.input.nodes()[site.value] else {
            return self.copy(site);
        };
        self.budget.reserve(0, 0)?;
        let mut result = Vec::new();
        for field in fields {
            self.budget.work(
                items
                    .len()
                    .saturating_mul(field.name.len().saturating_add(1)),
            )?;
            let present = items.iter().find(|(key, _)| matches!(&site.input.nodes()[*key], N::Text(name) if *name == field.name));
            if let Some(&(key, value)) = present {
                self.budget.edges(2)?;
                let key = self.copy(site.child(key))?;
                let value = self.descriptor(field.descriptor, site.child(value))?;
                result.push((key, value));
            } else if !site.fragment
                && self.schema.descriptors()[field.descriptor]
                    .descriptor
                    .default_node()
                    .is_some()
            {
                self.budget.edges(2)?;
                let value = self.default(field.descriptor, site.deeper())?;
                let key = self.key(&field.name, self.origins[value])?;
                result.push((key, value));
            }
        }
        Ok(self.emit(N::Mapping(result), site.origin()))
    }
    fn members(
        &mut self,
        members: &[Member<'_>],
        site: Site<'_>,
    ) -> Result<usize, NormalizationError> {
        self.step(site)?;
        if members.len() == 2 {
            for member in members {
                let TypeNode::List(inner) = member.expression.nodes()[member.node] else {
                    continue;
                };
                let inner = member.child(inner);
                self.budget
                    .work(members.iter().map(|m| m.text().len()).sum())?;
                if !members.iter().any(|m| m.text() == inner.text()) {
                    continue;
                }
                if self.matches(*member, site)? {
                    return self.single(*member, site.deeper());
                }
                if self.matches(inner, site)? {
                    self.budget.reserve(0, 1)?;
                    let value = self.members(&[inner], site.deeper())?;
                    return Ok(self.emit(
                        N::Sequence(vec![value]),
                        SchemaOrigin {
                            generated: true,
                            ..site.origin()
                        },
                    ));
                }
            }
            let schema = self.schema;
            for class_member in members {
                let Some(class) = schema.class_named(class_member.text()) else {
                    continue;
                };
                self.budget.work(class.fields.len())?;
                let mut required = class
                    .fields
                    .iter()
                    .filter(|f| schema.descriptors()[f.descriptor].descriptor.required());
                let Some(field) = required.next() else {
                    continue;
                };
                if required.next().is_some() {
                    continue;
                }
                let types = schema.descriptors()[field.descriptor].descriptor.members();
                if types.len() != 1 {
                    continue;
                }
                for member in members {
                    if member.text() == class_member.text()
                        || schema.class_named(member.text()).is_some()
                        || member.text() != Member::root(&types[0]).text()
                    {
                        continue;
                    }
                    if !self.matches(*member, site)? {
                        continue;
                    }
                    self.budget.reserve(0, 2)?;
                    let key = self.key(&field.name, site.origin())?;
                    let value = self.descriptor(field.descriptor, site.deeper())?;
                    let mut result = vec![(key, value)];
                    for other in &class.fields {
                        self.budget.work(1)?;
                        if other.name != field.name
                            && !site.fragment
                            && schema.descriptors()[other.descriptor]
                                .descriptor
                                .default_node()
                                .is_some()
                        {
                            self.budget.edges(2)?;
                            let value = self.default(other.descriptor, site.deeper())?;
                            let key = self.key(&other.name, self.origins[value])?;
                            result.push((key, value));
                        }
                    }
                    return Ok(self.emit(
                        N::Mapping(result),
                        SchemaOrigin {
                            generated: true,
                            ..site.origin()
                        },
                    ));
                }
            }
        }
        for member in members {
            if self.matches(*member, site)? {
                return self.single(*member, site.deeper());
            }
        }
        self.copy(site)
    }
    fn single(&mut self, member: Member<'_>, site: Site<'_>) -> Result<usize, NormalizationError> {
        self.step(site)?;
        match &member.expression.nodes()[member.node] {
            TypeNode::List(inner) => {
                let N::Sequence(items) = &site.input.nodes()[site.value] else {
                    return self.copy(site);
                };
                self.budget.reserve(0, items.len())?;
                let mut result = Vec::with_capacity(items.len());
                for &value in items {
                    result.push(self.members(&[member.child(*inner)], site.child(value))?);
                }
                Ok(self.emit(N::Sequence(result), site.origin()))
            }
            TypeNode::Dictionary { key, value } => {
                let N::Mapping(items) = &site.input.nodes()[site.value] else {
                    return self.copy(site);
                };
                self.budget.reserve(0, items.len().saturating_mul(2))?;
                let mut result = Vec::with_capacity(items.len());
                for &(k, v) in items {
                    let k = self.members(
                        &[member.child(*key)],
                        Site {
                            fragment: false,
                            ..site.child(k)
                        },
                    )?;
                    let v = self.members(&[member.child(*value)], site.child(v))?;
                    result.push((k, v));
                }
                Ok(self.emit(N::Mapping(result), site.origin()))
            }
            TypeNode::Name(_) => {
                let schema = self.schema;
                let name = member.text();
                if let Some(class) = schema.class_named(name) {
                    return self.class(&class.fields, site.deeper());
                }
                let Some((id, alias)) = schema.alias_named(name) else {
                    return self.copy(site);
                };
                if self.active.contains(&(site.scope, site.value, id)) {
                    return self.copy(site);
                }
                self.active.push((site.scope, site.value, id));
                let result = self.alias(name, alias.kind, site.deeper());
                self.active.pop();
                result
            }
        }
    }
    fn alias(
        &mut self,
        name: &str,
        kind: SchemaAliasKind,
        site: Site<'_>,
    ) -> Result<usize, NormalizationError> {
        self.step(site)?;
        let schema = self.schema;
        match kind {
            SchemaAliasKind::Descriptor(descriptor) => {
                let members = schema.descriptors()[descriptor].descriptor.members();
                self.budget.work(members.len())?;
                if name == "derivation" || name == "case_result" {
                    let members: Vec<_> = members
                        .iter()
                        .map(Member::root)
                        .filter(|m| m.text() != "str")
                        .collect();
                    if let N::Text(text) = &site.input.nodes()[site.value] {
                        let generated = self.derivation(text, name == "derivation")?;
                        let scope = self.scope()?;
                        return self.members(
                            &members,
                            Site {
                                input: &generated,
                                value: generated.root(),
                                scope,
                                provenance: Provenance::Generated(site.origin()),
                                ..site.deeper()
                            },
                        );
                    }
                    self.members(&members, site.deeper())
                } else {
                    self.descriptor(descriptor, site.deeper())
                }
            }
            SchemaAliasKind::Registry(registry) => {
                let N::Mapping(items) = &site.input.nodes()[site.value] else {
                    return self.copy(site);
                };
                if items.len() != 1 {
                    return self.copy(site);
                }
                let (key, payload) = items[0];
                let N::Text(operation) = &site.input.nodes()[key] else {
                    return self.copy(site);
                };
                let registry = &schema.registries()[registry];
                self.budget.work(
                    registry
                        .entries
                        .len()
                        .saturating_mul(operation.len().saturating_add(1)),
                )?;
                let Some(entry) = registry.entries.iter().find(|e| e.name == *operation) else {
                    return self.copy(site);
                };
                self.budget.reserve(0, 2)?;
                let key = self.copy(site.child(key))?;
                let value = match &entry.shape {
                    SchemaShape::Class(fields) => self.class(fields, site.child(payload))?,
                    SchemaShape::Descriptor(descriptor) => {
                        self.descriptor(*descriptor, site.child(payload))?
                    }
                };
                Ok(self.emit(N::Mapping(vec![(key, value)]), site.origin()))
            }
        }
    }
    fn derivation(&mut self, text: &str, calls: bool) -> Result<Document, NormalizationError> {
        self.budget.work(text.len())?;
        let trimmed =
            text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
        let call = if calls {
            trimmed.split_once('(').and_then(|(name, tail)| {
                let argument = tail.strip_suffix(')')?;
                if name.is_empty()
                    || !name.as_bytes()[0].is_ascii_alphabetic() && name.as_bytes()[0] != b'_'
                    || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    || argument.contains(['(', ')'])
                {
                    return None;
                }
                Some((
                    name,
                    argument.trim_matches(|c: char| {
                        c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
                    }),
                ))
            })
        } else {
            None
        };
        let argument = call.map_or(text, |(_, argument)| argument);
        self.budget.reserve(6, 0)?;
        self.budget.reserve(argument.len(), 0)?;
        self.budget.reserve(0, 2)?;
        let mut nodes = vec![
            N::Text("source".into()),
            N::Text(argument.into()),
            N::Mapping(vec![(0, 1)]),
        ];
        let root = if let Some((operation, _)) = call {
            self.budget.reserve(operation.len(), 0)?;
            self.budget.reserve(0, 2)?;
            nodes.push(N::Text(operation.into()));
            nodes.push(N::Mapping(vec![(3, 2)]));
            4
        } else {
            2
        };
        Document::new(nodes, root, self.budget.limits.storage).map_err(NormalizationError::Document)
    }
    fn finish(self, root: usize) -> Result<NormalizedDocument, NormalizationError> {
        let document = Document::new(self.nodes, root, self.budget.limits.storage)
            .map_err(NormalizationError::Document)?;
        Ok(NormalizedDocument {
            document,
            origins: self.origins,
        })
    }
}

impl SchemaStructure {
    /// Normalize a validated projected type union used by schema-shaped consumers.
    pub fn normalize_types(
        &self,
        types: &[TypeExpression],
        input: &Document,
        value: usize,
        fragment: bool,
        budget: &mut NormalizationBudget,
    ) -> Result<NormalizedDocument, NormalizationError> {
        self.normalization_defaults(budget)?;
        let findings = self.validate_types(
            types,
            input,
            value,
            "<normalization>",
            fragment,
            &mut budget.validation,
        )?;
        if !findings.is_empty() {
            return Err(NormalizationError::Invalid(findings));
        }
        budget.work(types.len())?;
        let site = Site {
            input,
            value,
            provenance: Provenance::Direct(SchemaSource::Input),
            scope: 0,
            depth: 0,
            fragment,
        };
        let mut run = Run {
            schema: self,
            budget,
            nodes: Vec::new(),
            origins: Vec::new(),
            active: Vec::new(),
            next_scope: 0,
        };
        let root = run.members(&types.iter().map(Member::root).collect::<Vec<_>>(), site)?;
        run.finish(root)
    }
    fn normalization_defaults(
        &self,
        budget: &mut NormalizationBudget,
    ) -> Result<(), NormalizationError> {
        if self.defaults_validated {
            return Ok(());
        }
        let defaults = self.validate_defaults(&mut budget.validation)?;
        if defaults.is_empty() {
            Ok(())
        } else {
            Err(NormalizationError::Defaults(defaults))
        }
    }
    /// Validate written forms and all schema defaults before producing canonical values.
    pub fn normalize_document(
        &self,
        input: &Document,
        budget: &mut NormalizationBudget,
    ) -> Result<NormalizedDocument, NormalizationError> {
        self.normalization_defaults(budget)?;
        let findings = self.validate_document(input, &mut budget.validation)?;
        if !findings.is_empty() {
            return Err(NormalizationError::Invalid(findings));
        }
        let site = Site {
            input,
            value: input.root(),
            provenance: Provenance::Direct(SchemaSource::Input),
            scope: 0,
            depth: 0,
            fragment: false,
        };
        let mut run = Run {
            schema: self,
            budget,
            nodes: Vec::new(),
            origins: Vec::new(),
            active: Vec::new(),
            next_scope: 0,
        };
        let root = run.class(&self.root_class().fields, site)?;
        run.finish(root)
    }
    /// Fragment normalization expands shorthand while deferring defaults at every depth.
    pub fn normalize_descriptor(
        &self,
        descriptor: usize,
        input: &Document,
        value: usize,
        fragment: bool,
        budget: &mut NormalizationBudget,
    ) -> Result<NormalizedDocument, NormalizationError> {
        self.normalization_defaults(budget)?;
        let findings = self.validate_descriptor(
            descriptor,
            input,
            value,
            "<normalization>",
            fragment,
            &mut budget.validation,
        )?;
        if !findings.is_empty() {
            return Err(NormalizationError::Invalid(findings));
        }
        let site = Site {
            input,
            value,
            provenance: Provenance::Direct(SchemaSource::Input),
            scope: 0,
            depth: 0,
            fragment,
        };
        let mut run = Run {
            schema: self,
            budget,
            nodes: Vec::new(),
            origins: Vec::new(),
            active: Vec::new(),
            next_scope: 0,
        };
        let root = run.descriptor(descriptor, site)?;
        run.finish(root)
    }
}
