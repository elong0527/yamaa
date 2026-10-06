//! Schema-directed composition of normalized fragments, before default materialization.

use super::{
    document::scalar_keys_equal, validation::matches_member, Document, DocumentNode as N,
    NormalizationBudget, NormalizationError, NormalizationResource, SchemaAliasKind, SchemaField,
    SchemaShape, SchemaStructure, TypeExpression, TypeNode, ValidationError,
};
use alloc::vec::Vec;

/// An occurrence in one retained input, not a host identity or content digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionOrigin {
    pub input: usize,
    pub node: usize,
}

/// Owned composed value. Defaults are deliberately withheld until composition ends.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposedValue {
    pub document: Document,
    pub origins: Vec<CompositionOrigin>,
}

#[derive(Clone, Copy)]
struct Site<'a> {
    document: &'a Document,
    node: usize,
    input: usize,
}
impl<'a> Site<'a> {
    fn child(self, node: usize) -> Self {
        Self { node, ..self }
    }
    fn value(self) -> &'a N {
        &self.document.nodes()[self.node]
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
    budget: &'b mut NormalizationBudget,
    nodes: Vec<N>,
    origins: Vec<CompositionOrigin>,
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
    fn push(&mut self, value: N, site: Site<'_>) -> usize {
        let id = self.nodes.len();
        self.nodes.push(value);
        self.origins.push(CompositionOrigin {
            input: site.input,
            node: site.node,
        });
        id
    }
    fn copy(&mut self, site: Site<'_>, depth: usize) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let value = match site.value() {
            N::Text(text) | N::Integer(text) => {
                self.budget.work(text.len())?;
                self.budget.reserve(text.len(), 0)?;
                site.value().clone()
            }
            N::Sequence(items) => {
                self.budget.reserve(0, items.len())?;
                let mut values = Vec::with_capacity(items.len());
                for &item in items {
                    values.push(self.copy(site.child(item), depth + 1)?);
                }
                N::Sequence(values)
            }
            N::Mapping(items) => {
                self.budget.reserve(0, items.len().saturating_mul(2))?;
                let mut values = Vec::with_capacity(items.len());
                for &(key, value) in items {
                    values.push((
                        self.copy(site.child(key), depth + 1)?,
                        self.copy(site.child(value), depth + 1)?,
                    ));
                }
                N::Mapping(values)
            }
            value => {
                self.budget.reserve(0, 0)?;
                value.clone()
            }
        };
        Ok(self.push(value, site))
    }
    fn selected<'a>(
        &mut self,
        descriptor: usize,
        site: Site<'_>,
        schema: &'a SchemaStructure,
    ) -> Result<Option<Member<'a>>, NormalizationError> {
        let descriptor = schema
            .descriptors()
            .get(descriptor)
            .ok_or(ValidationError::InvalidDescriptor)?;
        for expression in descriptor.descriptor.members() {
            self.budget.work(1)?;
            if matches_member(
                schema,
                site.document,
                (expression, expression.root()),
                site.node,
                true,
                &[],
                &mut self.budget.validation,
            )? {
                return Ok(Some(Member {
                    expression,
                    node: expression.root(),
                }));
            }
        }
        Ok(None)
    }
    fn descriptor(
        &mut self,
        descriptor: usize,
        old: Site<'_>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        if !matches!((old.value(), new.value()), (N::Mapping(_), N::Mapping(_))) {
            return self.copy(new, depth + 1);
        }
        let Some(incoming) = self.selected(descriptor, new, self.schema)? else {
            return self.copy(new, depth + 1);
        };
        let inherited = self.selected(descriptor, old, self.schema)?;
        match inherited {
            Some(inherited) if incoming.text() == inherited.text() => {
                self.member(incoming, old, new, depth + 1)
            }
            _ => self.copy(new, depth + 1),
        }
    }
    fn matching_key(
        &mut self,
        key: Site<'_>,
        mapping: Site<'_>,
    ) -> Result<Option<(usize, usize)>, NormalizationError> {
        let N::Mapping(items) = mapping.value() else {
            return Err(ValidationError::InvalidDescriptor.into());
        };
        for &(candidate, value) in items {
            let node = &mapping.document.nodes()[candidate];
            self.budget.work(
                key.value()
                    .length()
                    .unwrap_or(1)
                    .saturating_add(node.length().unwrap_or(1)),
            )?;
            if scalar_keys_equal(key.value(), node) {
                return Ok(Some((candidate, value)));
            }
        }
        Ok(None)
    }
    fn mapping(
        &mut self,
        fields: Option<&[SchemaField]>,
        inner: Option<Member<'_>>,
        old: Site<'_>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let (N::Mapping(before), N::Mapping(after)) = (old.value(), new.value()) else {
            return self.copy(new, depth + 1);
        };
        // Conservative reservation also bounds the temporary container capacity.
        let capacity = before.len().saturating_add(after.len());
        self.budget.reserve(0, capacity.saturating_mul(2))?;
        let mut output = Vec::with_capacity(capacity);
        for &(key, value) in before {
            let inherited = old.child(value);
            let incoming = self.matching_key(old.child(key), new)?;
            let value = if let Some((_, value)) = incoming {
                let incoming = new.child(value);
                if let Some(fields) = fields {
                    let name = match old.child(key).value() {
                        N::Text(text) => Some(text.as_str()),
                        _ => None,
                    };
                    self.budget
                        .work(fields.len().saturating_mul(name.map_or(1, str::len)))?;
                    if let Some(field) = fields.iter().find(|f| Some(f.name.as_str()) == name) {
                        self.descriptor(field.descriptor, inherited, incoming, depth + 1)?
                    } else {
                        self.copy(incoming, depth + 1)?
                    }
                } else if let Some(inner) = inner {
                    self.member(inner, inherited, incoming, depth + 1)?
                } else {
                    self.copy(incoming, depth + 1)?
                }
            } else {
                self.copy(inherited, depth + 1)?
            };
            output.push((self.copy(old.child(key), depth + 1)?, value));
        }
        for &(key, value) in after {
            if self.matching_key(new.child(key), old)?.is_none() {
                output.push((
                    self.copy(new.child(key), depth + 1)?,
                    self.copy(new.child(value), depth + 1)?,
                ));
            }
        }
        Ok(self.push(N::Mapping(output), old))
    }
    fn member(
        &mut self,
        member: Member<'_>,
        old: Site<'_>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        if !matches!((old.value(), new.value()), (N::Mapping(_), N::Mapping(_)))
            || member.text() == "match_key"
        {
            return self.copy(new, depth + 1);
        }
        match &member.expression.nodes()[member.node] {
            TypeNode::Dictionary { value, .. } => {
                self.mapping(None, Some(member.child(*value)), old, new, depth + 1)
            }
            TypeNode::Name(_) => {
                if let Some(class) = self.schema.class_named(member.text()) {
                    return self.mapping(Some(&class.fields), None, old, new, depth + 1);
                }
                if let Some((_, alias)) = self.schema.alias_named(member.text()) {
                    match alias.kind {
                        SchemaAliasKind::Descriptor(id) => {
                            return self.descriptor(id, old, new, depth + 1)
                        }
                        SchemaAliasKind::Registry(id) => {
                            let (N::Mapping(before), N::Mapping(after)) =
                                (old.value(), new.value())
                            else {
                                unreachable!()
                            };
                            if before.len() == 1 && after.len() == 1 {
                                let ((old_key, old_value), (new_key, new_value)) =
                                    (before[0], after[0]);
                                if let (N::Text(a), N::Text(b)) =
                                    (old.child(old_key).value(), new.child(new_key).value())
                                {
                                    self.budget.work(a.len().saturating_add(b.len()))?;
                                    if a == b {
                                        let registry = &self.schema.registries()[id];
                                        self.budget.work(
                                            registry
                                                .entries
                                                .len()
                                                .saturating_mul(b.len().saturating_add(1)),
                                        )?;
                                        if let Some(entry) =
                                            registry.entries.iter().find(|entry| entry.name == *b)
                                        {
                                            self.budget.reserve(0, 2)?;
                                            let value = match &entry.shape {
                                                SchemaShape::Class(fields) => self.mapping(
                                                    Some(fields),
                                                    None,
                                                    old.child(old_value),
                                                    new.child(new_value),
                                                    depth + 1,
                                                )?,
                                                SchemaShape::Descriptor(id) => self.descriptor(
                                                    *id,
                                                    old.child(old_value),
                                                    new.child(new_value),
                                                    depth + 1,
                                                )?,
                                            };
                                            let key = self.copy(old.child(old_key), depth + 1)?;
                                            return Ok(self
                                                .push(N::Mapping(alloc::vec![(key, value)]), old));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                self.copy(new, depth + 1)
            }
            TypeNode::List(_) => self.copy(new, depth + 1),
        }
    }
}
impl SchemaStructure {
    /// Compose two admitted normalized values by a field's declared kind (REQ-0630).
    /// Lists and match keys replace whole; null is a value here, not a clearing marker.
    /// Input 0 denotes the accumulated value and input 1 the later contribution.
    /// Default insertion and root/member clearing belong to the surrounding layer pass.
    pub fn compose_descriptor(
        &self,
        descriptor: usize,
        accumulated: &Document,
        incoming: &Document,
        budget: &mut NormalizationBudget,
    ) -> Result<ComposedValue, NormalizationError> {
        self.compose_descriptor_at(
            descriptor,
            (accumulated, accumulated.root()),
            (incoming, incoming.root()),
            budget,
        )
    }

    pub(super) fn compose_descriptor_at(
        &self,
        descriptor: usize,
        accumulated: (&Document, usize),
        incoming: (&Document, usize),
        budget: &mut NormalizationBudget,
    ) -> Result<ComposedValue, NormalizationError> {
        if descriptor >= self.descriptors().len()
            || accumulated.1 >= accumulated.0.nodes().len()
            || incoming.1 >= incoming.0.nodes().len()
        {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut run = Run {
            schema: self,
            budget,
            nodes: Vec::new(),
            origins: Vec::new(),
        };
        let root = run.descriptor(
            descriptor,
            Site {
                document: accumulated.0,
                node: accumulated.1,
                input: 0,
            },
            Site {
                document: incoming.0,
                node: incoming.1,
                input: 1,
            },
            0,
        )?;
        let document = Document::new(run.nodes, root, run.budget.limits.storage)
            .map_err(NormalizationError::Document)?;
        Ok(ComposedValue {
            document,
            origins: run.origins,
        })
    }
}
