//! SELF preparation uses authored phases and output metadata, never study ports.
use super::*;
use crate::dataset::{Intermediate, SecondarySource};

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

/// One named-read hook shared by row numeric, cases and first-available operands.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_operand(
    reference: &str,
    path: &str,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    output: &TableSchema,
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    items: &[Option<Intermediate>],
) -> Result<Option<Expression>, BindError> {
    if named.reference(reference).is_some()
        && reference
            .split_once('.')
            .is_none_or(|(owner, _)| owner != driver)
    {
        row_intermediates::bind(
            reference, path, named, secondary, items, output, catalog, row, available, edges,
            findings,
        )
    } else {
        rounding::bind_row_operand(
            reference, path, catalog, driver, row, phase, available, edges, findings,
        )
    }
}
