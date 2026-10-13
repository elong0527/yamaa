//! Complete-date declarations share operand binding with other scalar operations.
use super::*;
use crate::dataset::{CalendarUnit, DateBounds, Temporal, TemporalKind};

fn kind(d: &Document, payload: usize, operation: &str) -> Result<TemporalKind, PrepareError> {
    Ok(match operation {
        "date_diff" => TemporalKind::Difference {
            unit: match text(d, field(d, payload, "unit")?)? {
                "day" => CalendarUnit::Day,
                "week" => CalendarUnit::Week,
                "month" => CalendarUnit::Month,
                "year" => CalendarUnit::Year,
                _ => return Err(PrepareError::Internal),
            },
            bounds: match d
                .field(payload, "bounds")
                .map(|id| text(d, id))
                .transpose()?
                .unwrap_or("exclusive")
            {
                "exclusive" => DateBounds::Exclusive,
                "inclusive" => DateBounds::Inclusive,
                "between" => DateBounds::Between,
                _ => return Err(PrepareError::Internal),
            },
        },
        "study_day" => TemporalKind::StudyDay,
        "to_epoch_day" => TemporalKind::EpochDay,
        _ => return Err(PrepareError::Internal),
    })
}

/// CSV fields have an authored type or the schema-defined string default.
/// Other resource formats remain unknown until their schema is captured.
pub(super) fn declared_source_type(
    d: &Document,
    source: &str,
) -> Result<Option<ColumnType>, PrepareError> {
    let Some((relation, name)) = source.split_once('.') else {
        return Ok(None);
    };
    let Some(input) = d
        .field(d.root(), "input")
        .and_then(|id| d.field(id, relation))
    else {
        return Ok(None);
    };
    let path = text(d, field(d, input, "path")?)?;
    if SourceProfile::from_path(path) != Some(SourceProfile::Csv) {
        return Ok(None);
    }
    let declared = d.field(input, "types").and_then(|id| d.field(id, name));
    Ok(Some(
        match declared.map(|id| text(d, id)).transpose()?.unwrap_or("str") {
            "str" => ColumnType::Str,
            "int" => ColumnType::Int,
            "float" => ColumnType::Float,
            "date" => ColumnType::Date,
            "datetime" => ColumnType::DateTime,
            _ => return Err(PrepareError::Internal),
        },
    ))
}

pub(super) fn declaration_findings(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    #[allow(clippy::too_many_arguments)]
    fn visit(
        d: &Document,
        value: usize,
        prefix: &str,
        output: &TableSchema,
        findings: &mut Vec<PreflightFinding>,
        depth: usize,
        bytes: &mut usize,
        rows: bool,
    ) -> Result<(), PrepareError> {
        if depth > 64 || prefix.len() > 65_536 {
            return Err(PrepareError::Limit("case_depth"));
        }
        for op in ["date_diff", "study_day", "to_epoch_day"] {
            if let Some(payload) = d.field(value, op) {
                let operation = kind(d, payload, op)?;
                let path = format!("{prefix}.{op}");
                if let TemporalKind::Difference { unit, bounds } = operation {
                    if unit != CalendarUnit::Day && bounds != DateBounds::Exclusive {
                        let value = text(d, field(d, payload, "bounds")?)?;
                        *bytes = bytes
                            .checked_add(path.len())
                            .and_then(|n| n.checked_add(value.len()))
                            .filter(|&n| n <= 16_777_216)
                            .ok_or(PrepareError::Limit("temporal_diagnostics"))?;
                        findings.push(PreflightFinding::TemporalBounds {
                            path: path.clone(),
                            value: value.into(),
                        });
                    }
                }
                for field_name in operation.fields() {
                    let source = text(d, field(d, payload, field_name)?)?;
                    let declared = if source.contains('.') {
                        declared_source_type(d, source)?
                    } else if !rows {
                        output
                            .columns()
                            .iter()
                            .find(|item| item.name == source)
                            .map(|item| item.kind)
                    } else {
                        None
                    };
                    if let Some(declared) = declared.filter(|&kind| kind != ColumnType::Date) {
                        let actual = match declared {
                            ColumnType::Str => "str",
                            ColumnType::Int => "int",
                            ColumnType::Float => "float",
                            ColumnType::DateTime => "datetime",
                            ColumnType::Date => unreachable!(),
                        };
                        let path = format!("{path}.{field_name}");
                        *bytes = bytes
                            .checked_add(path.len())
                            .and_then(|n| n.checked_add(source.len()))
                            .filter(|&n| n <= 16_777_216)
                            .ok_or(PrepareError::Limit("temporal_diagnostics"))?;
                        findings.push(PreflightFinding::TemporalInput {
                            path,
                            source: source.into(),
                            actual,
                        });
                    }
                }
            }
        }
        if let Some(case) = d.field(value, "case") {
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
                    bytes,
                    rows,
                )?;
            }
        }
        Ok(())
    }
    let mut bytes = 0;
    let rows = present(d, d.root(), "rows");
    for (column, &id) in columns.iter().enumerate() {
        if let Some(value) = d
            .field(id, "derivation")
            .and_then(|id| d.field(id, "value"))
        {
            visit(
                d,
                value,
                &format!("columns.{}.derivation", output.columns()[column].name),
                output,
                findings,
                0,
                &mut bytes,
                rows,
            )?;
        }
    }
    Ok(())
}

pub(super) fn prepare(
    d: &Document,
    payload: usize,
    op: &str,
    path: &str,
    _driver: Option<&str>,
    _intermediates: Option<&intermediates::Declarations>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Temporal, PrepareError> {
    let kind = kind(d, payload, op)?;
    let mut sources = Vec::new();
    for &(name, _) in mapping(d, payload)? {
        let name = text(d, name)?;
        if !(kind.fields().contains(&name)
            || op == "date_diff" && ["unit", "bounds"].contains(&name))
        {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    for field_name in kind.fields() {
        let source = text(d, field(d, payload, field_name)?)?;
        sources.push(source.into());
    }
    match Temporal::new(kind, sources, path.into()) {
        Ok(operation) => Ok(operation),
        Err(crate::dataset::InvalidTemporal::Limit) => {
            Err(PrepareError::Limit("temporal_diagnostics"))
        }
        Err(crate::dataset::InvalidTemporal::Shape) => Err(PrepareError::Internal),
        Err(crate::dataset::InvalidTemporal::Bounds) => Err(PrepareError::Invalid(vec![
            PreflightFinding::TemporalBounds {
                path: path.into(),
                value: text(d, field(d, payload, "bounds")?)?.into(),
            },
        ])),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    operation: &Temporal,
    column: usize,
    keys: &[usize],
    catalog: &Catalog,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let mut operands = Vec::new();
    for (index, source) in operation.sources().iter().enumerate() {
        let path = operation.operand_path(index).ok_or(BindError::Internal)?;
        if let Some(expression) = rounding::bind_operand(
            source,
            &path,
            column,
            keys,
            catalog,
            declarations,
            secondary,
            intermediates,
            edges,
            findings,
        )? {
            operands.push(Assignment {
                column,
                expression,
                path,
            });
        }
    }
    Ok(
        (operands.len() == operation.sources().len()).then(|| Expression::Temporal {
            operands,
            operation: operation.clone(),
        }),
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row(
    operation: &Temporal,
    column: usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let mut operands = Vec::new();
    for (index, source) in operation.sources().iter().enumerate() {
        let path = operation.operand_path(index).ok_or(BindError::Internal)?;
        if let Some(expression) = rounding::bind_row_operand(
            source, &path, catalog, driver, row, phase, available, edges, findings,
        )? {
            let column_type = catalog
                .type_of(source)
                .map_err(BindError::Catalog)?
                .ok_or(BindError::Internal)?;
            if column_type != ColumnType::Date {
                let actual = match column_type {
                    ColumnType::Str => "str",
                    ColumnType::Int => "int",
                    ColumnType::Float => "float",
                    ColumnType::DateTime => "datetime",
                    ColumnType::Date => unreachable!(),
                };
                findings.push(BindFinding::Temporal(Temporal::input_diagnostic(
                    source, &path, actual,
                )));
                continue;
            }
            operands.push(Assignment {
                column,
                expression,
                path,
            });
        }
    }
    Ok(
        (operands.len() == operation.sources().len()).then(|| Expression::Temporal {
            operands,
            operation: operation.clone(),
        }),
    )
}
