//! Typed semantic diagnostics and canonical owned JSON context.
//! Public issue envelopes and host exceptions belong to adapters.
//!
//! Registry keys identify causes, not public condition strings: the language uses
//! `conversion_failed` for several requirements. Migrated causes cover numeric
//! evaluation, completed-result conversion, classified resource failures and
//! original-document preflight and output declarations. Other families retain their existing error types
//! until their semantics and provenance migrate here.

use std::collections::BTreeMap;

use crate::expression::SourceSpan;

mod dataset;
pub use dataset::DatasetFinding;

pub(crate) mod lookup;

mod aggregate;
pub use aggregate::{AggregateContextCause, AggregateFinding};

mod odm;
pub use odm::{OdmDeclared, OdmFinding, OdmLocation};

mod inheritance;
pub use inheritance::InheritanceFinding;

mod source;
pub use source::{source_diagnostics, ParquetFinding, SourceFinding};

mod inherited_dependencies;
pub use inherited_dependencies::inheritance_dependency_diagnostics;

mod expression;
mod numeric;
pub use expression::EvaluationDiagnosticError;
mod project_lock;
pub use project_lock::{
    lock_diagnostics, LockDiagnosticError, LockFinding, LockOrigins, LockReason,
};

/// Contradictory retained facts have no semantic projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProjectionError {
    #[error("diagnostic invalid context")]
    InvalidContext,
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

/// Normative ownership of scalar submission decoder facts. These citations do
/// not assign a wire condition or phase to raw document shape findings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScalarSubmissionRule {
    Identifier,
    DefineId,
    Href,
    Language,
    StandardName,
    Core,
    OriginType,
    OriginSource,
    MethodType,
    DataType,
    DatasetClass,
    DatasetSubclass,
}

impl ScalarSubmissionRule {
    pub fn requirement(self) -> &'static str {
        match self {
            Self::Identifier => "REQ-1046",
            Self::DefineId => "REQ-1068",
            Self::Href => "REQ-1070",
            Self::Language => "REQ-1073",
            Self::StandardName => "REQ-1074",
            Self::Core => "REQ-1141",
            Self::OriginType => "REQ-1142",
            Self::OriginSource => "REQ-1143",
            Self::MethodType => "REQ-1144",
            Self::DataType => "REQ-1145",
            Self::DatasetClass => "REQ-1146",
            Self::DatasetSubclass => "REQ-1147",
        }
    }
}

/// Each citation owner is reached by independent raw decoder controls.
pub const SCALAR_SUBMISSION_RULES: &[ScalarSubmissionRule] = &[
    ScalarSubmissionRule::Identifier,
    ScalarSubmissionRule::DefineId,
    ScalarSubmissionRule::Href,
    ScalarSubmissionRule::Language,
    ScalarSubmissionRule::StandardName,
    ScalarSubmissionRule::Core,
    ScalarSubmissionRule::OriginType,
    ScalarSubmissionRule::OriginSource,
    ScalarSubmissionRule::MethodType,
    ScalarSubmissionRule::DataType,
    ScalarSubmissionRule::DatasetClass,
    ScalarSubmissionRule::DatasetSubclass,
];

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
    ProjectEnvironmentMissing => ("validation", "project_environment_missing", Some("REQ-0694")),
    ProjectEnvironmentInvalid => ("validation", "project_environment_invalid", Some("REQ-0695")),
    ProjectLockMismatch => ("validation", "runtime_artifact_mismatch", Some("REQ-0697")),
    CodelistDuplicateIdentifier => ("validation", "duplicate_define_identifier", Some("REQ-0948")),
    CodelistInvalidShape => ("validation", "codelist_shape_invalid", Some("REQ-0947")),
    CodelistInvalidValue => ("validation", "codelist_shape_invalid", Some("REQ-0950")),
    CodelistDuplicateValue => ("validation", "codelist_duplicate_value", Some("REQ-0949")),
    CodelistPartialItemField => ("validation", "codelist_partial_item_field", Some("REQ-0951")),
    CodelistExtensionNotAdmitted => ("validation", "codelist_extension_not_admitted", Some("REQ-0952")),
    CodelistExtendedAlias => ("validation", "codelist_extended_alias", Some("CT-6")),
    UnknownProjectFunction => ("validation", "unknown_project_function", Some("REQ-0698")),
    InvalidFunctionArgument => ("validation", "invalid_function_argument", Some("REQ-0700")),
    RuntimeFunctionArgument => ("derivation", "invalid_function_argument", Some("REQ-0700")),
    RuntimeFunctionCallFailed => ("derivation", "function_call_failed", Some("REQ-0701")),
    RuntimeFunctionResult => ("derivation", "invalid_function_result", Some("REQ-0702")),
    FunctionConformanceFailed => ("validation", "function_conformance_failed", Some("REQ-0703")),
    ApplicationUnsupported => ("planning", "unsupported_operation", None),
    ApplicationRejected => ("engine", "engine_rejected", None),
    DatasetMultipleValues => ("derivation", "multiple_values_per_key", Some("REQ-0075")),
    LookupUnmatchedKey => ("join", "unmatched_key", Some("LOOKUP-24")),
    DatasetMultipleMatches => ("join", "multiple_matches", Some("REQ-0127")),
    DatasetBaselineAmbiguity => ("derivation", "ambiguous_baseline", Some("REQ-0322")),
    DatasetReductionInputType => ("validation", "incompatible_input_type", Some("REQ-0510")),
    NumericUnknownField => ("validation", "unknown_field", Some("REQ-0443")),
    NumericInputType => ("validation", "incompatible_input_type", Some("REQ-0444")),
    ExtremeIncomparable => ("validation", "incomparable_sources", Some("REQ-0324")),
    StrPadWidth => ("validation", "invalid_field_type", Some("REQ-1261")),
    DeclarativeRoundingInput => ("validation", "incompatible_input_type", Some("REQ-0418")),
    TemporalOperand => ("validation", "incompatible_input_type", Some("REQ-0606")),
    DateShiftInput => ("validation", "incompatible_input_type", Some("REQ-1298")),
    DateShiftRange => ("convert", "conversion_failed", Some("REQ-1298")),
    DatetimeDifferenceInput => ("validation", "incompatible_input_type", Some("REQ-1299")),
    DatetimeShiftInput => ("validation", "incompatible_input_type", Some("REQ-1300")),
    DatetimeShiftRange => ("convert", "conversion_failed", Some("REQ-1300")),
    DurationValueInput => ("validation", "incompatible_input_type", Some("REQ-1301")),
    DurationInvalid => ("impute", "invalid_duration_text", Some("REQ-1301")),
    DateTextInput => ("validation", "incompatible_input_type", Some("TIME-19")),
    DateTextGap => ("validation", "missing_required_field", Some("TIME-19")),
    DateTextInvalid => ("impute", "invalid_date_text", Some("TIME-19")),
    TimeOfDayInput => ("validation", "incompatible_input_type", Some("TIME-20")),
    TemporalUnit => ("validation", "value_not_permitted", Some("TIME-4")),
    TemporalFieldRequired => ("validation", "missing_required_field", Some("TIME-1")),
    TemporalFieldNotPermitted => ("validation", "unknown_field", Some("TIME-1")),
    ImputationMonthNotPermitted => ("validation", "month_not_permitted", Some("REQ-0592")),
    ImputationMonthRequired => ("validation", "month_required", Some("REQ-0592")),
    ImputationMonthRange => ("validation", "month_out_of_range", Some("REQ-0608")),
    ImputationDayRequired => ("validation", "day_required", Some("TIME-10")),
    ImputationDayNotPermitted => ("validation", "day_not_permitted", Some("TIME-10")),
    ImputationDayRange => ("validation", "value_not_permitted", Some("REQ-0608")),
    ImputationDayRule => ("validation", "value_not_permitted", Some("REQ-0609")),
    ImputeInvalid => ("impute", "invalid_date_text", Some("REQ-0588")),
    ImputeIntervalText => ("impute", "interval_or_duration_text", Some("TIME-13")),
    ImputeCalendar => ("impute", "invalid_calendar_date", Some("REQ-0608")),
    ImputeDatetimeInput => ("validation", "incompatible_input_type", Some("REQ-1182")),
    PrecisionDatetimeInput => ("validation", "incompatible_input_type", Some("REQ-1183")),
    ImputationFlagInput => ("validation", "incompatible_input_type", Some("TIME-18")),
    ToDateInput => ("validation", "incompatible_input_type", Some("REQ-0607")),
    ToDateInvalid => ("validation", "invalid_date_text", Some("REQ-0607")),
    ToDateIntervalText => ("validation", "interval_or_duration_text", Some("TIME-2")),
    TemplateInvalid => ("validation", "invalid_string_template", Some("REQ-0461")),
    TemplateMissing => ("mapping", "missing_input", Some("REQ-0334")),
    ExtractGroup => ("validation", "regex_group_out_of_range", Some("REQ-0816")),
    ExtractNoMatch => ("mapping", "unmatched_pattern", Some("REQ-0334")),
    ExtractUnknown => ("validation", "unknown_field", Some("REQ-0308")),
    StrCaseInput => ("validation", "incompatible_input_type", Some("REQ-0308")),
    MappingInput => ("validation", "incompatible_input_type", Some("REQ-0304")),
    MappingAmbiguousDictionary => ("validation", "ambiguous_dictionary", Some("REQ-0714")),
    MappingUnmapped => ("mapping", "unmapped_value", Some("REQ-0334")),
    CutInput => ("validation", "incompatible_input_type", Some("REQ-0306")),
    CutDeclaration => ("validation", "invalid_field_type", Some("REQ-1118")),
    TemporalBounds => ("validation", "value_not_permitted", Some("REQ-0613")),
    IntegerOverflow => ("derivation", "integer_overflow", Some("REQ-0434")),
    LiteralIntegerOverflow => ("validation", "integer_overflow", Some("REQ-0014")),
    DivisionByZero => ("derivation", "division_by_zero", Some("REQ-0430")),
    SqrtOfNegative => ("derivation", "sqrt_of_negative", Some("REQ-0431")),
    LnOfNonpositive => ("derivation", "ln_of_nonpositive", Some("REQ-0432")),
    InvalidPower => ("derivation", "invalid_power", Some("REQ-0433")),
    ConversionInput => ("convert", "conversion_failed", Some("REQ-0013")),
    ConversionInteger => ("convert", "conversion_failed", Some("REQ-0021")),
    ConversionTemporal => ("convert", "conversion_failed", Some("REQ-0601")),
    ConversionResultType => ("validation", "conversion_failed", Some("VALUE-24")),
    ResourceMissing => ("validation", "resource_path_missing", Some("REQ-0785")),
    ResourceNotRegularFile => ("validation", "resource_path_not_regular_file", Some("REQ-0785")),
    ResourceUriScheme => ("validation", "resource_path_uri_scheme", Some("REQ-0775")),
    ResourceBackslash => ("validation", "resource_path_not_normalized", Some("REQ-0776")),
    ResourceEmptySegment => ("validation", "resource_path_not_normalized", Some("REQ-0777")),
    ResourceSymlink => ("validation", "resource_path_symlink", Some("REQ-0783")),
    ResourceContentChanged => ("ingest", "resource_path_content_changed", Some("REQ-0788")),
    SchemaModelInvalidValue => ("validation", "model_contract_mismatch", Some("REQ-0287")),
    SchemaModelMissingField => ("validation", "model_contract_mismatch", Some("REQ-0285")),
    SchemaModelUnknownField => ("validation", "model_contract_mismatch", Some("REQ-0285")),
    SchemaModelInvalidOperationCount => ("validation", "model_contract_mismatch", Some("REQ-0286")),
    SchemaInvalidFieldType => ("validation", "invalid_field_type", Some("REQ-0287")),
    SchemaMissingRequiredField => ("validation", "missing_required_field", Some("REQ-0285")),
    SchemaUnknownField => ("validation", "unknown_field", Some("REQ-0285")),
    SchemaInvalidOperationCount => ("validation", "invalid_operation_count", Some("REQ-0286")),
    SchemaUnknownOperation => ("validation", "unknown_operation", Some("REQ-0286")),
    SchemaVersionMismatch => ("validation", "schema_version_mismatch", Some("REQ-0281")),
    SchemaBareDerivationScalar => ("validation", "bare_derivation_scalar", Some("REQ-0320")),
    SchemaRedundantLongForm => ("validation", "redundant_long_form", Some("SCHEMA-15")),
    SchemaCaseItems => ("validation", "invalid_field_type", Some("REQ-0339")),
    SchemaLayerInvalidFieldType => ("validation", "invalid_field_type", Some("REQ-0658")),
    SchemaLayerUnknownField => ("validation", "unknown_field", Some("REQ-0658")),
    SchemaLayerMissingRequiredField => ("validation", "missing_required_field", Some("REQ-0658")),
    SchemaLayerInvalidClear => ("validation", "invalid_clear", Some("REQ-0660")),
    SchemaLayerIdentifierRepeated => ("validation", "duplicate_identifier", Some("REQ-0659")),
    SchemaLayerVersionMismatch => ("validation", "schema_version_mismatch", Some("REQ-0656")),
    SchemaUnknownWindow => ("validation", "unknown_window", Some("REQ-1253")),
    SchemaResolvedOutputRequired => ("validation", "missing_required_field", Some("REQ-0657")),
    SchemaDescriptorValueNotPermitted => ("validation", "value_not_permitted", Some("REQ-0287")),
    SchemaDescriptorPatternMismatch => ("validation", "pattern_mismatch", Some("REQ-0287")),
    SchemaDescriptorMinimumLength => ("validation", "minimum_length", Some("REQ-0287")),
    SchemaDescriptorInvalidSize => ("validation", "invalid_size", Some("REQ-0287")),
    SchemaColumnTypeValueNotPermitted => ("validation", "value_not_permitted", Some("REQ-0012")),
    SchemaColumnTypePatternMismatch => ("validation", "pattern_mismatch", Some("REQ-0012")),
    SchemaColumnTypeMinimumLength => ("validation", "minimum_length", Some("REQ-0012")),
    SchemaColumnTypeInvalidSize => ("validation", "invalid_size", Some("REQ-0012")),
    SchemaDayRuleValueNotPermitted => ("validation", "value_not_permitted", Some("REQ-0609")),
    SchemaDayRulePatternMismatch => ("validation", "pattern_mismatch", Some("REQ-0609")),
    SchemaDayRuleMinimumLength => ("validation", "minimum_length", Some("REQ-0609")),
    SchemaDayRuleInvalidSize => ("validation", "invalid_size", Some("REQ-0609")),
    SchemaTimeRuleValueNotPermitted => ("validation", "value_not_permitted", Some("REQ-1184")),
    SchemaTimeRulePatternMismatch => ("validation", "pattern_mismatch", Some("REQ-1184")),
    SchemaTimeRuleMinimumLength => ("validation", "minimum_length", Some("REQ-1184")),
    SchemaTimeRuleInvalidSize => ("validation", "invalid_size", Some("REQ-1184")),
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
    PreflightEmptyKeys => ("validation", "invalid_field_type", Some("REQ-0220")),
    PreflightDuplicateKey => ("validation", "duplicate_identifier", Some("REQ-0220")),
    PreflightDuplicateColumn => ("validation", "duplicate_identifier", Some("REQ-0238")),
    PreflightDuplicateRow => ("validation", "duplicate_identifier", Some("REQ-1056")),
    PreflightDriverUnavailable => ("validation", "driver_unavailable", Some("REQ-0064")),
    PreflightDomainInputCollision => ("validation", "duplicate_identifier", Some("REQ-0080")),
    PreflightRedundantSourceType => ("validation", "redundant_field_type", Some("REQ-0533")),
    SourceUnknownProfile => ("validation", "source_profile_unknown", Some("REQ-0852")),
    SasSourceEncoding => ("validation", "source_encoding_required", Some("REQ-1311")),
    SasInvalid => ("ingest", "source_sas_invalid", Some("REQ-1317")),
    SourceOrdinalCollision => ("validation", "ordinal_field_collision", Some("REQ-1294")),
    SourceEmptyStringPresentUnsupported => ("validation", "empty_string_present_unsupported", Some("REQ-1161")),
    ProducerInvalidContract => ("validation", "invalid_producer_contract", Some("REQ-0534")),
    ProducerRedundantSourceType => ("validation", "redundant_field_type", Some("REQ-0523")),
    ProducerContractMismatch => ("validation", "producer_contract_mismatch", Some("REQ-0535")),
    ProjectDuplicateName => ("validation", "duplicate_specification_name", Some("REQ-1338")),
    ProjectUnknownName => ("validation", "unknown_specification_name", Some("REQ-1339")),
    ProjectUnlinkedProducer => ("validation", "unlinked_producer", Some("REQ-1344")),
    ProjectProducerNotCompleted => ("validation", "producer_not_completed", Some("REQ-1345")),
    ProducerWorkflowCycle => ("validation", "producer_workflow_cycle", Some("REQ-1342")),
    OutputUnknownProfile => ("validation", "unknown_artifact_profile", Some("REQ-0760")),
    MissingReport => ("validation", "missing_report", Some("REQ-0391")),
    ReportPathCollision => ("validation", "artifact_path_collision", Some("REQ-0756")),
    OutputInvalidDecimals => ("validation", "invalid_field_type", Some("REQ-0744")),
    OutputDecimalsNotApplicable => ("validation", "decimals_not_applicable", Some("REQ-0762")),
    OutputDuplicateColumn => ("validation", "duplicate_identifier", Some("REQ-0234")),
    OutputUndeclaredColumn => ("validation", "undeclared_column", Some("REQ-0234")),
    OutputInternalKey => ("validation", "internal_column_in_keys", Some("REQ-0220")),
    OutputDuplicateOrderTerm => ("validation", "duplicate_order_term", Some("REQ-0224")),
    OutputUndeclaredOrderColumn => ("validation", "undeclared_column", Some("REQ-0224")),
    OutputPublicationFailed => ("output", "publication_failed", Some("REQ-0764")),
    ExpressionInvalid => ("validation", "invalid_expression", Some("REQ-0188")),
    ExpressionInvalidEscape => ("validation", "invalid_expression", Some("REQ-0191")),
    ExpressionInvalidRegex => ("validation", "invalid_expression", Some("REQ-1244")),
    ExpressionProhibitedConstruct => ("validation", "prohibited_construct", Some("REQ-0441")),
    ExpressionProhibitedFunction => ("validation", "prohibited_function", Some("REQ-0440")),
    ExpressionNestedReduction => ("validation", "nested_reduction", Some("REQ-0502")),
    AggregateMultipleRecords => ("derivation", "aggregate_multiple_records", Some("REQ-0501")),
    AggregateRowMultipleRecords => ("row_construction", "aggregate_multiple_records", Some("REQ-0501")),
    AggregateIdentifierNotGrouped => ("validation", "aggregate_identifier_not_grouped", Some("REQ-0503")),
    AggregateIncomparableSources => ("validation", "incomparable_sources", Some("REQ-0511")),
    AggregateOutputGroups => ("validation", "invalid_aggregate_context", Some("REQ-0507")),
    AggregateOwnGroupKeys => ("validation", "invalid_aggregate_context", Some("REQ-0142")),
    AggregateMixedRelations => ("validation", "mixed_relations", Some("REQ-0504")),
    AggregateGroupNotOutputKey => ("validation", "invalid_aggregate_context", Some("REQ-0470")),
    SourceNonAscii => ("validation", "non_ascii_source", Some("REQ-0705")),
    SourceInvalidText => ("validation", "invalid_text", Some("REQ-0029")),
    SourceInvalidYaml => ("validation", "invalid_yaml", Some("REQ-0280")),
    OdmFieldMissing => ("validation", "odm_schema_field_missing", Some("REQ-1275")),
    OdmFieldAmbiguous => ("validation", "odm_schema_field_ambiguous", Some("REQ-1275")),
    OdmFieldType => ("validation", "odm_schema_field_type", Some("REQ-1275")),
    OdmDeclaredOrdinal => ("validation", "odm_schema_field_type", Some("REQ-1295")),
    OdmValueMissing => ("ingest", "odm_schema_value_missing", Some("REQ-1276")),
    OdmInvalidContext => ("validation", "invalid_odm_context", Some("REQ-1277")),
    OdmNotUnique => ("derivation", "odm_not_unique", Some("REQ-1278")),
    OdmRowNotUnique => ("row_construction", "odm_not_unique", Some("REQ-1278")),
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
    AggregateDeriveDeclaration => ("validation", "invalid_derive_step", Some("REQ-1189")),
    AggregateDeriveRelation => ("validation", "invalid_derive_step", Some("REQ-1191")),
    AggregateDeriveIntermediate => ("validation", "invalid_derive_step", Some("REQ-1242")),
    AggregateDeriveVariable => ("validation", "unknown_derive_variable", Some("REQ-1189")),
    RowFilterPhaseBoundary => ("validation", "phase_boundary", Some("REQ-0068")),
    RootFilterPhaseBoundary => ("validation", "phase_boundary", Some("REQ-1170")),
    IntermediateSelfPhaseBoundary => ("validation", "phase_boundary", Some("REQ-0120")),
    IntermediateDerivationReference => ("validation", "unknown_field", Some("REQ-1185")),
    IntermediateDerivationDuplicate => ("validation", "duplicate_derivation", Some("REQ-1185")),
    IntermediateDerivationWindowReference => ("validation", "unknown_field", Some("REQ-1263")),
    IntermediateDerivationDependency => ("validation", "unknown_field", Some("REQ-1263")),
    IntermediateDerivationSelfPhase => ("validation", "phase_boundary", Some("REQ-1263")),
    IntermediateDerivationCycle => ("validation", "dependency_cycle", Some("REQ-1263")),
    IntermediateDriverInvalid => ("validation", "invalid_intermediate_driver", Some("REQ-1262")),
    IntermediateDriverType => ("validation", "unknown_intermediate_driver_type", Some("REQ-1262")),
    IntermediateUniqueCorrelatedDerivation => ("validation", "correlated_derivation_with_unique_verification", Some("REQ-1245")),
    IntermediateDuplicateRecords => ("verification", "duplicate_intermediate_records", Some("REQ-1245")),
    IntermediateUniqueField => ("validation", "unknown_field", Some("REQ-1245")),
    IntermediateUniqueCorrelatedFilter => ("validation", "correlated_filter_with_unique_verification", Some("REQ-1245")),
    IntermediateIdentifierCollision => ("validation", "duplicate_identifier", Some("REQ-0113")),
    IntermediateDatasetReference => ("validation", "unknown_field", Some("REQ-0079")),
    IntermediateRenameOnly => ("validation", "rename_only_intermediate", Some("REQ-1248")),
    IntermediateRepeatedKey => ("validation", "invalid_field_type", Some("REQ-0115")),
    IntermediateColumnReference => ("validation", "unknown_field", Some("REQ-0122")),
    RowDependencyPhaseBoundary => ("validation", "phase_boundary", Some("REQ-0069")),
    QualifiedNumericReference => ("validation", "qualified_identifier", Some("REQ-0442")),
    RowNumericSourceScope => ("validation", "unknown_field", Some("REQ-0410")),
    GroupedRowNumericSourceScope => ("validation", "unknown_field", Some("REQ-0411")),
    SourceFilterReference => ("validation", "unknown_field", Some("REQ-0132")),
    SourceFilterSingleValue => ("validation", "prohibited_construct", Some("REQ-0148")),
    SourceRequireMatchScope => ("validation", "prohibited_construct", Some("LOOKUP-24")),
    SourceRangeScope => ("validation", "prohibited_construct", Some("LOOKUP-9")),
    SourceUnknownReference => ("validation", "unknown_field", Some("REQ-0103")),
    VerificationUnknownReference => ("validation", "unknown_field", Some("REQ-0405")),
    OutputMissingKeyFailed => ("output", "missing_key", Some("REQ-0240")),
    OutputDuplicateKeyFailed => ("output", "duplicate_key", Some("REQ-0240")),
    VerificationUniqueFailed => ("verification", "unique_failed", Some("REQ-0381")),
    VerificationUniqueRedundant => ("validation", "redundant_declaration", Some("REQ-0381")),
    VerificationAllOrNoneFailed => ("verification", "all_or_none_failed", Some("REQ-0382")),
    VerificationAssertFailed => ("verification", "assert_failed", Some("REQ-0384")),
    VerificationRowCountFailed => ("verification", "row_count_failed", Some("REQ-0385")),
    VerificationNotMissingFailed => ("verification", "not_missing_failed", Some("REQ-0375")),
    VerificationNotMissingRedundant => ("validation", "redundant_declaration", Some("REQ-0375")),
    VerificationAllowedValuesFailed => ("verification", "allowed_values_failed", Some("REQ-0376")),
    SubmissionUnknownDocument => ("validation", "unknown_document", Some("REQ-1016")),
    SubmissionReservedMetadata => ("validation", "reserved_metadata_key", Some("REQ-0910")),
    SubmissionMetadataMissing => ("validation", "submission_metadata_missing", Some("REQ-1015")),
    SubmissionOriginMissing => ("validation", "origin_missing", Some("REQ-0918")),
    SubmissionSasName => ("validation", "sas_name_too_long", Some("REQ-1023")),
    SubmissionCoreMandatory => ("validation", "core_mandatory_conflict", Some("REQ-0915")),
    SubmissionKeyMandatory => ("validation", "value_not_permitted", Some("REQ-0881")),
    SubmissionClassFamily => ("validation", "value_not_permitted", Some("REQ-0882")),
    SubmissionMandatoryFailed => ("verification", "not_missing_failed", Some("REQ-0885")),
    SubmissionDataTypeFailed => ("verification", "data_type_failed", Some("SUB-26")),
    SubmissionDatetimeColumn => ("validation", "value_not_permitted", Some("SUB-27")),
    SubmissionMandatoryRedundant => ("validation", "redundant_declaration", Some("REQ-0916")),
    SubmissionAdamField => ("validation", "value_not_permitted", Some("REQ-0917")),
    SubmissionOriginSource => ("validation", "origin_source_not_admitted", Some("REQ-0919")),
    SubmissionOriginDescription => ("validation", "origin_description_missing", Some("REQ-0920")),
    SubmissionMethodMissing => ("validation", "method_missing", Some("REQ-0923")),
    SubmissionOriginRefuted => ("validation", "origin_contradicts_derivation", Some("REQ-0922")),
    SubmissionAnnotatedCrfMissing => ("validation", "origin_document_missing", Some("REQ-0984")),
    SubmissionOriginDocument => ("validation", "origin_document_missing", Some("REQ-0893")),
    SubmissionDataType => ("validation", "submission_data_type_not_admitted", Some("REQ-0914")),
    SubmissionLengthMissing => ("validation", "submission_length_missing", Some("REQ-0912")),
    SubmissionLengthNotApplicable => ("validation", "submission_length_not_applicable", Some("REQ-0912")),
    SubmissionLengthInvalid => ("validation", "submission_length_invalid", Some("REQ-0911")),
    SubmissionLengthConflict => ("validation", "declared_length_conflict", Some("REQ-0913")),
    SubmissionValueWhereInvalid => ("validation", "value_level_where_invalid", Some("SUB-12")),
    SubmissionValueDuplicateId => ("validation", "duplicate_identifier", Some("SUB-12")),
    SubmissionValueOverlap => ("validation", "value_level_overlap", Some("SUB-12")),
    SubmissionValueOriginRefuted => ("validation", "refuted_value_origin", Some("SUB-13")),
    ValueLevelMultipleMatches => ("verification", "value_level_multiple_matches", Some("SUB-13")),
    ValueLevelUnmatched => ("verification", "value_level_unmatched", Some("SUB-13")),
    SubmissionPredecessorLabel => ("validation", "predecessor_label_mismatch", Some("SUB-20")),
    SubmissionStandardFamily => ("validation", "unknown_standard_family", Some("REQ-0924")),
    SubmissionDefineExtension => ("validation", "value_not_permitted", Some("REQ-1014")),
    SubmissionDefineName => ("validation", "value_not_permitted", Some("DEFINE-2")),
    SubmissionSourceVersion => ("validation", "source_system_incomplete", Some("REQ-0979")),
    SubmissionDuplicateIdentifier => ("validation", "duplicate_define_identifier", Some("REQ-1017")),
    SubmissionAnnotatedCrfMultiplicity => ("validation", "value_not_permitted", Some("REQ-1019")),
    SubmissionDuplicateDataset => ("validation", "duplicate_dataset_entry", Some("REQ-1018")),
    SubmissionReferenceRepeating => ("validation", "reference_data_repeating_conflict", Some("REQ-0925")),
    SubmissionNoDataComment => ("validation", "submission_metadata_missing", Some("REQ-1022")),
    SubmissionTransportEmpty => ("output", "transport_dataset_empty", Some("JSON-3")),
    SubmissionLabelInvalid => ("validation", "transport_label_invalid", Some("XPT-5")),
    SubmissionDisplayFormatConflict => ("validation", "value_not_permitted", Some("SUB-25")),
    TransportTextInvalid => ("output", "transport_text_invalid", Some("XPT-7")),
    XptValueOutOfRange => ("output", "xpt_value_out_of_range", Some("XPT-8")),
    XptDisplayFormatInvalid => ("validation", "xpt_display_format_invalid", Some("XPT-9")),
    SubmissionTransportDecimals => ("validation", "decimals_not_applicable", Some("JSON-5")),
    XptFileTooLarge => ("output", "xpt_file_too_large", Some("XPT-10")),
    XptLastRowBlank => ("output", "xpt_last_row_blank", Some("XPT-10")),
    SubmissionKeyNotOutput => ("validation", "value_not_permitted", Some("REQ-1015")),
    SubmissionColumnNotOutput => ("validation", "submission_not_output_column", Some("REQ-0909")),
    SubmissionCdiscAliasMissing => ("validation", "missing_required_field", Some("REQ-1004")),
    SubmissionXmlWhitespace => ("validation", "untransportable_text", Some("REQ-1025")),
    SubmissionXmlInvalid => ("validation", "value_not_permitted", Some("REQ-1026")),
    SubmissionJsonInvalid => ("validation", "value_not_permitted", Some("REQ-1231")),
    SubmissionInventoryFailed => ("verification", "allowed_values_failed", Some("REQ-1156")),
    DefineStandardName => ("validation", "value_not_permitted", Some("REQ-1074")),
    DefineRequiredDeclaration => ("validation", "missing_required_field", Some("REQ-1337")),
    DefineArtifactOutsidePackage => ("validation", "artifact_outside_document", Some("REQ-1021")),
    DefineArtifactCollision => ("validation", "artifact_path_collision", Some("REQ-1195")),
    DefineTransportCollision => ("validation", "transport_path_collision", Some("REQ-1226")),
    DefineCreatedInvalid => ("validation", "value_not_permitted", Some("REQ-0962")),
    CodelistUnknownBinding => ("validation", "unknown_codelist", Some("REQ-0953")),
    CodelistBindingTypeMismatch => ("validation", "codelist_type_mismatch", Some("REQ-0954")),
    CodelistAllowedValuesRedundant => ("validation", "redundant_declaration", Some("REQ-0955")),
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
    WindowRepeatedGroup => ("validation", "invalid_field_type", Some("REQ-0293")),
    WindowBaselineInputType => ("validation", "incompatible_input_type", Some("REQ-0323")),
    LookupSelectionReference => ("validation", "unknown_field", Some("REQ-0120")),
    LookupValueReference => ("validation", "unknown_field", Some("REQ-0125")),
    // One cause per lookup fault, whichever kind of lookup raises it
    // (LOOKUP-25): an implicit join, a named intermediate or an aggregate.
    LookupEmptyKey => ("validation", "invalid_field_type", Some("LOOKUP-5")),
    LookupNoApplicableKeys => ("validation", "no_applicable_keys", Some("LOOKUP-5")),
    LookupDonorKeyReference => ("validation", "unknown_field", Some("LOOKUP-5")),
    LookupMatchValueReference => ("validation", "unknown_field", Some("LOOKUP-5")),
    LookupKeyType => ("validation", "incompatible_input_type", Some("LOOKUP-9")),
    LookupRangeDeclaration => ("validation", "invalid_field_type", Some("LOOKUP-9")),
    LookupRangeReference => ("validation", "unknown_field", Some("REQ-0121")),
    LookupRangeTypes => ("validation", "incomparable_range_types", Some("REQ-0121")),
    LookupSelectionUnpaired => ("validation", "unpaired_fields", Some("LOOKUP-10")),
    LookupAbsenceConflict => ("validation", "conflicting_absence_policy", Some("LOOKUP-24")),
    FlagMissingValueRequired => ("validation", "missing_value_required", Some("REQ-1258")),
    PredicateUnknownField => ("validation", "unknown_field", Some("REQ-0189")),
    PredicateUnresolvableName => ("validation", "unresolvable_name", Some("REQ-0189")),
    PredicateInputType => ("validation", "incompatible_input_type", Some("REQ-0190")),
    PredicateIncompatiblePair => ("validation", "incompatible_input_type", Some("REQ-0190")),
    PredicateExpectedText => ("validation", "incompatible_input_type", Some("REQ-0190")),
    PredicateContainsExpectedText => ("validation", "incompatible_input_type", Some("REQ-1244")),
    PredicateDanglingEscape => ("validation", "invalid_expression", Some("REQ-0191")),
    RegexInvalidPattern => ("validation", "invalid_regex", Some("REQ-0827")),
    VerificationMatchesFailed => ("verification", "matches_failed", Some("REQ-0379")),
    VerificationIdentifierType => ("validation", "invalid_declaration", Some("REQ-0374")),
    VerificationDeclarationRequired => ("validation", "invalid_declaration", Some("REQ-0397")),
    VerificationIdentifierRepeated => ("validation", "duplicate_identifier", Some("REQ-0398")),
    VerificationGrouping => ("validation", "invalid_declaration", Some("REQ-0403")),
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

/// JSON context retains arbitrary-width integer numbers and ordered sequences.
pub type Context = BTreeMap<String, crate::json::Value>;

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
