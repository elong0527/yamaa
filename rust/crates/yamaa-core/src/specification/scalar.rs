//! Greatest and least, lowered without converting their intermediate values.
use super::*;
use crate::dataset::SecondarySource;

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    sources: Vec<String>,
    largest: bool,
    path: String,
}
pub(super) fn prepare(
    d: &Document,
    payload: usize,
    op: &str,
    path: &str,
    depth: usize,
) -> Result<Declaration, PrepareError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            None,
        ));
    }
    Ok(Declaration {
        sources: sequence(d, field(d, payload, "sources")?)?
            .iter()
            .map(|&id| text(d, id).map(String::from))
            .collect::<Result<_, _>>()?,
        largest: op == "greatest",
        path: path.into(),
    })
}
impl Declaration {}
/// VALUE-19's comparability of two declared operand types.
fn comparable(a: ColumnType, b: ColumnType) -> bool {
    crate::result_type::ResultType::from(a).comparable(b.into())
}
fn bound_types(
    sources: &[String],
    catalog: &Catalog,
) -> Result<Vec<Option<ColumnType>>, BindError> {
    Ok(sources.iter().map(|name| catalog.type_of(name)).collect())
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
) -> Result<Option<Expression>, BindError> {
    let start = findings.len();
    let Declaration {
        sources,
        largest,
        path,
    } = operation;
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
                *kind = item
                    .source_column(secondary, field)
                    .and_then(|column| item.source_kind(secondary, column));
            }
        }
    }
    finish_extreme(sources, path, *largest, types, findings, operands, start)
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
    output: &TableSchema,
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
) -> Result<Option<Expression>, BindError> {
    let start = findings.len();
    let Declaration {
        sources,
        largest,
        path,
    } = operation;
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
                        item.source_column(secondary, field)
                            .and_then(|column| item.source_kind(secondary, column))
                    }))
                } else {
                    bound_types(std::slice::from_ref(name), catalog).map(|types| types[0])
                }
            })
            .collect::<Result<Vec<_>, BindError>>()?,
        findings,
        operands,
        start,
    )
}
/// These new scalar operands may read the driver and an admitted named
/// intermediate; implicit secondary joins are a separate language work package.
pub(super) fn operand_references(
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
    if let Some(payload) = d.field(value, "source") {
        names.push(text(d, field(d, payload, "variable")?)?.into());
        // An implicit join's range reads its value from the current row.
        if let Some(range) = d
            .field(payload, "between")
            .filter(|&range| !matches!(d.nodes()[range], N::Null))
        {
            names.push(text(d, field(d, range, "value")?)?.into());
        }
    }
    for op in ["mapping", "cut"] {
        if let Some(payload) = d.field(value, op) {
            let source = field(d, payload, "source")?;
            names.push(match &d.nodes()[source] {
                N::Text(name) => name.clone(),
                _ => text(d, field(d, source, "variable")?)?.into(),
            });
        }
    }
    for op in ["greatest", "least"] {
        if let Some(payload) = d.field(value, op) {
            for &id in sequence(d, field(d, payload, "sources")?)? {
                names.push(text(d, id)?.into());
            }
        }
    }
    for &op in temporal::OPERATIONS {
        if let Some(payload) = d.field(value, op) {
            for &field_name in temporal::fields(op) {
                if let Some(N::Text(source)) = d.field(payload, field_name).map(|id| &d.nodes()[id])
                {
                    names.push(source.clone());
                }
            }
        }
    }
    for op in [
        "str_case",
        "str_extract",
        "str_pad",
        "str_replace",
        "round_half_away_from_zero",
        "impute",
        "precision",
        "imputation_flag",
        "to_date",
    ] {
        if let Some(payload) = d.field(value, op) {
            names.push(text(d, field(d, payload, "source")?)?.into());
        }
    }
    Ok(())
}
