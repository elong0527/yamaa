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
fn integer(d: &Document, id: usize) -> Result<&str, PrepareError> {
    match &d.nodes()[id] {
        N::Integer(value) => Ok(value),
        _ => Err(PrepareError::Internal),
    }
}
/// The declared `type`, never inferred from runtime text.
fn target(d: &Document, payload: usize) -> Result<TemporalType, PrepareError> {
    match text(d, field(d, payload, "type")?)? {
        "date" => Ok(TemporalType::Date),
        "datetime" => Ok(TemporalType::DateTime),
        _ => Err(PrepareError::Internal),
    }
}
/// Read `impute`'s completion policy, collecting month, day and time findings
/// in declaration order.
fn completion(d: &Document, payload: usize, path: &str) -> Result<ImputationKind, PrepareError> {
    let target = target(d, payload)?;
    let minimum = match d
        .field(payload, "minimum_source_precision")
        .map(|id| text(d, id))
        .transpose()?
        .unwrap_or("year")
    {
        "year" => Precision::Year,
        "month" => Precision::Month,
        "day" => Precision::Day,
        _ => return Err(PrepareError::Internal),
    };
    let month = d
        .field(payload, "month")
        .map(|id| integer(d, id))
        .transpose()?;
    let day = d
        .field(payload, "day")
        .map(|id| match &d.nodes()[id] {
            N::Text(value) => Ok(DayDeclaration::Position(value)),
            N::Integer(value) => Ok(DayDeclaration::Number(value)),
            _ => Err(PrepareError::Internal),
        })
        .transpose()?;
    let time = d
        .field(payload, "time")
        .map(|id| match text(d, id)? {
            "first" => Ok(TimeRule::First),
            "last" => Ok(TimeRule::Last),
            _ => Err(PrepareError::Internal),
        })
        .transpose()?;
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
pub(super) fn prepare(
    d: &Document,
    payload: usize,
    op: &str,
    path: &str,
    _driver: Option<&str>,
    _intermediates: Option<&intermediates::Declarations>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Imputation, PrepareError> {
    let allowed: &[&str] = match op {
        "impute" => &[
            "type",
            "source",
            "month",
            "day",
            "minimum_source_precision",
            "time",
            "not_before",
            "not_after",
            "fallback",
            "invalid",
        ],
        "precision" => &["type", "source", "invalid"],
        "imputation_flag" => &["source", "part", "seconds_collected"],
        "to_date" => &["source"],
        _ => return Err(PrepareError::Internal),
    };
    for &(key, _) in mapping(d, payload)? {
        let name = text(d, key)?;
        if !allowed.contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let source = text(d, field(d, payload, "source")?)?;
    let operand = |name: &str| {
        d.field(payload, name)
            .map(|id| text(d, id).map(String::from))
            .transpose()
    };
    let kind = match op {
        "impute" => completion(d, payload, path)?,
        "precision" => ImputationKind::Precision {
            target: target(d, payload)?,
        },
        "imputation_flag" => ImputationKind::Flag {
            part: match text(d, field(d, payload, "part")?)? {
                "date" => FlagPart::Date,
                "time" => FlagPart::Time,
                _ => return Err(PrepareError::Internal),
            },
            seconds_collected: match d
                .field(payload, "seconds_collected")
                .map(|id| &d.nodes()[id])
            {
                None => true,
                Some(N::Boolean(value)) => *value,
                Some(_) => return Err(PrepareError::Internal),
            },
        },
        "to_date" => ImputationKind::ToDate,
        _ => return Err(PrepareError::Internal),
    };
    let invalid = d
        .field(payload, "invalid")
        .map(|id| literal(d, id, &format!("{path}.invalid")))
        .transpose()?;
    Imputation::new(
        kind,
        source.into(),
        operand("not_before")?,
        operand("not_after")?,
        operand("fallback")?,
        path.into(),
        invalid,
    )
    .ok_or(PrepareError::Internal)
}
/// One imputation verb's declaration findings at `path`: its completion
/// policy, then each operand whose declared type it does not accept. With
/// `rows` a bare name's phase is binding's to decide, so its type is not
/// checked here.
pub(super) fn operand_findings(
    d: &Document,
    payload: usize,
    op: &str,
    path: &str,
    declared: &types::DeclaredTypes<'_>,
    rows: bool,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let kind = |name: &str| {
        (name.contains('.') || !rows)
            .then(|| declared.name(name).column_type())
            .flatten()
    };
    match prepare(d, payload, op, path, None, None, &mut Vec::new()) {
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
