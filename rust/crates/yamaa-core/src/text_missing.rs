//! The text operations of one source that recover a missing source: case
//! substitution, a piece or a length by scalar position, and fixed-decimal
//! text. Each answers a missing source with its declared `missing`, or with
//! missing when that is omitted, and the run counts the recovery under
//! `missing`; only how a present value is transformed differs.
use crate::{diagnostic::Diagnostic, limits::LimitExceeded, table::ValueRef, value::Value};

/// `Recovered` is a missing source, answered by the declared `missing` or by
/// the default missing value.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Value(Value),
    Recovered,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    /// A source of a type the operation does not take.
    #[error("text operation diagnostic: {0:?}")]
    Diagnostic(Diagnostic),
    /// Text longer than `Limits::TEXT_BYTES`; only fixed-decimal text writes
    /// text longer than its source.
    #[error("text operation: {0}")]
    Limit(LimitExceeded),
}

/// One text operation whose missing source is recovered and counted.
pub trait MissingCountedText {
    /// The operation's declaration path, which its `missing` handler extends.
    fn path(&self) -> &str;
    /// The declared `missing` literal, if any.
    fn missing(&self) -> Option<&Value>;
    /// Missing precedes the source's type: a missing source is `Recovered`.
    fn apply(&self, value: ValueRef<'_>) -> Result<Step, Error>;
    /// The declared `missing` literal, or missing when it is omitted.
    fn missing_substitute(&self) -> Value {
        self.missing().cloned().unwrap_or(Value::Missing)
    }
}
