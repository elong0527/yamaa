//! Structural descriptor admission precedes name resolution and default validation.

use super::{Document, DocumentNode, TypeError, TypeExpression};
use crate::regex;
use std::vec;

/// Ordered structural findings retain source occurrence ids where rendering needs them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DescriptorIssue {
    ExpectedMapping,
    UnknownKeyword {
        key: usize,
    },
    MissingType,
    InvalidTypeValue,
    TypeSyntax {
        member: String,
        byte: usize,
    },
    RequiredBoolean,
    RequiredDefault,
    Description,
    PatternRequiresString,
    PatternText,
    InvalidPattern {
        pattern: String,
        byte: usize,
        reason: &'static str,
    },
    MinimumRequiresString,
    MinimumNonnegative,
    SizeRequiresCollection,
    SizeNonnegative,
    ValuesRequiresString,
    ValuesTextSequence,
}

/// Policy and unsupported-language outcomes are never structural schema diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DescriptorError {
    #[error("schema descriptor invalid node")]
    InvalidNode,
    #[error("schema descriptor type limit: {0:?}")]
    TypeLimit(TypeError),
    #[error("schema descriptor regex: {0:?}")]
    Regex(regex::CompileError),
}

/// Admitted structure; references and any default still require the complete bundle.
#[derive(Clone, Debug)]
pub struct Descriptor {
    members: Vec<TypeExpression>,
    required: bool,
    default: Option<usize>,
    permitted: Option<Vec<String>>,
    pattern: Option<(String, regex::Pattern)>,
    minimum: Option<String>,
    size: Option<String>,
}

/// A failed descriptor still contributes valid type references to bundle diagnostics.
#[derive(Clone, Debug)]
pub struct DescriptorReport {
    issues: Vec<DescriptorIssue>,
    references: Vec<String>,
    descriptor: Option<Descriptor>,
}
impl DescriptorReport {
    /// Preserve schema declaration order; no diagnostic is sorted or deduplicated here.
    pub fn issues(&self) -> &[DescriptorIssue] {
        &self.issues
    }
    /// Return unresolved occurrences, including duplicates, in written union order.
    pub fn references(&self) -> &[String] {
        &self.references
    }
    /// Only structurally valid descriptors can enter subsequent bundle resolution.
    pub fn into_descriptor(self) -> Option<Descriptor> {
        self.descriptor
    }
}

/// Match the existing reader's strip convention for authored descriptor strings.
fn trim(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

impl Descriptor {
    /// Admit closed syntax and compile patterns without resolving references or defaults.
    pub fn admit(
        document: &Document,
        node: usize,
        class_field: bool,
    ) -> Result<DescriptorReport, DescriptorError> {
        let nodes = document.nodes();
        let value = nodes.get(node).ok_or(DescriptorError::InvalidNode)?;
        let DocumentNode::Mapping(entries) = value else {
            return Ok(DescriptorReport {
                issues: vec![DescriptorIssue::ExpectedMapping],
                references: Vec::new(),
                descriptor: None,
            });
        };
        let mut issues = Vec::new();
        for &(key, _) in entries {
            let allowed = match &nodes[key] {
                DocumentNode::Text(name) => {
                    matches!(
                        name.as_str(),
                        "type"
                            | "description"
                            | "values"
                            | "pattern"
                            | "min_length"
                            | "size"
                            | "default"
                    ) || (class_field && name == "required")
                }
                _ => false,
            };
            if !allowed {
                issues.push(DescriptorIssue::UnknownKeyword { key });
            }
        }
        let field = |name| document.field(node, name);
        let Some(type_id) = field("type") else {
            issues.push(DescriptorIssue::MissingType);
            return Ok(DescriptorReport {
                issues,
                references: Vec::new(),
                descriptor: None,
            });
        };
        let spellings = match &nodes[type_id] {
            DocumentNode::Text(text) => vec![text.as_str()],
            DocumentNode::Sequence(items)
                if !items.is_empty()
                    && items
                        .iter()
                        .all(|&id| matches!(nodes[id], DocumentNode::Text(_))) =>
            {
                items
                    .iter()
                    .map(|&id| match &nodes[id] {
                        DocumentNode::Text(text) => text.as_str(),
                        _ => unreachable!(),
                    })
                    .collect()
            }
            _ => {
                issues.push(DescriptorIssue::InvalidTypeValue);
                Vec::new()
            }
        };
        let mut members = Vec::new();
        let mut references = Vec::new();
        for spelling in &spellings {
            match TypeExpression::parse(spelling) {
                Ok(parsed) => {
                    references.extend(parsed.names().map(String::from));
                    members.push(parsed);
                }
                Err(TypeError::Invalid { byte }) => issues.push(DescriptorIssue::TypeSyntax {
                    member: trim(spelling).into(),
                    byte,
                }),
                Err(error) => return Err(DescriptorError::TypeLimit(error)),
            }
        }
        let required_id = field("required");
        let required = required_id.is_some_and(|id| nodes[id] == DocumentNode::Boolean(true));
        if required_id.is_some_and(|id| !matches!(nodes[id], DocumentNode::Boolean(_))) {
            issues.push(DescriptorIssue::RequiredBoolean);
        }
        let default = field("default");
        if required && default.is_some() {
            issues.push(DescriptorIssue::RequiredDefault);
        }
        if field("description").is_some_and(
            |id| !matches!(&nodes[id], DocumentNode::Text(text) if !trim(text).is_empty()),
        ) {
            issues.push(DescriptorIssue::Description);
        }

        let string_only = spellings.len() == 1 && trim(spellings[0]) == "str";
        let sized_only = !spellings.is_empty()
            && spellings.iter().all(|text| {
                let text = trim(text);
                matches!(text, "list" | "dict")
                    || text.starts_with("list[")
                    || text.starts_with("dict[")
            });
        let mut pattern = None;
        if let Some(id) = field("pattern") {
            if !string_only {
                issues.push(DescriptorIssue::PatternRequiresString);
            }
            match &nodes[id] {
                DocumentNode::Text(text) => match regex::Pattern::compile(text) {
                    Ok(compiled) => pattern = Some((text.clone(), compiled)),
                    Err(regex::CompileError::Invalid { byte, reason }) => {
                        issues.push(DescriptorIssue::InvalidPattern {
                            pattern: text.clone(),
                            byte,
                            reason,
                        })
                    }
                    Err(error) => return Err(DescriptorError::Regex(error)),
                },
                _ => issues.push(DescriptorIssue::PatternText),
            }
        }
        let mut minimum = None;
        if let Some(id) = field("min_length") {
            if !string_only {
                issues.push(DescriptorIssue::MinimumRequiresString);
            }
            match &nodes[id] {
                DocumentNode::Integer(text) if !text.starts_with('-') => {
                    minimum = Some(text.clone())
                }
                _ => issues.push(DescriptorIssue::MinimumNonnegative),
            }
        }
        let mut size = None;
        if let Some(id) = field("size") {
            if !sized_only {
                issues.push(DescriptorIssue::SizeRequiresCollection);
            }
            match &nodes[id] {
                DocumentNode::Integer(text) if !text.starts_with('-') => size = Some(text.clone()),
                _ => issues.push(DescriptorIssue::SizeNonnegative),
            }
        }
        let mut permitted = None;
        if let Some(id) = field("values") {
            if !string_only {
                issues.push(DescriptorIssue::ValuesRequiresString);
            }
            match &nodes[id] {
                DocumentNode::Sequence(items)
                    if items
                        .iter()
                        .all(|&id| matches!(nodes[id], DocumentNode::Text(_))) =>
                {
                    permitted = Some(
                        items
                            .iter()
                            .map(|&id| match &nodes[id] {
                                DocumentNode::Text(text) => text.clone(),
                                _ => unreachable!(),
                            })
                            .collect(),
                    )
                }
                _ => issues.push(DescriptorIssue::ValuesTextSequence),
            }
        }
        let descriptor = issues.is_empty().then_some(Self {
            members,
            required,
            default,
            permitted,
            pattern,
            minimum,
            size,
        });
        Ok(DescriptorReport {
            issues,
            references,
            descriptor,
        })
    }

    /// Keep ordered union occurrences rather than collapsing equivalent named types.
    pub fn members(&self) -> &[TypeExpression] {
        &self.members
    }
    /// Requiredness is applied by the containing class, and suppressed for fragments.
    pub fn required(&self) -> bool {
        self.required
    }
    /// A default points into the same immutable schema document used for admission.
    pub fn default_node(&self) -> Option<usize> {
        self.default
    }
    /// Empty permitted lists remain a real constraint, distinct from absence.
    pub fn permitted(&self) -> Option<&[String]> {
        self.permitted.as_deref()
    }
    /// Retain exact pattern spelling alongside its precompiled portable matcher.
    pub fn pattern(&self) -> Option<(&str, &regex::Pattern)> {
        self.pattern
            .as_ref()
            .map(|(text, pattern)| (text.as_str(), pattern))
    }
    /// Decimal bounds retain arbitrary width for portable diagnostic context.
    pub fn minimum(&self) -> Option<&str> {
        self.minimum.as_deref()
    }
    /// Exact sizes are not truncated to the compiling host's pointer width.
    pub fn size(&self) -> Option<&str> {
        self.size.as_deref()
    }
    /// Compare finite document lengths with an arbitrary-width admitted minimum.
    pub fn length_meets_minimum(&self, length: usize) -> bool {
        self.minimum.as_ref().is_none_or(|bound| {
            let actual = length.to_string();
            actual.len() > bound.len() || (actual.len() == bound.len() && actual >= *bound)
        })
    }
    /// Compare size in canonical decimal form, avoiding overflow or lossy floats.
    pub fn length_matches_size(&self, length: usize) -> bool {
        self.size
            .as_ref()
            .is_none_or(|bound| length.to_string() == *bound)
    }
}
