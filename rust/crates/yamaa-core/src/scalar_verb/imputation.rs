//! Imputation declarations read their declared type, literal handlers and
//! named operand scope, and `date_text` its declared forms.
use super::*;
use crate::dataset::{
    completion_policy_findings, DatePolicyError, DayDeclaration, FlagPart, ImputationKind,
    TemporalType, TimeRule,
};
use crate::date_forms::DateForms;
/// The completion policy of `impute`, or the month, day and time findings
/// that refuse it, in declaration order.
fn policy(
    target: TemporalType,
    month: Option<&str>,
    day: Option<DayDeclaration<'_>>,
    time: Option<TimeRule>,
    path: &str,
) -> Result<ImputationKind, Refusal> {
    // `time` belongs to `type: datetime` alone; a finding keeps the policy
    // shape so the month and day findings are still reported.
    let (time, time_finding) = match (target, time) {
        (TemporalType::DateTime, None) => (Some(TimeRule::First), Some(true)),
        (TemporalType::Date, Some(_)) => (None, Some(false)),
        (_, time) => (time, None),
    };
    let time_finding = time_finding
        .map(|required| Temporal::type_field_diagnostic(required, path, "time", target.name()));
    match completion_policy_findings(target, month, day, time, path) {
        Ok(Ok(completion)) => match time_finding {
            None => Ok(ImputationKind::Impute(completion)),
            Some(finding) => Err(Refusal::Findings(vec![finding])),
        },
        Ok(Err(policy)) => Err(Refusal::Findings(
            policy.into_iter().chain(time_finding).collect(),
        )),
        Err(DatePolicyError::Diagnostic(diagnostic)) => Err(Refusal::Findings(vec![diagnostic])),
        Err(DatePolicyError::Shape) => Err(Refusal::Internal),
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
    day: Option<&ast::ImputeDay>,
    time: Option<ast::TimeRule>,
    path: &str,
) -> Result<ImputationKind, Refusal> {
    let target = tree_type(kind);
    let day = day.map(|day| match day {
        ast::ImputeDay::Int(value) => DayDeclaration::Number(value),
        ast::ImputeDay::DayRule(rule) => DayDeclaration::Position(rule.name()),
    });
    let time = time.map(|time| match time {
        ast::TimeRule::First => TimeRule::First,
        ast::TimeRule::Last => TimeRule::Last,
    });
    policy(target, month, day, time, path)
}

/// One imputation verb's executor operation from the tree. Each operand
/// binds as the scope decides; the operation needs every one.
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let operation = imputation(spec, e)?;
    let operands = operation
        .operands()
        .map(|(field, name)| Operand::name(name, format!("{}.{}", operation.path(), field)))
        .collect();
    Ok(Operation {
        kind: Kind::Imputation(operation),
        operands,
    })
}

fn imputation(spec: &Spec, e: ExprId) -> Result<Imputation, Refusal> {
    let ExprKind::Scalar {
        op,
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let path = path(spec, e);
    let kind = match op {
        ScalarOp::Impute {
            kind,
            month,
            day,
            time,
        } => tree_completion(*kind, month.as_deref(), day.as_ref(), *time, &path)?,
        ScalarOp::Precision { kind } => ImputationKind::Precision {
            target: tree_type(*kind),
        },
        ScalarOp::ImputationFlag {
            part,
            seconds_collected,
        } => ImputationKind::Flag {
            part: match part {
                ast::FlagPart::Date => FlagPart::Date,
                ast::FlagPart::Time => FlagPart::Time,
            },
            seconds_collected: *seconds_collected,
        },
        ScalarOp::ToDate => ImputationKind::ToDate,
        // The front end refuses a faulty list first (TIME-22); a tree it did
        // not check reports the same findings here.
        ScalarOp::DateText { forms } => {
            ImputationKind::DateText(DateForms::new(forms).map_err(|faults| {
                Refusal::Findings(
                    faults
                        .into_iter()
                        .map(|fault| fault.diagnostic(&path))
                        .collect(),
                )
            })?)
        }
        _ => return Err(Refusal::Internal),
    };
    let operand = |slot| operand_name(spec, operands, slot).map(String::from);
    Imputation::new(
        kind,
        operand(Slot::Source).ok_or(Refusal::Internal)?,
        operand(Slot::NotBefore),
        operand(Slot::NotAfter),
        operand(Slot::Fallback),
        path,
        handler(spec, handlers.invalid.as_ref())?,
    )
    .ok_or(Refusal::Internal)
}

/// One imputation verb's declaration findings at `path`: its completion
/// policy, then each operand whose declared type it does not accept. With
/// `rows` a bare name's phase is binding's to decide, so its type is not
/// checked here.
pub(super) fn preflight(
    spec: &Spec,
    e: ExprId,
    declared: &dyn Fn(&str) -> Option<ColumnType>,
    rows: bool,
) -> Result<Vec<Diagnostic>, Refusal> {
    let kind = |name: &str| {
        (name.contains('.') || !rows)
            .then(|| declared(name))
            .flatten()
    };
    match imputation(spec, e) {
        Ok(operation) => {
            let mut findings = Vec::new();
            if let Some(kind) = kind(operation.source()) {
                if !operation.accepts(kind) {
                    findings.push(operation.input_diagnostic(kind.name()));
                }
            }
            for (field, name) in operation.operands().skip(1) {
                if let Some(kind) = kind(name).filter(|&kind| !operation.accepts_operand(kind)) {
                    findings.push(operation.operand_input_diagnostic(field, name, kind.name()));
                }
            }
            Ok(findings)
        }
        Err(Refusal::Findings(errors)) => Ok(errors),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::Completion;
    use crate::temporal::Precision;
    use crate::test_document::Tree;
    use crate::test_specification::{column, derivation, document, lower, op};

    /// The completion of an `impute` of a date with `rules`.
    fn completion(rules: &[(&str, Tree)]) -> Result<Completion, Refusal> {
        let mut payload = vec![
            ("source", Tree::text("SRC.DTC")),
            ("type", Tree::text("date")),
        ];
        payload.extend(rules.iter().cloned());
        let spec = lower(document(
            vec![column(
                "DT",
                "date",
                derivation(op("impute", Tree::map(payload))),
            )],
            vec![],
        ));
        let e = spec.columns[1].derivation.as_ref().unwrap().expr;
        match imputation(&spec, e)?.kind() {
            ImputationKind::Impute(completion) => Ok(completion),
            _ => Err(Refusal::Internal),
        }
    }

    /// An `impute` completes the coarsest precision its rules reach, a year
    /// alone with `month` and `day`, a year and month with `day` alone, and a
    /// whole date with neither (TIME-8); `month` without `day` completes no
    /// date, a missing required field at the operation (TIME-10).
    #[test]
    fn an_impute_completes_the_precision_its_rules_imply() {
        let month = ("month", Tree::integer(6));
        let day = ("day", Tree::text("first"));
        for (rules, minimum) in [
            (vec![month.clone(), day.clone()], Precision::Year),
            (vec![day.clone()], Precision::Month),
            (vec![], Precision::Day),
        ] {
            assert_eq!(completion(&rules).unwrap().minimum, minimum);
        }
        let Err(Refusal::Findings(findings)) = completion(&[month]) else {
            panic!("a month without a day completes no date");
        };
        assert_eq!(
            findings,
            [crate::diagnostic::Diagnostic {
                code: crate::diagnostic::ConditionCode::ImputeMonthWithoutDay,
                spec_paths: vec!["columns.DT.derivation.impute".into()],
                context: crate::diagnostic::Context::from([(
                    "field".into(),
                    crate::json::Value::String("day".into())
                )]),
                source_span: None,
                operand_route: None,
            }]
        );
    }
}
