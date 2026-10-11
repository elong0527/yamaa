//! Portable semantic diagnostics. Serialization and host exceptions stay outside core.
//!
//! Registry keys identify causes, not public condition strings: the language uses
//! `conversion_failed` for several requirements. Migrated causes cover numeric
//! evaluation, completed-result conversion, classified resource failures and
//! original-document preflight and output declarations. Other families retain their existing error types
//! until their semantics and provenance migrate here.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use crate::{numeric_parser::SourceSpan, value::Value};

#[path = "diagnostic_dataset.rs"]
mod dataset;
pub use dataset::DatasetFinding;

#[path = "diagnostic_inheritance.rs"]
mod inheritance;
pub use inheritance::InheritanceFinding;

#[path = "diagnostic_source.rs"]
mod source;
pub use source::{admit_source_diagnostics, ParquetFinding, SourceDiagnosticReport, SourceFinding};

#[path = "diagnostic_inherited_dependencies.rs"]
mod inherited_dependencies;
pub use inherited_dependencies::inheritance_dependency_diagnostics;

#[path = "diagnostic_grammar.rs"]
mod grammar;
#[path = "diagnostic_numeric.rs"]
mod numeric;
#[path = "diagnostic_predicate.rs"]
mod predicate;
pub use predicate::{
    PredicateDiagnosticError, PredicateGrammarDiagnosticError, MAX_PREDICATE_DIAGNOSTIC_BYTES,
};
#[path = "diagnostic_project_lock.rs"]
mod project_lock;
pub use project_lock::{
    lock_diagnostics, lock_diagnostics_with_limit, LockDiagnosticError, LockFinding, LockOrigins,
    LockReason, MAX_LOCK_DIAGNOSTIC_BYTES,
};

/// Context projection failures are resource/representation refusals, never a
/// replacement semantic cause. Admission precedes every owned context copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionError {
    Limit,
    InvalidContext,
}

pub const MAX_PROJECTION_BYTES: usize = 16_777_216;

pub(crate) struct ProjectionBudget(usize);
impl ProjectionBudget {
    pub(crate) fn new(maximum: usize) -> Result<Self, ProjectionError> {
        Ok(Self(
            maximum.checked_sub(256).ok_or(ProjectionError::Limit)?,
        ))
    }
    pub(crate) fn charge(&mut self, bytes: usize) -> Result<(), ProjectionError> {
        self.0 = self.0.checked_sub(bytes).ok_or(ProjectionError::Limit)?;
        Ok(())
    }
    pub(crate) fn text(&mut self, text: &str) -> Result<(), ProjectionError> {
        self.charge(
            text.len()
                .checked_mul(6)
                .and_then(|n| n.checked_add(2))
                .ok_or(ProjectionError::Limit)?,
        )
    }
    pub(crate) fn field(&mut self, name: &str, text: &str) -> Result<(), ProjectionError> {
        self.text(name)?;
        self.text(text)?;
        self.charge(2)
    }
}

/// A cause's canonical public vocabulary, including explicit application refusals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Definition {
    pub phase: &'static str,
    pub condition: &'static str,
    /// Language failures cite their owning rule. Only implementation refusals
    /// listed by REQ-1297 retain an absent normative requirement.
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
    ProjectEnvironmentInvalid => ("validation", "project_environment_invalid", Some("REQ-0695")),
    ProjectLockMismatch => ("validation", "runtime_artifact_mismatch", Some("REQ-0697")),
    RunnerLanguageMismatch => ("validation", "runner_language_mismatch", Some("REQ-0696")),
    CodelistDuplicateIdentifier => ("validation", "duplicate_define_identifier", Some("REQ-0948")),
    CodelistInvalidShape => ("validation", "codelist_shape_invalid", Some("REQ-0947")),
    CodelistInvalidValue => ("validation", "codelist_shape_invalid", Some("REQ-0950")),
    CodelistDuplicateValue => ("validation", "codelist_duplicate_value", Some("REQ-0949")),
    CodelistPartialItemField => ("validation", "codelist_partial_item_field", Some("REQ-0951")),
    CodelistExtensionNotAdmitted => ("validation", "codelist_extension_not_admitted", Some("REQ-0952")),
    UnknownProjectFunction => ("validation", "unknown_project_function", Some("REQ-0698")),
    InvalidFunctionArgument => ("validation", "invalid_function_argument", Some("REQ-0700")),
    RuntimeFunctionArgument => ("derivation", "invalid_function_argument", Some("REQ-0700")),
    RuntimeFunctionCallFailed => ("derivation", "function_call_failed", Some("REQ-0701")),
    RuntimeFunctionResult => ("derivation", "invalid_function_result", Some("REQ-0702")),
    FunctionConformanceFailed => ("validation", "function_conformance_failed", Some("REQ-0703")),
    ApplicationUnsupported => ("planning", "unsupported_operation", None),
    ApplicationRejected => ("engine", "engine_rejected", None),
    DatasetMultipleValues => ("derivation", "multiple_values_per_key", Some("REQ-0075")),
    DatasetUnmatchedKey => ("join", "unmatched_key", Some("REQ-0124")),
    DatasetMultipleMatches => ("join", "multiple_matches", Some("REQ-0127")),
    DatasetBaselineAmbiguity => ("derivation", "ambiguous_baseline", Some("REQ-0322")),
    DatasetReductionInputType => ("validation", "incompatible_input_type", Some("REQ-0510")),
    NumericUnknownField => ("validation", "unknown_field", Some("REQ-0443")),
    NumericInputType => ("validation", "incompatible_input_type", Some("REQ-0444")),
    RoundingDigitsType => ("validation", "incompatible_input_type", Some("REQ-0418")),
    StrPadWidth => ("validation", "invalid_field_type", Some("REQ-1261")),
    DeclarativeRoundingInput => ("validation", "incompatible_input_type", Some("REQ-0418")),
    TemporalOperand => ("validation", "incompatible_input_type", Some("REQ-0606")),
    ImputationMonthNotPermitted => ("validation", "month_not_permitted", Some("REQ-0592")),
    ImputationMonthRequired => ("validation", "month_required", Some("REQ-0592")),
    ImputationMonthRange => ("validation", "month_out_of_range", Some("REQ-0608")),
    ImputationDayRange => ("validation", "value_not_permitted", Some("REQ-0608")),
    ImputationDayRule => ("validation", "value_not_permitted", Some("REQ-0609")),
    DateImputeMissing => ("impute", "missing_input", Some("REQ-0588")),
    DateImputeInvalid => ("impute", "invalid_date_text", Some("REQ-0588")),
    DateImputeCalendar => ("impute", "invalid_calendar_date", Some("REQ-0608")),
    DatetimeImputeInput => ("validation", "incompatible_input_type", Some("REQ-1182")),
    DatetimeImputeMissing => ("impute", "missing_input", Some("REQ-1182")),
    DatetimeImputeInvalid => ("impute", "invalid_datetime_text", Some("REQ-1182")),
    DatetimePrecisionInput => ("validation", "incompatible_input_type", Some("REQ-1183")),
    DatetimePrecisionMissing => ("impute", "missing_input", Some("REQ-1183")),
    DatetimePrecisionInvalid => ("impute", "invalid_datetime_text", Some("REQ-1183")),
    ToDateInput => ("validation", "incompatible_input_type", Some("REQ-0607")),
    ToDateInvalid => ("validation", "invalid_date_text", Some("REQ-0607")),
    TemplateInvalid => ("validation", "invalid_string_template", Some("REQ-0461")),
    ExtractGroup => ("validation", "regex_group_out_of_range", Some("REQ-0816")),
    ExtractNoMatch => ("mapping", "unmatched_pattern", Some("REQ-0334")),
    ExtractUnknown => ("validation", "unknown_field", Some("REQ-0308")),
    StrCaseInput => ("validation", "incompatible_input_type", Some("REQ-0308")),
    StrCaseMissing => ("mapping", "missing_input", Some("REQ-0334")),
    TemporalBounds => ("validation", "value_not_permitted", Some("REQ-0613")),
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
    InheritanceInvalidParent => ("validation", "invalid_parent_path", Some("REQ-0653")),
    InheritanceUnknownDependency => ("validation", "unknown_reference", Some("REQ-0070")),
    InheritanceParentUnavailable => ("validation", "parent_not_found", Some("REQ-0654")),
    InheritanceCycle => ("validation", "inheritance_cycle", Some("REQ-0655")),
    InheritanceParentVersion => ("validation", "schema_version_mismatch", Some("REQ-0656")),
    InheritanceEntryVersion => ("validation", "schema_version_mismatch", Some("REQ-0245")),
    PreflightUndeclaredRowColumn => ("validation", "undeclared_column", Some("REQ-0202")),
    PreflightDuplicateRowDefault => ("validation", "duplicate_derivation", Some("REQ-1260")),
    PreflightMissingRowDerivation => ("validation", "missing_derivation", Some("REQ-0200")),
    PreflightConflictingRowConstruction => ("validation", "conflicting_row_construction", Some("REQ-1171")),
    PreflightInvalidGroup => ("validation", "invalid_field_type", Some("REQ-0065")),
    PreflightGroupReference => ("validation", "unknown_field", Some("REQ-0066")),
    PreflightRowDriverUnavailable => ("validation", "driver_unavailable", Some("REQ-0034")),
    PreflightMissingDerivation => ("validation", "missing_derivation", Some("REQ-0198")),
    PreflightUndeclaredKey => ("validation", "undeclared_column", Some("REQ-0220")),
    PreflightDriverUnavailable => ("validation", "driver_unavailable", Some("REQ-0064")),
    PreflightDomainInputCollision => ("validation", "duplicate_identifier", Some("REQ-0080")),
    PreflightRedundantSourceType => ("validation", "redundant_field_type", Some("REQ-0533")),
    ProducerInvalidContract => ("validation", "invalid_producer_contract", Some("REQ-0534")),
    ProducerWorkflowCycle => ("validation", "producer_workflow_cycle", Some("REQ-0534")),
    ProducerOutputPathMismatch => ("validation", "producer_output_path_mismatch", Some("REQ-0534")),
    ProducerRedundantSourceType => ("validation", "redundant_field_type", Some("REQ-0523")),
    ProducerContractMismatch => ("validation", "producer_contract_mismatch", Some("REQ-0535")),
    OutputUnknownProfile => ("validation", "unknown_artifact_profile", Some("REQ-0760")),
    MissingWarningLog => ("validation", "missing_warning_log", Some("REQ-0391")),
    WarningLogPathCollision => ("validation", "artifact_path_collision", Some("REQ-0756")),
    VerificationLogPathCollision => ("validation", "artifact_path_collision", Some("REQ-1180")),
    OutputInvalidDecimals => ("validation", "invalid_field_type", Some("REQ-0744")),
    OutputDecimalsNotApplicable => ("validation", "decimals_not_applicable", Some("REQ-0762")),
    OutputDuplicateColumn => ("validation", "duplicate_identifier", Some("REQ-0234")),
    OutputUndeclaredColumn => ("validation", "undeclared_column", Some("REQ-0234")),
    OutputInternalKey => ("validation", "internal_column_in_keys", Some("REQ-0220")),
    OutputPublicationFailed => ("output", "publication_failed", Some("REQ-0764")),
    NumericInvalidExpression => ("validation", "invalid_numeric_expression", Some("REQ-0439")),
    NumericProhibitedConstruct => ("validation", "prohibited_construct", Some("REQ-0441")),
    NumericProhibitedFunction => ("validation", "prohibited_function", Some("REQ-0440")),
    AggregateInvalidExpression => ("validation", "invalid_aggregate_expression", Some("REQ-0499")),
    AggregateProhibitedConstruct => ("validation", "prohibited_construct", Some("REQ-0512")),
    AggregateProhibitedFunction => ("validation", "prohibited_function", Some("REQ-0500")),
    AggregateNestedReduction => ("validation", "nested_reduction", Some("REQ-0502")),
    CsvInvalidText => ("ingest", "invalid_text", Some("REQ-0853")),
    CsvByteOrderMark => ("ingest", "source_byte_order_mark", Some("REQ-0851")),
    CsvHeaderAbsent => ("ingest", "source_header_absent", Some("REQ-0851")),
    CsvFieldNameEmpty => ("ingest", "source_field_name_empty", Some("REQ-0851")),
    CsvFieldNameDuplicate => ("ingest", "source_field_name_duplicate", Some("REQ-0851")),
    CsvRecordWidth => ("ingest", "source_record_width", Some("REQ-0851")),
    CsvQuoteUnterminated => ("ingest", "source_quote_unterminated", Some("REQ-0851")),
    CsvCarriageReturn => ("ingest", "source_carriage_return", Some("REQ-0851")),
    CsvTextAfterQuote => ("ingest", "source_text_after_quote", Some("REQ-0851")),
    CsvQuoteInBareField => ("ingest", "source_quote_in_bare_field", Some("REQ-0851")),
    GroupedRowReference => ("validation", "ungrouped_driver_field", Some("REQ-0067")),
    GroupedColumnReference => ("validation", "ungrouped_driver_field", Some("REQ-0107")),
    AggregateDriverScope => ("validation", "invalid_aggregate_context", Some("REQ-0329")),
    RowFilterPhaseBoundary => ("validation", "phase_boundary", Some("REQ-0068")),
    RowDependencyPhaseBoundary => ("validation", "phase_boundary", Some("REQ-0069")),
    QualifiedNumericReference => ("validation", "qualified_identifier", Some("REQ-0442")),
    SourceFilterReference => ("validation", "unknown_field", Some("REQ-0132")),
    SourceFilterSingleValue => ("validation", "prohibited_construct", Some("REQ-0148")),
    SourceUnknownReference => ("validation", "unknown_field", Some("REQ-0103")),
    VerificationUnknownReference => ("validation", "unknown_field", Some("REQ-0405")),
    VerificationNotMissingFailed => ("verification", "not_missing_failed", Some("REQ-0375")),
    VerificationAllowedValuesFailed => ("verification", "allowed_values_failed", Some("REQ-0376")),
    CodelistUnknownBinding => ("validation", "unknown_codelist", Some("REQ-0953")),
    CodelistBindingTypeMismatch => ("validation", "codelist_type_mismatch", Some("REQ-0954")),
    CodelistBindingValuesConflict => ("validation", "codelist_values_conflict", Some("REQ-0955")),
    VerificationCodelistFailed => ("verification", "allowed_values_failed", Some("REQ-0957")),
    VerificationRangeFailed => ("verification", "range_failed", Some("REQ-0377")),
    VerificationMaxLengthFailed => ("verification", "length_failed", Some("REQ-0378")),
    OutputUnknownReference => ("validation", "unknown_field", Some("REQ-0106")),
    OutputUnresolvableReference => ("validation", "unresolvable_name", Some("REQ-0106")),
    ColumnDependencyCycle => ("validation", "dependency_cycle", Some("REQ-0072")),
    ColumnForwardReference => ("validation", "forward_reference", Some("REQ-0071")),
    ColumnMissingKeyDerivation => ("validation", "key_dependency", Some("REQ-0074")),
    ColumnKeyDependency => ("validation", "key_dependency", Some("REQ-0074")),
    WindowZeroOffset => ("validation", "zero_offset", Some("REQ-0328")),
    WindowOrderRequired => ("validation", "window_order_by_required", Some("REQ-0340")),
    WindowOrderForbidden => ("validation", "window_order_by_forbidden", Some("REQ-0341")),
    LookupSelectionReference => ("validation", "unknown_field", Some("REQ-0120")),
    LookupDonorKeyReference => ("validation", "unknown_field", Some("REQ-0116")),
    LookupOutputKeyReference => ("validation", "unknown_field", Some("REQ-0117")),
    LookupValueReference => ("validation", "unknown_field", Some("REQ-0125")),
    LookupKeyType => ("validation", "incompatible_input_type", Some("REQ-0323")),
    FlagMissingValueRequired => ("validation", "missing_value_required", Some("REQ-1258")),
    PredicateUnknownField => ("validation", "unknown_field", Some("REQ-0189")),
    PredicateUnresolvableName => ("validation", "unresolvable_name", Some("REQ-0189")),
    PredicateInputType => ("validation", "incompatible_input_type", Some("REQ-0190")),
    PredicateIncompatiblePair => ("validation", "incompatible_input_type", Some("REQ-0190")),
    PredicateExpectedText => ("validation", "incompatible_input_type", Some("REQ-0190")),
    PredicateContainsExpectedText => ("validation", "incompatible_input_type", Some("REQ-1244")),
    PredicateDanglingEscape => ("validation", "invalid_predicate", Some("REQ-0191")),
    PredicateInvalidExpression => ("validation", "invalid_predicate", Some("REQ-0188")),
    PredicateInvalidEscape => ("validation", "invalid_predicate", Some("REQ-0191")),
    PredicateInvalidRegex => ("validation", "invalid_predicate", Some("REQ-1244")),
    RegexInvalidPattern => ("validation", "invalid_regex", Some("REQ-0827")),
    VerificationMatchesFailed => ("verification", "matches_failed", Some("REQ-0379")),
    VerificationIdentifierType => ("validation", "invalid_declaration", Some("REQ-0374")),
    VerificationDeclarationRequired => ("validation", "invalid_declaration", Some("REQ-0397")),
    VerificationIdentifierRepeated => ("validation", "duplicate_identifier", Some("REQ-0398")),
    VerificationBounds => ("validation", "invalid_declaration", Some("REQ-0399")),
    VerificationLengthBounds => ("validation", "invalid_declaration", Some("REQ-0400")),
    VerificationCardinality => ("validation", "invalid_declaration", Some("REQ-0401")),
    VerificationColumnType => ("validation", "invalid_declaration", Some("REQ-0404")),
    VerificationDeclaredColumnUnknown => ("validation", "unknown_field", Some("REQ-0405")),
    SourceTypeUnknownField => ("validation", "unknown_field", Some("REQ-0532")),
    SourceTypeFieldParse => ("ingest", "field_parse_failed", Some("REQ-0536")),
    ParquetInvalid => ("ingest", "source_parquet_invalid", Some("REQ-1038")),
    ParquetFieldNameEmpty => ("ingest", "source_field_name_empty", Some("REQ-1039")),
    ParquetFieldNameDuplicate => ("ingest", "source_field_name_duplicate", Some("REQ-1039")),
    ParquetFieldTypeUnsupported => ("ingest", "source_field_type_unsupported", Some("REQ-1040")),
    ParquetFieldValueInvalid => ("ingest", "source_field_value_invalid", Some("REQ-1041")),
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
