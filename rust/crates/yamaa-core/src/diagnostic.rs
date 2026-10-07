//! Portable semantic diagnostics. Serialization and host exceptions stay outside core.
//!
//! Registry keys identify causes, not public condition strings: the language uses
//! `conversion_failed` for several requirements. Migrated causes cover numeric
//! evaluation, completed-result conversion and classified resource failures; other families retain their
//! existing error types until their semantics and provenance migrate here.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use crate::{numeric_parser::SourceSpan, value::Value};

#[path = "diagnostic_numeric.rs"]
mod numeric;

/// A normative failure's public vocabulary, defined once for each semantic cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Definition {
    pub phase: &'static str,
    pub condition: &'static str,
    pub requirement: &'static str,
}

macro_rules! conditions {
    ($($code:ident => ($phase:literal, $condition:literal, $requirement:literal),)*) => {
        /// Stable internal cause identity; no wire spelling or numeric discriminant.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum ConditionCode { $($code,)* }

        impl ConditionCode {
            /// Resolve one canonical public mapping without consulting a host.
            pub fn definition(self) -> Definition {
                match self {
                    $(Self::$code => Definition {
                        phase: $phase, condition: $condition, requirement: $requirement,
                    },)*
                }
            }
        }

        /// Every migrated cause must be reached by the diagnostic integration tests.
        pub const CONDITIONS: &[ConditionCode] = &[$(ConditionCode::$code,)*];
    };
}

conditions! {
    NumericUnknownField => ("validation", "unknown_field", "REQ-0443"),
    NumericInputType => ("validation", "incompatible_input_type", "REQ-0444"),
    RoundingDigitsType => ("validation", "incompatible_input_type", "REQ-0418"),
    IntegerOverflow => ("derivation", "integer_overflow", "REQ-0434"),
    DivisionByZero => ("derivation", "division_by_zero", "REQ-0430"),
    SqrtOfNegative => ("derivation", "sqrt_of_negative", "REQ-0431"),
    LnOfNonpositive => ("derivation", "ln_of_nonpositive", "REQ-0432"),
    InvalidPower => ("derivation", "invalid_power", "REQ-0433"),
    ConversionInput => ("convert", "conversion_failed", "REQ-0013"),
    ConversionInteger => ("convert", "conversion_failed", "REQ-0021"),
    ConversionTemporal => ("convert", "conversion_failed", "REQ-0601"),
    ResourceMissing => ("validation", "resource_path_missing", "REQ-0785"),
    ResourceNotRegularFile => ("validation", "resource_path_not_regular_file", "REQ-0785"),
}

/// Diagnostic integers can exceed runtime i64; their canonical decimal text must
/// not be narrowed by a host or mistaken for a successful scalar result.
#[derive(Clone, Debug, PartialEq)]
pub enum ContextValue {
    Scalar(Value),
    Integer(String),
}

pub type Context = BTreeMap<String, ContextValue>;

/// Owned semantic data from the failure site. Missing optional geometry means
/// unavailable, not zero; ordered paths/routes preserve the original evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub code: ConditionCode,
    pub spec_paths: Vec<String>,
    pub context: Context,
    pub source_span: Option<SourceSpan>,
    pub operand_route: Option<Vec<String>>,
}

impl Diagnostic {
    pub fn definition(&self) -> Definition {
        self.code.definition()
    }
}
