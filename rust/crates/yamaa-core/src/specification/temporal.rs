//! Complete-date declarations share operand binding with other scalar operations.
use super::*;
use crate::dataset::{CalendarUnit, DateBounds, ElapsedUnit, Temporal, TemporalKind};

pub(super) const OPERATIONS: &[&str] = &[
    "date_diff",
    "study_day",
    "to_epoch_day",
    "date_shift",
    "datetime_diff",
    "datetime_shift",
    "duration_value",
    "duration_text",
];

fn elapsed_unit(d: &Document, payload: usize) -> Result<ElapsedUnit, PrepareError> {
    Ok(match text(d, field(d, payload, "unit")?)? {
        "second" => ElapsedUnit::Second,
        "minute" => ElapsedUnit::Minute,
        "hour" => ElapsedUnit::Hour,
        "day" => ElapsedUnit::Day,
        _ => return Err(PrepareError::Internal),
    })
}

pub(super) fn fields(operation: &str) -> &'static [&'static str] {
    match operation {
        "date_diff" | "datetime_diff" => &["start", "end"],
        "study_day" => &["date", "reference"],
        "date_shift" => &["source", "days"],
        "datetime_shift" => &["source", "amount"],
        _ => &["source"],
    }
}

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
        "date_shift" => TemporalKind::DateShift,
        "datetime_diff" => TemporalKind::DateTimeDifference {
            unit: elapsed_unit(d, payload)?,
        },
        "datetime_shift" => TemporalKind::DateTimeShift {
            unit: elapsed_unit(d, payload)?,
        },
        "duration_value" => TemporalKind::DurationValue {
            unit: elapsed_unit(d, payload)?,
        },
        "duration_text" => TemporalKind::DurationText {
            unit: elapsed_unit(d, payload)?,
        },
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
        for &op in OPERATIONS {
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
                // Bounds and operand types are independent declaration findings.
                let witness_kind = match operation {
                    TemporalKind::Difference { unit, .. } => TemporalKind::Difference {
                        unit,
                        bounds: DateBounds::Exclusive,
                    },
                    other => other,
                };
                let witness = Temporal::new(
                    witness_kind,
                    operation
                        .fields()
                        .iter()
                        .map(|name| {
                            let id = field(d, payload, name)?;
                            Ok(match &d.nodes()[id] {
                                N::Text(source) => source.clone(),
                                _ => String::from(*name),
                            })
                        })
                        .collect::<Result<Vec<_>, PrepareError>>()?,
                    path.clone(),
                )
                .map_err(|_| PrepareError::Limit("temporal_diagnostics"))?;
                for (index, field_name) in operation.fields().iter().enumerate() {
                    let id = field(d, payload, field_name)?;
                    let N::Text(source) = &d.nodes()[id] else {
                        if matches!(d.nodes()[id], N::Float(_)) && witness.expected(index) == "int"
                        {
                            findings.push(PreflightFinding::Scalar(
                                witness.operand_diagnostic(index, "float"),
                            ));
                        }
                        continue;
                    };
                    let declared = if source.contains('.') {
                        declared_source_type(d, source)?
                    } else if !rows {
                        output
                            .columns()
                            .iter()
                            .find(|item| item.name == *source)
                            .map(|item| item.kind)
                    } else {
                        None
                    };
                    if let Some(declared) = declared.filter(|&kind| !witness.accepts(index, kind)) {
                        let actual = match declared {
                            ColumnType::Str => "str",
                            ColumnType::Int => "int",
                            ColumnType::Float => "float",
                            ColumnType::DateTime => "datetime",
                            ColumnType::Date => "date",
                        };
                        let path = format!("{path}.{field_name}");
                        *bytes = bytes
                            .checked_add(path.len())
                            .and_then(|n| n.checked_add(source.len()))
                            .filter(|&n| n <= 16_777_216)
                            .ok_or(PrepareError::Limit("temporal_diagnostics"))?;
                        findings.push(PreflightFinding::Scalar(
                            witness.operand_diagnostic(index, actual),
                        ));
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
            || op == "date_diff" && ["unit", "bounds"].contains(&name)
            || [
                "datetime_diff",
                "datetime_shift",
                "duration_value",
                "duration_text",
            ]
            .contains(&op)
                && name == "unit"
            || op == "duration_value" && name == "invalid")
        {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    for field_name in kind.fields() {
        let id = field(d, payload, field_name)?;
        sources.push(match &d.nodes()[id] {
            N::Text(source) => source.clone(),
            _ => String::from(*field_name),
        });
    }
    match Temporal::new(kind, sources, path.into()) {
        Ok(mut operation) => {
            for (index, field_name) in kind.fields().iter().enumerate() {
                let id = field(d, payload, field_name)?;
                if !matches!(d.nodes()[id], N::Text(_)) {
                    operation = operation
                        .with_literal(index, literal(d, id, &format!("{path}.{field_name}"))?);
                }
            }
            if let Some(id) = d.field(payload, "invalid") {
                operation = operation
                    .with_invalid(literal(d, id, &format!("{path}.invalid"))?)
                    .map_err(|_| PrepareError::Limit("temporal_diagnostics"))?;
            }
            Ok(operation)
        }
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
        if let Some(value) = operation.literal(index) {
            operands.push(Assignment {
                column,
                expression: Expression::Literal(value.clone()),
                path,
            });
            continue;
        }
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
            if let Some(actual) =
                static_types::kind(source, declarations, catalog, secondary, intermediates)?
            {
                let (kind, name) = match actual {
                    crate::value::ValueType::Str => (ColumnType::Str, "str"),
                    crate::value::ValueType::Int => (ColumnType::Int, "int"),
                    crate::value::ValueType::Float => (ColumnType::Float, "float"),
                    crate::value::ValueType::Date => (ColumnType::Date, "date"),
                    crate::value::ValueType::DateTime => (ColumnType::DateTime, "datetime"),
                    _ => return Err(BindError::Internal),
                };
                if !operation.accepts(index, kind) {
                    findings.push(BindFinding::Temporal(
                        operation.operand_diagnostic(index, name),
                    ));
                    continue;
                }
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
    output: &TableSchema,
    named: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
) -> Result<Option<Expression>, BindError> {
    let mut operands = Vec::new();
    for (index, source) in operation.sources().iter().enumerate() {
        let path = operation.operand_path(index).ok_or(BindError::Internal)?;
        if let Some(value) = operation.literal(index) {
            operands.push(Assignment {
                column,
                expression: Expression::Literal(value.clone()),
                path,
            });
            continue;
        }
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
            let column_type = if let Expression::Intermediate { index, column } = &expression {
                intermediates[*index]
                    .as_ref()
                    .and_then(|item| item.source_schema(secondary))
                    .and_then(|schema| schema.columns().get(*column))
                    .map(|column| column.kind)
                    .ok_or(BindError::Internal)?
            } else {
                catalog
                    .type_of(source)
                    .map_err(BindError::Catalog)?
                    .ok_or(BindError::Internal)?
            };
            if !operation.accepts(index, column_type) {
                let actual = match column_type {
                    ColumnType::Str => "str",
                    ColumnType::Int => "int",
                    ColumnType::Float => "float",
                    ColumnType::DateTime => "datetime",
                    ColumnType::Date => "date",
                };
                findings.push(BindFinding::Temporal(
                    operation.operand_diagnostic(index, actual),
                ));
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
