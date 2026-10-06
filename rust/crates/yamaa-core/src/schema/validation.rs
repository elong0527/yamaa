//! Ordered decoded-value validation and post-resolution default validation.

use super::{
    ConstraintBudget, ConstraintError, ConstraintViolation, Document, DocumentNode as N,
    SchemaAliasKind, SchemaField, SchemaShape, SchemaStructure, TypeExpression, TypeNode,
};
use crate::regex::MatchLimits;
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

/// Large context values remain references until the adapter's bounded serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaContext {
    Text(String),
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
    pub condition: &'static str,
    pub requirement: Option<&'static str>,
    pub context: Vec<(&'static str, SchemaContext)>,
}

/// Each default is interpreted against its descriptor after all references resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultDiagnostics {
    pub descriptor: usize,
    pub diagnostics: Vec<SchemaDiagnostic>,
}

/// Logical policies are independent of authored schema constraints.
#[derive(Clone, Copy, Debug)]
pub struct ValidationLimits {
    pub work: usize,
    pub diagnostics: usize,
    pub diagnostic_text_bytes: usize,
    pub depth: usize,
    pub regex: MatchLimits,
}
impl Default for ValidationLimits {
    /// Cap recursive alias/type traversal even when the decoded value itself is shallow.
    fn default() -> Self {
        Self {
            work: 4_194_304,
            diagnostics: 65_536,
            diagnostic_text_bytes: 8_388_608,
            depth: 128,
            regex: MatchLimits::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationError {
    Constraint(ConstraintError),
    Depth { limit: usize },
    Diagnostics { limit: usize },
    DiagnosticText { limit: usize },
    InvalidDescriptor,
}

/// Reuse a request scope across defaults, documents and normalization selection checks.
pub struct ValidationBudget {
    limits: ValidationLimits,
    constraints: ConstraintBudget,
    diagnostics: usize,
    text: usize,
}
impl ValidationBudget {
    /// A new attempt owns fresh accounting; individual branches cannot reset it.
    pub fn new(limits: ValidationLimits) -> Self {
        Self {
            limits,
            constraints: ConstraintBudget::new(limits.work, limits.regex),
            diagnostics: 0,
            text: 0,
        }
    }
    /// Expose charged work without allowing callers to refund failed union attempts.
    pub fn work_used(&self) -> usize {
        self.constraints.used().work
    }
    /// Every attempted diagnostic consumes its allocation quota, even if a later union wins.
    pub fn diagnostics_used(&self) -> usize {
        self.diagnostics
    }
    pub(super) fn work(&mut self, amount: usize) -> Result<(), ValidationError> {
        self.constraints
            .charge(amount)
            .map_err(ValidationError::Constraint)
    }
}

/// Normalization selects only validated members and retains the caller's alias guards.
pub(super) fn matches_member(
    schema: &SchemaStructure,
    input: &Document,
    member: (&TypeExpression, usize),
    value: usize,
    fragment: bool,
    active: &[(usize, usize)],
    budget: &mut ValidationBudget,
) -> Result<bool, ValidationError> {
    budget.work(active.len())?;
    Ok(Run {
        schema,
        input,
        budget,
        active: active.to_vec(),
    }
    .single(
        member.0,
        member.1,
        Site {
            value,
            path: "<normalization>",
            depth: 0,
            fragment,
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
}
impl<'a> Site<'a> {
    fn child<'b>(self, value: usize, path: &'b str) -> Site<'b> {
        Site {
            value,
            path,
            depth: self.depth + 1,
            fragment: self.fragment,
        }
    }
    fn deeper(self) -> Self {
        Self {
            depth: self.depth + 1,
            ..self
        }
    }
}

struct Run<'a, 'b> {
    schema: &'a SchemaStructure,
    input: &'a Document,
    budget: &'b mut ValidationBudget,
    active: Vec<(usize, usize)>,
}

/// Python-compatible finite float spelling for diagnostic mapping keys, not value conversion.
fn float_key(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .into();
    }
    let mut buffer = ryu::Buffer::new();
    let text = buffer.format_finite(value.abs());
    let (coefficient, exponent) = text.split_once('e').map_or((text, 0), |(a, b)| {
        (a, b.parse::<i32>().expect("finite exponent"))
    });
    let point = coefficient.find('.').unwrap_or(coefficient.len()) as i32;
    let digits: String = coefficient.chars().filter(|&c| c != '.').collect();
    let leading = digits.bytes().take_while(|&b| b == b'0').count();
    let power = point + exponent - leading as i32 - 1;
    let digits = digits[leading..].trim_end_matches('0');
    let mut result = if value.is_sign_negative() {
        String::from("-")
    } else {
        String::new()
    };
    if (-4..16).contains(&power) {
        let point = power + 1;
        if point <= 0 {
            result.push_str("0.");
            for _ in 0..-point {
                result.push('0');
            }
            result.push_str(digits);
        } else if point as usize >= digits.len() {
            result.push_str(digits);
            for _ in digits.len()..point as usize {
                result.push('0');
            }
            result.push_str(".0");
        } else {
            let point = point as usize;
            result.push_str(&digits[..point]);
            result.push('.');
            result.push_str(&digits[point..]);
        }
    } else {
        result.push_str(&digits[..1]);
        if digits.len() > 1 {
            result.push('.');
            result.push_str(&digits[1..]);
        }
        result.push('e');
        result.push(if power < 0 { '-' } else { '+' });
        result.push_str(&format!("{:02}", power.abs()));
    }
    result
}

impl Run<'_, '_> {
    fn step(&mut self, site: Site<'_>) -> Result<(), ValidationError> {
        let limit = self.budget.limits.depth.min(128);
        if site.depth > limit {
            return Err(ValidationError::Depth { limit });
        }
        self.budget.work(1)
    }
    fn text(value: impl Into<String>) -> SchemaContext {
        SchemaContext::Text(value.into())
    }
    fn diagnostic(
        &mut self,
        site: Site<'_>,
        condition: &'static str,
        requirement: Option<&'static str>,
        context: Vec<(&'static str, SchemaContext)>,
    ) -> Result<SchemaDiagnostic, ValidationError> {
        self.budget.diagnostics = self
            .budget
            .diagnostics
            .checked_add(1)
            .filter(|n| *n <= self.budget.limits.diagnostics)
            .ok_or(ValidationError::Diagnostics {
                limit: self.budget.limits.diagnostics,
            })?;
        let size = context
            .iter()
            .try_fold(site.path.len(), |total, (key, value)| {
                total.checked_add(key.len()).and_then(|n| {
                    n.checked_add(match value {
                        SchemaContext::Text(s) => s.len(),
                        _ => 0,
                    })
                })
            })
            .ok_or(ValidationError::DiagnosticText {
                limit: self.budget.limits.diagnostic_text_bytes,
            })?;
        self.budget.text = self
            .budget
            .text
            .checked_add(size)
            .filter(|n| *n <= self.budget.limits.diagnostic_text_bytes)
            .ok_or(ValidationError::DiagnosticText {
                limit: self.budget.limits.diagnostic_text_bytes,
            })?;
        Ok(SchemaDiagnostic {
            path: if site.path.is_empty() {
                "$".into()
            } else {
                site.path.into()
            },
            condition,
            requirement,
            context,
        })
    }
    fn invalid(
        &mut self,
        site: Site<'_>,
        expected: &str,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        Ok(vec![self.diagnostic(
            site,
            "invalid_field_type",
            Some("REQ-0287"),
            vec![
                ("expected", Self::text(expected)),
                (
                    "actual",
                    Self::text(self.input.nodes()[site.value].type_name()),
                ),
            ],
        )?])
    }
    fn field(&mut self, value: usize, name: &str) -> Result<Option<usize>, ValidationError> {
        self.budget.work(match &self.input.nodes()[value] {
            N::Mapping(items) => items.len().saturating_mul(name.len().saturating_add(1)),
            _ => 1,
        })?;
        Ok(self.input.field(value, name))
    }
    fn key(&mut self, node: usize) -> Result<(String, bool), ValidationError> {
        let value = &self.input.nodes()[node];
        self.budget.work(match value {
            N::Text(s) | N::Integer(s) => s.len(),
            _ => 1,
        })?;
        Ok(match value {
            N::Text(s) => (s.clone(), false),
            N::Integer(s) => (s.clone(), true),
            N::Boolean(v) => (if *v { "True" } else { "False" }.into(), true),
            N::Null => ("None".into(), false),
            N::Float(v) => (float_key(*v), false),
            _ => return Err(ValidationError::InvalidDescriptor),
        })
    }
    fn joined(&mut self, path: &str, member: &str, index: bool) -> Result<String, ValidationError> {
        self.budget
            .work(path.len().saturating_add(member.len()).saturating_add(2))?;
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
        requirement: &'static str,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        let descriptor_ref = &self.schema.descriptors()[descriptor].descriptor;
        let violations = descriptor_ref
            .check_constraints(
                &self.input.nodes()[site.value],
                self.budget.limits.regex,
                &mut self.budget.constraints,
            )
            .map_err(ValidationError::Constraint)?;
        let mut result = Vec::new();
        for violation in violations {
            let (condition, context) = match violation {
                ConstraintViolation::ValueNotPermitted => (
                    "value_not_permitted",
                    vec![
                        ("value", SchemaContext::InputValue(site.value)),
                        ("permitted", SchemaContext::DescriptorValues(descriptor)),
                    ],
                ),
                ConstraintViolation::PatternMismatch => (
                    "pattern_mismatch",
                    vec![
                        ("value", SchemaContext::InputValue(site.value)),
                        ("pattern", SchemaContext::DescriptorPattern(descriptor)),
                    ],
                ),
                ConstraintViolation::MinimumLength => (
                    "minimum_length",
                    vec![("minimum", SchemaContext::DescriptorMinimum(descriptor))],
                ),
                ConstraintViolation::InvalidSize => (
                    "invalid_size",
                    vec![("size", SchemaContext::DescriptorSize(descriptor))],
                ),
            };
            result.push(self.diagnostic(site, condition, Some(requirement), context)?);
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
        let result = self.union(schema.descriptors()[id].descriptor.members(), site.deeper())?;
        if !result.is_empty() {
            return Ok(result);
        }
        self.constraints(id, site, "REQ-0287")
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
            self.budget.work(1)?;
            let path = self.joined(site.path, &field.name, false)?;
            match self.field(site.value, &field.name)? {
                Some(value) => {
                    diagnostics.extend(self.descriptor(field.descriptor, site.child(value, &path))?)
                }
                None if !site.fragment
                    && self.schema.descriptors()[field.descriptor]
                        .descriptor
                        .required() =>
                {
                    diagnostics.push(self.diagnostic(
                        site.child(site.value, &path),
                        "missing_required_field",
                        None,
                        vec![
                            ("field", Self::text(&field.name)),
                            ("class", Self::text(name)),
                        ],
                    )?)
                }
                None => {}
            }
        }
        for &(key, _) in items {
            let name_bytes = match &self.input.nodes()[key] {
                N::Text(name) => name.len(),
                _ => 0,
            };
            self.budget
                .work(fields.len().saturating_mul(name_bytes.saturating_add(1)))?;
            let known = matches!(&self.input.nodes()[key],N::Text(name) if fields.iter().any(|f|f.name==*name));
            if !known {
                let (key, index) = self.key(key)?;
                let path = self.joined(site.path, &key, index)?;
                diagnostics.push(self.diagnostic(
                    site.child(site.value, &path),
                    "unknown_field",
                    None,
                    vec![("field", Self::text(key)), ("class", Self::text(name))],
                )?);
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
    fn union(
        &mut self,
        members: &[TypeExpression],
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
                return Ok(result);
            }
            attempted.push((member, result));
        }
        for (member, diagnostics) in &mut attempted {
            if self.outer(member, member.root(), site.deeper())? {
                return Ok(core::mem::take(diagnostics));
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
                            "invalid_operation_count",
                            None,
                            vec![
                                ("registry", Self::text(&registry.name)),
                                ("count", SchemaContext::Count(items.len())),
                            ],
                        )?]);
                    }
                    let (key, payload) = items[0];
                    let (operation, index) = self.key(key)?;
                    let path = self.joined(site.path, &operation, index)?;
                    self.budget.work(
                        registry
                            .entries
                            .len()
                            .saturating_mul(operation.len().saturating_add(1)),
                    )?;
                    let entry = if matches!(self.input.nodes()[key], N::Text(_)) {
                        registry.entries.iter().find(|e| e.name == operation)
                    } else {
                        None
                    };
                    let Some(entry) = entry else {
                        return Ok(vec![self.diagnostic(
                            site.child(site.value, &path),
                            "unknown_operation",
                            None,
                            vec![
                                ("registry", Self::text(&registry.name)),
                                ("operation", Self::text(operation)),
                            ],
                        )?]);
                    };
                    match &entry.shape {
                        SchemaShape::Class(fields) => {
                            self.class(fields, &entry.name, site.child(payload, &path))
                        }
                        SchemaShape::Descriptor(descriptor) => {
                            self.descriptor(*descriptor, site.child(payload, &path))
                        }
                    }
                }
                SchemaAliasKind::Descriptor(descriptor) => {
                    let members = schema.descriptors()[descriptor].descriptor.members();
                    let diagnostics = self.union(members, site.deeper())?;
                    if name == "derivation"
                        && !diagnostics.is_empty()
                        && !matches!(self.input.nodes()[site.value], N::Text(_) | N::Mapping(_))
                    {
                        return Ok(vec![self.diagnostic(site,"bare_derivation_scalar",Some("REQ-0320"),vec![("actual",Self::text(self.input.nodes()[site.value].type_name())),("hint",Self::text("a bare derivation must be a string column reference such as ADSL.AGE; write a fixed value as {literal: ...}"))])?]);
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
                                .all(|d| d.condition == "invalid_field_type")
                        {
                            return self.invalid(site, name);
                        }
                        return Ok(diagnostics);
                    }
                    let requirement = match name {
                        "column_type" => "REQ-0012",
                        "day_rule" => "REQ-0609",
                        "time_rule" => "REQ-1184",
                        _ => "REQ-0287",
                    };
                    self.constraints(descriptor, site, requirement)
                }
            }
        })();
        self.active.pop();
        result
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
    /// Validate a raw document, checking its version before every other authored field.
    pub fn validate_document(
        &self,
        input: &Document,
        budget: &mut ValidationBudget,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        let mut run = Run {
            schema: self,
            input,
            budget,
            active: Vec::new(),
        };
        let site = Site {
            value: input.root(),
            path: "$",
            depth: 0,
            fragment: false,
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
                    N::Mapping(_) | N::Sequence(_) => {
                        context.push(("actual_type", Run::text(input.nodes()[id].type_name())))
                    }
                    _ => context.push(("actual", SchemaContext::InputValue(id))),
                },
            };
            return Ok(vec![run.diagnostic(
                Site {
                    path: "schema_version",
                    ..site
                },
                "schema_version_mismatch",
                None,
                context,
            )?]);
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
        budget: &mut ValidationBudget,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        if descriptor >= self.descriptors().len() || value >= input.nodes().len() {
            return Err(ValidationError::InvalidDescriptor);
        }
        Run {
            schema: self,
            input,
            budget,
            active: Vec::new(),
        }
        .descriptor(
            descriptor,
            Site {
                value,
                path,
                depth: 0,
                fragment,
            },
        )
    }
    /// Validate defaults only after structural and named-reference admission succeeded.
    /// InputValue diagnostic contexts refer to the descriptor's origin module document.
    pub fn validate_defaults(
        &self,
        budget: &mut ValidationBudget,
    ) -> Result<Vec<DefaultDiagnostics>, ValidationError> {
        let mut result = Vec::new();
        for (descriptor, located) in self.descriptors().iter().enumerate() {
            budget.work(1)?;
            if let Some(value) = located.descriptor.default_node() {
                let input = &self.modules()[located.module].document;
                let path = format!("{}.default", located.path);
                let diagnostics =
                    self.validate_descriptor(descriptor, input, value, &path, false, budget)?;
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
