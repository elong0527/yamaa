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
//! [`CompiledExpression::result_type`], and one evaluator runs every context,
//! a row at a time or, in its column form ([`ColumnResolver`]), over a whole
//! selection.

mod bind;
mod columns;
mod evaluate;
mod syntax;
mod typed;
mod types;

pub use bind::{compile, AggregateResult, CompiledExpression, Reduction, ReductionOperand};
pub use columns::{Column, ColumnResolver, Lent, Stop};
pub use evaluate::{Condition, ErrorKind, EvaluationError, Resolved, Resolver, Step, Truth};
pub use syntax::{
    parse, Comparison, Node, NodeKind, Parsed, SourcePosition, SourceSpan, TemporalKind, RESERVED,
};
pub use types::TypeFinding;

/// The closed function and reducer vocabularies, generated from the
/// grammar's `vocabulary`: each name with its argument bounds, its result
/// and the contexts that admit it.
pub use crate::expression_table::{Function, FunctionResult, Reducer};

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
