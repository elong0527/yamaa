//! Schema-directed dependencies for R017 reachability, without evaluating expressions.

use super::{
    validation::matches_member, Document, DocumentNode as N, NormalizationBudget,
    NormalizationError, NormalizationResource, SchemaAliasKind, SchemaField, SchemaShape,
    SchemaStructure, TypeExpression, TypeNode, ValidationError,
};
use crate::{aggregate_parser, numeric_parser, predicate_parser};
use alloc::{format, string::String, vec::Vec};

/// Namespace selected by a schema role, rather than by the spelling of a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InheritanceReferenceKind {
    Dataset,
    Variable,
}

/// Distinct semantic reference; a record count uses the qualified spelling `D.*`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InheritanceReference {
    pub kind: InheritanceReferenceKind,
    pub name: String,
}

/// Policy refusals must not turn an expensive live expression into a dead declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InheritanceReferenceError {
    Normalization(NormalizationError),
    Numeric(numeric_parser::ParseError),
    Aggregate(aggregate_parser::ParseError),
    Predicate(predicate_parser::ParseError),
}
impl From<NormalizationError> for InheritanceReferenceError {
    fn from(error: NormalizationError) -> Self {
        Self::Normalization(error)
    }
}
impl From<ValidationError> for InheritanceReferenceError {
    fn from(error: ValidationError) -> Self {
        Self::Normalization(error.into())
    }
}

type Error = InheritanceReferenceError;
type Scope<'a> = Option<(&'a str, &'a str)>;

struct Run<'a, 'b> {
    schema: &'a SchemaStructure,
    input: &'a Document,
    budget: &'b mut NormalizationBudget,
    active: Vec<(usize, usize)>,
    references: Vec<InheritanceReference>,
}

impl Run<'_, '_> {
    fn step(&mut self, depth: usize) -> Result<(), Error> {
        let limit = self.budget.limits.depth.min(128);
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            }
            .into());
        }
        self.budget.work(1)?;
        Ok(())
    }

    fn add(&mut self, kind: InheritanceReferenceKind, name: &str) -> Result<(), Error> {
        self.budget.work(
            self.references
                .len()
                .saturating_add(1)
                .saturating_mul(name.len().saturating_add(1)),
        )?;
        if !self
            .references
            .iter()
            .any(|r| r.kind == kind && r.name == name)
        {
            self.budget.reserve(name.len(), 1)?;
            self.references.push(InheritanceReference {
                kind,
                name: name.into(),
            });
        }
        Ok(())
    }

    fn members(
        &mut self,
        members: &[TypeExpression],
        value: usize,
        scope: Scope<'_>,
        depth: usize,
    ) -> Result<(), Error> {
        self.step(depth)?;
        for member in members {
            if self.matches(member, member.root(), value)? {
                return self.walk(member, member.root(), value, scope, depth + 1);
            }
        }
        Ok(())
    }

    fn matches(
        &mut self,
        expression: &TypeExpression,
        node: usize,
        value: usize,
    ) -> Result<bool, Error> {
        Ok(matches_member(
            self.schema,
            self.input,
            (expression, node),
            value,
            false,
            &self.active,
            &mut self.budget.validation,
        )?)
    }

    fn fields(
        &mut self,
        fields: &[SchemaField],
        owner: &str,
        value: usize,
        depth: usize,
    ) -> Result<(), Error> {
        self.step(depth)?;
        let N::Mapping(entries) = &self.input.nodes()[value] else {
            return Ok(());
        };
        // Authored field order is the reference traversal's deterministic discovery order.
        for &(key, child) in entries {
            let N::Text(name) = &self.input.nodes()[key] else {
                continue;
            };
            self.budget
                .work(fields.len().saturating_mul(name.len().saturating_add(1)))?;
            if let Some(field) = fields.iter().find(|field| field.name == *name) {
                self.members(
                    self.schema.descriptors()[field.descriptor]
                        .descriptor
                        .members(),
                    child,
                    Some((owner, name)),
                    depth + 1,
                )?;
            }
        }
        Ok(())
    }

    fn walk(
        &mut self,
        expression: &TypeExpression,
        node: usize,
        value: usize,
        scope: Scope<'_>,
        depth: usize,
    ) -> Result<(), Error> {
        self.step(depth)?;
        let kind = expression
            .node_text(node)
            .ok_or(ValidationError::InvalidDescriptor)?;
        if let N::Text(text) = &self.input.nodes()[value] {
            if kind == "variable" {
                return self.add(InheritanceReferenceKind::Variable, text);
            }
            if kind == "identifier" {
                let reference_kind = match scope {
                    Some(
                        ("root_class", "base")
                        | ("row_class", "dataset")
                        | ("intermediate_class", "dataset"),
                    ) => Some(InheritanceReferenceKind::Dataset),
                    Some(
                        ("column_class", "name")
                        | ("root_class", "keys")
                        | ("output_class", "columns"),
                    ) => Some(InheritanceReferenceKind::Variable),
                    _ => None,
                };
                if let Some(reference_kind) = reference_kind {
                    self.add(reference_kind, text)?;
                }
                return Ok(());
            }
            if matches!(
                kind,
                "numeric_expression" | "aggregate_expression" | "predicate" | "string_template"
            ) {
                return self.language(kind, text);
            }
        }
        match &expression.nodes()[node] {
            TypeNode::List(inner) => {
                if let N::Sequence(items) = &self.input.nodes()[value] {
                    for &child in items {
                        if self.matches(expression, *inner, child)? {
                            self.walk(expression, *inner, child, scope, depth + 1)?;
                        }
                    }
                }
            }
            TypeNode::Dictionary { value: inner, .. } => {
                if let N::Mapping(items) = &self.input.nodes()[value] {
                    for &(_, child) in items {
                        if self.matches(expression, *inner, child)? {
                            self.walk(expression, *inner, child, scope, depth + 1)?;
                        }
                    }
                }
            }
            TypeNode::Name(_) => {
                if let Some(class) = self.schema.class_named(kind) {
                    return self.fields(&class.fields, kind, value, depth + 1);
                }
                if let Some((index, alias)) = self.schema.alias_named(kind) {
                    self.budget.work(self.active.len().saturating_add(1))?;
                    if self.active.contains(&(value, index)) {
                        return Ok(());
                    }
                    self.budget.reserve(0, 2)?;
                    self.active.push((value, index));
                    let result = match alias.kind {
                        SchemaAliasKind::Descriptor(id) => self.members(
                            self.schema.descriptors()[id].descriptor.members(),
                            value,
                            scope,
                            depth + 1,
                        ),
                        SchemaAliasKind::Registry(id) => self.registry(id, value, scope, depth + 1),
                    };
                    self.active.pop();
                    result?;
                }
            }
        }
        Ok(())
    }

    fn registry(
        &mut self,
        id: usize,
        value: usize,
        scope: Scope<'_>,
        depth: usize,
    ) -> Result<(), Error> {
        self.step(depth)?;
        let N::Mapping(items) = &self.input.nodes()[value] else {
            return Ok(());
        };
        if items.len() != 1 {
            return Ok(());
        }
        let (key, payload) = items[0];
        let N::Text(name) = &self.input.nodes()[key] else {
            return Ok(());
        };
        let entries = &self.schema.registries()[id].entries;
        self.budget
            .work(entries.len().saturating_mul(name.len().saturating_add(1)))?;
        if let Some(entry) = entries.iter().find(|e| e.name == *name) {
            match &entry.shape {
                SchemaShape::Class(fields) => self.fields(fields, name, payload, depth + 1)?,
                SchemaShape::Descriptor(id) => self.members(
                    self.schema.descriptors()[*id].descriptor.members(),
                    payload,
                    scope,
                    depth + 1,
                )?,
            }
        }
        Ok(())
    }

    fn language(&mut self, kind: &str, text: &str) -> Result<(), Error> {
        // Reserve bounded syntax scratch before parsing, even for invalid syntax.
        // Regex compilation inside predicates retains its own per-parse finite policy.
        let mut limits = numeric_parser::ParseLimits::default();
        if text.len() > limits.bytes {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::TextBytes,
                limit: limits.bytes,
            }
            .into());
        }
        limits.tokens = limits.tokens.min(text.len().saturating_add(1));
        limits.nodes = limits.nodes.min(text.len().saturating_add(1));
        self.budget
            .work(text.len().saturating_add(1).saturating_mul(16))?;
        self.budget.reserve(
            text.len(),
            limits.tokens.saturating_add(limits.nodes.saturating_mul(8)),
        )?;
        use InheritanceReferenceKind::Variable;
        match kind {
            "numeric_expression" => match numeric_parser::parse_numeric(text, limits) {
                Ok(parsed) => {
                    for name in parsed.identifiers() {
                        self.add(Variable, name)?;
                    }
                }
                Err(numeric_parser::ParseError::Grammar { .. }) => {}
                Err(error) => return Err(Error::Numeric(error)),
            },
            "aggregate_expression" => match aggregate_parser::parse_aggregate(text, limits) {
                Ok(parsed) => {
                    for name in parsed.identifiers() {
                        self.add(Variable, name)?;
                    }
                    for name in parsed.star_datasets() {
                        self.budget.reserve(name.len().saturating_add(2), 0)?;
                        self.add(Variable, &format!("{name}.*"))?;
                    }
                }
                Err(aggregate_parser::ParseError::Grammar { .. }) => {}
                Err(error) => return Err(Error::Aggregate(error)),
            },
            "predicate" => match predicate_parser::parse_predicate(text, limits) {
                Ok(parsed) => {
                    for name in parsed.identifiers() {
                        self.add(Variable, name)?;
                    }
                }
                Err(predicate_parser::ParseError::Grammar { .. }) => {}
                Err(error) => return Err(Error::Predicate(error)),
            },
            "string_template" => {
                if let Some(names) = template_names(text) {
                    for name in names {
                        self.add(Variable, name)?;
                    }
                }
            }
            _ => unreachable!("selected closed expression kind"),
        }
        Ok(())
    }
}

/// A syntax-only R012 scan. Any malformed suffix discards every earlier placeholder.
fn template_names(text: &str) -> Option<Vec<&str>> {
    let bytes = text.as_bytes();
    let mut names = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if (bytes[i] == b'{' || bytes[i] == b'}') && bytes.get(i + 1) == Some(&bytes[i]) {
            i += 2;
        } else if bytes[i] == b'}' {
            return None;
        } else if bytes[i] != b'{' {
            i += 1;
        } else {
            let end = i + 1 + bytes[i + 1..].iter().position(|&b| b == b'}')?;
            let name = &text[i + 1..end];
            if !name.split('.').all(|part| {
                let mut chars = part.bytes();
                matches!(chars.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
                    && chars.all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }) {
                return None;
            }
            names.push(name);
            i = end + 1;
        }
    }
    Some(names)
}

impl SchemaStructure {
    /// Collect first-matching schema references for reachability, without evaluating values.
    ///
    /// Structurally invalid values and malformed closed expressions contribute no references;
    /// their surviving declarations remain subject to subsequent complete validation. Resource
    /// exhaustion and unsupported parser features always return errors. Results own their names.
    pub fn inheritance_references(
        &self,
        types: &[TypeExpression],
        input: &Document,
        value: usize,
        scope: Scope<'_>,
        budget: &mut NormalizationBudget,
    ) -> Result<Vec<InheritanceReference>, InheritanceReferenceError> {
        if value >= input.nodes().len() {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut run = Run {
            schema: self,
            input,
            budget,
            active: Vec::new(),
            references: Vec::new(),
        };
        run.members(types, value, scope, 0)?;
        Ok(run.references)
    }
}
