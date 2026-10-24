//! The one closed expression language of fields typed `predicate` and
//! `aggregate.expr`, and of text in an expression field.
//!
//! One lexer and parser ([`parse`]) read the grammar of
//! `yaml/grammar/expression.yaml` into one arena. The binder ([`compile`])
//! takes the field's [`Context`] and checks, once, the constructs and
//! functions the context admits, the root, reductions and literal policy, and
//! compiles each regular-expression literal. Static operand types are checked
//! against the site's names by [`CompiledExpression::type_findings`], the
//! result type of expression-field text and of `aggregate` is
//! [`CompiledExpression::result_type`], and one evaluator runs every context.

mod bind;
mod evaluate;
mod syntax;
mod types;

pub use bind::{compile, AggregateResult, CompiledExpression, Reduction, ReductionOperand};
pub use evaluate::{Condition, ErrorKind, EvaluationError, Resolved, Resolver, Step, Truth};
pub use syntax::{
    parse, Comparison, Node, NodeKind, Parsed, SourcePosition, SourceSpan, TemporalKind, RESERVED,
};
pub use types::TypeFinding;

use crate::temporal::TemporalError;

/// The field kind an expression is written in, which fixes its root type and
/// the constructs and functions it admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    /// A field typed `predicate`: a Boolean root with three-valued logic.
    Predicate,
    /// `aggregate.expr`: a numeric root, or a bare reduction, over a relation.
    Aggregate,
    /// Text in an expression field, such as a `derivation`: any value as
    /// root over the current row. It admits numbers, quoted text, temporal
    /// literals, `NULL`, names, arithmetic and the numeric functions
    /// (FORMULA-27).
    Derivation,
}

/// A construct a context refuses wherever it is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Construct {
    /// A comparison, `IN`, `BETWEEN`, `LIKE` or `IS NULL`.
    Comparison,
    /// `AND`, `OR`, `NOT`, `TRUE` or `FALSE`.
    Boolean,
    String,
    Temporal,
}

impl Construct {
    /// The construct's name in diagnostic context.
    pub fn name(self) -> &'static str {
        match self {
            Self::Comparison => "comparison",
            Self::Boolean => "boolean",
            Self::String => "string",
            Self::Temporal => "temporal",
        }
    }
}

/// Closed reducer vocabulary; only the aggregate context admits a reduction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reducer {
    Sum,
    Count,
    Min,
    Max,
    Mean,
    Only,
    /// Joins a group's values into one text in the aggregate's declared
    /// order, with its separator (FORMULA-18).
    StrJoin,
    /// The statistical reducers, each with one named definition
    /// (FORMULA-18, FORMULA-19): the type 7 quantile at 0.5, 0.25, 0.75 and
    /// at the aggregate's `quantile`, the sample variance and its square
    /// root, and the geometric mean.
    Median,
    Q1,
    Q3,
    Quantile,
    StdDev,
    Variance,
    GeoMean,
}

impl Reducer {
    pub const ALL: [Self; 14] = [
        Self::Sum,
        Self::Count,
        Self::Min,
        Self::Max,
        Self::Mean,
        Self::Only,
        Self::StrJoin,
        Self::Median,
        Self::Q1,
        Self::Q3,
        Self::Quantile,
        Self::StdDev,
        Self::Variance,
        Self::GeoMean,
    ];
    /// Canonical spelling from the grammar contract.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sum => "SUM",
            Self::Count => "COUNT",
            Self::Min => "MIN",
            Self::Max => "MAX",
            Self::Mean => "MEAN",
            Self::Only => "ONLY",
            Self::StrJoin => "STR_JOIN",
            Self::Median => "MEDIAN",
            Self::Q1 => "Q1",
            Self::Q3 => "Q3",
            Self::Quantile => "QUANTILE",
            Self::StdDev => "STDDEV",
            Self::Variance => "VARIANCE",
            Self::GeoMean => "GEOMEAN",
        }
    }
    /// Recognize a reducer call without reserving the spelling as a name.
    pub(crate) fn lookup(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|reducer| name.eq_ignore_ascii_case(reducer.name()))
    }
}

/// Closed function vocabulary; the binder checks arity and context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    Abs,
    Ceil,
    Floor,
    Trunc,
    Sqrt,
    Power,
    Exp,
    Ln,
    Log10,
    NormCdf,
    NormInv,
    Mod,
    NullIf,
    StrContains,
}

impl Function {
    pub const ALL: [Self; 14] = [
        Self::Abs,
        Self::Ceil,
        Self::Floor,
        Self::Trunc,
        Self::Sqrt,
        Self::Power,
        Self::Exp,
        Self::Ln,
        Self::Log10,
        Self::NormCdf,
        Self::NormInv,
        Self::Mod,
        Self::NullIf,
        Self::StrContains,
    ];
    /// Canonical spelling from the grammar contract.
    pub fn name(self) -> &'static str {
        match self {
            Self::Abs => "ABS",
            Self::Ceil => "CEIL",
            Self::Floor => "FLOOR",
            Self::Trunc => "TRUNC",
            Self::Sqrt => "SQRT",
            Self::Power => "POWER",
            Self::Exp => "EXP",
            Self::Ln => "LN",
            Self::Log10 => "LOG10",
            Self::NormCdf => "NORM_CDF",
            Self::NormInv => "NORM_INV",
            Self::Mod => "MOD",
            Self::NullIf => "NULLIF",
            Self::StrContains => "STR_CONTAINS",
        }
    }
    /// Look up a case-insensitive spelling without extending the vocabulary.
    pub(crate) fn lookup(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|function| name.eq_ignore_ascii_case(function.name()))
    }
    /// Every function takes exactly this many arguments.
    pub fn arity(self) -> usize {
        match self {
            Self::Power | Self::Mod | Self::NullIf | Self::StrContains => 2,
            _ => 1,
        }
    }
    /// `str_contains` is the one function whose result is a truth value.
    pub fn is_numeric(self) -> bool {
        self != Self::StrContains
    }
}

/// A language finding of the parser or the binder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Text outside the grammar.
    Syntax,
    /// A temporal literal whose text the named type rejects.
    InvalidTemporal {
        kind: TemporalKind,
        error: TemporalError,
    },
    /// An `ESCAPE` literal that is not one scalar, or a dangling escape.
    InvalidEscape,
    /// A `str_contains` pattern the portable regex contract rejects.
    InvalidRegex { byte: usize, reason: &'static str },
    /// A `str_contains` pattern that is not a string literal (FORMULA-8).
    PatternNotLiteral,
    /// An integer literal outside the signed 64-bit range, as written.
    IntegerOverflow { literal: String },
    /// A construct the context refuses.
    ProhibitedConstruct { construct: Construct },
    /// A function or reducer the context does not admit, or another arity.
    /// The count is present only for a function the context admits.
    ProhibitedFunction {
        /// The written name, which starts at the finding's position.
        function: String,
        argument_count: Option<usize>,
    },
    /// A reduction inside another reduction's argument.
    NestedReduction { outer: Reducer, inner: Reducer },
}

/// A fixed boundary a compilation refusal names, never a language finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    /// Expression nesting beyond `Limits::SEMANTIC_DEPTH`.
    Depth,
    /// A regular-expression literal's own boundary.
    Regex(crate::portable_regex::Resource),
}

/// The text is refused, or a fixed resource boundary stops its admission.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error("expression invalid: position={position:?}, failure={failure:?}, message={message:?}")]
    Invalid {
        position: SourcePosition,
        failure: Failure,
        /// The production's explanation; hosts add the source location.
        message: String,
    },
    #[error("expression limit: position={position:?}, resource={resource:?}, limit={limit:?}")]
    Limit {
        position: SourcePosition,
        resource: Resource,
        limit: usize,
    },
}
