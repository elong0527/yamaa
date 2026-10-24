//! Structured source operands preserve options in every scalar row wrapper.
use super::*;

/// A structured source of a row template, the column phase or a lookup's
/// own records: a lookup read through its selector, another input's field
/// through its implicit join, a driver field or a column, each in this
/// scope ([`BindContext::direct_read`]).
pub(in crate::specification) fn bind(
    cx: &mut BindContext<'_, '_>,
    declaration: &Declaration,
    column: usize,
    path: &str,
) -> Result<Option<Expression>, BindError> {
    if declaration.report_declaration(cx.findings) {
        return Ok(None);
    }
    let driver = cx.scope.driver;
    let phase = cx.scope.phase;
    if declaration.reject_group_key_filter(path, driver, phase, cx.findings) {
        return Ok(None);
    }
    let binder::Donors { named, items, .. } = cx.donors;
    let catalog = cx.catalog;
    let before = cx.findings.len();
    let name = &declaration.variable;
    let expression = if let Some((index, _)) = named.reference(name).filter(|_| {
        name.split_once('.')
            .is_none_or(|(owner, _)| owner != driver)
    }) {
        if items.get(index).and_then(Option::as_ref).is_none() {
            // An earlier selector may be unavailable in this donor scope.
            // Let the named-read owner report it at the authored operand path.
            return row_intermediates::bind(cx, name, path);
        }
        declaration.bind_row_filter(path, driver, catalog, named, cx.findings)?;
        row_intermediates::bind(cx, name, path)?
    } else {
        let dataset = name.split_once('.').map(|(dataset, _)| dataset);
        let donor = dataset.and_then(|dataset| catalog.joined(dataset));
        // The relation's layout, with each field's kind.
        let schema = if let Some(donor) = donor {
            Some((donor.schema.clone(), donor.kinds()))
        } else if let Some(dataset) = dataset {
            catalog
                .fields(dataset)
                .map(|fields| {
                    let kinds = fields.iter().map(|field| field.column_type).collect();
                    TableSchema::new(
                        fields
                            .into_iter()
                            .map(|field| crate::table::Column {
                                name: field.name.into(),
                                // A layout slot; its kind is `kinds`'s.
                                kind: field.column_type.unwrap_or(ColumnType::Str),
                            })
                            .collect(),
                    )
                    .map(|schema| (std::sync::Arc::new(schema), kinds))
                })
                .transpose()
                .map_err(|_| BindError::Internal)?
        } else {
            None
        };
        let filter = if let (Some(dataset), Some((schema, kinds))) = (dataset, schema.as_ref()) {
            declaration.bind_filter_named(path, dataset, (schema, &kinds[..]), named, cx.findings)?
        } else {
            declaration.bind_row_filter(path, driver, catalog, named, cx.findings)?
        };
        let selection = if declaration.is_group_key(driver, phase) {
            // NAME-4: the key is already scalar; keep/order cannot choose a new record.
            None
        } else if let (Some(dataset), Some((schema, _))) = (dataset, schema.as_ref()) {
            declaration.bind_selection(path, dataset, schema, cx.findings)
        } else {
            None
        };
        match cx.direct_read(name, path)? {
            // A source binding keeps its range, options and absence answer
            // on the implicit join (LOOKUP-25).
            Some(Expression::Joined(read)) => {
                // A range value naming the row's driver reads a single value
                // of its record, a group key in a grouped row.
                if let Some(value) = declaration.range_value().filter(|value| {
                    value
                        .split_once('.')
                        .is_some_and(|(owner, _)| owner == driver)
                }) {
                    use crate::reference_scope::{self, Reach, Scope};
                    let row = cx.row();
                    cx.findings.extend(
                        reference_scope::validate(
                            catalog,
                            value,
                            None,
                            Scope {
                                drivers: &[driver],
                                current_driver: false,
                                reach: Reach::Scalar,
                                joined: false,
                                phase,
                            },
                        )
                        .map_err(BindError::Catalog)?
                        .into_iter()
                        .map(|finding| BindFinding::QualifiedReference {
                            path: format!("{path}.between.value"),
                            name: value.into(),
                            row: row.map(String::from),
                            finding,
                        }),
                    );
                }
                let kinds = catalog
                    .joined(&read.name)
                    .map(reference_binding::JoinedRelation::kinds)
                    .unwrap_or_default();
                declaration
                    .bind_range(cx, (&read.schema, &kinds), Some(driver))?
                    .map(|range| Expression::JoinedSource {
                        read,
                        range,
                        filter,
                        selection,
                        absence: Some(declaration.absence(path)),
                    })
            }
            Some(Expression::Source(column)) if filter.is_some() || selection.is_some() => {
                Some(Expression::Collect {
                    column,
                    identifier: name.clone(),
                    filter,
                    selection,
                })
            }
            expression => expression,
        }
    };
    if cx.findings.len() != before {
        return Ok(None);
    }
    if expression.as_ref().is_some_and(|expression| {
        declaration.refuse_require_match(path, expression, cx.findings)
            | declaration.refuse_range(path, expression, cx.findings)
    }) {
        return Ok(None);
    }
    Ok(expression.map(|expression| declaration.wrap(expression, column, path)))
}
