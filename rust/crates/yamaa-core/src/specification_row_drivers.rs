//! Build per-template stored-source metadata through the existing source-port ordering.
use super::*;
pub(super) fn catalog(
    output: &TableSchema,
    source: &TableSchema,
    name: &str,
) -> Result<Catalog, BindError> {
    catalog_visible(output, source, name, None)
}
pub(super) fn catalog_visible(
    output: &TableSchema,
    source: &TableSchema,
    name: &str,
    item: Option<&crate::dataset::Intermediate>,
) -> Result<Catalog, BindError> {
    let outputs = output
        .columns()
        .iter()
        .map(|c| reference_binding::Field {
            name: &c.name,
            column_type: c.kind,
        })
        .collect::<Vec<_>>();
    let names = source
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            if item.is_none_or(|item| item.admits_column(index)) {
                column.name.clone()
            } else {
                format!("\0hidden{index}")
            }
        })
        .collect::<Vec<_>>();
    let fields = source
        .columns()
        .iter()
        .zip(&names)
        .map(|(column, name)| reference_binding::Field {
            name,
            column_type: column.kind,
        })
        .collect::<Vec<_>>();
    Catalog::compile(
        &outputs,
        &[reference_binding::Dataset {
            name,
            fields: &fields,
        }],
        Default::default(),
    )
    .map_err(BindError::Catalog)
}
pub(super) fn joined_catalog(
    output: &TableSchema,
    primary: &TableSchema,
    name: &str,
    keys: &[usize],
    secondary: &[crate::dataset::SecondarySource],
    driver: &str,
) -> Result<Catalog, BindError> {
    let selected = if name == driver {
        primary
    } else {
        &secondary
            .iter()
            .find(|source| source.name == driver)
            .ok_or(BindError::Internal)?
            .schema
    };
    let catalog = catalog(output, selected, driver)?;
    extend_catalog(catalog, primary, name, keys, secondary, driver)
}
pub(super) fn extend_catalog(
    catalog: Catalog,
    primary: &TableSchema,
    name: &str,
    keys: &[usize],
    secondary: &[crate::dataset::SecondarySource],
    driver: &str,
) -> Result<Catalog, BindError> {
    let fields = core::iter::once(primary)
        .chain(secondary.iter().map(|source| &source.schema))
        .map(|schema| {
            schema
                .columns()
                .iter()
                .map(|column| reference_binding::Field {
                    name: &column.name,
                    column_type: column.kind,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let relations = core::iter::once(name)
        .chain(secondary.iter().map(|source| source.name.as_str()))
        .enumerate()
        .filter(|(_, name)| *name != driver)
        .map(|(index, name)| {
            (
                index,
                reference_binding::Dataset {
                    name,
                    fields: &fields[index],
                },
            )
        })
        .collect::<Vec<_>>();
    catalog
        .with_joined(&relations, keys)
        .map_err(BindError::Catalog)
}
pub(super) fn select<'a>(
    source: &'a TableSchema,
    primary: &str,
    secondary: &'a [crate::dataset::SecondarySource],
    selected: &str,
    named: &intermediates::Declarations,
    items: &'a [Option<crate::dataset::Intermediate>],
) -> Result<(usize, &'a TableSchema, Option<usize>), BindError> {
    if selected == primary {
        return Ok((0, source, None));
    }
    if let Some((index, _)) = named.reference(&format!("{selected}.driver")) {
        let item = items
            .get(index)
            .and_then(Option::as_ref)
            .ok_or(BindError::Internal)?;
        let source = match item.source {
            crate::dataset::IntermediateSource::Input(index) => index + 1,
            crate::dataset::IntermediateSource::Primary { .. } => 0,
            _ => {
                return Err(BindError::InvalidPlan(
                    crate::dataset::PlanError::InvalidIntermediate,
                ))
            }
        };
        return Ok((
            source,
            item.source_schema(secondary).ok_or(BindError::Internal)?,
            Some(index),
        ));
    }
    secondary
        .iter()
        .enumerate()
        .find(|(_, item)| item.name == selected)
        .map(|(index, item)| (index + 1, &item.schema, None))
        .ok_or(BindError::Internal)
}

/// Admit reference traversal before retaining every expanded row occurrence.
#[allow(clippy::too_many_arguments)]
pub(super) fn admit_reads(
    d: &Document,
    declaration: usize,
    _driver: Option<&str>,
    _path: &str,
    project: Option<&functions::Prepared>,
    templates: &str_template::Prepared,
    named: &intermediates::Declarations,
) -> Result<(), PrepareError> {
    fn template_names(
        d: &Document,
        value: usize,
        templates: &str_template::Prepared,
        names: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), PrepareError> {
        if depth > 64 {
            return Err(PrepareError::Limit("expression_depth"));
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
                    template_names(d, child, templates, names, depth + 1)?;
                }
            }
        }
        if let Some(payload) = d.field(value, "str_concat") {
            for &child in sequence(d, field(d, payload, "sources")?)? {
                template_names(d, child, templates, names, depth + 1)?;
            }
        }
        Ok(())
    }
    let value = field(d, declaration, "value")?;
    let mut names = Vec::new();
    derivations::current_references(d, value, project, &mut names, 0)?;
    template_names(d, value, templates, &mut names, 0)?;
    let matching = names
        .iter()
        .filter_map(|name| named.reference(name))
        .flat_map(|(index, _)| named.match_references(index))
        .map(String::from)
        .collect::<Vec<_>>();
    names.extend(matching);
    Ok(())
}
