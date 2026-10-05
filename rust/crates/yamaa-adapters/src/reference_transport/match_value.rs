//! Decode shape metadata without interpreting expression result types in the adapter.
use serde::Deserialize;
use yamaa_core::match_value as core;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LiteralKind {
    Str,
    Int,
    Float,
    Bool,
    Missing,
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Expression {
    Source { name: String },
    Literal { scalar: LiteralKind },
    Operation { name: String },
    Unresolved {},
}

impl Expression {
    /// Borrow names and project scalar tags; the core alone selects the comparison type.
    pub(super) fn core(&self) -> core::Expression<'_> {
        match self {
            Self::Source { name } => core::Expression::Source(name),
            Self::Operation { name } => core::Expression::Operation(name),
            Self::Unresolved {} => core::Expression::Unresolved,
            Self::Literal { scalar } => core::Expression::Literal(match scalar {
                LiteralKind::Str => core::LiteralKind::Str,
                LiteralKind::Int => core::LiteralKind::Int,
                LiteralKind::Float => core::LiteralKind::Float,
                LiteralKind::Bool => core::LiteralKind::Bool,
                LiteralKind::Missing => core::LiteralKind::Missing,
                LiteralKind::Other => core::LiteralKind::Other,
            }),
        }
    }
}
