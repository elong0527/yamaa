//! Structured source operands preserve options in every scalar row wrapper.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::specification) fn bind(
    declaration: &Declaration,
    column: usize,
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
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
) -> Result<Option<Expression>, BindError> {
    if declaration.report_declaration(findings) {
        return Ok(None);
    }
    if declaration.reject_group_key_filter(path, driver, phase, findings) {
        return Ok(None);
    }
    let before = findings.len();
    let name = &declaration.variable;
    let mut variable_absent = false;
    let expression = if let Some((index, field)) = named.reference(name).filter(|_| {
        name.split_once('.')
            .is_none_or(|(owner, _)| owner != driver)
    }) {
        let Some(item) = intermediates.get(index).and_then(Option::as_ref) else {
            // An earlier selector may be unavailable in this donor scope.
            // Let the named-read owner report it at the authored operand path.
            return row_intermediates::bind(
                name,
                path,
                named,
                secondary,
                intermediates,
                output,
                catalog,
                available,
                edges,
                findings,
            );
        };
        declaration.bind_row_filter(path, driver, catalog, named, findings)?;
        if declaration.absent.is_some()
            && !item.source.is_self()
            && item.source_column(secondary, field).is_none()
        {
            variable_absent = true;
            Some(Expression::Literal(Value::Missing))
        } else {
            row_intermediates::bind(
                name,
                path,
                named,
                secondary,
                intermediates,
                output,
                catalog,
                available,
                edges,
                findings,
            )?
        }
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
            declaration.bind_filter_named(path, dataset, schema, named, findings)?
        } else {
            declaration.bind_row_filter(path, driver, catalog, named, findings)?
        };
        let selection = if declaration.is_group_key(driver, phase) {
            // REQ-0086: the key is already scalar; keep/order cannot choose a new record.
            None
        } else if let (Some(dataset), Some(schema)) = (dataset, schema.as_deref()) {
            declaration.bind_selection(path, dataset, schema, findings)
        } else {
            None
        };
        let missing_field = if let Some((_, field)) = name.split_once('.') {
            schema
                .as_deref()
                .is_some_and(|schema| !schema.columns().iter().any(|column| column.name == field))
        } else {
            catalog.bind(name).is_none()
        };
        if missing_field && declaration.absent.is_some() {
            variable_absent = true;
            Some(Expression::Literal(Value::Missing))
        } else {
            match rounding::bind_row_operand(
                name, path, catalog, driver, row, phase, available, edges, findings,
            )? {
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
                        findings.extend(
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
                            .map(|finding| {
                                BindFinding::QualifiedReference {
                                    path: format!("{path}.between.value"),
                                    name: value.into(),
                                    row: row.map(String::from),
                                    finding,
                                }
                            }),
                        );
                    }
                    declaration
                        .bind_range(
                            &read.schema,
                            catalog,
                            secondary,
                            intermediates,
                            row,
                            available,
                            edges,
                            findings,
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
        }
    };
    if findings.len() != before {
        return Ok(None);
    }
    if expression.as_ref().is_some_and(|expression| {
        declaration.refuse_require_match(path, expression, findings)
            | declaration.refuse_range(path, expression, findings)
    }) {
        return Ok(None);
    }
    Ok(expression.map(|expression| declaration.wrap(expression, column, path, variable_absent)))
}
