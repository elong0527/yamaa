//! Temporal declarations dispatch on their declared `type` and share operand
//! binding with other scalar operations.
use super::*;
use crate::dataset::{CalendarUnit, DateBounds, ElapsedUnit, TemporalKind, DATE_PARTS};

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

/// A written unit's name.
fn unit_name(unit: ast::TimeUnit) -> &'static str {
    match unit {
        ast::TimeUnit::Day => "day",
        ast::TimeUnit::Week => "week",
        ast::TimeUnit::Month => "month",
        ast::TimeUnit::Year => "year",
        ast::TimeUnit::Second => "second",
        ast::TimeUnit::Minute => "minute",
        ast::TimeUnit::Hour => "hour",
    }
}

/// What `diff` counts from one value of `kind` to another in `unit`, the
/// end only for dates (TIME-4), or the refusal of a unit `kind` does not
/// count in, at `<path>.unit`, as `diff`'s own. A nearest selection measures
/// its distances so (LOOKUP-26).
pub(crate) fn difference(
    kind: ast::TemporalType,
    unit: ast::TimeUnit,
    path: &str,
) -> Result<TemporalKind, Diagnostic> {
    let name = unit_name(unit);
    let refusal = |permitted| Temporal::unit_diagnostic(path, name, permitted);
    match kind {
        ast::TemporalType::DateTime => elapsed_unit(name)
            .map(|unit| TemporalKind::DateTimeDifference { unit })
            .ok_or_else(|| refusal(ELAPSED_UNITS)),
        ast::TemporalType::Date => match unit {
            ast::TimeUnit::Day => Ok(CalendarUnit::Day),
            ast::TimeUnit::Week => Ok(CalendarUnit::Week),
            ast::TimeUnit::Month => Ok(CalendarUnit::Month),
            ast::TimeUnit::Year => Ok(CalendarUnit::Year),
            _ => Err(refusal(CALENDAR_UNITS)),
        }
        .map(|unit| TemporalKind::Difference {
            unit,
            bounds: DateBounds::Exclusive,
        }),
    }
}

/// The endpoints a `diff` counts; `left` belongs to `cut` alone.
fn date_bounds(closed: ast::Closed) -> Result<DateBounds, Refusal> {
    match closed {
        ast::Closed::Right => Ok(DateBounds::Exclusive),
        ast::Closed::Both => Ok(DateBounds::Inclusive),
        ast::Closed::Neither => Ok(DateBounds::Between),
        ast::Closed::Left => Err(Refusal::Internal),
    }
}

/// TIME-5's refusal of the endpoints a `diff` counts with a datetime or a
/// unit coarser than a day, as `closed` writes them, permitting `right`
/// (P-ORD-2).
fn bounds_finding(path: String, closed: ast::Closed) -> Result<Diagnostic, Refusal> {
    let value = match closed {
        ast::Closed::Both => "both",
        ast::Closed::Neither => "neither",
        _ => return Err(Refusal::Internal),
    };
    Ok(Temporal::bounds_diagnostic(&path, "closed", value, "right"))
}

/// The operand field of a temporal verb that `field` names: each verb's
/// fields in the executor's order ([`TemporalKind::fields`]).
fn slot(field: &str) -> Option<Slot> {
    Some(match field {
        "start" => Slot::Start,
        "end" => Slot::End,
        "date" | "source" => Slot::Source,
        "reference" => Slot::Reference,
        "days" | "amount" => Slot::Amount,
        "year" => Slot::Year,
        "month" => Slot::Month,
        "day" => Slot::Day,
        "hour" => Slot::Hour,
        "minute" => Slot::Minute,
        "second" => Slot::Second,
        _ => return None,
    })
}

/// `operation` with each operand's path as the tree writes it, whose operand
/// fields differ from the executor's names (`study_day.source`, P-OP-1).
fn surface_operands(
    spec: &Spec,
    operands: &[ast::Operand],
    operation: Temporal,
) -> Result<Temporal, Refusal> {
    let paths = operation
        .kind()
        .fields()
        .iter()
        .map(|field| {
            slot(field)
                .and_then(|slot| operand(operands, slot))
                .map(|operand| spec.cite(spec.expr(operand).meta))
                .ok_or(Refusal::Internal)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(operation.with_operand_paths(paths))
}

/// Whether an operand of the verb is written in the field `field`.
fn written(spec: &Spec, operands: &[ast::Operand], field: &str) -> bool {
    operands.iter().any(|operand| {
        matches!(spec.paths.segment(spec.expr(operand.expr).meta.path),
            ast::Segment::Field(name) if *name == field)
    })
}

/// The verb's kind, or the declaration findings that keep it from having
/// one: a unit or bounds its `type` does not take, a field its `type`
/// requires or does not take, or a gap in its date parts.
pub(super) fn tree_kind(
    spec: &Spec,
    e: ExprId,
) -> Result<Result<TemporalKind, Vec<Diagnostic>>, Refusal> {
    let ExprKind::Scalar { op, operands, .. } = &spec.expr(e).kind else {
        return Err(Refusal::Internal);
    };
    let path = path(spec, e);
    Ok(Ok(match op {
        ScalarOp::Diff { kind, unit, closed } => {
            let bounds = date_bounds(*closed)?;
            let unit = unit_name(*unit);
            if *kind == ast::TemporalType::DateTime {
                let Some(unit) = elapsed_unit(unit) else {
                    return Ok(Err(vec![Temporal::unit_diagnostic(
                        &path,
                        unit,
                        ELAPSED_UNITS,
                    )]));
                };
                if bounds != DateBounds::Exclusive {
                    return Ok(Err(vec![bounds_finding(path, *closed)?]));
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
                            return Ok(Err(vec![Temporal::unit_diagnostic(
                                &path,
                                other,
                                CALENDAR_UNITS,
                            )]))
                        }
                    },
                    bounds,
                }
            }
        }
        ScalarOp::Shift { kind, unit } => {
            let datetime = *kind == ast::TemporalType::DateTime;
            let target = if datetime { "datetime" } else { "date" };
            let findings = [
                ("days", !datetime, written(spec, operands, "days")),
                ("amount", datetime, written(spec, operands, "amount")),
                ("unit", datetime, unit.is_some()),
            ]
            .into_iter()
            .filter(|&(_, wanted, present)| present != wanted)
            .map(|(name, wanted, _)| Temporal::type_field_diagnostic(wanted, &path, name, target))
            .collect::<Vec<_>>();
            if !findings.is_empty() {
                return Ok(Err(findings));
            }
            if datetime {
                TemporalKind::DateTimeShift {
                    unit: unit
                        .and_then(|unit| elapsed_unit(unit_name(unit)))
                        .ok_or(Refusal::Internal)?,
                }
            } else {
                TemporalKind::DateShift
            }
        }
        ScalarOp::StudyDay => TemporalKind::StudyDay,
        ScalarOp::ToEpochDay => TemporalKind::EpochDay,
        ScalarOp::DurationValue { unit } => TemporalKind::DurationValue {
            unit: elapsed_unit(unit_name(*unit)).ok_or(Refusal::Internal)?,
        },
        ScalarOp::TimeOfDay => TemporalKind::TimeOfDay,
        ScalarOp::Dtc => {
            // Every part before the finest declared one must be declared too.
            let declared = DATE_PARTS
                .map(|name| slot(name).is_some_and(|slot| operand(operands, slot).is_some()));
            let depth = declared
                .iter()
                .rposition(|&present| present)
                .map_or(0, |index| index + 1);
            if depth == 0 {
                return Err(Refusal::Internal);
            }
            let gaps = (0..depth)
                .filter(|&index| !declared[index])
                .map(|index| {
                    Temporal::date_part_gap_diagnostic(
                        &path,
                        DATE_PARTS[index],
                        DATE_PARTS[depth - 1],
                    )
                })
                .collect::<Vec<_>>();
            if !gaps.is_empty() {
                return Ok(Err(gaps));
            }
            TemporalKind::DateText { depth: depth as u8 }
        }
        _ => return Err(Refusal::Internal),
    }))
}

/// The executor's temporal operation for the verb at `e`: each operand a
/// name, or a written number with the field's name as its source. Each
/// operand binds as the scope decides, at the path its surface writes.
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let ExprKind::Scalar {
        op,
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let path = path(spec, e);
    let kind = tree_kind(spec, e)?.map_err(Refusal::Findings)?;
    let mut sources = Vec::new();
    let mut literals = Vec::new();
    for (index, field_name) in kind.fields().iter().enumerate() {
        let operand = slot(field_name)
            .and_then(|slot| operand(operands, slot))
            .ok_or(Refusal::Internal)?;
        match &spec.expr(operand).kind {
            ExprKind::Ref(name) => sources.push(name.0.clone()),
            ExprKind::Literal(value) => {
                sources.push(String::from(*field_name));
                literals.push((index, value, spec.expr(operand).meta));
            }
            _ => return Err(Refusal::Internal),
        }
    }
    match Temporal::new(kind, sources, path.clone()) {
        Ok(operation) => {
            let mut operation = surface_operands(spec, operands, operation)?;
            for (index, value, meta) in literals {
                operation = operation.with_literal(index, literal(value, spec.cite(meta))?);
            }
            if let Some(invalid) = handler(spec, handlers.invalid.as_ref())? {
                operation = operation
                    .with_invalid(invalid)
                    .map_err(|_| Refusal::Internal)?;
            }
            let operands = (0..operation.sources().len())
                .map(|index| {
                    Ok(Operand {
                        read: match operation.literal(index) {
                            Some(value) => Read::Literal(value.clone()),
                            None => Read::Name(operation.sources()[index].clone()),
                        },
                        path: operation.operand_path(index).ok_or(Refusal::Internal)?,
                    })
                })
                .collect::<Result<_, Refusal>>()?;
            Ok(Operation {
                kind: Kind::Temporal(operation),
                operands,
            })
        }
        Err(crate::dataset::InvalidTemporal::Shape) => Err(Refusal::Internal),
        Err(crate::dataset::InvalidTemporal::Bounds) => {
            let ScalarOp::Diff { closed, .. } = op else {
                return Err(Refusal::Internal);
            };
            Err(Refusal::Findings(vec![bounds_finding(path, *closed)?]))
        }
    }
}

/// A whole number the operation counts, written as a fraction.
pub(super) fn literal_findings(operation: &Temporal) -> Vec<Diagnostic> {
    (0..operation.sources().len())
        .filter(|&index| {
            matches!(operation.literal(index), Some(Value::Float(_)))
                && operation.expected(index) == "int"
        })
        .map(|index| operation.operand_diagnostic(index, "float"))
        .collect()
}

/// One temporal verb's declaration findings: a unit, bounds or field its
/// `type` does not take, a gap in its date parts, and each operand whose
/// declared type it does not accept. With `rows` a bare name's phase is
/// binding's to decide, so its type is not checked here.
pub(super) fn preflight(
    spec: &Spec,
    e: ExprId,
    declared: &dyn Fn(&str) -> Option<ColumnType>,
    rows: bool,
) -> Result<Vec<Diagnostic>, Refusal> {
    let ExprKind::Scalar { op, operands, .. } = &spec.expr(e).kind else {
        return Err(Refusal::Internal);
    };
    let path = path(spec, e);
    let operation = match tree_kind(spec, e)? {
        Ok(operation) => operation,
        Err(errors) => return Ok(errors),
    };
    let mut findings = Vec::new();
    if let TemporalKind::Difference { unit, bounds } = operation {
        if unit != CalendarUnit::Day && bounds != DateBounds::Exclusive {
            let ScalarOp::Diff { closed, .. } = op else {
                return Err(Refusal::Internal);
            };
            findings.push(bounds_finding(path.clone(), *closed)?);
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
    let operand = |field: &str| {
        slot(field)
            .and_then(|slot| operand(operands, slot))
            .ok_or(Refusal::Internal)
    };
    let witness = Temporal::new(
        witness_kind,
        operation
            .fields()
            .iter()
            .map(|name| {
                Ok(match &spec.expr(operand(name)?).kind {
                    ExprKind::Ref(source) => source.0.clone(),
                    _ => String::from(*name),
                })
            })
            .collect::<Result<Vec<_>, Refusal>>()?,
        path,
    )
    .map_err(|_| Refusal::Internal)?;
    let witness = surface_operands(spec, operands, witness)?;
    for (index, field_name) in operation.fields().iter().enumerate() {
        let ExprKind::Ref(source) = &spec.expr(operand(field_name)?).kind else {
            if matches!(
                spec.expr(operand(field_name)?).kind,
                ExprKind::Literal(ast::Literal::Float(_))
            ) && witness.expected(index) == "int"
            {
                findings.push(witness.operand_diagnostic(index, "float"));
            }
            continue;
        };
        let kind = (source.0.contains('.') || !rows)
            .then(|| declared(&source.0))
            .flatten();
        if let Some(kind) = kind.filter(|&kind| !witness.accepts(index, kind)) {
            findings.push(witness.operand_diagnostic(index, kind.name()));
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::Value as Json;
    use crate::test_document::Tree;
    use crate::test_specification::{column, derivation, document, lower, op};

    /// TIME-5 refuses a datetime `diff` that counts both endpoints, naming
    /// `closed` and the value it writes, and permitting `right` (P-ORD-2).
    #[test]
    fn a_datetime_diff_s_endpoints_are_refused_as_closed_writes_them() {
        let days = |endpoints: (&str, &str)| {
            column(
                "DAYS",
                "int",
                derivation(op(
                    "diff",
                    Tree::map(vec![
                        ("start", Tree::text("SRC.START")),
                        ("end", Tree::text("SRC.END")),
                        ("type", Tree::text("datetime")),
                        ("unit", Tree::text("day")),
                        (endpoints.0, Tree::text(endpoints.1)),
                    ]),
                )),
            )
        };
        let refusal = |spec: &ast::Spec| {
            let e = spec.columns[1].derivation.as_ref().unwrap().expr;
            let Ok(Err(findings)) = tree_kind(spec, e) else {
                panic!("TIME-5 refuses the endpoints");
            };
            let [finding] = findings.as_slice() else {
                panic!("one finding: {findings:?}");
            };
            finding.clone()
        };
        let permitted = |value: &str| Json::Array(vec![Json::String(value.into())]);

        let spec = lower(document(vec![days(("closed", "both"))], vec![]));
        let diagnostic = refusal(&spec);
        assert_eq!(
            diagnostic.code,
            crate::diagnostic::ConditionCode::TemporalBounds
        );
        assert_eq!(
            diagnostic.spec_paths,
            ["columns.DAYS.derivation.diff.closed"]
        );
        assert_eq!(diagnostic.context["value"], Json::String("both".into()));
        assert_eq!(diagnostic.context["permitted"], permitted("right"));
    }
}
