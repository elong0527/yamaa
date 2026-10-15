//! Validated schema normalization with request-wide storage and traversal policies.

use super::{
    validation::matches_member, DefaultDiagnostics, Document, DocumentError, DocumentNode as N,
    SchemaAliasKind, SchemaDiagnostic, SchemaField, SchemaShape, SchemaStructure, TypeExpression,
    TypeNode, ValidationError,
};
use std::vec;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalizationResource {
    Depth,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NormalizationError {
    #[error("schema normalization validation: {0:?}")]
    Validation(ValidationError),
    #[error("schema normalization invalid: {0:?}")]
    Invalid(Vec<SchemaDiagnostic>),
    #[error("schema normalization defaults: {0:?}")]
    Defaults(Vec<DefaultDiagnostics>),
    #[error("schema normalization limit: resource={resource:?}, limit={limit:?}")]
    Limit {
        resource: NormalizationResource,
        limit: usize,
    },
    #[error("schema normalization document: {0:?}")]
    Document(DocumentError),
}
impl From<ValidationError> for NormalizationError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error)
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

struct Run<'a> {
    schema: &'a SchemaStructure,
    nodes: Vec<N>,
    origins: Vec<SchemaOrigin>,
    active: Vec<(usize, usize, usize)>,
    next_scope: usize,
}
impl Run<'_> {
    fn step(&mut self, site: Site<'_>) -> Result<(), NormalizationError> {
        let limit = crate::limits::Limits::SEMANTIC_DEPTH;
        if site.depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        Ok(())
    }
    fn emit(&mut self, node: N, origin: SchemaOrigin) -> usize {
        let id = self.nodes.len();
        self.nodes.push(node);
        self.origins.push(origin);
        id
    }
    fn scope(&mut self) -> Result<usize, NormalizationError> {
        self.next_scope += 1;
        Ok(self.next_scope)
    }
    fn key(&mut self, name: &str, origin: SchemaOrigin) -> Result<usize, NormalizationError> {
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
            N::Text(_) | N::Integer(_) => site.input.nodes()[site.value].clone(),
            N::Sequence(items) => {
                let mut result = Vec::with_capacity(items.len());
                for &value in items {
                    result.push(self.copy(site.child(value))?);
                }
                N::Sequence(result)
            }
            N::Mapping(items) => {
                let mut result = Vec::with_capacity(items.len());
                for &(key, value) in items {
                    result.push((self.copy(site.child(key))?, self.copy(site.child(value))?));
                }
                N::Mapping(result)
            }
            node => node.clone(),
        };
        Ok(self.emit(node, site.origin()))
    }
    fn matches(&mut self, member: Member<'_>, site: Site<'_>) -> Result<bool, NormalizationError> {
        self.step(site)?;
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
        )?)
    }
    fn descriptor(
        &mut self,
        descriptor: usize,
        site: Site<'_>,
    ) -> Result<usize, NormalizationError> {
        self.step(site)?;
        let members = self.schema.descriptors()[descriptor].descriptor.members();
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
        let mut result = Vec::new();
        for field in fields {
            let present = items.iter().find(|(key, _)| matches!(&site.input.nodes()[*key], N::Text(name) if *name == field.name));
            if let Some(&(key, value)) = present {
                let key = self.copy(site.child(key))?;
                let value = self.descriptor(field.descriptor, site.child(value))?;
                result.push((key, value));
            } else if !site.fragment
                && self.schema.descriptors()[field.descriptor]
                    .descriptor
                    .default_node()
                    .is_some()
            {
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
                if !members.iter().any(|m| m.text() == inner.text()) {
                    continue;
                }
                if self.matches(*member, site)? {
                    return self.single(*member, site.deeper());
                }
                if self.matches(inner, site)? {
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
                    let key = self.key(&field.name, site.origin())?;
                    let value = self.descriptor(field.descriptor, site.deeper())?;
                    let mut result = vec![(key, value)];
                    for other in &class.fields {
                        if other.name != field.name
                            && !site.fragment
                            && schema.descriptors()[other.descriptor]
                                .descriptor
                                .default_node()
                                .is_some()
                        {
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
                if matches!(
                    name,
                    "derivation" | "untyped_intermediate_derivation" | "case_result"
                ) {
                    let members: Vec<_> = members
                        .iter()
                        .map(Member::root)
                        .filter(|m| m.text() != "str")
                        .collect();
                    if let N::Text(text) = &site.input.nodes()[site.value] {
                        let generated = self.derivation(text, name != "case_result")?;
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
                let Some(entry) = registry.entries.iter().find(|e| e.name == *operation) else {
                    return self.copy(site);
                };
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
        let mut nodes = vec![
            N::Text("source".into()),
            N::Text(argument.into()),
            N::Mapping(vec![(0, 1)]),
        ];
        let root = if let Some((operation, _)) = call {
            nodes.push(N::Text(operation.into()));
            nodes.push(N::Mapping(vec![(3, 2)]));
            4
        } else {
            2
        };
        Document::new(nodes, root).map_err(NormalizationError::Document)
    }
    fn finish(self, root: usize) -> Result<NormalizedDocument, NormalizationError> {
        let document = Document::new(self.nodes, root).map_err(NormalizationError::Document)?;
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
    ) -> Result<NormalizedDocument, NormalizationError> {
        self.normalization_defaults()?;
        let findings = self.validate_types(types, input, value, "<normalization>", fragment)?;
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
            nodes: Vec::new(),
            origins: Vec::new(),
            active: Vec::new(),
            next_scope: 0,
        };
        let root = run.members(&types.iter().map(Member::root).collect::<Vec<_>>(), site)?;
        run.finish(root)
    }
    fn normalization_defaults(&self) -> Result<(), NormalizationError> {
        if self.defaults_validated {
            return Ok(());
        }
        let defaults = self.validate_defaults()?;
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
    ) -> Result<NormalizedDocument, NormalizationError> {
        self.normalize_document_mode(input, false)
    }
    /// Normalize the final composed specification with resolved-output citation ownership.
    pub fn normalize_resolved_document(
        &self,
        input: &Document,
    ) -> Result<NormalizedDocument, NormalizationError> {
        self.normalize_document_mode(input, true)
    }
    fn normalize_document_mode(
        &self,
        input: &Document,
        resolved_output: bool,
    ) -> Result<NormalizedDocument, NormalizationError> {
        self.normalization_defaults()?;
        let findings = if resolved_output {
            self.validate_resolved_document(input)?
        } else {
            self.validate_document(input)?
        };
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
    ) -> Result<NormalizedDocument, NormalizationError> {
        self.normalization_defaults()?;
        let findings =
            self.validate_descriptor(descriptor, input, value, "<normalization>", fragment)?;
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
            nodes: Vec::new(),
            origins: Vec::new(),
            active: Vec::new(),
            next_scope: 0,
        };
        let root = run.descriptor(descriptor, site)?;
        run.finish(root)
    }
}
