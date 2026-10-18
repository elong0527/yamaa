//! EXPR-14: each prepared operation's one result type, from the static types
//! of what it reads, and VALUE-24's refusal of a result type its declared
//! column type can never take.
use super::*;
use crate::{
    dataset::{ElapsedUnit, ImputationKind, TemporalKind, TemporalType},
    diagnostic::{ConditionCode, Diagnostic},
    result_type::ResultType,
};

impl Operation {
    /// The expression's result type: fixed by the expression, set by a
    /// declared parameter, or taken from `input`, the static type of each
    /// name it reads. Local `missing`, `invalid`, `absent`, `unmapped` and
    /// `no_match` replacements take no part.
    pub(super) fn result_type(&self, input: &dyn Fn(&str) -> ResultType) -> ResultType {
        match self {
            // Fixed by the expression.
            Self::StrExtract(_)
            | Self::StrReplace(_)
            | Self::StrTemplate(_)
            | Self::StrCase(_)
            | Self::StrPad(_)
            | Self::Odm(_) => ResultType::STR,
            Self::Round(_) => ResultType::FLOAT,
            // Fixed or set by a declared parameter.
            Self::Temporal(operation) => temporal(operation.kind()),
            Self::Imputation(operation) => imputation(operation.kind()),
            Self::ProjectFunction { returns, .. } => ResultType::column(*returns),
            // Fixed, or taken from what it reads.
            Self::Window(window) => window.result_type(input),
            Self::MappingCut(declaration) => declaration.operation.result_type(),
            // Taken from what it reads.
            Self::Literal(value) => ResultType::of_value(value),
            Self::Source(source) => input(&source.variable),
            Self::FirstAvailable(selection) => selection
                .sources
                .iter()
                .fold(ResultType::of_value(&selection.missing), |kind, source| {
                    kind.or(input(&source.variable))
                }),
            Self::Scalar(operation) => operation.result_type(input),
            Self::Flag(operation) => operation.result_type(),
            Self::Case(branches) => branches.iter().fold(ResultType::MISSING, |kind, branch| {
                kind.or(branch.operation.result_type(input))
            }),
            Self::Compute(compiled) => compiled.result_type(input),
            Self::Aggregate(aggregate) => aggregate.result_type(input),
            Self::InvalidNumeric { .. } => ResultType::UNKNOWN,
        }
    }
}

/// Differences, study days, epoch days and times of day are counts; a
/// difference of datetimes in seconds is a whole count and in a longer unit a
/// fraction; a shift keeps its declared type; a duration is a fraction in
/// every unit; assembled date parts are text.
fn temporal(kind: TemporalKind) -> ResultType {
    match kind {
        TemporalKind::Difference { .. }
        | TemporalKind::StudyDay
        | TemporalKind::EpochDay
        | TemporalKind::TimeOfDay => ResultType::INT,
        TemporalKind::DateText { .. } => ResultType::STR,
        TemporalKind::DateTimeDifference {
            unit: ElapsedUnit::Second,
        } => ResultType::INT,
        TemporalKind::DateTimeDifference { .. } | TemporalKind::DurationValue { .. } => {
            ResultType::FLOAT
        }
        TemporalKind::DateShift => ResultType::DATE,
        TemporalKind::DateTimeShift { .. } => ResultType::DATETIME,
    }
}

/// `impute` returns its declared type, `to_date` a date, and `precision` and
/// `imputation_flag` text.
fn imputation(kind: ImputationKind) -> ResultType {
    match kind {
        ImputationKind::Impute(completion) => match completion.target {
            TemporalType::Date => ResultType::DATE,
            TemporalType::DateTime => ResultType::DATETIME,
        },
        ImputationKind::ToDate => ResultType::DATE,
        ImputationKind::Precision { .. } | ImputationKind::Flag { .. } => ResultType::STR,
    }
}

impl PreparedSpecification {
    /// VALUE-24 for an output column, named as a conversion failure names the
    /// column whose lifecycle cannot complete.
    pub(super) fn conversion_finding(
        &self,
        column: usize,
        result: ResultType,
    ) -> Option<Diagnostic> {
        let column = &self.output.columns()[column];
        conversion_finding(&format!("columns.{}", column.name), result, column.kind)
    }
}

/// VALUE-24: the derivation at `path` declares `target`, which no
/// non-missing result of type `result` can convert to. The caller exempts a
/// derivation that declares `unconvertible`, whose literal answers each one.
pub(super) fn conversion_finding(
    path: &str,
    result: ResultType,
    target: ColumnType,
) -> Option<Diagnostic> {
    (!result.converts_to(target)).then(|| Diagnostic {
        code: ConditionCode::ConversionResultType,
        spec_paths: vec![path.into()],
        context: [
            ("from".into(), crate::json::Value::String(result.name())),
            (
                "to".into(),
                crate::json::Value::String(target.name().into()),
            ),
        ]
        .into(),
        source_span: None,
        operand_route: None,
    })
}
