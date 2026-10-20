//! Imputation declarations read their declared type, literal handlers and
//! named operand scope.
use super::*;
use crate::{
    dataset::{
        completion_policy_findings, DatePolicyError, DayDeclaration, FlagPart, Imputation,
        ImputationKind, Temporal, TemporalType, TimeRule,
    },
    temporal::Precision,
};
/// The completion policy of `impute`, or the month, day and time findings
/// that refuse it, in declaration order.
fn policy(
    target: TemporalType,
    month: Option<&str>,
    day: Option<DayDeclaration<'_>>,
    minimum: Precision,
    time: Option<TimeRule>,
    path: &str,
) -> Result<ImputationKind, PrepareError> {
    // `time` belongs to `type: datetime` alone; a finding keeps the policy
    // shape so the month and day findings are still reported.
    let (time, time_finding) = match (target, time) {
        (TemporalType::DateTime, None) => (Some(TimeRule::First), Some(true)),
        (TemporalType::Date, Some(_)) => (None, Some(false)),
        (_, time) => (time, None),
    };
    let time_finding = time_finding.map(|required| {
        PreflightFinding::Imputation(Temporal::type_field_diagnostic(
            required,
            path,
            "time",
            target.name(),
        ))
    });
    match completion_policy_findings(target, month, day, minimum, time, path) {
        Ok(Ok(completion)) => match time_finding {
            None => Ok(ImputationKind::Impute(completion)),
            Some(finding) => Err(PrepareError::Invalid(vec![finding])),
        },
        Ok(Err(policy)) => Err(PrepareError::Invalid(
            policy
                .into_iter()
                .map(PreflightFinding::Imputation)
                .chain(time_finding)
                .collect(),
        )),
        Err(DatePolicyError::Diagnostic(diagnostic)) => {
            Err(PrepareError::Invalid(vec![PreflightFinding::Imputation(
                diagnostic,
            )]))
        }
        Err(DatePolicyError::Shape) => Err(PrepareError::Internal),
    }
}
fn tree_type(kind: ast::TemporalType) -> TemporalType {
    match kind {
        ast::TemporalType::Date => TemporalType::Date,
        ast::TemporalType::DateTime => TemporalType::DateTime,
    }
}

/// `impute`'s completion policy from the tree, collecting month, day and
/// time findings in declaration order.
fn tree_completion(
    kind: ast::TemporalType,
    month: Option<&str>,
    day: Option<&ast::DayRule>,
    minimum: ast::Precision,
    time: Option<ast::TimeRule>,
    path: &str,
) -> Result<ImputationKind, PrepareError> {
    let target = tree_type(kind);
    let minimum = match minimum {
        ast::Precision::Year => Precision::Year,
        ast::Precision::Month => Precision::Month,
        ast::Precision::Day => Precision::Day,
    };
    let day = day.map(|day| match day {
        ast::DayRule::Day(value) => DayDeclaration::Number(value),
        ast::DayRule::First => DayDeclaration::Position("first"),
        ast::DayRule::Last => DayDeclaration::Position("last"),
    });
    let time = time.map(|time| match time {
        ast::TimeRule::First => TimeRule::First,
        ast::TimeRule::Last => TimeRule::Last,
    });
    policy(target, month, day, minimum, time, path)
}

/// The coarsest collected precision an `impute` that writes no
/// `minimum_source_precision` completes. 0.5 defaults the field to `year`.
/// A 1.0 `impute` holds no such field (D61): its rules imply the precision,
/// a year alone with `month` and `day`, a year and month with `day` alone,
/// and a whole date with neither (TIME-8). Validation refuses `month`
/// without `day` (TIME-10).
fn unwritten_precision(spec: &ast::Spec, month: bool, day: bool) -> ast::Precision {
    match (spec.header.surface, month, day) {
        (ast::Surface::V0_5, _, _) | (_, true, _) => ast::Precision::Year,
        (_, false, true) => ast::Precision::Month,
        (_, false, false) => ast::Precision::Day,
    }
}

/// One imputation verb's executor operation from the tree.
pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Imputation, PrepareError> {
    let ExprKind::Scalar {
        op,
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(PrepareError::Internal);
    };
    let path = checks::path(spec, e);
    let kind = match op {
        ast::ScalarOp::Impute {
            kind,
            month,
            day,
            minimum_source_precision,
            time,
        } => tree_completion(
            *kind,
            month.as_deref(),
            day.as_ref(),
            minimum_source_precision
                .unwrap_or_else(|| unwritten_precision(spec, month.is_some(), day.is_some())),
            *time,
            &path,
        )?,
        ast::ScalarOp::Precision { kind } => ImputationKind::Precision {
            target: tree_type(*kind),
        },
        ast::ScalarOp::ImputationFlag {
            part,
            seconds_collected,
        } => ImputationKind::Flag {
            part: match part {
                ast::FlagPart::Date => FlagPart::Date,
                ast::FlagPart::Time => FlagPart::Time,
            },
            seconds_collected: *seconds_collected,
        },
        ast::ScalarOp::ToDate => ImputationKind::ToDate,
        _ => return Err(PrepareError::Internal),
    };
    let operand = |slot| checks::operand_name(spec, operands, slot).map(String::from);
    Imputation::new(
        kind,
        operand(ast::Slot::Source).ok_or(PrepareError::Internal)?,
        operand(ast::Slot::NotBefore),
        operand(ast::Slot::NotAfter),
        operand(ast::Slot::Fallback),
        path,
        checks::handler(spec, handlers.invalid.as_ref())?,
    )
    .ok_or(PrepareError::Internal)
}

/// One imputation verb's declaration findings at `path`: its completion
/// policy, then each operand whose declared type it does not accept. With
/// `rows` a bare name's phase is binding's to decide, so its type is not
/// checked here.
pub(super) fn operand_findings(
    spec: &ast::Spec,
    e: ExprId,
    declared: &types::DeclaredTypes<'_>,
    rows: bool,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let kind = |name: &str| {
        (name.contains('.') || !rows)
            .then(|| declared.name(name).column_type())
            .flatten()
    };
    match from_tree(spec, e) {
        Ok(operation) => {
            if let Some(kind) = kind(operation.source()) {
                if !operation.accepts(kind) {
                    findings.push(PreflightFinding::Imputation(
                        operation.input_diagnostic(checks::type_name(kind)),
                    ));
                }
            }
            for (field, name) in operation.operands().skip(1) {
                if let Some(kind) = kind(name).filter(|&kind| !operation.accepts_operand(kind)) {
                    findings.push(PreflightFinding::Imputation(
                        operation.operand_input_diagnostic(field, name, checks::type_name(kind)),
                    ));
                }
            }
            Ok(())
        }
        Err(PrepareError::Invalid(errors)) => {
            findings.extend(errors);
            Ok(())
        }
        Err(error) => Err(error),
    }
}
/// Each operand binds as the scope decides; the operation needs every one.
pub(super) fn bind(
    operation: &Imputation,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let mut operands = Vec::new();
    for (field, name) in operation.operands() {
        let path = format!("{}.{}", operation.path(), field);
        if let Some(expression) = cx.operand(name, &path)? {
            operands.push(Assignment {
                column,
                path,
                expression,
            });
        }
    }
    Ok(
        (operands.len() == operation.operands().count()).then(|| Expression::Imputation {
            operands,
            operation: operation.clone(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::Completion;
    use crate::front_end::v0_5::tests::{column, derivation, document, op};
    use crate::schema::Document;
    use crate::test_document::Tree;

    /// The completion of an `impute` of a date with `rules`, read from a
    /// tree lowered in `surface`.
    fn completion(
        rules: &[(&str, Tree)],
        surface: ast::Surface,
    ) -> Result<Completion, PrepareError> {
        let mut payload = vec![
            ("source", Tree::text("SRC.DTC")),
            ("type", Tree::text("date")),
        ];
        payload.extend(rules.iter().cloned());
        let tree = document(
            vec![column(
                "DT",
                "date",
                derivation(op("impute", Tree::map(payload))),
            )],
            vec![],
        );
        let mut nodes = Vec::new();
        let root = tree.append(&mut nodes);
        let document = Document::new(nodes, root).unwrap();
        let spec = crate::front_end::v0_5::lower_surface(&document, None, surface).into_spec();
        let e = spec.columns[1].derivation.as_ref().unwrap().expr;
        match from_tree(&spec, e)?.kind() {
            ImputationKind::Impute(completion) => Ok(completion),
            _ => Err(PrepareError::Internal),
        }
    }

    /// A 1.0 `impute` holds no `minimum_source_precision` (D61): it
    /// completes the coarsest precision its rules reach, a year alone with
    /// `month` and `day`, a year and month with `day` alone, and a whole date
    /// with neither (TIME-8). On a 0.5 tree the unwritten field is `year`,
    /// which requires a `month` (TIME-10 at the `yamaa-v0.5.0` tag).
    #[test]
    fn a_1_0_impute_completes_the_precision_its_rules_imply() {
        let v1 = ast::Surface::V1 { minor: 0 };
        let month = ("month", Tree::integer(6));
        let day = ("day", Tree::text("first"));
        for (rules, minimum) in [
            (vec![month.clone(), day.clone()], Precision::Year),
            (vec![day.clone()], Precision::Month),
            (vec![], Precision::Day),
        ] {
            assert_eq!(completion(&rules, v1).unwrap().minimum, minimum);
        }

        let Err(PrepareError::Invalid(findings)) = completion(&[day], ast::Surface::V0_5) else {
            panic!("a 0.5 minimum of year requires a month");
        };
        let [PreflightFinding::Imputation(diagnostic)] = findings.as_slice() else {
            panic!("one finding: {findings:?}");
        };
        // TIME-10 at the `yamaa-v0.5.0` tag.
        assert_eq!(
            diagnostic.code,
            crate::diagnostic::ConditionCode::ImputationMonthRequired
        );
    }
}
