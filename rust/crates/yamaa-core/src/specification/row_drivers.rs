//! Build per-template stored-source metadata through the existing source-port ordering.
use super::*;
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
    )
    .map_err(BindError::Catalog)
}
/// The whole-dataset catalog: the primary record is current, and every other
/// stored source joins through the output keys.
pub(super) fn joined_catalog(
    output: &TableSchema,
    primary: &TableSchema,
    name: &str,
    keys: &[usize],
    secondary: &[crate::dataset::SecondarySource],
) -> Result<Catalog, BindError> {
    let catalog = catalog_visible(output, primary, name, None)?;
    extend_catalog(catalog, primary, name, keys, secondary, name)
}
pub(super) fn extend_catalog(
    catalog: Catalog,
    primary: &TableSchema,
    name: &str,
    keys: &[usize],
    secondary: &[crate::dataset::SecondarySource],
    driver: &str,
) -> Result<Catalog, BindError> {
    let fields = std::iter::once(primary)
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
    let relations = std::iter::once(name)
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

/// Whether a column completed only for rows the named intermediate `driver`
/// builds reads a field of it that the column phase cannot address. That
/// phase reads a driver record from the input behind the first row template's
/// driver (`primary`), at the position of the record that built the row. An
/// intermediate over another input, or a field it derives, has no record
/// there, and a column holding a qualified aggregate keeps the whole-dataset
/// scope in which the intermediate is no driver. Preparation refuses such a
/// read before any data is read, so `check` and `domain` report it alike.
pub(super) fn unaddressable(
    d: &Document,
    declaration: usize,
    driver: &str,
    primary: &str,
    aggregate: bool,
    project: Option<&functions::Prepared>,
    templates: &str_template::Prepared,
) -> Result<bool, PrepareError> {
    /// The imputation bounds and fallback name variables the shared reference
    /// scan skips.
    fn bounds(
        d: &Document,
        value: usize,
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
        if let Some(payload) = d.field(value, "impute") {
            for operand in ["not_before", "not_after", "fallback"] {
                if let Some(N::Text(name)) = d.field(payload, operand).map(|id| &d.nodes()[id]) {
                    names.push(name.clone());
                }
            }
        }
        if let Some(payload) = d.field(value, "case") {
            for &branch in sequence(d, payload)? {
                if let Some(child) = d
                    .field(branch, "then")
                    .or_else(|| d.field(branch, "otherwise"))
                {
                    bounds(d, child, names, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    let Some(item) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
        .map(|id| sequence(d, id))
        .transpose()?
        .and_then(|items| {
            items.iter().copied().find(|&item| {
                d.field(item, "id")
                    .is_some_and(|id| text(d, id).is_ok_and(|id| id == driver))
            })
        })
    else {
        return Ok(false);
    };
    let over_primary = text(d, field(d, item, "dataset")?)? == primary;
    let derived = match d
        .field(item, "derivations")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    {
        Some(id) => mapping(d, id)?
            .iter()
            .map(|&(name, _)| text(d, name))
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    let value = field(d, declaration, "value")?;
    let mut names = Vec::new();
    derivations::current_references(d, value, project, &mut names, 0)?;
    self_relations::template_references(d, value, templates, &mut names, 0)?;
    bounds(d, value, &mut names, 0)?;
    Ok(names.iter().any(|name| {
        name.split_once('.').is_some_and(|(owner, field)| {
            owner == driver && (aggregate || !over_primary || derived.contains(&field))
        })
    }))
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
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                None,
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
                    template_names(d, child, templates, names, depth + 1)?;
                }
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
