//! Complete a compiled numeric result before a dependent consumer can read it.
//!
//! This application slice covers evaluate -> convert -> unconvertible replacement
//! (REQ-0211-0214), not table execution, verification or specification decoding.

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use yamaa_core::conversion::{convert, ConversionError};
use yamaa_core::evaluation::NumericResolver;
use yamaa_core::numeric_compiler::{CompiledEvaluationError, CompiledNumeric};
use yamaa_core::value::{ColumnType, Value};

/// Handler sites supported by this application slice, not the full language registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlerKind {
    Unconvertible,
    MultipleMatches,
    NoMatch,
}

impl HandlerKind {
    /// Portable handler vocabulary spelling for later host report serialization.
    pub fn name(self) -> &'static str {
        match self {
            Self::Unconvertible => "unconvertible",
            Self::MultipleMatches => "multiple_matches",
            Self::NoMatch => "no_match",
        }
    }
}

/// One declared handler path's run-local count, including zero firings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerCount {
    pub spec_path: String,
    pub handler: HandlerKind,
    pub count: u64,
}

/// Explicit resource failure rather than wrapped or saturated audit data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerCountOverflow {
    pub spec_path: String,
    pub handler: HandlerKind,
}

/// Deterministic run-local accounting; register declarations before evaluating rows.
/// Re-registering a path preserves its position and count. No global state is used.
#[derive(Debug, Default)]
pub struct HandlerCounter {
    counts: Vec<HandlerCount>,
}

impl HandlerCounter {
    /// Register a declared path at zero, retaining first declaration order.
    pub fn register(&mut self, spec_path: &str, handler: HandlerKind) {
        if !self
            .counts
            .iter()
            .any(|entry| entry.spec_path == spec_path && entry.handler == handler)
        {
            self.counts.push(HandlerCount {
                spec_path: spec_path.to_string(),
                handler,
                count: 0,
            });
        }
    }

    /// Inspect all registered counts, including after a later evaluation failure.
    pub fn snapshot(&self) -> &[HandlerCount] {
        &self.counts
    }

    /// Count one selected replacement before its conversion; do not retry or overflow.
    pub(crate) fn record(
        &mut self,
        spec_path: &str,
        handler: HandlerKind,
    ) -> Result<(), HandlerCountOverflow> {
        self.register(spec_path, handler);
        let entry = self
            .counts
            .iter_mut()
            .find(|entry| entry.spec_path == spec_path && entry.handler == handler)
            .expect("handler was registered above");
        entry.count = entry
            .count
            .checked_add(1)
            .ok_or_else(|| HandlerCountOverflow {
                spec_path: spec_path.to_string(),
                handler,
            })?;
        Ok(())
    }
}

pub use yamaa_core::conversion::LiteralHandler;

/// Failures retain original expression provenance or completed-result conversion data.
/// Accounting overflow is a resource failure, not a normative conversion condition.
#[derive(Debug, PartialEq)]
pub enum NumericLifecycleError<E> {
    Evaluation(CompiledEvaluationError<E>),
    Conversion {
        spec_path: String,
        error: ConversionError,
    },
    HandlerConversion {
        spec_path: String,
        column_path: String,
        original: ConversionError,
        replacement: ConversionError,
    },
    Accounting {
        error: HandlerCountOverflow,
        column_path: String,
        original: ConversionError,
    },
}

/// Shared completed-value recovery failures; expression errors never enter this stage.
pub(crate) enum ConversionRecoveryError {
    Conversion {
        spec_path: String,
        error: ConversionError,
    },
    HandlerConversion {
        spec_path: String,
        column_path: String,
        original: ConversionError,
        replacement: ConversionError,
    },
    Accounting {
        error: HandlerCountOverflow,
        column_path: String,
        original: ConversionError,
    },
}

/// Count and convert at most one literal replacement after initial conversion failed.
pub(crate) fn recover_conversion(
    target: ColumnType,
    column_path: &str,
    original: ConversionError,
    handler: Option<&LiteralHandler>,
    counter: &mut HandlerCounter,
) -> Result<Value, Box<ConversionRecoveryError>> {
    let Some(handler) = handler else {
        return Err(Box::new(ConversionRecoveryError::Conversion {
            spec_path: column_path.into(),
            error: original,
        }));
    };
    if let Err(error) = counter.record(&handler.spec_path, HandlerKind::Unconvertible) {
        return Err(Box::new(ConversionRecoveryError::Accounting {
            error,
            column_path: column_path.into(),
            original,
        }));
    }
    convert(&handler.value, target).map_err(|replacement| {
        Box::new(ConversionRecoveryError::HandlerConversion {
            spec_path: handler.spec_path.clone(),
            column_path: column_path.into(),
            original,
            replacement,
        })
    })
}

/// An immutable application plan around already compiled numeric syntax.
/// Paths and handler literals come from a caller's normalized declaration; this
/// type does not parse specifications or validate expression-local handler fields.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericDerivation {
    expression: CompiledNumeric,
    target: ColumnType,
    column_path: String,
    unconvertible: Option<LiteralHandler>,
}

impl NumericDerivation {
    /// Bind result conversion and an optional literal handler without evaluating either.
    /// None means no handler; Some with Value::Missing means an explicit null handler.
    pub fn new(
        expression: CompiledNumeric,
        target: ColumnType,
        column_path: &str,
        unconvertible: Option<LiteralHandler>,
    ) -> Self {
        Self {
            expression,
            target,
            column_path: column_path.to_string(),
            unconvertible,
        }
    }

    /// Register declared zero counts before any row can fail or skip later declarations.
    pub fn register_handlers(&self, counter: &mut HandlerCounter) {
        if let Some(handler) = &self.unconvertible {
            counter.register(&handler.spec_path, HandlerKind::Unconvertible);
        }
    }

    /// Evaluate once, convert once, and if applicable convert one literal replacement.
    /// Arithmetic/resolver errors bypass conversion handling. A failed replacement is
    /// fatal without recursion, while the already-fired count remains in the counter.
    /// Reusing this plan repeats resolver effects; it provides no callback rollback.
    pub fn evaluate<R: NumericResolver>(
        &self,
        resolver: &mut R,
        counter: &mut HandlerCounter,
    ) -> Result<Value, Box<NumericLifecycleError<R::Error>>> {
        self.register_handlers(counter);
        let value = Value::from(
            self.expression
                .evaluate(resolver)
                .map_err(|error| Box::new(NumericLifecycleError::Evaluation(error)))?,
        );
        let original = match convert(&value, self.target) {
            Ok(converted) => return Ok(converted),
            Err(error) => error,
        };
        recover_conversion(
            self.target,
            &self.column_path,
            original,
            self.unconvertible.as_ref(),
            counter,
        )
        .map_err(|error| {
            Box::new(match *error {
                ConversionRecoveryError::Conversion { spec_path, error } => {
                    NumericLifecycleError::Conversion { spec_path, error }
                }
                ConversionRecoveryError::HandlerConversion {
                    spec_path,
                    column_path,
                    original,
                    replacement,
                } => NumericLifecycleError::HandlerConversion {
                    spec_path,
                    column_path,
                    original,
                    replacement,
                },
                ConversionRecoveryError::Accounting {
                    error,
                    column_path,
                    original,
                } => NumericLifecycleError::Accounting {
                    error,
                    column_path,
                    original,
                },
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Audit overflow fails without corrupting the previous count or adding entries.
    #[test]
    fn accounting_never_wraps_or_saturates_silently() {
        let mut counter = HandlerCounter::default();
        counter.register("handler", HandlerKind::Unconvertible);
        counter.counts[0].count = u64::MAX;
        assert_eq!(
            counter.record("handler", HandlerKind::Unconvertible),
            Err(HandlerCountOverflow {
                spec_path: "handler".into(),
                handler: HandlerKind::Unconvertible,
            })
        );
        assert_eq!(counter.snapshot().len(), 1);
        assert_eq!(counter.snapshot()[0].count, u64::MAX);
    }
}
