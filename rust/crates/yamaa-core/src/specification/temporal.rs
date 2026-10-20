//! Temporal declarations dispatch on their declared `type` and share operand
//! binding with other scalar operations.
use super::*;
use crate::dataset::{CalendarUnit, DateBounds, ElapsedUnit, Temporal, TemporalKind, DATE_PARTS};

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

/// The endpoints a `diff` counts; `left` belongs to `cut` alone.
fn date_bounds(closed: ast::Closed) -> Result<DateBounds, PrepareError> {
    match closed {
        ast::Closed::Right => Ok(DateBounds::Exclusive),
        ast::Closed::Both => Ok(DateBounds::Inclusive),
        ast::Closed::Neither => Ok(DateBounds::Between),
        ast::Closed::Left => Err(PrepareError::Internal),
    }
}

/// TIME-5's refusal of the endpoints a `diff` counts with a datetime or a
/// unit coarser than a day, as the tree's surface writes them: 0.5's
/// `bounds`, permitting `exclusive`, or 1.0's `closed`, permitting `right`
/// (P-ORD-2).
fn bounds_finding(
    spec: &ast::Spec,
    path: String,
    closed: ast::Closed,
) -> Result<PreflightFinding, PrepareError> {
    let v1 = !matches!(spec.header.surface, ast::Surface::V0_5);
    let value = match (v1, closed) {
        (false, ast::Closed::Both) => "inclusive",
        (false, ast::Closed::Neither) => "between",
        (true, ast::Closed::Both) => "both",
        (true, ast::Closed::Neither) => "neither",
        _ => return Err(PrepareError::Internal),
    };
    Ok(PreflightFinding::TemporalBounds {
        path,
        field: if v1 { "closed" } else { "bounds" },
        value,
        permitted: if v1 { "right" } else { "exclusive" },
    })
}

/// The operand field of a temporal verb that `field` names: each verb's
/// fields in the executor's order ([`TemporalKind::fields`]).
fn slot(field: &str) -> Option<ast::Slot> {
    Some(match field {
        "start" => ast::Slot::Start,
        "end" => ast::Slot::End,
        "date" | "source" => ast::Slot::Source,
        "reference" => ast::Slot::Reference,
        "days" | "amount" => ast::Slot::Amount,
        "year" => ast::Slot::Year,
        "month" => ast::Slot::Month,
        "day" => ast::Slot::Day,
        "hour" => ast::Slot::Hour,
        "minute" => ast::Slot::Minute,
        "second" => ast::Slot::Second,
        _ => return None,
    })
}

/// Whether an operand of the verb is written in the field `field`.
fn written(spec: &ast::Spec, operands: &[ast::Operand], field: &str) -> bool {
    operands.iter().any(|operand| {
        matches!(spec.paths.segment(spec.expr(operand.expr).meta.path),
            ast::Segment::Field(name) if *name == field)
    })
}

/// The verb's kind, or the declaration findings that keep it from having
/// one: a unit or bounds its `type` does not take, a field its `type`
/// requires or does not take, or a gap in its date parts.
pub(super) fn tree_kind(
    spec: &ast::Spec,
    e: ExprId,
) -> Result<Result<TemporalKind, Vec<PreflightFinding>>, PrepareError> {
    let ExprKind::Scalar { op, operands, .. } = &spec.expr(e).kind else {
        return Err(PrepareError::Internal);
    };
    let path = checks::path(spec, e);
    Ok(Ok(match op {
        ast::ScalarOp::Diff { kind, unit, closed } => {
            let bounds = date_bounds(*closed)?;
            let unit = unit_name(*unit);
            if *kind == ast::TemporalType::DateTime {
                let Some(unit) = elapsed_unit(unit) else {
                    return Ok(Err(vec![PreflightFinding::Scalar(
                        Temporal::unit_diagnostic(&path, unit, ELAPSED_UNITS),
                    )]));
                };
                if bounds != DateBounds::Exclusive {
                    return Ok(Err(vec![bounds_finding(spec, path, *closed)?]));
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
                                Temporal::unit_diagnostic(&path, other, CALENDAR_UNITS),
                            )]))
                        }
                    },
                    bounds,
                }
            }
        }
        ast::ScalarOp::Shift { kind, unit } => {
            let datetime = *kind == ast::TemporalType::DateTime;
            let target = if datetime { "datetime" } else { "date" };
            let findings = [
                ("days", !datetime, written(spec, operands, "days")),
                ("amount", datetime, written(spec, operands, "amount")),
                ("unit", datetime, unit.is_some()),
            ]
            .into_iter()
            .filter(|&(_, wanted, present)| present != wanted)
            .map(|(name, wanted, _)| {
                PreflightFinding::Scalar(Temporal::type_field_diagnostic(
                    wanted, &path, name, target,
                ))
            })
            .collect::<Vec<_>>();
            if !findings.is_empty() {
                return Ok(Err(findings));
            }
            if datetime {
                TemporalKind::DateTimeShift {
                    unit: unit
                        .and_then(|unit| elapsed_unit(unit_name(unit)))
                        .ok_or(PrepareError::Internal)?,
                }
            } else {
                TemporalKind::DateShift
            }
        }
        ast::ScalarOp::StudyDay => TemporalKind::StudyDay,
        ast::ScalarOp::ToEpochDay => TemporalKind::EpochDay,
        ast::ScalarOp::DurationValue { unit } => TemporalKind::DurationValue {
            unit: elapsed_unit(unit_name(*unit)).ok_or(PrepareError::Internal)?,
        },
        ast::ScalarOp::TimeOfDay => TemporalKind::TimeOfDay,
        ast::ScalarOp::Dtc => {
            // Every part before the finest declared one must be declared too.
            let declared = DATE_PARTS.map(|name| {
                slot(name).is_some_and(|slot| checks::operand(operands, slot).is_some())
            });
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
                        &path,
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

/// The executor's temporal operation for the verb at `e`: each operand a
/// name, or a written number with the field's name as its source.
pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Temporal, PrepareError> {
    let ExprKind::Scalar {
        op,
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(PrepareError::Internal);
    };
    let path = checks::path(spec, e);
    let kind = tree_kind(spec, e)?.map_err(PrepareError::Invalid)?;
    let mut sources = Vec::new();
    let mut literals = Vec::new();
    for (index, field_name) in kind.fields().iter().enumerate() {
        let operand = slot(field_name)
            .and_then(|slot| checks::operand(operands, slot))
            .ok_or(PrepareError::Internal)?;
        match &spec.expr(operand).kind {
            ExprKind::Ref(name) => sources.push(name.0.clone()),
            ExprKind::Literal(value) => {
                sources.push(String::from(*field_name));
                literals.push((index, value, spec.expr(operand).meta));
            }
            _ => return Err(PrepareError::Internal),
        }
    }
    match Temporal::new(kind, sources, path.clone()) {
        Ok(mut operation) => {
            for (index, value, meta) in literals {
                operation =
                    operation.with_literal(index, constant(value, &checks::render(spec, meta))?);
            }
            if let Some(invalid) = checks::handler(spec, handlers.invalid.as_ref())? {
                operation = operation
                    .with_invalid(invalid)
                    .map_err(|_| PrepareError::Internal)?;
            }
            Ok(operation)
        }
        Err(crate::dataset::InvalidTemporal::Shape) => Err(PrepareError::Internal),
        Err(crate::dataset::InvalidTemporal::Bounds) => {
            let ast::ScalarOp::Diff { closed, .. } = op else {
                return Err(PrepareError::Internal);
            };
            Err(PrepareError::Invalid(vec![bounds_finding(
                spec, path, *closed,
            )?]))
        }
    }
}

/// One temporal verb's declaration findings: a unit, bounds or field its
/// `type` does not take, a gap in its date parts, and each operand whose
/// declared type it does not accept. With `rows` a bare name's phase is
/// binding's to decide, so its type is not checked here.
pub(super) fn operand_findings(
    spec: &ast::Spec,
    e: ExprId,
    declared: &types::DeclaredTypes<'_>,
    rows: bool,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let ExprKind::Scalar { op, operands, .. } = &spec.expr(e).kind else {
        return Err(PrepareError::Internal);
    };
    let path = checks::path(spec, e);
    let operation = match tree_kind(spec, e)? {
        Ok(operation) => operation,
        Err(errors) => {
            findings.extend(errors);
            return Ok(());
        }
    };
    if let TemporalKind::Difference { unit, bounds } = operation {
        if unit != CalendarUnit::Day && bounds != DateBounds::Exclusive {
            let ast::ScalarOp::Diff { closed, .. } = op else {
                return Err(PrepareError::Internal);
            };
            findings.push(bounds_finding(spec, path.clone(), *closed)?);
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
            .and_then(|slot| checks::operand(operands, slot))
            .ok_or(PrepareError::Internal)
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
            .collect::<Result<Vec<_>, PrepareError>>()?,
        path,
    )
    .map_err(|_| PrepareError::Internal)?;
    for (index, field_name) in operation.fields().iter().enumerate() {
        let ExprKind::Ref(source) = &spec.expr(operand(field_name)?).kind else {
            if matches!(
                spec.expr(operand(field_name)?).kind,
                ExprKind::Literal(ast::Literal::Float(_))
            ) && witness.expected(index) == "int"
            {
                findings.push(PreflightFinding::Scalar(
                    witness.operand_diagnostic(index, "float"),
                ));
            }
            continue;
        };
        let kind = (source.0.contains('.') || !rows)
            .then(|| declared.name(&source.0).column_type())
            .flatten();
        if let Some(kind) = kind.filter(|&kind| !witness.accepts(index, kind)) {
            findings.push(PreflightFinding::Scalar(
                witness.operand_diagnostic(index, checks::type_name(kind)),
            ));
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::front_end::v0_5::tests::{admit, column, derivation, document, op};
    use crate::json::Value as Json;
    use crate::test_document::Tree;

    /// TIME-5 refuses a datetime `diff` that counts both endpoints, naming
    /// the field and the values its surface writes: 0.5's `bounds`, and 1.0's
    /// `closed`, whose `both` lowers to the node 0.5's `inclusive` does
    /// (P-ORD-2).
    #[test]
    fn a_datetime_diff_s_endpoints_are_refused_as_its_surface_writes_them() {
        let tree = document(
            vec![column(
                "DAYS",
                "int",
                derivation(op(
                    "diff",
                    Tree::map(vec![
                        ("start", Tree::text("SRC.START")),
                        ("end", Tree::text("SRC.END")),
                        ("type", Tree::text("datetime")),
                        ("unit", Tree::text("day")),
                        ("bounds", Tree::text("inclusive")),
                    ]),
                )),
            )],
            vec![],
        );
        let mut spec = crate::front_end::v0_5::lower(&admit(tree), None).into_spec();
        let refusal = |spec: &ast::Spec| {
            let e = spec.columns[1].derivation.as_ref().unwrap().expr;
            let Ok(Err(findings)) = tree_kind(spec, e) else {
                panic!("TIME-5 refuses the endpoints");
            };
            let [finding] = findings.as_slice() else {
                panic!("one finding: {findings:?}");
            };
            finding.diagnostic()
        };
        let permitted = |value: &str| Json::Array(vec![Json::String(value.into())]);

        let diagnostic = refusal(&spec);
        assert_eq!(
            diagnostic.code,
            crate::diagnostic::ConditionCode::TemporalBounds
        );
        assert_eq!(
            diagnostic.spec_paths,
            ["columns.DAYS.derivation.diff.bounds"]
        );
        assert_eq!(
            diagnostic.context["value"],
            Json::String("inclusive".into())
        );
        assert_eq!(diagnostic.context["permitted"], permitted("exclusive"));

        spec.header.surface = ast::Surface::V1 { minor: 0 };
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
