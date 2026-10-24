//! Build per-template stored-source metadata through the existing source-port ordering.
use super::*;
/// The catalog of a row whose driver is `name`, laid out as `source`: the
/// output columns, and the driver's fields, those of the intermediate `item`
/// when one drives the rows, each typed as `donors` types it.
pub(super) fn catalog_visible(
    output: &TableSchema,
    source: &TableSchema,
    name: &str,
    item: Option<&crate::dataset::Intermediate>,
    donors: binder::Donors<'_>,
) -> Result<Catalog, BindError> {
    let outputs = output
        .columns()
        .iter()
        .map(|c| reference_binding::Field {
            name: &c.name,
            column_type: Some(c.kind),
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
        .enumerate()
        .zip(&names)
        .map(|((index, column), field)| reference_binding::Field {
            name: field,
            column_type: match item {
                Some(item) => donors.kind(item, index),
                None => donors.inputs.kind_of(name, &column.name, column.kind),
            },
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
/// over it without `key`, matches on: the input's declared `key`, none when
/// it declares none (LOOKUP-1, D23, D63).
pub(super) fn declared_key<'a>(spec: &'a ast::Spec, relation: &str) -> Vec<&'a str> {
    spec.input_key(relation)
        .unwrap_or_default()
        .iter()
        .map(|column| column.name.as_str())
        .collect()
}

/// The whole-dataset catalog: the primary record is current, and every other
/// stored source joins through its implicit join's key ([`declared_key`]).
pub(super) fn joined_catalog(
    spec: &ast::Spec,
    output: &TableSchema,
    primary: &TableSchema,
    name: &str,
    donors: binder::Donors<'_>,
) -> Result<Catalog, BindError> {
    let catalog = catalog_visible(output, primary, name, None, donors)?;
    extend_catalog(spec, catalog, primary, name, donors, name)
}
pub(super) fn extend_catalog(
    spec: &ast::Spec,
    catalog: Catalog,
    primary: &TableSchema,
    name: &str,
    donors: binder::Donors<'_>,
    driver: &str,
) -> Result<Catalog, BindError> {
    let secondary = donors.secondary;
    let fields = std::iter::once((name, primary))
        .chain(secondary.iter().map(|source| (source.name.as_str(), &source.schema)))
        .map(|(relation, schema)| donors.inputs.fields_of(relation, schema))
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
                declared_key(spec, name),
                spec.pair_relation(name).is_some(),
            )
        })
        .collect::<Vec<_>>();
    catalog.with_joined(&relations).map_err(BindError::Catalog)
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
