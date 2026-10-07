//! Portable semantic diagnostics. Serialization and host exceptions stay outside core.
//!
//! Registry keys identify causes, not public condition strings: the language uses
//! `conversion_failed` for several requirements. Migrated causes cover numeric
//! evaluation, completed-result conversion, classified resource failures and
//! original-document preflight and output declarations. Other families retain their existing error types
//! until their semantics and provenance migrate here.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use crate::{numeric_parser::SourceSpan, value::Value};

#[path = "diagnostic_grammar.rs"]
mod grammar;
#[path = "diagnostic_numeric.rs"]
mod numeric;

/// A normative failure's public vocabulary, defined once for each semantic cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Definition {
    pub phase: &'static str,
    pub condition: &'static str,
    /// Some existing language findings have no individually assigned requirement.
    /// Preserve that absence rather than inventing an identifier at the boundary.
    pub requirement: Option<&'static str>,
}

macro_rules! conditions {
    ($($code:ident => ($phase:literal, $condition:literal, $requirement:expr),)*) => {
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
    NumericUnknownField => ("validation", "unknown_field", Some("REQ-0443")),
    NumericInputType => ("validation", "incompatible_input_type", Some("REQ-0444")),
    RoundingDigitsType => ("validation", "incompatible_input_type", Some("REQ-0418")),
    IntegerOverflow => ("derivation", "integer_overflow", Some("REQ-0434")),
    DivisionByZero => ("derivation", "division_by_zero", Some("REQ-0430")),
    SqrtOfNegative => ("derivation", "sqrt_of_negative", Some("REQ-0431")),
    LnOfNonpositive => ("derivation", "ln_of_nonpositive", Some("REQ-0432")),
    InvalidPower => ("derivation", "invalid_power", Some("REQ-0433")),
    ConversionInput => ("convert", "conversion_failed", Some("REQ-0013")),
    ConversionInteger => ("convert", "conversion_failed", Some("REQ-0021")),
    ConversionTemporal => ("convert", "conversion_failed", Some("REQ-0601")),
    ResourceMissing => ("validation", "resource_path_missing", Some("REQ-0785")),
    ResourceNotRegularFile => ("validation", "resource_path_not_regular_file", Some("REQ-0785")),
    PreflightUndeclaredRowColumn => ("validation", "undeclared_column", None),
    PreflightDuplicateRowDefault => ("validation", "duplicate_derivation", Some("REQ-1260")),
    PreflightMissingRowDerivation => ("validation", "missing_derivation", Some("REQ-0200")),
    PreflightConflictingRowConstruction => ("validation", "conflicting_row_construction", Some("REQ-1171")),
    PreflightInvalidGroup => ("validation", "invalid_field_type", Some("REQ-0065")),
    PreflightGroupReference => ("validation", "unknown_field", Some("REQ-0066")),
    PreflightRowDriverUnavailable => ("validation", "driver_unavailable", None),
    PreflightMissingDerivation => ("validation", "missing_derivation", Some("REQ-0198")),
    PreflightUndeclaredKey => ("validation", "undeclared_column", Some("REQ-0220")),
    PreflightDriverUnavailable => ("validation", "driver_unavailable", None),
    PreflightDomainInputCollision => ("validation", "duplicate_identifier", Some("REQ-0080")),
    PreflightRedundantSourceType => ("validation", "redundant_field_type", Some("REQ-0533")),
    OutputUnknownProfile => ("validation", "unknown_artifact_profile", Some("REQ-0760")),
    OutputDuplicateColumn => ("validation", "duplicate_identifier", Some("REQ-0234")),
    OutputUndeclaredColumn => ("validation", "undeclared_column", Some("REQ-0234")),
    OutputInternalKey => ("validation", "internal_column_in_keys", Some("REQ-0220")),
    NumericInvalidExpression => ("validation", "invalid_numeric_expression", Some("REQ-0439")),
    NumericProhibitedConstruct => ("validation", "prohibited_construct", Some("REQ-0441")),
    NumericProhibitedFunction => ("validation", "prohibited_function", Some("REQ-0440")),
    AggregateInvalidExpression => ("validation", "invalid_aggregate_expression", Some("REQ-0499")),
    AggregateProhibitedConstruct => ("validation", "prohibited_construct", Some("REQ-0512")),
    AggregateProhibitedFunction => ("validation", "prohibited_function", Some("REQ-0500")),
    AggregateNestedReduction => ("validation", "nested_reduction", Some("REQ-0502")),
    GroupedRowReference => ("validation", "ungrouped_driver_field", Some("REQ-0067")),
    GroupedColumnReference => ("validation", "ungrouped_driver_field", Some("REQ-0107")),
    AggregateDriverScope => ("validation", "invalid_aggregate_context", Some("REQ-0329")),
    QualifiedNumericReference => ("validation", "qualified_identifier", Some("REQ-0442")),
    SourceUnknownReference => ("validation", "unknown_field", Some("REQ-0103")),
    OutputUnknownReference => ("validation", "unknown_field", None),
    OutputUnresolvableReference => ("validation", "unresolvable_name", None),
    ColumnDependencyCycle => ("validation", "dependency_cycle", Some("REQ-0072")),
    ColumnForwardReference => ("validation", "forward_reference", Some("REQ-0071")),
    ColumnMissingKeyDerivation => ("validation", "key_dependency", Some("REQ-0074")),
    ColumnKeyDependency => ("validation", "key_dependency", Some("REQ-0074")),
}

/// Owned context retains scalar kinds and ordered sequences. Diagnostic integers
/// can exceed runtime i64; their canonical decimal text must not be narrowed by a
/// host or mistaken for a successful scalar result.
#[derive(Clone, Debug, PartialEq)]
pub enum ContextValue {
    Scalar(Value),
    Integer(String),
    Sequence(Vec<ContextValue>),
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
