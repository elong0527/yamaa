//! Four scalar operations lowered without converting their intermediate values.
use super::*;
use crate::{dataset::SecondarySource, value::Value};

#[derive(Clone, Debug)]
pub(super) enum Declaration {
    Extreme {
        sources: Vec<String>,
        largest: bool,
        path: String,
    },
    Concat {
        sources: Vec<cases::Branch>,
        missing: Option<Value>,
        path: String,
    },
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    d: &Document,
    payload: usize,
    op: &str,
    path: &str,
    project: Option<&functions::Prepared>,
    intermediates: &intermediates::Declarations,
    record: bool,
    depth: usize,
    extraction: &str_extract::Prepared,
    templates: &str_template::Prepared,
) -> Result<Declaration, PrepareError> {
    if depth > 64 {
        return Err(PrepareError::Limit("scalar_depth"));
    }
    let sources = sequence(d, field(d, payload, "sources")?)?;
    if op == "str_concat" {
        Ok(Declaration::Concat {
            sources: sources
                .iter()
                .enumerate()
                .map(|(index, &value)| {
                    derivations::prepare_value(
                        d,
                        value,
                        &format!("{path}.sources[{index}]"),
                        project,
                        intermediates,
                        record,
                        depth + 1,
                        extraction,
                        templates,
                    )
                })
                .collect::<Result<_, _>>()?,
            missing: d
                .field(payload, "missing")
                .map(|id| literal(d, id, &format!("{path}.missing")))
                .transpose()?,
            path: path.into(),
        })
    } else {
        Ok(Declaration::Extreme {
            sources: sources
                .iter()
                .map(|&id| text(d, id).map(String::from))
                .collect::<Result<_, _>>()?,
            largest: op == "greatest",
            path: path.into(),
        })
    }
}
fn comparable(a: ColumnType, b: ColumnType) -> bool {
    a == b
        || matches!(
            (a, b),
            (ColumnType::Int, ColumnType::Float) | (ColumnType::Float, ColumnType::Int)
        )
}
/// Known declared types reject incompatible extremes before source activation.
pub(super) fn declaration_findings(
    d: &Document,
    output: &TableSchema,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    fn visit(
        d: &Document,
        id: usize,
        path: &str,
        output: &TableSchema,
        findings: &mut Vec<PreflightFinding>,
        depth: usize,
    ) -> Result<(), PrepareError> {
        if depth > 64 {
            return Err(PrepareError::Limit("scalar_depth"));
        }
        for op in ["greatest", "least"] {
            if let Some(payload) = d.field(id, op) {
                let sources = sequence(d, field(d, payload, "sources")?)?
                    .iter()
                    .map(|&id| text(d, id).map(String::from))
                    .collect::<Result<Vec<_>, _>>()?;
                let types = sources
                    .iter()
                    .map(|name| {
                        Ok(output
                            .columns()
                            .iter()
                            .find(|column| column.name == *name)
                            .map(|c| c.kind)
                            .or(temporal::declared_source_type(d, name)?))
                    })
                    .collect::<Result<Vec<_>, PrepareError>>()?;
                let known = types.iter().flatten().copied().collect::<Vec<_>>();
                if known
                    .first()
                    .is_some_and(|&first| known.iter().any(|&kind| !comparable(first, kind)))
                    && known.len() == types.len()
                {
                    findings.push(PreflightFinding::Scalar(
                        crate::scalar_language::incomparable_diagnostic(
                            &sources,
                            &known,
                            &format!("{path}.{op}"),
                        ),
                    ));
                }
            }
        }
        if let Some(payload) = d.field(id, "str_concat") {
            for (index, &child) in sequence(d, field(d, payload, "sources")?)?
                .iter()
                .enumerate()
            {
                visit(
                    d,
                    child,
                    &format!("{path}.str_concat.sources[{index}]"),
                    output,
                    findings,
                    depth + 1,
                )?;
            }
        }
        if let Some(items) = d.field(id, "case") {
            for (index, &branch) in sequence(d, items)?.iter().enumerate() {
                for name in ["then", "otherwise"] {
                    if let Some(child) = d.field(branch, name) {
                        visit(
                            d,
                            child,
                            &format!("{path}.case[{index}].{name}"),
                            output,
                            findings,
                            depth + 1,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
    for &column in sequence(d, field(d, d.root(), "columns")?)? {
        if let Some(value) = d
            .field(column, "derivation")
            .and_then(|id| d.field(id, "value"))
        {
            visit(
                d,
                value,
                &format!("columns.{}.derivation", text(d, field(d, column, "name")?)?),
                output,
                findings,
                0,
            )?;
        }
    }
    if let Some(rows) = d.field(d.root(), "rows") {
        for (index, &row) in sequence(d, rows)?.iter().enumerate() {
            if let Some(items) = d.field(row, "derivations") {
                for &(name, value) in mapping(d, items)? {
                    if let Some(value) = d.field(value, "value") {
                        visit(
                            d,
                            value,
                            &format!("rows[{index}].derivations.{}", text(d, name)?),
                            output,
                            findings,
                            0,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn bound_types(
    sources: &[String],
    catalog: &Catalog,
) -> Result<Vec<Option<ColumnType>>, BindError> {
    sources
        .iter()
        .map(|name| {
            catalog.bind(name).map(|binding| {
                binding.map(|binding| match binding {
                    reference_binding::Binding::Output { column_type, .. }
                    | reference_binding::Binding::Dataset { column_type, .. } => column_type,
                })
            })
        })
        .collect::<Result<_, _>>()
        .map_err(BindError::Catalog)
}
fn finish_extreme(
    sources: &[String],
    path: &str,
    largest: bool,
    types: Vec<Option<ColumnType>>,
    findings: &mut Vec<BindFinding>,
    operands: Vec<Assignment>,
    start: usize,
) -> Result<Option<Expression>, BindError> {
    let types = types.into_iter().collect::<Option<Vec<_>>>();
    if let Some(types) = types {
        if types
            .first()
            .is_some_and(|&first| types.iter().any(|&kind| !comparable(first, kind)))
        {
            findings.push(BindFinding::Scalar(
                crate::scalar_language::incomparable_diagnostic(sources, &types, path),
            ));
        }
    }
    Ok((findings.len() == start).then_some(Expression::Extreme { operands, largest }))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    spec: &PreparedSpecification,
    operation: &Declaration,
    column: usize,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    reported: bool,
) -> Result<Option<Expression>, BindError> {
    let start = findings.len();
    match operation {
        Declaration::Extreme {
            sources,
            largest,
            path,
        } => {
            let mut operands = Vec::new();
            for (index, source) in sources.iter().enumerate() {
                let path = format!("{path}.sources[{index}]");
                if let Some(expression) = rounding::bind_operand(
                    source,
                    &path,
                    column,
                    &spec.keys,
                    catalog,
                    &spec.intermediates,
                    secondary,
                    intermediates,
                    edges,
                    findings,
                )? {
                    operands.push(Assignment {
                        column,
                        path,
                        expression,
                    });
                }
            }
            let mut types = bound_types(sources, catalog)?;
            for (name, kind) in sources.iter().zip(&mut types) {
                if let Some((index, field)) = spec.intermediates.reference(name) {
                    if let Some(item) = &intermediates[index] {
                        *kind = item.source_column(secondary, field).and_then(|column| {
                            item.source_schema(secondary)
                                .map(|schema| schema.columns()[column].kind)
                        });
                    }
                }
            }
            finish_extreme(sources, path, *largest, types, findings, operands, start)
        }
        Declaration::Concat {
            sources,
            missing,
            path,
        } => {
            let mut operands = Vec::new();
            for source in sources {
                let expression = if let Operation::Source(item) = source.operation.as_ref() {
                    rounding::bind_operand(
                        &item.variable,
                        &source.path,
                        column,
                        &spec.keys,
                        catalog,
                        &spec.intermediates,
                        secondary,
                        intermediates,
                        edges,
                        findings,
                    )?
                } else {
                    cases::bind(
                        spec,
                        core::slice::from_ref(source),
                        column,
                        catalog,
                        secondary,
                        intermediates,
                        edges,
                        findings,
                        reported,
                    )?
                    .and_then(|expression| {
                        // Strip the synthetic branch container; this leaf has
                        // no predicate and must not form an otherwise-only case.
                        let Expression::Case(mut branches) = expression else {
                            unreachable!("case binder result")
                        };
                        branches.pop().map(|branch| branch.assignment.expression)
                    })
                };
                if let Some(expression) = expression {
                    operands.push(Assignment {
                        column,
                        path: source.path.clone(),
                        expression,
                    });
                }
            }
            Ok((findings.len() == start).then_some(Expression::StrConcat {
                operands,
                missing: missing.clone(),
                path: path.clone(),
            }))
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_contains(
    spec: &PreparedSpecification,
    operation: &crate::text_extract::StringExtract,
    column: usize,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    Ok(str_extract::bind(
        operation,
        column,
        &spec.keys,
        catalog,
        &spec.intermediates,
        secondary,
        intermediates,
        edges,
        findings,
    )?
    .map(|expression| match expression {
        Expression::StrExtract { source, operation } => {
            Expression::StrContains { source, operation }
        }
        _ => unreachable!(),
    }))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row(
    operation: &Declaration,
    column: usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    reported: bool,
    output: &TableSchema,
    project: Option<&crate::project_calls::ProjectCalls>,
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
) -> Result<Option<Expression>, BindError> {
    let start = findings.len();
    match operation {
        Declaration::Extreme {
            sources,
            largest,
            path,
        } => {
            let mut operands = Vec::new();
            for (index, source) in sources.iter().enumerate() {
                let path = format!("{path}.sources[{index}]");
                if let Some(expression) = self_relations::bind_operand(
                    source,
                    &path,
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
                )? {
                    operands.push(Assignment {
                        column,
                        path,
                        expression,
                    });
                }
            }
            finish_extreme(
                sources,
                path,
                *largest,
                sources
                    .iter()
                    .map(|name| {
                        if let Some((index, field)) = named.reference(name) {
                            Ok(intermediates[index].as_ref().and_then(|item| {
                                item.source_column(secondary, field).and_then(|column| {
                                    item.source_schema(secondary)
                                        .map(|schema| schema.columns()[column].kind)
                                })
                            }))
                        } else {
                            bound_types(core::slice::from_ref(name), catalog).map(|types| types[0])
                        }
                    })
                    .collect::<Result<Vec<_>, BindError>>()?,
                findings,
                operands,
                start,
            )
        }
        Declaration::Concat {
            sources,
            missing,
            path,
        } => {
            let mut operands = Vec::new();
            for source in sources {
                let expression = derivations::bind_row(
                    &source.operation,
                    &source.path,
                    column,
                    catalog,
                    driver,
                    row,
                    phase,
                    available,
                    edges,
                    findings,
                    reported,
                    output,
                    project,
                    named,
                    secondary,
                    intermediates,
                )?;
                if let Some(expression) = expression {
                    operands.push(Assignment {
                        column,
                        path: source.path.clone(),
                        expression,
                    });
                }
            }
            Ok((findings.len() == start).then_some(Expression::StrConcat {
                operands,
                missing: missing.clone(),
                path: path.clone(),
            }))
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row_contains(
    operation: &crate::text_extract::StringExtract,
    column: usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    Ok(str_extract::bind_row(
        operation, column, catalog, driver, row, phase, available, edges, findings,
    )?
    .map(|expression| match expression {
        Expression::StrExtract { source, operation } => {
            Expression::StrContains { source, operation }
        }
        _ => unreachable!(),
    }))
}

/// These new scalar operands may read the driver and an admitted named
/// intermediate; implicit secondary joins are a separate language work package.
pub(super) fn unsupported_relations(
    operation: &Operation,
    driver: &str,
    declarations: &intermediates::Declarations,
    extra: &mut Vec<UnsupportedFeature>,
) {
    fn reference(
        name: &str,
        path: &str,
        driver: &str,
        declarations: &intermediates::Declarations,
        extra: &mut Vec<UnsupportedFeature>,
    ) {
        if name
            .split_once('.')
            .is_some_and(|(relation, _)| relation != driver && !declarations.contains(relation))
        {
            reject(extra, "secondary_source_expression", path.into());
        }
    }
    match operation {
        Operation::Scalar(operation) => match operation.as_ref() {
            Declaration::Extreme { sources, path, .. } => {
                for (index, name) in sources.iter().enumerate() {
                    reference(
                        name,
                        &format!("{path}.sources[{index}]"),
                        driver,
                        declarations,
                        extra,
                    );
                }
            }
            Declaration::Concat { sources, .. } => {
                for source in sources {
                    if let Operation::Source(item) = source.operation.as_ref() {
                        reference(&item.variable, &source.path, driver, declarations, extra);
                    }
                    unsupported_relations(&source.operation, driver, declarations, extra);
                }
            }
        },
        Operation::StrContains(operation) => reference(
            operation.source(),
            &format!("{}.source", operation.path()),
            driver,
            declarations,
            extra,
        ),
        Operation::Case(branches) => {
            for source in branches {
                unsupported_relations(&source.operation, driver, declarations, extra);
            }
        }
        _ => (),
    }
}

pub(super) fn operand_references(
    d: &Document,
    value: usize,
    names: &mut Vec<String>,
    depth: usize,
) -> Result<(), PrepareError> {
    if depth > 64 {
        return Err(PrepareError::Limit("scalar_depth"));
    }
    if let Some(payload) = d.field(value, "source") {
        names.push(text(d, field(d, payload, "variable")?)?.into());
    }
    for op in ["greatest", "least"] {
        if let Some(payload) = d.field(value, op) {
            for &id in sequence(d, field(d, payload, "sources")?)? {
                names.push(text(d, id)?.into());
            }
        }
    }
    for op in [
        "str_contains",
        "str_case",
        "str_extract",
        "str_pad",
        "round_half_away_from_zero",
        "date_impute",
        "datetime_impute",
        "date_precision",
        "datetime_precision",
        "to_date",
        "to_epoch_day",
    ] {
        if let Some(payload) = d.field(value, op) {
            names.push(text(d, field(d, payload, "source")?)?.into());
        }
    }
    Ok(())
}
