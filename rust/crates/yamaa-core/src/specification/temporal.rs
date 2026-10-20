//! Temporal declarations dispatch on their declared `type` and share operand
//! binding with other scalar operations.
use super::*;
use crate::dataset::{CalendarUnit, DateBounds, ElapsedUnit, Temporal, TemporalKind, DATE_PARTS};

pub(super) const OPERATIONS: &[&str] = &[
    "diff",
    "study_day",
    "to_epoch_day",
    "shift",
    "duration_value",
    "dtc",
    "time_of_day",
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
        "dtc" => &DATE_PARTS,
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
fn declared_kind(
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
        "time_of_day" => TemporalKind::TimeOfDay,
        "dtc" => {
            // Every part before the finest declared one must be declared too.
            let declared = DATE_PARTS.map(|name| d.field(payload, name).is_some());
            let depth = declared
                .iter()
                .rposition(|&present| present)
                .map_or(0, |index| index + 1);
            if depth == 0 {
                return Err(PrepareError::Internal);
            }
            let gaps = (0..depth)
                .filter(|&index| !declared[index])
                .map(|index| {
                    PreflightFinding::Scalar(Temporal::date_part_gap_diagnostic(
                        path,
                        DATE_PARTS[index],
                        DATE_PARTS[depth - 1],
                    ))
                })
                .collect::<Vec<_>>();
            if !gaps.is_empty() {
                return Ok(Err(gaps));
            }
            TemporalKind::DateText { depth: depth as u8 }
        }
        _ => return Err(PrepareError::Internal),
    }))
}

/// One temporal verb's declaration findings at `path`: a unit, bounds or
/// field its `type` does not take, a gap in its date parts, and each operand
/// whose declared type it does not accept. With `rows` a bare name's phase
/// is binding's to decide, so its type is not checked here.
pub(super) fn operand_findings(
    d: &Document,
    payload: usize,
    op: &str,
    path: &str,
    declared: &types::DeclaredTypes<'_>,
    rows: bool,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let operation = match declared_kind(d, payload, op, path)? {
        Ok(operation) => operation,
        Err(errors) => {
            findings.extend(errors);
            return Ok(());
        }
    };
    if let TemporalKind::Difference { unit, bounds } = operation {
        if unit != CalendarUnit::Day && bounds != DateBounds::Exclusive {
            let value = text(d, field(d, payload, "bounds")?)?;
            findings.push(PreflightFinding::TemporalBounds {
                path: path.into(),
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
        path.into(),
    )
    .map_err(|_| PrepareError::Internal)?;
    for (index, field_name) in operation.fields().iter().enumerate() {
        let id = field(d, payload, field_name)?;
        let N::Text(source) = &d.nodes()[id] else {
            if matches!(d.nodes()[id], N::Float(_)) && witness.expected(index) == "int" {
                findings.push(PreflightFinding::Scalar(
                    witness.operand_diagnostic(index, "float"),
                ));
            }
            continue;
        };
        let kind = (source.contains('.') || !rows)
            .then(|| declared.name(source).column_type())
            .flatten();
        if let Some(kind) = kind.filter(|&kind| !witness.accepts(index, kind)) {
            findings.push(PreflightFinding::Scalar(
                witness.operand_diagnostic(index, checks::type_name(kind)),
            ));
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
    let allowed: &[&str] = match op {
        "diff" => &["type", "start", "end", "unit", "bounds"],
        "shift" => &["type", "source", "days", "amount", "unit"],
        "study_day" => &["date", "reference"],
        "to_epoch_day" => &["source"],
        "duration_value" => &["source", "unit", "invalid"],
        "time_of_day" => &["source"],
        "dtc" => &[
            "year", "month", "day", "hour", "minute", "second", "invalid",
        ],
        _ => return Err(PrepareError::Internal),
    };
    for &(name, _) in mapping(d, payload)? {
        let name = text(d, name)?;
        if !allowed.contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let kind = declared_kind(d, payload, op, path)?.map_err(PrepareError::Invalid)?;
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

/// Each operand binds as the scope decides, then its type must be one the
/// operation accepts: a lookup read's from its selector's donor, any other
/// read's from the catalog.
pub(super) fn bind(
    operation: &Temporal,
    column: usize,
    cx: &mut BindContext<'_, '_>,
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
        if let Some(expression) = cx.operand(source, &path)? {
            let column_type = if let Expression::Intermediate { index, column } = &expression {
                cx.donors.items[*index]
                    .as_ref()
                    .and_then(|item| item.source_kind(cx.donors.secondary, *column))
            } else {
                cx.catalog.type_of(source)
            };
            if let Some(column_type) = column_type.filter(|&kind| !operation.accepts(index, kind)) {
                cx.findings.push(BindFinding::Temporal(
                    operation.operand_diagnostic(index, checks::type_name(column_type)),
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
