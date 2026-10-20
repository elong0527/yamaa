//! Ordered decoded-value validation and post-resolution default validation.

use super::{
    ConstraintViolation, Document, DocumentKind, DocumentNode as N, SchemaAliasKind, SchemaClass,
    SchemaField, SchemaShape, SchemaStructure, TypeError, TypeExpression, TypeNode,
};
use crate::diagnostic::{ConditionCode, Definition};
use std::vec;

/// Value types whose bare string is the source shorthand `{source: <string>}`
/// before registry dispatch (EXPR-5).
pub(super) const SOURCE_SHORTHAND: [&str; 3] = [
    "derivation",
    "untyped_intermediate_derivation",
    "case_result",
];

/// SCHEMA-10: the field a bare union member `shorthand` fills in `class`, when
/// `shorthand` names no class and is the single type of the class's only
/// required field.
pub(super) fn shorthand_field<'s>(
    schema: &'s SchemaStructure,
    class: &'s SchemaClass,
    shorthand: &str,
) -> Option<&'s SchemaField> {
    if schema.class_named(shorthand).is_some() {
        return None;
    }
    let mut required = class
        .fields
        .iter()
        .filter(|f| schema.descriptors()[f.descriptor].descriptor.required());
    let field = required.next()?;
    if required.next().is_some() {
        return None;
    }
    let types = schema.descriptors()[field.descriptor].descriptor.members();
    match types {
        [only] if only.node_text(only.root()) == Some(shorthand) => Some(field),
        _ => None,
    }
}

/// Render a shorthand to write as compact YAML flow text; nested strings are
/// double-quoted so the result reads back as the same value.
fn flow_text(input: &Document, node: usize, level: usize) -> Result<String, ValidationError> {
    depth(level)?;
    Ok(match &input.nodes()[node] {
        N::Text(text) if level == 0 => text.clone(),
        N::Text(text) => {
            let mut quoted = String::from("\"");
            for c in text.chars() {
                match c {
                    '"' => quoted.push_str("\\\""),
                    '\\' => quoted.push_str("\\\\"),
                    '\n' => quoted.push_str("\\n"),
                    '\t' => quoted.push_str("\\t"),
                    c if c.is_control() => quoted.push_str(&format!("\\u{:04x}", c as u32)),
                    c => quoted.push(c),
                }
            }
            quoted.push('"');
            quoted
        }
        N::Integer(text) => text.clone(),
        N::Float(value) => format!("{value:?}"),
        N::Boolean(value) => value.to_string(),
        N::Null => "null".into(),
        N::Sequence(items) => {
            let items = items
                .iter()
                .map(|&item| flow_text(input, item, level + 1))
                .collect::<Result<Vec<_>, _>>()?;
            format!("[{}]", items.join(", "))
        }
        N::Mapping(items) => {
            let items = items
                .iter()
                .map(|&(key, value)| {
                    Ok(format!(
                        "{}: {}",
                        flow_text(input, key, level + 1)?,
                        flow_text(input, value, level + 1)?
                    ))
                })
                .collect::<Result<Vec<_>, ValidationError>>()?;
            format!("{{{}}}", items.join(", "))
        }
    })
}

/// Large context values remain references until core admits the complete projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaContext {
    Text(String),
    Boolean(bool),
    /// A decoded value rejected by a schema descriptor or value constraint.
    ValueKind(DocumentKind),
    /// A decoded collection or member rejected by inheritance layer admission.
    LayerKind(DocumentKind),
    Count(usize),
    Null,
    InputValue(usize),
    DescriptorValues(usize),
    DescriptorPattern(usize),
    DescriptorMinimum(usize),
    DescriptorSize(usize),
}

/// Portable validation findings retain ordered context and exact declaring paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaDiagnostic {
    pub path: String,
    pub code: ConditionCode,
    pub context: Vec<(&'static str, SchemaContext)>,
}

impl SchemaDiagnostic {
    pub fn definition(&self) -> Definition {
        self.code.definition()
    }
}

/// Each default is interpreted against its descriptor after all references resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultDiagnostics {
    pub descriptor: usize,
    pub diagnostics: Vec<SchemaDiagnostic>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("schema validation type: {0:?}")]
    Type(TypeError),
    #[error("schema validation depth: limit={limit:?}")]
    Depth { limit: usize },
    #[error("schema validation invalid descriptor")]
    InvalidDescriptor,
}

/// How YAML null reads in a decoded document (LAYER-9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NullPolicy {
    /// Null is an ordinary value, as in environments and held dictionaries.
    #[default]
    Value,
    /// A specification: null written for a class field or a mapping entry leaves
    /// it absent, and null for a required field is an invalid clear. Normalization
    /// drops the field. A keyed member itself, a list item and a registry payload
    /// such as `literal: null` are values, never clears.
    Absent,
    /// As `Absent`, except normalization keeps each null as the clear marker a
    /// composed layer applies to the value it inherits.
    Clear,
}
impl NullPolicy {
    pub(super) fn clears(self) -> bool {
        !matches!(self, Self::Value)
    }
}

/// Root fields whose members compose by identifier from a mapping; each member
/// value is a definition, never a clearing null.
pub(super) const MAPPING_MEMBERS: [&str; 2] = ["input", "windows"];

pub(super) fn diagnostic(
    path: &str,
    code: ConditionCode,
    context: Vec<(&'static str, SchemaContext)>,
) -> SchemaDiagnostic {
    SchemaDiagnostic {
        path: if path.is_empty() {
            "$".into()
        } else {
            path.into()
        },
        code,
        context,
    }
}

pub(super) fn depth(depth: usize) -> Result<(), ValidationError> {
    let limit = crate::limits::Limits::SEMANTIC_DEPTH;
    if depth > limit {
        Err(ValidationError::Depth { limit })
    } else {
        Ok(())
    }
}

/// Normalization selects only validated members and retains the caller's alias guards.
pub(super) fn matches_member(
    schema: &SchemaStructure,
    input: &Document,
    member: (&TypeExpression, usize),
    value: usize,
    fragment: bool,
    nulls: NullPolicy,
    active: &[(usize, usize)],
) -> Result<bool, ValidationError> {
    Ok(Run {
        schema,
        input,
        active: active.to_vec(),
        resolved_output: false,
        nulls,
        written: false,
    }
    .single(
        member.0,
        member.1,
        Site {
            value,
            path: "<normalization>",
            depth: 0,
            fragment,
            root: false,
            members: false,
        },
    )?
    .is_empty())
}

#[derive(Clone, Copy)]
struct Site<'a> {
    value: usize,
    path: &'a str,
    depth: usize,
    fragment: bool,
    /// The specification root, whose mapping-keyed collections hold members.
    root: bool,
    /// A mapping whose entries are keyed members rather than clearable fields.
    members: bool,
}
impl<'a> Site<'a> {
    fn child(self, value: usize, path: &str) -> Site<'_> {
        Site {
            value,
            path,
            depth: self.depth + 1,
            fragment: self.fragment,
            root: false,
            members: false,
        }
    }
    fn deeper(self) -> Self {
        Self {
            depth: self.depth + 1,
            ..self
        }
    }
}

#[derive(Clone, Copy)]
enum ConstraintOwner {
    Descriptor,
    ColumnType,
    DayRule,
    TimeRule,
}
impl ConstraintOwner {
    fn code(self, violation: ConstraintViolation) -> ConditionCode {
        match (self, violation) {
            (Self::Descriptor, ConstraintViolation::ValueNotPermitted) => {
                ConditionCode::SchemaDescriptorValueNotPermitted
            }
            (Self::Descriptor, ConstraintViolation::PatternMismatch) => {
                ConditionCode::SchemaDescriptorPatternMismatch
            }
            (Self::Descriptor, ConstraintViolation::MinimumLength) => {
                ConditionCode::SchemaDescriptorMinimumLength
            }
            (Self::Descriptor, ConstraintViolation::InvalidSize) => {
                ConditionCode::SchemaDescriptorInvalidSize
            }
            (Self::ColumnType, ConstraintViolation::ValueNotPermitted) => {
                ConditionCode::SchemaColumnTypeValueNotPermitted
            }
            (Self::ColumnType, ConstraintViolation::PatternMismatch) => {
                ConditionCode::SchemaColumnTypePatternMismatch
            }
            (Self::ColumnType, ConstraintViolation::MinimumLength) => {
                ConditionCode::SchemaColumnTypeMinimumLength
            }
            (Self::ColumnType, ConstraintViolation::InvalidSize) => {
                ConditionCode::SchemaColumnTypeInvalidSize
            }
            (Self::DayRule, ConstraintViolation::ValueNotPermitted) => {
                ConditionCode::SchemaDayRuleValueNotPermitted
            }
            (Self::DayRule, ConstraintViolation::PatternMismatch) => {
                ConditionCode::SchemaDayRulePatternMismatch
            }
            (Self::DayRule, ConstraintViolation::MinimumLength) => {
                ConditionCode::SchemaDayRuleMinimumLength
            }
            (Self::DayRule, ConstraintViolation::InvalidSize) => {
                ConditionCode::SchemaDayRuleInvalidSize
            }
            (Self::TimeRule, ConstraintViolation::ValueNotPermitted) => {
                ConditionCode::SchemaTimeRuleValueNotPermitted
            }
            (Self::TimeRule, ConstraintViolation::PatternMismatch) => {
                ConditionCode::SchemaTimeRulePatternMismatch
            }
            (Self::TimeRule, ConstraintViolation::MinimumLength) => {
                ConditionCode::SchemaTimeRuleMinimumLength
            }
            (Self::TimeRule, ConstraintViolation::InvalidSize) => {
                ConditionCode::SchemaTimeRuleInvalidSize
            }
        }
    }
}

struct Run<'a> {
    schema: &'a SchemaStructure,
    input: &'a Document,
    active: Vec<(usize, usize)>,
    resolved_output: bool,
    nulls: NullPolicy,
    /// The input is the document as its author wrote it, where a long form
    /// that only restates its shorthand fails (SCHEMA-15). A canonical document
    /// produced by normalization holds those long forms and is never checked.
    written: bool,
}

impl Run<'_> {
    fn step(&mut self, site: Site<'_>) -> Result<(), ValidationError> {
        depth(site.depth)
    }
    fn text(value: impl Into<String>) -> SchemaContext {
        SchemaContext::Text(value.into())
    }
    fn diagnostic(
        &mut self,
        site: Site<'_>,
        code: ConditionCode,
        context: Vec<(&'static str, SchemaContext)>,
    ) -> SchemaDiagnostic {
        diagnostic(site.path, code, context)
    }

    fn invalid(
        &mut self,
        site: Site<'_>,
        expected: &str,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        Ok(vec![self.diagnostic(
            site,
            ConditionCode::SchemaInvalidFieldType,
            vec![
                ("expected", Self::text(expected)),
                (
                    "actual",
                    SchemaContext::ValueKind(self.input.nodes()[site.value].kind()),
                ),
            ],
        )])
    }
    fn field(&mut self, value: usize, name: &str) -> Result<Option<usize>, ValidationError> {
        Ok(self.input.field(value, name))
    }
    fn key(&mut self, node: usize) -> Result<(String, bool), ValidationError> {
        Ok((
            super::path_render::label(self.input, node)?,
            matches!(self.input.nodes()[node], N::Integer(_) | N::Boolean(_)),
        ))
    }
    fn joined(&mut self, path: &str, member: &str, index: bool) -> Result<String, ValidationError> {
        Ok(if index {
            format!("{path}[{member}]")
        } else if path.is_empty() {
            member.into()
        } else {
            format!("{path}.{member}")
        })
    }
    fn constraints(
        &mut self,
        descriptor: usize,
        site: Site<'_>,
        owner: ConstraintOwner,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        let descriptor_ref = &self.schema.descriptors()[descriptor].descriptor;
        let violations = descriptor_ref.check_constraints(&self.input.nodes()[site.value]);
        let mut result = Vec::new();
        for violation in violations {
            let context = match violation {
                ConstraintViolation::ValueNotPermitted => vec![
                    ("value", SchemaContext::InputValue(site.value)),
                    ("permitted", SchemaContext::DescriptorValues(descriptor)),
                ],
                ConstraintViolation::PatternMismatch => vec![
                    ("value", SchemaContext::InputValue(site.value)),
                    ("pattern", SchemaContext::DescriptorPattern(descriptor)),
                ],
                ConstraintViolation::MinimumLength => {
                    vec![("minimum", SchemaContext::DescriptorMinimum(descriptor))]
                }
                ConstraintViolation::InvalidSize => {
                    vec![("size", SchemaContext::DescriptorSize(descriptor))]
                }
            };
            result.push(self.diagnostic(site, owner.code(violation), context));
        }
        Ok(result)
    }
    fn descriptor(
        &mut self,
        id: usize,
        site: Site<'_>,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.step(site)?;
        let schema = self.schema;
        let result = self.union(
            schema.descriptors()[id].descriptor.members(),
            None,
            site.deeper(),
        )?;
        if !result.is_empty() {
            return Ok(result);
        }
        self.constraints(id, site, ConstraintOwner::Descriptor)
    }
    fn class(
        &mut self,
        fields: &[SchemaField],
        name: &str,
        site: Site<'_>,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.step(site)?;
        let N::Mapping(items) = &self.input.nodes()[site.value] else {
            return self.invalid(site, name);
        };
        let mut diagnostics = Vec::new();
        for field in fields {
            let path = self.joined(site.path, &field.name, false)?;
            match self.field(site.value, &field.name)? {
                Some(value)
                    if self.nulls.clears() && matches!(self.input.nodes()[value], N::Null) =>
                {
                    // A null field is absent; a required field cannot be cleared.
                    if self.schema.descriptors()[field.descriptor]
                        .descriptor
                        .required()
                    {
                        diagnostics.push(self.diagnostic(
                            site.child(value, &path),
                            ConditionCode::SchemaLayerInvalidClear,
                            vec![("field", Self::text(&field.name))],
                        ));
                    }
                }
                Some(value) => {
                    let mut child = site.child(value, &path);
                    child.members = site.root
                        && self.nulls.clears()
                        && MAPPING_MEMBERS.contains(&field.name.as_str());
                    diagnostics.extend(self.descriptor(field.descriptor, child)?)
                }
                None if !site.fragment
                    && self.schema.descriptors()[field.descriptor]
                        .descriptor
                        .required() =>
                {
                    diagnostics.push(self.diagnostic(
                        site.child(site.value, &path),
                        if self.resolved_output && site.path.is_empty() && field.name == "output" {
                            ConditionCode::SchemaResolvedOutputRequired
                        } else {
                            ConditionCode::SchemaMissingRequiredField
                        },
                        vec![
                            ("field", Self::text(&field.name)),
                            ("class", Self::text(name)),
                        ],
                    ))
                }
                None => {}
            }
        }
        for &(key, _) in items {
            let known = matches!(&self.input.nodes()[key],N::Text(name) if fields.iter().any(|f|f.name==*name));
            if !known {
                let (key, index) = self.key(key)?;
                let path = self.joined(site.path, &key, index)?;
                diagnostics.push(self.diagnostic(
                    site.child(site.value, &path),
                    ConditionCode::SchemaUnknownField,
                    vec![("field", Self::text(&key)), ("class", Self::text(name))],
                ));
            }
        }
        Ok(diagnostics)
    }
    fn outer(
        &mut self,
        expression: &TypeExpression,
        node: usize,
        site: Site<'_>,
    ) -> Result<bool, ValidationError> {
        self.step(site)?;
        let value = &self.input.nodes()[site.value];
        match &expression.nodes()[node] {
            TypeNode::List(_) => Ok(matches!(value, N::Sequence(_))),
            TypeNode::Dictionary { .. } => Ok(matches!(value, N::Mapping(_))),
            TypeNode::Name(span) => {
                let name = &expression.expression()[span.clone()];
                let builtin = match name {
                    "str" => Some(matches!(value, N::Text(_))),
                    "int" => Some(matches!(value, N::Integer(_))),
                    "float" => Some(matches!(value, N::Integer(_) | N::Float(_))),
                    "bool" => Some(matches!(value, N::Boolean(_))),
                    "null" => Some(matches!(value, N::Null)),
                    "list" => Some(matches!(value, N::Sequence(_))),
                    "dict" => Some(matches!(value, N::Mapping(_))),
                    _ => None,
                };
                if let Some(matched) = builtin {
                    return Ok(matched);
                }
                let schema = self.schema;
                if schema.class_named(name).is_some() {
                    return Ok(matches!(value, N::Mapping(_)));
                }
                let Some((id, alias)) = schema.alias_named(name) else {
                    return Ok(false);
                };
                if self.active.contains(&(site.value, id)) {
                    return Ok(false);
                }
                match alias.kind {
                    SchemaAliasKind::Registry(_) => Ok(matches!(value, N::Mapping(_))),
                    SchemaAliasKind::Descriptor(descriptor) => {
                        self.active.push((site.value, id));
                        let result = (|| {
                            for member in schema.descriptors()[descriptor].descriptor.members() {
                                if self.outer(member, member.root(), site.deeper())? {
                                    return Ok(true);
                                }
                            }
                            Ok(false)
                        })();
                        self.active.pop();
                        result
                    }
                }
            }
        }
    }
    fn declares_written_keys(
        &mut self,
        expression: &TypeExpression,
        node: usize,
        site: Site<'_>,
    ) -> Result<bool, ValidationError> {
        let N::Mapping(items) = &self.input.nodes()[site.value] else {
            return Ok(false);
        };
        if items.is_empty() {
            return Ok(false);
        }
        let mut keys = Vec::new();
        for &(key, _) in items {
            let N::Text(name) = &self.input.nodes()[key] else {
                return Ok(false);
            };
            keys.push(name.clone());
        }
        self.declares_keys(expression, node, site, &keys)
    }
    fn declares_keys(
        &mut self,
        expression: &TypeExpression,
        node: usize,
        site: Site<'_>,
        keys: &[String],
    ) -> Result<bool, ValidationError> {
        self.step(site)?;
        let TypeNode::Name(span) = &expression.nodes()[node] else {
            return Ok(false);
        };
        let name = expression.expression()[span.clone()].to_string();
        if matches!(
            name.as_str(),
            "str" | "int" | "float" | "bool" | "null" | "list" | "dict"
        ) {
            return Ok(false);
        }
        let schema = self.schema;
        if let Some(class) = schema.class_named(&name) {
            return Ok(keys
                .iter()
                .all(|key| class.fields.iter().any(|field| &field.name == key)));
        }
        let Some((id, alias)) = schema.alias_named(&name) else {
            return Ok(false);
        };
        if self.active.contains(&(site.value, id)) {
            return Ok(false);
        }
        match alias.kind {
            SchemaAliasKind::Registry(registry) => Ok(keys.iter().all(|key| {
                schema.registries()[registry]
                    .entries
                    .iter()
                    .any(|entry| &entry.name == key)
            })),
            SchemaAliasKind::Descriptor(descriptor) => {
                self.active.push((site.value, id));
                let result = (|| {
                    for member in schema.descriptors()[descriptor].descriptor.members() {
                        if self.declares_keys(member, member.root(), site.deeper(), keys)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                })();
                self.active.pop();
                result
            }
        }
    }
    /// SCHEMA-15: after a written value matched `member` of `members`, a mapping
    /// that holds only the field a shorthand of the same union fills restates
    /// that shorthand. `alias` names the value type whose union this is, for the
    /// source shorthand of EXPR-5.
    fn restated(
        &mut self,
        members: &[TypeExpression],
        member: &TypeExpression,
        alias: Option<&str>,
        site: Site<'_>,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        if !self.written || members.len() < 2 {
            return Ok(Vec::new());
        }
        // Only a mapping that writes nothing but the shorthand's field restates
        // it; any other written field, even a null, is the author's addition.
        let N::Mapping(items) = &self.input.nodes()[site.value] else {
            return Ok(Vec::new());
        };
        let &[(key, value)] = items.as_slice() else {
            return Ok(Vec::new());
        };
        let N::Text(field) = &self.input.nodes()[key] else {
            return Ok(Vec::new());
        };
        let source = alias.is_some_and(|name| SOURCE_SHORTHAND.contains(&name));
        // The string of a written `{source: <string>}`, the source shorthand's
        // long form in a derivation or case result.
        let source_text = |node: usize| match &self.input.nodes()[node] {
            N::Mapping(items) => match items.as_slice() {
                &[(key, value)]
                    if matches!(&self.input.nodes()[key], N::Text(name) if name == "source")
                        && matches!(self.input.nodes()[value], N::Text(_)) =>
                {
                    Some(value)
                }
                _ => None,
            },
            _ => None,
        };
        let schema = self.schema;
        let class = member
            .node_text(member.root())
            .and_then(|name| schema.class_named(name));
        let shorthand = if let Some(class) = class {
            let restates = members.iter().any(|other| {
                other
                    .node_text(other.root())
                    .and_then(|text| shorthand_field(schema, class, text))
                    .is_some_and(|filled| filled.name == *field)
            });
            if !restates {
                return Ok(Vec::new());
            }
            // A restated handled expression that only copies a source names
            // the bare string a derivation writes for it.
            source
                .then(|| source_text(value))
                .flatten()
                .unwrap_or(value)
        } else if let Some(text) = source.then(|| source_text(site.value)).flatten() {
            text
        } else {
            return Ok(Vec::new());
        };
        let shorthand = match &self.input.nodes()[shorthand] {
            N::Sequence(_) | N::Mapping(_) => Self::text(flow_text(self.input, shorthand, 0)?),
            _ => SchemaContext::InputValue(shorthand),
        };
        Ok(vec![self.diagnostic(
            site,
            ConditionCode::SchemaRedundantLongForm,
            vec![
                ("field", Self::text(field.as_str())),
                ("shorthand", shorthand),
            ],
        )])
    }
    fn union(
        &mut self,
        members: &[TypeExpression],
        alias: Option<&str>,
        site: Site<'_>,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.step(site)?;
        let mut attempted = Vec::new();
        for member in members {
            if let TypeNode::Name(span) = &member.nodes()[member.root()] {
                let name = &member.expression()[span.clone()];
                if self
                    .schema
                    .alias_named(name)
                    .is_some_and(|(id, _)| self.active.contains(&(site.value, id)))
                {
                    continue;
                }
            }
            let result = self.single(member, member.root(), site.deeper())?;
            if result.is_empty() {
                return self.restated(members, member, alias, site);
            }
            attempted.push((member, result));
        }
        for (member, diagnostics) in &mut attempted {
            if self.outer(member, member.root(), site.deeper())?
                && self.declares_written_keys(member, member.root(), site.deeper())?
            {
                return Ok(std::mem::take(diagnostics));
            }
        }
        for (member, diagnostics) in &mut attempted {
            if self.outer(member, member.root(), site.deeper())? {
                return Ok(std::mem::take(diagnostics));
            }
        }
        if let Some((_, diagnostics)) = attempted.into_iter().next() {
            return Ok(diagnostics);
        }
        let expected = members
            .iter()
            .map(|m| m.expression().trim())
            .collect::<Vec<_>>()
            .join(" | ");
        self.invalid(site, &expected)
    }
    fn alias(
        &mut self,
        name: &str,
        id: usize,
        kind: SchemaAliasKind,
        site: Site<'_>,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        if self.active.contains(&(site.value, id)) {
            return self.invalid(site, name);
        }
        self.active.push((site.value, id));
        let result = (|| {
            let schema = self.schema;
            match kind {
                SchemaAliasKind::Registry(registry) => {
                    let N::Mapping(items) = &self.input.nodes()[site.value] else {
                        return self.invalid(site, name);
                    };
                    let registry = &schema.registries()[registry];
                    if items.len() != 1 {
                        return Ok(vec![self.diagnostic(
                            site,
                            ConditionCode::SchemaInvalidOperationCount,
                            vec![
                                ("registry", Self::text(&registry.name)),
                                ("count", SchemaContext::Count(items.len())),
                            ],
                        )]);
                    }
                    let (key, payload) = items[0];
                    let (operation, index) = self.key(key)?;
                    let path = self.joined(site.path, &operation, index)?;
                    let entry = if matches!(self.input.nodes()[key], N::Text(_)) {
                        registry.entries.iter().find(|e| e.name == operation)
                    } else {
                        None
                    };
                    let Some(entry) = entry else {
                        return Ok(vec![self.diagnostic(
                            site.child(site.value, &path),
                            ConditionCode::SchemaUnknownOperation,
                            vec![
                                ("registry", Self::text(&registry.name)),
                                ("operation", Self::text(&operation)),
                            ],
                        )]);
                    };
                    match &entry.shape {
                        SchemaShape::Class(fields) => {
                            self.class(fields, &entry.name, site.child(payload, &path))
                        }
                        SchemaShape::Descriptor(descriptor) => {
                            let payload = site.child(payload, &path);
                            let diagnostics = self.descriptor(*descriptor, payload)?;
                            if diagnostics.is_empty()
                                && registry.name == "expressions"
                                && entry.name == "case"
                            {
                                return self.case_items(payload);
                            }
                            Ok(diagnostics)
                        }
                    }
                }
                SchemaAliasKind::Descriptor(descriptor) => {
                    let members = schema.descriptors()[descriptor].descriptor.members();
                    let diagnostics = self.union(members, Some(name), site.deeper())?;
                    if matches!(name, "derivation" | "untyped_intermediate_derivation")
                        && !diagnostics.is_empty()
                        && !matches!(self.input.nodes()[site.value], N::Text(_) | N::Mapping(_))
                    {
                        return Ok(vec![self.diagnostic(site,ConditionCode::SchemaBareDerivationScalar,vec![("actual",SchemaContext::ValueKind(self.input.nodes()[site.value].kind())),("hint",Self::text("a bare derivation must be a string column reference such as ADSL.AGE; write a fixed value as {literal: ...}"))])]);
                    }
                    if !diagnostics.is_empty() {
                        let mut matching = false;
                        for member in members {
                            if self.outer(member, member.root(), site.deeper())? {
                                matching = true;
                                break;
                            }
                        }
                        if !matching
                            && diagnostics
                                .iter()
                                .all(|d| d.code == ConditionCode::SchemaInvalidFieldType)
                        {
                            return self.invalid(site, name);
                        }
                        return Ok(diagnostics);
                    }
                    let owner = match name {
                        "column_type" => ConstraintOwner::ColumnType,
                        "day_rule" => ConstraintOwner::DayRule,
                        "time_rule" => ConstraintOwner::TimeRule,
                        _ => ConstraintOwner::Descriptor,
                    };
                    let diagnostics = self.constraints(descriptor, site, owner)?;
                    if !diagnostics.is_empty() {
                        return Ok(diagnostics);
                    }
                    if name == "project_path" {
                        if let N::Text(written) = &self.input.nodes()[site.value] {
                            if let Some(failure) = crate::resource::written_path_failure(written) {
                                let mut path = site.path.to_string();
                                let mut context =
                                    vec![("path", SchemaContext::InputValue(site.value))];
                                if let Some(input) = site.path.strip_prefix("input.") {
                                    let dataset = input.split('.').next().unwrap_or(input);
                                    context.insert(0, ("dataset", Self::text(dataset)));
                                    if !input.contains('.') {
                                        path = self.joined(site.path, "path", false)?;
                                    }
                                }
                                return Ok(vec![self.diagnostic(
                                    site.child(site.value, &path),
                                    failure.code(),
                                    context,
                                )]);
                            }
                        }
                    }
                    Ok(diagnostics)
                }
            }
        })();
        self.active.pop();
        result
    }
    /// EXPR-7 orders the items of a `case`, which its list type cannot state.
    /// Like a descriptor constraint, it runs only after every item matched its
    /// class, so each item is either a `when`/`then` branch or an `otherwise`.
    fn case_items(&mut self, site: Site<'_>) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        let N::Sequence(items) = &self.input.nodes()[site.value] else {
            return Ok(Vec::new());
        };
        let mut fallbacks = Vec::new();
        for (index, &item) in items.iter().enumerate() {
            if self.field(item, "otherwise")?.is_some() {
                fallbacks.push((index, item));
            }
        }
        let mut diagnostics = Vec::new();
        if fallbacks.len() == items.len() {
            diagnostics.push(self.diagnostic(
                site,
                ConditionCode::SchemaCaseItems,
                vec![("expected", Self::text("at least one when/then item"))],
            ));
        }
        for (index, item) in fallbacks {
            if index + 1 == items.len() {
                continue;
            }
            let path = self.joined(site.path, &index.to_string(), true)?;
            diagnostics.push(self.diagnostic(
                site.child(item, &path),
                ConditionCode::SchemaCaseItems,
                vec![("expected", Self::text("otherwise only as the last item"))],
            ));
        }
        Ok(diagnostics)
    }
    fn single(
        &mut self,
        expression: &TypeExpression,
        node: usize,
        site: Site<'_>,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.step(site)?;
        match &expression.nodes()[node] {
            TypeNode::List(inner) => {
                let N::Sequence(items) = &self.input.nodes()[site.value] else {
                    return self.invalid(site, "list");
                };
                let mut diagnostics = Vec::new();
                for (index, &value) in items.iter().enumerate() {
                    let suffix = if matches!(self.input.nodes()[value], N::Mapping(_)) {
                        self.field(value, "name")?.or(self.field(value, "id")?)
                    } else {
                        None
                    };
                    let (suffix, index) = match suffix {
                        Some(node) => self.key(node)?,
                        None => (index.to_string(), true),
                    };
                    let path = self.joined(site.path, &suffix, index)?;
                    diagnostics.extend(self.single(
                        expression,
                        *inner,
                        site.child(value, &path),
                    )?);
                }
                Ok(diagnostics)
            }
            TypeNode::Dictionary { key, value } => {
                let N::Mapping(items) = &self.input.nodes()[site.value] else {
                    return self.invalid(site, "dict");
                };
                let mut diagnostics = Vec::new();
                for &(k, v) in items {
                    let (name, index) = self.key(k)?;
                    let keypath = self.joined(site.path, &format!("key({name})"), false)?;
                    diagnostics.extend(self.single(expression, *key, site.child(k, &keypath))?);
                    if self.nulls.clears()
                        && !site.members
                        && matches!(self.input.nodes()[v], N::Null)
                    {
                        // A null entry is absent.
                        continue;
                    }
                    let path = self.joined(site.path, &name, index)?;
                    diagnostics.extend(self.single(expression, *value, site.child(v, &path))?);
                }
                Ok(diagnostics)
            }
            TypeNode::Name(span) => {
                let name = &expression.expression()[span.clone()];
                let value = &self.input.nodes()[site.value];
                let matches = match name {
                    "str" => Some(matches!(value, N::Text(_))),
                    "int" => Some(matches!(value, N::Integer(_))),
                    "float" => Some(matches!(value, N::Integer(_) | N::Float(_))),
                    "bool" => Some(matches!(value, N::Boolean(_))),
                    "null" => Some(matches!(value, N::Null)),
                    "list" => Some(matches!(value, N::Sequence(_))),
                    "dict" => Some(matches!(value, N::Mapping(_))),
                    _ => None,
                };
                if let Some(matches) = matches {
                    return if matches {
                        Ok(Vec::new())
                    } else {
                        self.invalid(site, name)
                    };
                }
                let schema = self.schema;
                if let Some(class) = schema.class_named(name) {
                    return self.class(&class.fields, name, site.deeper());
                }
                if let Some((id, alias)) = schema.alias_named(name) {
                    return self.alias(name, id, alias.kind, site.deeper());
                }
                Err(ValidationError::InvalidDescriptor)
            }
        }
    }
}

impl SchemaStructure {
    /// Bind caller-selected type expressions to this closed snapshot before reading values.
    pub fn parse_query_types(
        &self,
        types: &[String],
    ) -> Result<Vec<TypeExpression>, ValidationError> {
        let mut result = Vec::new();
        for text in types {
            let parsed = TypeExpression::parse(text).map_err(ValidationError::Type)?;
            for name in parsed.names() {
                if !matches!(
                    name,
                    "str" | "int" | "float" | "bool" | "null" | "list" | "dict"
                ) && self.class_named(name).is_none()
                    && self.alias_named(name).is_none()
                {
                    return Err(ValidationError::InvalidDescriptor);
                }
            }
            result.push(parsed);
        }
        Ok(result)
    }
    /// Validate a projected type union without inventing an extra schema declaration.
    pub fn validate_types(
        &self,
        types: &[TypeExpression],
        input: &Document,
        value: usize,
        path: &str,
        fragment: bool,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        if value >= input.nodes().len() {
            return Err(ValidationError::InvalidDescriptor);
        }
        Run {
            schema: self,
            input,
            active: Vec::new(),
            resolved_output: false,
            nulls: NullPolicy::Value,
            written: true,
        }
        .union(
            types,
            None,
            Site {
                value,
                path,
                depth: 0,
                fragment,
                root: false,
                members: false,
            },
        )
    }
    /// Validate a raw document, checking its version before every other authored field.
    pub fn validate_document(
        &self,
        input: &Document,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.validate_document_mode(input, false, NullPolicy::Value)
    }
    /// Validate a specification, in which a null field or mapping entry is absent.
    pub fn validate_specification(
        &self,
        input: &Document,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.validate_document_mode(input, false, NullPolicy::Absent)
    }
    /// Final inheritance validation assigns the resolved-output rule to an output
    /// to which no contribution survived. Other required fields retain schema ownership.
    pub fn validate_resolved_document(
        &self,
        input: &Document,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.validate_document_mode(input, true, NullPolicy::Absent)
    }
    fn validate_document_mode(
        &self,
        input: &Document,
        resolved_output: bool,
        nulls: NullPolicy,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        let mut run = Run {
            schema: self,
            input,
            active: Vec::new(),
            resolved_output,
            nulls,
            // A resolved document is the canonical result of normalization.
            written: !resolved_output,
        };
        let site = Site {
            value: input.root(),
            path: "$",
            depth: 0,
            fragment: false,
            root: true,
            members: false,
        };
        if !matches!(&input.nodes()[input.root()],N::Mapping(items) if !items.is_empty()) {
            return run.invalid(site, &self.root_class().name);
        }
        let version = run.field(site.value, "schema_version")?;
        if !version
            .is_some_and(|id| matches!(&input.nodes()[id],N::Text(value) if value==self.version()))
        {
            let mut context = vec![("expected", Run::text(self.version()))];
            match version {
                None => context.push(("actual", SchemaContext::Null)),
                Some(id) => match &input.nodes()[id] {
                    N::Mapping(_) | N::Sequence(_) => context.push((
                        "actual_type",
                        SchemaContext::ValueKind(input.nodes()[id].kind()),
                    )),
                    _ => context.push(("actual", SchemaContext::InputValue(id))),
                },
            };
            return Ok(vec![run.diagnostic(
                Site {
                    path: "schema_version",
                    ..site
                },
                ConditionCode::SchemaVersionMismatch,
                context,
            )]);
        }
        run.class(
            &self.root_class().fields,
            &self.root_class().name,
            Site { path: "", ..site },
        )
    }
    /// Interpret an admitted descriptor against a decoded value or inherited fragment.
    pub fn validate_descriptor(
        &self,
        descriptor: usize,
        input: &Document,
        value: usize,
        path: &str,
        fragment: bool,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.validate_descriptor_with(descriptor, input, value, path, fragment, NullPolicy::Value)
    }
    /// Interpret a descriptor under an explicit null policy, as a specification field.
    pub fn validate_descriptor_with(
        &self,
        descriptor: usize,
        input: &Document,
        value: usize,
        path: &str,
        fragment: bool,
        nulls: NullPolicy,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.validate_descriptor_mode(descriptor, input, value, path, fragment, nulls, true)
    }
    /// Interpret a descriptor over a canonical document that normalization or
    /// composition produced, whose long forms restate expanded shorthand.
    pub(super) fn validate_canonical_descriptor(
        &self,
        descriptor: usize,
        input: &Document,
        value: usize,
        path: &str,
        fragment: bool,
        nulls: NullPolicy,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.validate_descriptor_mode(descriptor, input, value, path, fragment, nulls, false)
    }
    #[allow(clippy::too_many_arguments)]
    fn validate_descriptor_mode(
        &self,
        descriptor: usize,
        input: &Document,
        value: usize,
        path: &str,
        fragment: bool,
        nulls: NullPolicy,
        written: bool,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        if descriptor >= self.descriptors().len() || value >= input.nodes().len() {
            return Err(ValidationError::InvalidDescriptor);
        }
        Run {
            schema: self,
            input,
            active: Vec::new(),
            resolved_output: false,
            nulls,
            written,
        }
        .descriptor(
            descriptor,
            Site {
                value,
                path,
                depth: 0,
                fragment,
                root: false,
                members: false,
            },
        )
    }
    /// Retain successful default admission for reuse by this immutable schema snapshot.
    /// Failed findings never establish the prepared state.
    pub fn prepare_defaults(&mut self) -> Result<Vec<DefaultDiagnostics>, ValidationError> {
        if self.defaults_validated {
            return Ok(Vec::new());
        }
        let findings = self.validate_defaults()?;
        if findings.is_empty() {
            self.defaults_validated = true;
        }
        Ok(findings)
    }
    /// Validate defaults only after structural and named-reference admission succeeded.
    /// InputValue diagnostic contexts refer to the descriptor's origin module document.
    pub fn validate_defaults(&self) -> Result<Vec<DefaultDiagnostics>, ValidationError> {
        let mut result = Vec::new();
        for (descriptor, located) in self.descriptors().iter().enumerate() {
            if let Some(value) = located.descriptor.default_node() {
                let input = &self.modules()[located.module].document;
                let path = format!("{}.default", located.path);
                // A default is part of the schema, not a written document.
                let diagnostics = self.validate_canonical_descriptor(
                    descriptor,
                    input,
                    value,
                    &path,
                    false,
                    NullPolicy::Value,
                )?;
                if !diagnostics.is_empty() {
                    result.push(DefaultDiagnostics {
                        descriptor,
                        diagnostics,
                    });
                }
            }
        }
        Ok(result)
    }
}
