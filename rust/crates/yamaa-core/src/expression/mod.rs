//! The one closed expression language of fields typed `predicate`,
//! `compute.expr` and `aggregate.expr`.
//!
//! One lexer and parser ([`parse`]) read the grammar of
//! `yaml/grammar/expression.yaml` into one arena. The binder ([`compile`])
//! takes the field's [`Context`] and checks, once, the constructs and
//! functions the context admits, the root, reductions and literal policy, and
//! compiles each regular-expression literal. Static operand types are checked
//! against the site's names by [`CompiledExpression::type_findings`], and one
//! evaluator runs every context.

mod bind;
mod evaluate;
mod syntax;
mod types;

pub use bind::{
    compile, AggregateResult, CompiledExpression, Reduction, ReductionOperand, NUMERIC_RESULT_TYPE,
};
pub use evaluate::{Condition, ErrorKind, EvaluationError, Resolved, Resolver, Step, Truth};
pub use syntax::{
    parse, Comparison, Node, NodeKind, Parsed, SourcePosition, SourceSpan, TemporalKind, RESERVED,
};
pub(crate) use types::unify_exact;
pub use types::TypeFinding;

use crate::temporal::TemporalError;

/// The field kind an expression is written in, which fixes its root type and
/// the constructs and functions it admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    /// A field typed `predicate`: a Boolean root with three-valued logic.
    Predicate,
    /// `compute.expr`: a numeric root over the current row.
    Compute,
    /// `aggregate.expr`: a numeric root, or a bare reduction, over a relation.
    Aggregate,
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
}

impl Reducer {
    pub const ALL: [Self; 6] = [
        Self::Sum,
        Self::Count,
        Self::Min,
        Self::Max,
        Self::Mean,
        Self::Only,
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
    Mod,
    NullIf,
    StrContains,
}

impl Function {
    pub const ALL: [Self; 11] = [
        Self::Abs,
        Self::Ceil,
        Self::Floor,
        Self::Trunc,
        Self::Sqrt,
        Self::Power,
        Self::Exp,
        Self::Ln,
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
    /// An integer literal outside the signed 64-bit range.
    IntegerOverflow,
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
