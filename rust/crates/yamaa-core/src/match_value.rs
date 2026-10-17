//! REQ-1259's closed static comparison types, without evaluating expressions.
use crate::{key_relation::ComparableType, reference_binding::Catalog, value::ColumnType};

/// Host-projected scalar representation; no literal bytes or values cross this port.
#[derive(Clone, Copy, Debug)]
pub enum LiteralKind {
    Str,
    Int,
    Float,
    Bool,
    Missing,
    Other,
}

/// Normalized expression metadata; handlers and operation arguments are
/// irrelevant here, except the temporal type an operation declares with `type`.
#[derive(Clone, Copy, Debug)]
pub enum Expression<'a> {
    Source(&'a str),
    Literal(LiteralKind),
    Operation {
        name: &'a str,
        target: Option<ColumnType>,
    },
    Unresolved,
}

/// Select only the rule's known types; unknown types explicitly defer comparison to runtime.
pub fn result_type(catalog: &Catalog, expression: Expression<'_>) -> Option<ComparableType> {
    use ColumnType::{Date, DateTime, Float, Int, Str};
    match expression {
        Expression::Source(name) => catalog.type_of(name).map(Into::into),
        Expression::Literal(kind) => match kind {
            LiteralKind::Str => Some(Str.into()),
            LiteralKind::Int => Some(Int.into()),
            LiteralKind::Float => Some(Float.into()),
            LiteralKind::Bool => Some(ComparableType::Boolean),
            LiteralKind::Missing | LiteralKind::Other => None,
        },
        Expression::Operation { name, target } => match (name, target) {
            ("impute", Some(target @ (Date | DateTime))) => Some(target.into()),
            ("to_date", _) => Some(Date.into()),
            ("diff", Some(Date)) | ("rank" | "row_number" | "study_day" | "to_epoch_day", _) => {
                Some(Int.into())
            }
            ("compute", _) => Some(crate::numeric_compiler::NUMERIC_RESULT_TYPE.into()),
            ("round_half_away_from_zero", _) => Some(Float.into()),
            (
                "baseline_flag" | "cut" | "imputation_flag" | "precision" | "str_case"
                | "str_extract" | "str_pad" | "str_replace" | "str_template",
                _,
            ) => Some(Str.into()),
            _ => None,
        },
        Expression::Unresolved => None,
    }
}
