//! Original YAML rounding is one admitted scalar operation, not a formula rewrite.
use super::*;
use crate::dataset::Rounding;

/// Known bare output types are admitted without entering study authority. Source
/// field types that are not declared remain a binding/runtime responsibility.
pub(super) fn declaration_findings(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    fn visit(
        d: &Document,
        value: usize,
        prefix: &str,
        output: &TableSchema,
        findings: &mut Vec<PreflightFinding>,
        depth: usize,
        diagnostic_bytes: &mut usize,
    ) -> Result<(), PrepareError> {
        if depth > 64 || prefix.len() > 65_536 {
            return Err(PrepareError::Limit("case_depth"));
        }
        if let Some(payload) = d.field(value, "round_half_away_from_zero") {
            let source = text(d, field(d, payload, "source")?)?;
            if let Some(declared) = output.columns().iter().find(|item| item.name == source) {
                if !matches!(declared.kind, ColumnType::Int | ColumnType::Float) {
                    let actual = match declared.kind {
                        ColumnType::Str => "str",
                        ColumnType::Date => "date",
                        ColumnType::DateTime => "datetime",
                        _ => unreachable!("numeric types handled above"),
                    };
                    let suffix = ".round_half_away_from_zero";
                    *diagnostic_bytes = diagnostic_bytes
                        .checked_add(prefix.len())
                        .and_then(|n| n.checked_add(suffix.len()))
                        .and_then(|n| n.checked_add(source.len()))
                        .filter(|&n| n <= 16_777_216)
                        .ok_or(PrepareError::Limit("rounding_diagnostics"))?;
                    findings.push(PreflightFinding::RoundingInput {
                        path: format!("{prefix}{suffix}"),
                        source: source.into(),
                        actual,
                    });
                }
            }
        } else if let Some(case) = d.field(value, "case") {
            for (index, &branch) in sequence(d, case)?.iter().enumerate() {
                let (field, child) = d
                    .field(branch, "then")
                    .map(|child| ("then", child))
                    .or_else(|| {
                        d.field(branch, "otherwise")
                            .map(|child| ("otherwise", child))
                    })
                    .ok_or(PrepareError::Internal)?;
                visit(
                    d,
                    child,
                    &format!("{prefix}.case[{index}].{field}"),
                    output,
                    findings,
                    depth + 1,
                    diagnostic_bytes,
                )?;
            }
        }
        Ok(())
    }
    // Row reference availability and group scope use their established binder
    // checkpoint; do not let a type cause displace an earlier phase finding.
    if present(d, d.root(), "rows") {
        return Ok(());
    }
    let mut diagnostic_bytes = 0;
    for (column, &id) in columns.iter().enumerate() {
        let Some(derivation) = d.field(id, "derivation") else {
            continue;
        };
        let Some(value) = d.field(derivation, "value") else {
            continue;
        };
        visit(
            d,
            value,
            &format!("columns.{}.derivation", output.columns()[column].name),
            output,
            findings,
            0,
            &mut diagnostic_bytes,
        )?;
    }
    Ok(())
}

pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
    driver: Option<&str>,
    intermediates: Option<&intermediates::Declarations>,
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
    if source.split_once('.').is_some_and(|(relation, _)| {
        driver.is_some_and(|driver| relation != driver)
            && !intermediates.is_some_and(|items| items.contains(relation))
    }) {
        reject(
            extra,
            "secondary_source_expression",
            format!("{path}.source"),
        );
    }
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
    let path = String::from(path);
    if !name.contains('.') {
        if let Some(finding) = catalog
            .validate_output(name, None, None, &[0])
            .map_err(BindError::Catalog)?
        {
            findings.push(BindFinding::OutputReference {
                path,
                name: name.into(),
                finding,
            });
            return Ok(None);
        }
    }
    let source = if let Some((index, field)) = declarations.reference(name) {
        if let Some(item) = &intermediates[index] {
            edges.extend(item.output_dependencies());
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
        match catalog.bind(name).map_err(BindError::Catalog)? {
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
        source: alloc::boxed::Box::new(Assignment {
            column,
            expression: source,
            path: format!("{}.source", rounding.path()),
        }),
        rounding: rounding.clone(),
    }
}

/// Reuse the core scope/catalog services for each admitted row's driver and
/// completed outputs. Row/intermediate composition is rejected by its existing
/// compiler gate before this function, never widened by the scalar operation.
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
) -> Result<Option<Expression>, BindError> {
    bind_row_operand(
        rounding.source(),
        &format!("{}.source", rounding.path()),
        catalog,
        driver,
        row,
        phase,
        available,
        edges,
        findings,
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
            Default::default(),
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
            finding,
        });
        return Ok(None);
    }
    let source = match catalog.bind(name).map_err(BindError::Catalog)? {
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
