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
    let binder::Donors {
        named,
        secondary,
        items,
    } = cx.donors;
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
            return row_intermediates::bind(
                name,
                path,
                named,
                secondary,
                items,
                cx.output,
                catalog,
                cx.row(),
                cx.scope.available,
                cx.edges,
                cx.findings,
            );
        }
        declaration.bind_row_filter(path, driver, catalog, named, cx.findings)?;
        row_intermediates::bind(
            name,
            path,
            named,
            secondary,
            items,
            cx.output,
            catalog,
            cx.row(),
            cx.scope.available,
            cx.edges,
            cx.findings,
        )?
    } else {
        let dataset = name.split_once('.').map(|(dataset, _)| dataset);
        let donor = dataset.and_then(|dataset| catalog.joined(dataset));
        let schema = if let Some(donor) = donor {
            Some(donor.schema.clone())
        } else if let Some(dataset) = dataset {
            catalog
                .fields(dataset)
                .map(|fields| {
                    TableSchema::new(
                        fields
                            .into_iter()
                            .map(|field| crate::table::Column {
                                name: field.name.into(),
                                kind: field.column_type,
                            })
                            .collect(),
                    )
                    .map(std::sync::Arc::new)
                })
                .transpose()
                .map_err(|_| BindError::Internal)?
        } else {
            None
        };
        let filter = if let (Some(dataset), Some(schema)) = (dataset, schema.as_deref()) {
            declaration.bind_filter_named(path, dataset, schema, named, cx.findings)?
        } else {
            declaration.bind_row_filter(path, driver, catalog, named, cx.findings)?
        };
        let selection = if declaration.is_group_key(driver, phase) {
            // NAME-4: the key is already scalar; keep/order cannot choose a new record.
            None
        } else if let (Some(dataset), Some(schema)) = (dataset, schema.as_deref()) {
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
                declaration
                    .bind_range(
                        &read.schema,
                        catalog,
                        secondary,
                        items,
                        cx.row(),
                        Some(driver),
                        cx.scope.available,
                        cx.edges,
                        cx.findings,
                    )?
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
