//! Schema-directed named-window expansion, without host values or filesystem IO.

use super::{
    scalar_diagnostic_label, validation::matches_member, Document, DocumentNode as N,
    NormalizationBudget, NormalizationError, NormalizationResource, SchemaAliasKind, SchemaContext,
    SchemaDiagnostic, SchemaField, SchemaShape, SchemaStructure, TypeExpression, TypeNode,
    ValidationError,
};
use alloc::{collections::BTreeMap, format, string::String, vec, vec::Vec};

/// Logical use-site and final definition; hosts may attach their retained layer origins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowReference {
    pub path: String,
    pub definition: String,
}

/// Every expanded occurrence owns its children. Origins address the retained input tree.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpandedWindows {
    pub document: Document,
    /// A replacement root keeps the reference origin; its children name definition nodes.
    pub origins: Vec<usize>,
    /// Schema traversal order, independent of the input mapping's storage order.
    pub references: Vec<WindowReference>,
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
    strict: bool,
    definitions: Option<usize>,
    replacements: BTreeMap<usize, usize>,
    references: Vec<WindowReference>,
    diagnostics: Vec<SchemaDiagnostic>,
    active: Vec<(usize, usize)>,
    nodes: Vec<N>,
    origins: Vec<usize>,
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
        self.budget.work(value.length().unwrap_or(1))?;
        let label = scalar_diagnostic_label(value).ok_or(ValidationError::InvalidDescriptor)?;
        self.budget.validation.text(label.len())?;
        Ok(label)
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
    fn reference(
        &mut self,
        value: usize,
        logical: &str,
        name: &str,
    ) -> Result<(), NormalizationError> {
        let definition = match self.definitions {
            Some(id) => self.field(id, name)?,
            None => None,
        };
        if let Some(definition) = definition {
            // Account for lookup and retained logical metadata before allocation.
            self.budget
                .work(self.replacements.len().saturating_add(1))?;
            self.budget
                .validation
                .text(logical.len().saturating_add(name.len()))?;
            self.replacements.insert(value, definition);
            self.references.push(WindowReference {
                path: logical.into(),
                definition: name.into(),
            });
        } else if self.strict {
            self.budget
                .validation
                .text(logical.len().saturating_add(name.len()))?;
            let mut path = logical.replace(".derivation.value.", ".derivation.");
            if path.ends_with(".derivation.value") {
                path.truncate(path.len() - ".value".len());
            }
            self.diagnostics.push(self.budget.validation.diagnostic(
                &path,
                "unknown_window",
                Some("REQ-1253"),
                vec![("window", SchemaContext::Text(name.into()))],
            )?);
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
        if kind == "window_selection" {
            if let N::Text(name) = &self.input.nodes()[value] {
                return self.reference(value, logical, name);
            }
        }
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
    fn copy(
        &mut self,
        value: usize,
        depth: usize,
        replace: bool,
    ) -> Result<usize, NormalizationError> {
        self.step(depth)?;
        // A definition is copied whole, never recursively treated as another reference.
        let source = if replace {
            self.replacements.get(&value).copied().unwrap_or(value)
        } else {
            value
        };
        let expanded = source != value;
        let node = match &self.input.nodes()[source] {
            N::Text(text) | N::Integer(text) => {
                self.budget.work(text.len())?;
                self.budget.reserve(text.len(), 0)?;
                self.input.nodes()[source].clone()
            }
            N::Sequence(items) => {
                self.budget.reserve(0, items.len())?;
                let mut output = Vec::with_capacity(items.len());
                for &item in items {
                    output.push(self.copy(item, depth + 1, replace && !expanded)?);
                }
                N::Sequence(output)
            }
            N::Mapping(items) => {
                // Reserving the omitted root definition's edges is conservative.
                self.budget.reserve(0, items.len().saturating_mul(2))?;
                let mut output = Vec::with_capacity(items.len());
                for &(key, item) in items {
                    if self.strict
                        && value == self.input.root()
                        && matches!(&self.input.nodes()[key], N::Text(name) if name == "windows")
                    {
                        continue;
                    }
                    output.push((
                        self.copy(key, depth + 1, false)?,
                        self.copy(item, depth + 1, replace && !expanded)?,
                    ));
                }
                N::Mapping(output)
            }
            node => {
                self.budget.reserve(0, 0)?;
                node.clone()
            }
        };
        let result = self.nodes.len();
        self.nodes.push(node);
        self.origins.push(value);
        Ok(result)
    }
}

impl SchemaStructure {
    /// Expand normalized composed documents without changing their caller scope.
    /// Non-strict mode preserves unknown references for inherited-declaration pruning.
    /// Strict mode diagnoses surviving unknown names and removes root definitions.
    /// Definitions are validated even when unused. Failed attempts retain budget charges.
    pub fn expand_named_windows(
        &self,
        input: &Document,
        strict: bool,
        budget: &mut NormalizationBudget,
    ) -> Result<ExpandedWindows, NormalizationError> {
        let mut run = Run {
            schema: self,
            input,
            budget,
            strict,
            definitions: None,
            replacements: BTreeMap::new(),
            references: Vec::new(),
            diagnostics: Vec::new(),
            active: Vec::new(),
            nodes: Vec::new(),
            origins: Vec::new(),
        };
        if !matches!(input.nodes()[input.root()], N::Mapping(_)) {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        run.definitions = run.field(input.root(), "windows")?;
        if let Some(value) = run.definitions {
            run.budget.work(self.root_class().fields.len())?;
            let field = self
                .root_class()
                .fields
                .iter()
                .find(|field| field.name == "windows")
                .ok_or(ValidationError::InvalidDescriptor)?;
            let diagnostics = self.validate_descriptor(
                field.descriptor,
                input,
                value,
                "windows",
                false,
                &mut run.budget.validation,
            )?;
            if !diagnostics.is_empty() {
                return Err(NormalizationError::Invalid(diagnostics));
            }
        }
        run.fields(&self.root_class().fields, input.root(), "", 0)?;
        if !run.diagnostics.is_empty() {
            return Err(NormalizationError::Invalid(run.diagnostics));
        }
        let root = run.copy(input.root(), 1, true)?;
        let document = Document::new(run.nodes, root, run.budget.limits.storage)
            .map_err(NormalizationError::Document)?;
        Ok(ExpandedWindows {
            document,
            origins: run.origins,
            references: run.references,
        })
    }
}
