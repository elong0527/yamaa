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

/// Normalized expression metadata; handlers and operation arguments are irrelevant here.
#[derive(Clone, Copy, Debug)]
pub enum Expression<'a> {
    Source(&'a str),
    Literal(LiteralKind),
    Operation(&'a str),
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
        Expression::Operation(name) => match name {
            "date_impute" | "to_date" => Some(Date.into()),
            "datetime_impute" => Some(DateTime.into()),
            "date_diff" | "rank" | "row_number" | "study_day" | "to_epoch_day" => Some(Int.into()),
            "compute" => Some(crate::numeric_compiler::NUMERIC_RESULT_TYPE.into()),
            "round_half_away_from_zero" => Some(Float.into()),
            "baseline_flag" | "cut" | "date_precision" | "datetime_precision" | "str_case"
            | "str_concat" | "str_extract" | "str_pad" | "str_template" => Some(Str.into()),
            "str_contains" => Some(ComparableType::Boolean),
            _ => None,
        },
        Expression::Unresolved => None,
    }
}
