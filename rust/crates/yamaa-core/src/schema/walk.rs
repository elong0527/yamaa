//! One schema-guided walk over a normalized or composed document: every value
//! the schema reaches, in schema traversal order, with its logical path.
//! Window expansion and mapping-dictionary planning are its two visitors.

use super::{
    scalar_diagnostic_label,
    validation::{matches_member, NullPolicy},
    Document, DocumentNode as N, NormalizationError, SchemaAliasKind, SchemaField, SchemaShape,
    SchemaStructure, TypeExpression, TypeNode, ValidationError,
};

/// What a walk does beyond following the schema; every hook defaults to nothing.
pub(super) trait Visit {
    /// Called on each value of the type named `kind` before the walk follows
    /// that type; `true` ends the walk at this value.
    fn member(
        &mut self,
        _kind: &str,
        _value: usize,
        _logical: &str,
    ) -> Result<bool, NormalizationError> {
        Ok(false)
    }
    /// A class of which the walk follows only a present `value` field, at the
    /// class's own logical path.
    fn value_only(&self, _class: &str) -> bool {
        false
    }
    /// Called on the registry entry `name` selected by a single-key mapping,
    /// at the entry's logical path, before the walk follows its payload.
    fn entry(
        &mut self,
        _registry: &str,
        _name: &str,
        _payload: usize,
        _logical: &str,
    ) -> Result<(), NormalizationError> {
        Ok(())
    }
}

/// The logical path of `name` under `parent`; the root path is empty.
pub(super) fn join(parent: &str, name: &str) -> String {
    let separator = if parent.is_empty() { "" } else { "." };
    format!("{parent}{separator}{name}")
}

impl SchemaStructure {
    /// Walk `input` from the root class's fields; a root that is not a mapping
    /// is an invalid descriptor.
    pub(super) fn walk_document<V: Visit>(
        &self,
        input: &Document,
        visit: &mut V,
    ) -> Result<(), NormalizationError> {
        if !matches!(input.nodes()[input.root()], N::Mapping(_)) {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut walker = Walker {
            schema: self,
            input,
            active: Vec::new(),
            visit,
        };
        walker.fields(&self.root_class().fields, input.root(), "", 0)
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

struct Walker<'a, 'v, V> {
    schema: &'a SchemaStructure,
    input: &'a Document,
    /// Alias guards held while a descriptor alias is followed at a value.
    active: Vec<(usize, usize)>,
    visit: &'v mut V,
}

impl<V: Visit> Walker<'_, '_, V> {
    fn label(&self, node: usize) -> Result<String, NormalizationError> {
        let value = &self.input.nodes()[node];
        Ok(scalar_diagnostic_label(value).ok_or(ValidationError::InvalidDescriptor)?)
    }
    fn descriptor(
        &mut self,
        descriptor: usize,
        value: usize,
        logical: &str,
        depth: usize,
    ) -> Result<(), NormalizationError> {
        NormalizationError::check_depth(depth)?;
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
                    NullPolicy::Absent,
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
        NormalizationError::check_depth(depth)?;
        if !matches!(self.input.nodes()[value], N::Mapping(_)) {
            return Ok(());
        }
        for field in fields {
            if let Some(child) = self.input.field(value, &field.name) {
                let path = join(logical, &field.name);
                self.descriptor(field.descriptor, child, &path, depth + 1)?;
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
        NormalizationError::check_depth(depth)?;
        let kind = member.text();
        if self.visit.member(kind, value, logical)? {
            return Ok(());
        }
        match &member.expression.nodes()[member.node] {
            TypeNode::List(inner) => self.list(member.child(*inner), value, logical, depth),
            TypeNode::Dictionary { value: inner, .. } => {
                if let N::Mapping(items) = &self.input.nodes()[value] {
                    for &(key, item) in items {
                        let path = join(logical, &self.label(key)?);
                        self.walk(member.child(*inner), item, &path, depth + 1)?;
                    }
                }
                Ok(())
            }
            TypeNode::Name(_) => self.name(kind, value, logical, depth),
        }
    }
    /// List items are addressed by their identity field when they have one,
    /// and otherwise by position.
    fn list(
        &mut self,
        child: Member<'_>,
        value: usize,
        logical: &str,
        depth: usize,
    ) -> Result<(), NormalizationError> {
        let N::Sequence(items) = &self.input.nodes()[value] else {
            return Ok(());
        };
        let identity = match child.text() {
            // A 1.0 column entry is a column or a copy entry.
            "column_class" | "column_entry" => Some("name"),
            "row_class" | "intermediate_class" => Some("id"),
            _ => None,
        };
        for (index, &item) in items.iter().enumerate() {
            let path = if let Some(identity) =
                identity.filter(|_| matches!(self.input.nodes()[item], N::Mapping(_)))
            {
                let label = match self.input.field(item, identity) {
                    Some(id) => self.label(id)?,
                    None => format!("{index}"),
                };
                join(logical, &label)
            } else {
                format!("{logical}[{index}]")
            };
            self.walk(child, item, &path, depth + 1)?;
        }
        Ok(())
    }
    fn name(
        &mut self,
        kind: &str,
        value: usize,
        logical: &str,
        depth: usize,
    ) -> Result<(), NormalizationError> {
        if let Some(class) = self.schema.class_named(kind) {
            if self.visit.value_only(kind) {
                if let Some(child) = self.input.field(value, "value") {
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
        let Some((index, alias)) = self.schema.alias_named(kind) else {
            return Ok(());
        };
        if self.active.contains(&(value, index)) {
            return Ok(());
        }
        match alias.kind {
            SchemaAliasKind::Descriptor(id) => {
                self.active.push((value, index));
                let result = self.descriptor(id, value, logical, depth + 1);
                self.active.pop();
                result
            }
            SchemaAliasKind::Registry(id) => self.registry(id, value, logical, depth),
        }
    }
    /// A registry value is a single-key mapping whose key names its entry.
    fn registry(
        &mut self,
        id: usize,
        value: usize,
        logical: &str,
        depth: usize,
    ) -> Result<(), NormalizationError> {
        let N::Mapping(items) = &self.input.nodes()[value] else {
            return Ok(());
        };
        let &[(key, payload)] = items.as_slice() else {
            return Ok(());
        };
        let N::Text(name) = &self.input.nodes()[key] else {
            return Ok(());
        };
        let registry = &self.schema.registries()[id];
        let Some(entry) = registry.entries.iter().find(|entry| entry.name == *name) else {
            return Ok(());
        };
        let path = join(logical, name);
        self.visit.entry(&registry.name, name, payload, &path)?;
        match &entry.shape {
            SchemaShape::Class(fields) => self.fields(fields, payload, &path, depth + 1),
            SchemaShape::Descriptor(id) => self.descriptor(*id, payload, &path, depth + 1),
        }
    }
}
