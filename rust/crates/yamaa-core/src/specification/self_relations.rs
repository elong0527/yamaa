//! SELF preparation uses authored phases and output metadata, never study ports.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum Relation {
    Input(usize),
    SelfRows,
}

/// Supplemental template placeholders share the already-admitted parser metadata.
pub(super) fn template_references(
    d: &Document,
    value: usize,
    templates: &str_template::Prepared,
    names: &mut Vec<String>,
    depth: usize,
) -> Result<(), PrepareError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            None,
        ));
    }
    if let Some(payload) = d.field(value, "str_template") {
        names.extend(templates.get(payload)?.identifiers().iter().cloned());
    }
    if let Some(payload) = d.field(value, "case") {
        for &branch in sequence(d, payload)? {
            if let Some(child) = d
                .field(branch, "then")
                .or_else(|| d.field(branch, "otherwise"))
            {
                template_references(d, child, templates, names, depth + 1)?;
            }
        }
    }
    Ok(())
}

