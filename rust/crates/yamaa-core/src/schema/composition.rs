//! Schema-directed composition of normalized fragments, before default materialization.

use super::{
    document::scalar_keys_equal,
    validation::{matches_member, NullPolicy},
    Document, DocumentNode as N, NormalizationError, NormalizationResource, SchemaAliasKind,
    SchemaField, SchemaShape, SchemaStructure, TypeExpression, TypeNode, ValidationError,
};

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
    fn mapping(self) -> Option<&'a [(usize, usize)]> {
        match self.value() {
            N::Mapping(items) => Some(items),
            _ => None,
        }
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

/// How a mapping's entries compose: class fields by their own descriptors, or
/// dictionary values by the dictionary's value type.
#[derive(Clone, Copy)]
enum Entries<'a> {
    Fields(&'a [SchemaField]),
    Values(Member<'a>),
}

struct Run<'a> {
    schema: &'a SchemaStructure,
    nodes: Vec<N>,
    origins: Vec<CompositionOrigin>,
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
            N::Text(_) | N::Integer(_) => site.value().clone(),
            N::Sequence(items) => {
                let mut values = Vec::with_capacity(items.len());
                for &item in items {
                    values.push(self.copy(site.child(item), depth + 1)?);
                }
                N::Sequence(values)
            }
            N::Mapping(items) => {
                let mut values = Vec::with_capacity(items.len());
                for &(key, value) in items {
                    values.push((
                        self.copy(site.child(key), depth + 1)?,
                        self.copy(site.child(value), depth + 1)?,
                    ));
                }
                N::Mapping(values)
            }
            value => value.clone(),
        };
        Ok(self.push(value, site))
    }
    /// The first union member a fragment matches.
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
            if matches_member(
                schema,
                site.document,
                (expression, expression.root()),
                site.node,
                true,
                NullPolicy::Value,
                &[],
            )? {
                return Ok(Some(Member {
                    expression,
                    node: expression.root(),
                }));
            }
        }
        Ok(None)
    }
    /// Compose one value by its descriptor. A mapping composes with an inherited
    /// mapping that selects the same union member; anything else replaces it.
    fn descriptor(
        &mut self,
        descriptor: usize,
        old: Option<Site<'_>>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        if new.mapping().is_none() {
            return self.copy(new, depth + 1);
        }
        let Some(incoming) = self.selected(descriptor, new, self.schema)? else {
            return self.copy(new, depth + 1);
        };
        let inherited = match old {
            Some(old) if old.mapping().is_some() => self.selected(descriptor, old, self.schema)?,
            _ => None,
        };
        let old = match inherited {
            Some(inherited) if inherited.text() == incoming.text() => old,
            _ => None,
        };
        self.member(incoming, old, new, depth + 1)
    }
    fn matching_key(
        &mut self,
        key: Site<'_>,
        mapping: Site<'_>,
    ) -> Result<Option<(usize, usize)>, NormalizationError> {
        let Some(items) = mapping.mapping() else {
            return Err(ValidationError::InvalidDescriptor.into());
        };
        for &(candidate, value) in items {
            let node = &mapping.document.nodes()[candidate];
            if scalar_keys_equal(key.value(), node) {
                return Ok(Some((candidate, value)));
            }
        }
        Ok(None)
    }
    fn entry(
        &mut self,
        entries: Entries<'_>,
        key: Site<'_>,
        old: Option<Site<'_>>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        match entries {
            Entries::Fields(fields) => {
                let name = match key.value() {
                    N::Text(text) => Some(text.as_str()),
                    _ => None,
                };
                match fields.iter().find(|f| Some(f.name.as_str()) == name) {
                    Some(field) => self.descriptor(field.descriptor, old, new, depth),
                    None => self.copy(new, depth),
                }
            }
            Entries::Values(member) => self.member(member, old, new, depth),
        }
    }
    /// A class merges field by field and a dictionary key by key; a null entry
    /// composes as the value it is (LAYER-9).
    fn mapping(
        &mut self,
        entries: Entries<'_>,
        old: Option<Site<'_>>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let Some(after) = new.mapping() else {
            return self.copy(new, depth + 1);
        };
        let old = old.filter(|old| old.mapping().is_some());
        let before = old.and_then(Site::mapping).unwrap_or(&[]);
        // Conservative reservation also bounds the temporary container capacity.
        let capacity = before.len().saturating_add(after.len());
        let mut output = Vec::with_capacity(capacity);
        if let Some(old) = old {
            for &(key, value) in before {
                let inherited = old.child(value);
                let value = match self.matching_key(old.child(key), new)? {
                    Some((_, value)) => self.entry(
                        entries,
                        old.child(key),
                        Some(inherited),
                        new.child(value),
                        depth + 1,
                    )?,
                    None => self.copy(inherited, depth + 1)?,
                };
                output.push((self.copy(old.child(key), depth + 1)?, value));
            }
        }
        for &(key, value) in after {
            if let Some(old) = old {
                if self.matching_key(new.child(key), old)?.is_some() {
                    continue;
                }
            }
            let value = self.entry(entries, new.child(key), None, new.child(value), depth + 1)?;
            output.push((self.copy(new.child(key), depth + 1)?, value));
        }
        Ok(self.push(N::Mapping(output), old.unwrap_or(new)))
    }
    fn member(
        &mut self,
        member: Member<'_>,
        old: Option<Site<'_>>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let Some(after) = new.mapping() else {
            return self.copy(new, depth + 1);
        };
        let old = old.filter(|old| old.mapping().is_some());
        match &member.expression.nodes()[member.node] {
            TypeNode::Dictionary { value, .. } => {
                self.mapping(Entries::Values(member.child(*value)), old, new, depth + 1)
            }
            TypeNode::Name(_) => {
                if let Some(class) = self.schema.class_named(member.text()) {
                    return self.mapping(Entries::Fields(&class.fields), old, new, depth + 1);
                }
                match self.schema.alias_named(member.text()) {
                    Some((_, alias)) => match alias.kind {
                        SchemaAliasKind::Descriptor(id) => self.descriptor(id, old, new, depth + 1),
                        SchemaAliasKind::Registry(id) => {
                            self.registry(id, after, old, new, depth + 1)
                        }
                    },
                    None => self.copy(new, depth + 1),
                }
            }
            TypeNode::List(_) => self.copy(new, depth + 1),
        }
    }
    /// A registry value composes only with an inherited value naming the same
    /// keyword; any other inherited value is replaced whole.
    fn registry(
        &mut self,
        registry: usize,
        after: &[(usize, usize)],
        old: Option<Site<'_>>,
        new: Site<'_>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        let &[(new_key, new_value)] = after else {
            return self.copy(new, depth + 1);
        };
        let N::Text(keyword) = new.child(new_key).value() else {
            return self.copy(new, depth + 1);
        };
        let Some(entry) = self.schema.registries()[registry]
            .entries
            .iter()
            .find(|entry| entry.name == *keyword)
        else {
            return self.copy(new, depth + 1);
        };
        let inherited = old.and_then(|old| match old.mapping() {
            Some(&[(key, value)])
                if matches!(old.child(key).value(), N::Text(text) if text == keyword) =>
            {
                Some((old, key, value))
            }
            _ => None,
        });
        let payload = inherited.map(|(old, _, value)| old.child(value));
        let value = match &entry.shape {
            SchemaShape::Class(fields) => self.mapping(
                Entries::Fields(fields),
                payload,
                new.child(new_value),
                depth + 1,
            )?,
            SchemaShape::Descriptor(id) => {
                self.descriptor(*id, payload, new.child(new_value), depth + 1)?
            }
        };
        let (key, site) = match inherited {
            Some((old, key, _)) => (self.copy(old.child(key), depth + 1)?, old),
            None => (self.copy(new.child(new_key), depth + 1)?, new),
        };
        Ok(self.push(N::Mapping(std::vec![(key, value)]), site))
    }
}
impl SchemaStructure {
    /// Compose one value of the field whose descriptor is `descriptor` with
    /// the value it inherits, as a matching keyed member composes (LAYER-6):
    /// a class merges field by field and a mapping key by key, and scalars
    /// and lists replace. A project's sections compose so (PROJECT-16). Input
    /// 0 denotes the accumulated value and input 1 the later contribution.
    pub(super) fn compose_value_at(
        &self,
        descriptor: usize,
        accumulated: Option<(&Document, usize)>,
        incoming: (&Document, usize),
    ) -> Result<ComposedValue, NormalizationError> {
        if self.descriptors().get(descriptor).is_none()
            || accumulated.is_some_and(|(document, node)| node >= document.nodes().len())
            || incoming.1 >= incoming.0.nodes().len()
        {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut run = Run {
            schema: self,
            nodes: Vec::new(),
            origins: Vec::new(),
        };
        let root = run.descriptor(
            descriptor,
            accumulated.map(|(document, node)| Site {
                document,
                node,
                input: 0,
            }),
            Site {
                document: incoming.0,
                node: incoming.1,
                input: 1,
            },
            0,
        )?;
        let document = Document::new(run.nodes, root).map_err(NormalizationError::Document)?;
        Ok(ComposedValue {
            document,
            origins: run.origins,
        })
    }
    /// Compose one member of a keyed root collection, a `list[T]` or `dict[K, T]`
    /// field, by its member type `T` (LAYER-5): a class merges field by field
    /// and a mapping key by key, a registry value composes with the same
    /// keyword and otherwise replaces, and scalars and lists replace (LAYER-6).
    /// YAML null composes as a value. Input 0 denotes the accumulated value and
    /// input 1 the later contribution. Defaults belong to the surrounding pass.
    pub(super) fn compose_member_at(
        &self,
        collection: usize,
        accumulated: Option<(&Document, usize)>,
        incoming: (&Document, usize),
    ) -> Result<ComposedValue, NormalizationError> {
        let located = self
            .descriptors()
            .get(collection)
            .ok_or(ValidationError::InvalidDescriptor)?;
        let [expression] = located.descriptor.members() else {
            return Err(ValidationError::InvalidDescriptor.into());
        };
        let member = match &expression.nodes()[expression.root()] {
            TypeNode::List(inner) => *inner,
            TypeNode::Dictionary { value, .. } => *value,
            TypeNode::Name(_) => return Err(ValidationError::InvalidDescriptor.into()),
        };
        if accumulated.is_some_and(|(document, node)| node >= document.nodes().len())
            || incoming.1 >= incoming.0.nodes().len()
        {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut run = Run {
            schema: self,
            nodes: Vec::new(),
            origins: Vec::new(),
        };
        let root = run.member(
            Member {
                expression,
                node: member,
            },
            accumulated.map(|(document, node)| Site {
                document,
                node,
                input: 0,
            }),
            Site {
                document: incoming.0,
                node: incoming.1,
                input: 1,
            },
            0,
        )?;
        let document = Document::new(run.nodes, root).map_err(NormalizationError::Document)?;
        Ok(ComposedValue {
            document,
            origins: run.origins,
        })
    }
}
