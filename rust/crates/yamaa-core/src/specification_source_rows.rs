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
    if let Some(finding) = &declaration.declaration_finding {
        findings.push(BindFinding::Source(SourceFinding(finding.clone())));
        return Ok(None);
    }
    let before = findings.len();
    let name = &declaration.variable;
    let mut variable_absent = false;
    let expression = if let Some((index, field)) = named.reference(name).filter(|_| {
        name.split_once('.')
            .is_none_or(|(owner, _)| owner != driver)
    }) {
        let item = intermediates
            .get(index)
            .and_then(Option::as_ref)
            .ok_or(BindError::Internal)?;
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
                    .map(alloc::sync::Arc::new)
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
        let selection = if let (Some(dataset), Some(schema)) = (dataset, schema.as_deref()) {
            declaration.bind_selection(path, dataset, schema, findings)
        } else {
            None
        };
        let missing_field = if let Some((_, field)) = name.split_once('.') {
            schema
                .as_deref()
                .is_some_and(|schema| !schema.columns().iter().any(|column| column.name == field))
        } else {
            catalog.bind(name).map_err(BindError::Catalog)?.is_none()
        };
        if missing_field && declaration.absent.is_some() {
            variable_absent = true;
            Some(Expression::Literal(Value::Missing))
        } else {
            rounding::bind_row_operand(
                name, path, catalog, driver, row, phase, available, edges, findings,
            )?
            .map(|expression| {
                if filter.is_some() || selection.is_some() {
                    match expression {
                        Expression::Joined(read) => Expression::JoinedSource {
                            read,
                            filter,
                            selection,
                        },
                        Expression::Source(column) => Expression::Collect {
                            column,
                            identifier: name.clone(),
                            filter,
                            selection,
                        },
                        _ => expression,
                    }
                } else {
                    expression
                }
            })
        }
    };
    Ok(if findings.len() != before {
        None
    } else {
        expression.map(|expression| declaration.wrap(expression, column, path, variable_absent))
    })
}
