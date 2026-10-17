//! Temporal declarations dispatch on their declared `type` and share operand
//! binding with other scalar operations.
use super::*;
use crate::dataset::{CalendarUnit, DateBounds, ElapsedUnit, Temporal, TemporalKind};

pub(super) const OPERATIONS: &[&str] = &[
    "diff",
    "study_day",
    "to_epoch_day",
    "shift",
    "duration_value",
];

const CALENDAR_UNITS: &[&str] = &["day", "week", "month", "year"];
const ELAPSED_UNITS: &[&str] = &["second", "minute", "hour", "day"];

fn elapsed_unit(unit: &str) -> Option<ElapsedUnit> {
    Some(match unit {
        "second" => ElapsedUnit::Second,
        "minute" => ElapsedUnit::Minute,
        "hour" => ElapsedUnit::Hour,
        "day" => ElapsedUnit::Day,
        _ => return None,
    })
}

/// Every field that may hold an operand, whichever `type` is declared.
pub(super) fn fields(operation: &str) -> &'static [&'static str] {
    match operation {
        "diff" => &["start", "end"],
        "study_day" => &["date", "reference"],
        "shift" => &["source", "days", "amount"],
        _ => &["source"],
    }
}

/// The declared `type` of a type-dispatched operation.
fn datetime(d: &Document, payload: usize) -> Result<bool, PrepareError> {
    match text(d, field(d, payload, "type")?)? {
        "date" => Ok(false),
        "datetime" => Ok(true),
        _ => Err(PrepareError::Internal),
    }
}

/// Read the operation's kind, or the declaration findings that keep it from
/// having one: a unit or bounds its `type` does not take, or a field its
/// `type` requires or does not take.
fn declared(
    d: &Document,
    payload: usize,
    operation: &str,
    path: &str,
) -> Result<Result<TemporalKind, Vec<PreflightFinding>>, PrepareError> {
    let unit = || text(d, field(d, payload, "unit")?);
    Ok(Ok(match operation {
        "diff" => {
            let bounds = d
                .field(payload, "bounds")
                .map(|id| text(d, id))
                .transpose()?
                .unwrap_or("exclusive");
            let unit = unit()?;
            if datetime(d, payload)? {
                let Some(unit) = elapsed_unit(unit) else {
                    return Ok(Err(vec![PreflightFinding::Scalar(
                        Temporal::unit_diagnostic(path, unit, ELAPSED_UNITS),
                    )]));
                };
                if bounds != "exclusive" {
                    return Ok(Err(vec![PreflightFinding::TemporalBounds {
                        path: path.into(),
                        value: bounds.into(),
                    }]));
                }
                TemporalKind::DateTimeDifference { unit }
            } else {
                TemporalKind::Difference {
                    unit: match unit {
                        "day" => CalendarUnit::Day,
                        "week" => CalendarUnit::Week,
                        "month" => CalendarUnit::Month,
                        "year" => CalendarUnit::Year,
                        other => {
                            return Ok(Err(vec![PreflightFinding::Scalar(
                                Temporal::unit_diagnostic(path, other, CALENDAR_UNITS),
                            )]))
                        }
                    },
                    bounds: match bounds {
                        "exclusive" => DateBounds::Exclusive,
                        "inclusive" => DateBounds::Inclusive,
                        "between" => DateBounds::Between,
                        _ => return Err(PrepareError::Internal),
                    },
                }
            }
        }
        "shift" => {
            let datetime = datetime(d, payload)?;
            let target = if datetime { "datetime" } else { "date" };
            let findings = [
                ("days", !datetime),
                ("amount", datetime),
                ("unit", datetime),
            ]
            .into_iter()
            .filter(|&(name, wanted)| d.field(payload, name).is_some() != wanted)
            .map(|(name, wanted)| {
                PreflightFinding::Scalar(Temporal::type_field_diagnostic(
                    wanted, path, name, target,
                ))
            })
            .collect::<Vec<_>>();
            if !findings.is_empty() {
                return Ok(Err(findings));
            }
            if datetime {
                TemporalKind::DateTimeShift {
                    unit: elapsed_unit(unit()?).ok_or(PrepareError::Internal)?,
                }
            } else {
                TemporalKind::DateShift
            }
        }
        "study_day" => TemporalKind::StudyDay,
        "to_epoch_day" => TemporalKind::EpochDay,
        "duration_value" => TemporalKind::DurationValue {
            unit: elapsed_unit(unit()?).ok_or(PrepareError::Internal)?,
        },
        _ => return Err(PrepareError::Internal),
    }))
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
    let profile = SourceProfile::from_path(path);
    if !matches!(
        profile,
        Some(SourceProfile::Csv | SourceProfile::Xport | SourceProfile::Sas7bdat)
    ) {
        return Ok(None);
    }
    let declared = d.field(input, "types").and_then(|id| d.field(id, name));
    if declared.is_none() && profile != Some(SourceProfile::Csv) {
        return Ok(None);
    }
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

        rows: bool,
    ) -> Result<(), PrepareError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                Some(depth),
                Some(prefix),
            ));
        }
        for &op in OPERATIONS {
            if let Some(payload) = d.field(value, op) {
                let path = format!("{prefix}.{op}");
                let operation = match declared(d, payload, op, &path)? {
                    Ok(operation) => operation,
                    Err(errors) => {
                        findings.extend(errors);
                        continue;
                    }
                };
                if let TemporalKind::Difference { unit, bounds } = operation {
                    if unit != CalendarUnit::Day && bounds != DateBounds::Exclusive {
                        let value = text(d, field(d, payload, "bounds")?)?;
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
                .map_err(|_| PrepareError::Internal)?;
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
                    rows,
                )?;
            }
        }
        Ok(())
    }

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
                rows,
            )?;
        }
    }
    match_values::visit_keys(d, &mut |value, path| {
        visit(
            d,
            value,
            path,
            output,
            findings,
            0,
            present(d, d.root(), "rows"),
        )
    })?;
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
    let allowed: &[&str] = match op {
        "diff" => &["type", "start", "end", "unit", "bounds"],
        "shift" => &["type", "source", "days", "amount", "unit"],
        "study_day" => &["date", "reference"],
        "to_epoch_day" => &["source"],
        "duration_value" => &["source", "unit", "invalid"],
        _ => return Err(PrepareError::Internal),
    };
    for &(name, _) in mapping(d, payload)? {
        let name = text(d, name)?;
        if !allowed.contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let kind = declared(d, payload, op, path)?.map_err(PrepareError::Invalid)?;
    let mut sources = Vec::new();
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
                    .map_err(|_| PrepareError::Internal)?;
            }
            Ok(operation)
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
                    .and_then(|item| item.source_kind(secondary, *column))
            } else {
                catalog.type_of(source)
            };
            if let Some(column_type) = column_type.filter(|&kind| !operation.accepts(index, kind)) {
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
