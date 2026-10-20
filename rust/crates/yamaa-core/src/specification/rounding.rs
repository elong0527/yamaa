//! Original YAML rounding is one admitted scalar operation, not a formula rewrite.
use super::*;
use crate::dataset::Rounding;

pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
    _driver: Option<&str>,
    _intermediates: Option<&intermediates::Declarations>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Rounding, PrepareError> {
    // Schema admission checks the written source/digits fields before this
    // compiler. Keep its integer declaration, including widths beyond i64.
    for &(name, _) in mapping(d, payload)? {
        let name = text(d, name)?;
        if !["source", "digits"].contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let N::Integer(digits) = &d.nodes()[field(d, payload, "digits")?] else {
        return Err(PrepareError::Internal);
    };
    let source = text(d, field(d, payload, "source")?)?;
    Rounding::new(source, path, digits).map_err(|_| PrepareError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    rounding: &Rounding,
    column: usize,
    keys: &[usize],
    catalog: &Catalog,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    bind_operand(
        rounding.source(),
        &format!("{}.source", rounding.path()),
        column,
        keys,
        catalog,
        declarations,
        secondary,
        intermediates,
        edges,
        findings,
    )
    .map(|source| source.map(|source| wrap(rounding, column, source)))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind_operand(
    name: &str,
    path: &str,
    column: usize,
    keys: &[usize],
    catalog: &Catalog,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    if name
        .split_once('.')
        .is_some_and(|(dataset, _)| secondary.iter().any(|source| source.name == dataset))
    {
        return source_expressions::bind_implicit(
            name, path, keys, catalog, secondary, edges, findings,
        );
    }
    let path = String::from(path);
    if !name.contains('.') {
        if let Some(finding) = catalog
            .validate_output(name, None, None, &[0])
            .map_err(BindError::Catalog)?
        {
            findings.push(BindFinding::OutputReference {
                path,
                name: name.into(),
                row: None,
                finding,
            });
            return Ok(None);
        }
    }
    let source = if let Some((index, field)) = declarations.reference(name) {
        if let Some(item) = &intermediates[index] {
            edges.extend(row_intermediates::dependencies(index, intermediates)?);
            let operand = item.source_column(secondary, field);
            if let Some(column) = operand {
                Expression::Intermediate { index, column }
            } else {
                findings.push(BindFinding::Lookup(LookupFinding::reference(
                    &path,
                    name,
                    None,
                    ReferenceCause::Value,
                    None,
                )));
                return Ok(None);
            }
        } else {
            findings.push(BindFinding::UnknownReference {
                path,
                name: name.into(),
            });
            return Ok(None);
        }
    } else {
        match catalog.bind(name) {
            Some(reference_binding::Binding::Output { column, .. }) => {
                edges.push(column);
                Expression::Column(column)
            }
            Some(reference_binding::Binding::Dataset { field, .. }) => {
                if keys.contains(&column) {
                    Expression::Source(field)
                } else {
                    Expression::Collect {
                        column: field,
                        identifier: name.into(),
                        filter: None,
                        selection: None,
                    }
                }
            }
            None => {
                findings.push(BindFinding::UnknownReference {
                    path,
                    name: name.into(),
                });
                return Ok(None);
            }
        }
    };

    Ok(Some(source))
}
pub(super) fn wrap(rounding: &Rounding, column: usize, source: Expression) -> Expression {
    Expression::Round {
        source: std::boxed::Box::new(Assignment {
            column,
            expression: source,
            path: format!("{}.source", rounding.path()),
        }),
        rounding: rounding.clone(),
    }
}

/// Reuse the core scope/catalog services for each admitted row's driver and
/// completed outputs, including named intermediate selections.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row(
    rounding: &Rounding,
    column: usize,
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
    self_relations::bind_operand(
        rounding.source(),
        &format!("{}.source", rounding.path()),
        catalog,
        driver,
        row,
        phase,
        available,
        edges,
        findings,
        output,
        named,
        secondary,
        intermediates,
    )
    .map(|source| source.map(|source| wrap(rounding, column, source)))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row_operand(
    name: &str,
    path: &str,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    use crate::reference_scope::{self, Reach, Scope};
    if name
        .split_once('.')
        .is_some_and(|(dataset, _)| catalog.joined(dataset).is_some())
    {
        return source_expressions::bind_catalog_join(
            name, path, catalog, row, available, edges, findings,
        );
    }
    let path = String::from(path);
    if name.contains('.') {
        let scoped = reference_scope::validate(
            catalog,
            name,
            None,
            Scope {
                drivers: &[driver],
                current_driver: false,
                reach: Reach::Scalar,
                joined: false,
                phase,
            },
        )
        .map_err(BindError::Catalog)?;
        if !scoped.is_empty() {
            findings.extend(
                scoped
                    .into_iter()
                    .map(|finding| BindFinding::QualifiedReference {
                        path: path.clone(),
                        name: name.into(),
                        row: row.map(String::from),
                        finding,
                    }),
            );
            return Ok(None);
        }
    } else if let Some(finding) = catalog
        .validate_output(name, None, available, &[0])
        .map_err(BindError::Catalog)?
    {
        findings.push(BindFinding::OutputReference {
            path,
            name: name.into(),
            row: row.map(String::from),
            finding,
        });
        return Ok(None);
    }
    let source = match catalog.bind(name) {
        Some(reference_binding::Binding::Output { column, .. }) => {
            edges.push(column);
            Expression::Column(column)
        }
        Some(reference_binding::Binding::Dataset { field, .. }) => Expression::Source(field),
        None => {
            findings.push(BindFinding::UnknownReference {
                path,
                name: name.into(),
            });
            return Ok(None);
        }
    };
    Ok(Some(source))
}
