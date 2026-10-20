//! Build per-template stored-source metadata through the existing source-port ordering.
use super::*;
use crate::key_relation::Implied;
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
/// What an implicit join to the input `relation`, or a qualified aggregate
/// over it without `key`, matches on: on a 0.5 tree the applicable output
/// keys, inferred from the fields the input stores; on a 1.0 tree the
/// input's declared `key`, none when it declares none (LOOKUP-1, D23, D63).
pub(super) fn implied<'a>(spec: &'a ast::Spec, relation: &str) -> Implied<'a> {
    if spec.header.surface.infers_join_keys() {
        return Implied::Applicable;
    }
    Implied::Declared(
        spec.input_key(relation)
            .unwrap_or_default()
            .iter()
            .map(|column| column.name.as_str())
            .collect(),
    )
}

/// The whole-dataset catalog: the primary record is current, and every other
/// stored source joins through its implicit join's key ([`implied`]).
pub(super) fn joined_catalog(
    spec: &ast::Spec,
    output: &TableSchema,
    primary: &TableSchema,
    name: &str,
    keys: &[usize],
    secondary: &[crate::dataset::SecondarySource],
) -> Result<Catalog, BindError> {
    let catalog = catalog_visible(output, primary, name, None)?;
    extend_catalog(spec, catalog, primary, name, keys, secondary, name)
}
pub(super) fn extend_catalog(
    spec: &ast::Spec,
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
                implied(spec, name),
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
