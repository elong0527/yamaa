//! SELF preparation uses authored phases and output metadata, never study ports.
use super::*;
use crate::dataset::{Intermediate, SecondarySource};

#[derive(Clone, Copy, Debug)]
pub(super) enum Relation {
    Input(usize),
    SelfRows,
}

/// Donor reads and selector fields promote row-local defaults through the normal graph.
pub(super) fn donor_defaults(
    d: &Document,
    output: &TableSchema,
    project: Option<&functions::Prepared>,
    named: &intermediates::Declarations,
    templates: &str_template::Prepared,
) -> Result<std::collections::BTreeSet<usize>, PrepareError> {
    let mut fields = std::collections::BTreeSet::new();
    let mut register = |field: &str| {
        if let Some(column) = output
            .columns()
            .iter()
            .position(|column| column.name == field)
        {
            fields.insert(column);
        }
    };
    for field in named.self_donor_fields() {
        register(field);
    }
    let mut inspect = |declaration: usize| -> Result<(), PrepareError> {
        let mut names = Vec::new();
        derivations::references(d, field(d, declaration, "value")?, project, &mut names, 0)?;
        template_references(d, field(d, declaration, "value")?, templates, &mut names, 0)?;
        for name in names {
            if let Some((index, field)) = named.reference(&name) {
                if named.is_self(index) {
                    register(field);
                }
            }
        }
        Ok(())
    };
    for &column in sequence(d, field(d, d.root(), "columns")?)? {
        if let Some(derivation) = d
            .field(column, "derivation")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            inspect(derivation)?;
        }
    }
    for &row in sequence(d, field(d, d.root(), "rows")?)? {
        for &(_, declaration) in mapping(d, field(d, row, "derivations")?)? {
            inspect(declaration)?;
        }
    }
    Ok(fields)
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

/// A column default reading SELF retains its completed-donor phase (REQ-0120).
pub(super) fn is_self_reference(d: &Document, name: &str) -> Result<bool, PrepareError> {
    let Some((relation, _)) = name.split_once('.') else {
        return Ok(false);
    };
    let Some(items) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(false);
    };
    for &item in sequence(d, items)? {
        if text(d, field(d, item, "id")?)? == relation {
            return Ok(text(d, field(d, item, "dataset")?)? == "SELF");
        }
    }
    Ok(false)
}
