//! Typed semantic diagnostics and canonical owned JSON context.
//! Public issue envelopes and host exceptions belong to adapters. Each
//! record's severity and message come from the condition registry: its
//! message renders from the template `yaml/conditions.yaml` registers for its
//! condition (`message`).
//!
//! Registry keys identify causes, not public condition strings: the language uses
//! `conversion_failed` for several requirements. Migrated causes cover numeric
//! evaluation, completed-result conversion, classified resource failures and
//! original-document preflight and output declarations. Other families retain their existing error types
//! until their semantics and provenance migrate here.

use crate::expression::SourceSpan;

mod dataset;
pub use dataset::DatasetFinding;

pub(crate) mod lookup;

mod aggregate;
pub use aggregate::{AggregateContextCause, AggregateFinding};

mod odm;
pub use odm::{OdmDeclared, OdmFinding, OdmLocation};

mod input_override;
pub use input_override::{input_frame, input_override};

mod odm_read;
pub use odm_read::{invalid_odm_limit, OdmReadFinding};

mod inheritance;
pub use inheritance::InheritanceFinding;

mod source;
pub use source::{source_diagnostics, DatasetJsonCause, ParquetFinding, SourceFinding};

mod inherited_dependencies;
pub use inherited_dependencies::inheritance_dependency_diagnostics;

mod expression;
mod numeric;
pub use expression::EvaluationDiagnosticError;
mod project_lock;
pub use project_lock::{
    lock_diagnostics, LockDiagnosticError, LockFinding, LockOrigins, LockReason,
};

mod message;
mod message_templates;
pub use message::{template, UnresolvedPlaceholder};

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
    /// listed by COLUMN-15 retain an absent rule.
    pub rule: Option<&'static str>,
}

/// The severity of an issue. The registry gives every 1.0 condition error
/// severity (D28); warning-severity findings are rows of the run report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
}

impl Severity {
    /// The value of the issues table's `severity` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
        }
    }
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
    pub fn rule(self) -> &'static str {
        match self {
            Self::Identifier => "SPEC-2",
            Self::DefineId => "DEFINE-9",
            Self::Href => "DEFINE-13",
            Self::Language => "DEFINE-21",
            Self::StandardName => "DEFINE-6",
            Self::Core => "SUB-10",
            Self::OriginType => "SUB-14",
            Self::OriginSource => "SUB-14",
            Self::MethodType => "SUB-21",
            Self::DataType => "SUB-6",
            Self::DatasetClass => "SUB-4",
            Self::DatasetSubclass => "SUB-4",
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
    ($($code:ident => ($phase:literal, $condition:literal, $rule:expr),)*) => {
        /// Stable internal cause identity; no wire spelling or numeric discriminant.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum ConditionCode { $($code,)* }

        impl ConditionCode {
            /// Resolve one canonical public mapping without consulting a host.
            pub fn definition(self) -> Definition {
                match self {
                    $(Self::$code => Definition {
                        phase: $phase, condition: $condition, rule: $rule,
                    },)*
                }
            }

            /// The severity of the cause's condition: error for every 1.0
            /// condition (D28).
            pub fn severity(self) -> Severity {
                Severity::Error
            }
        }

        /// Every migrated cause must be reached by the diagnostic integration tests,
        /// which render the message of each record a cause makes there.
        pub const CONDITIONS: &[ConditionCode] = &[$(ConditionCode::$code,)*];
    };
}

conditions! {
    FunctionImplementationMissing => ("validation", "function_implementation_missing", Some("FUNC-1")),
    FunctionFileInvalid => ("validation", "function_file_invalid", Some("FUNC-2")),
    ProjectLockMismatch => ("validation", "runtime_artifact_mismatch", Some("FUNC-3")),
    CodelistDuplicateIdentifier => ("validation", "duplicate_define_identifier", Some("CT-1")),
    CodelistInvalidShape => ("validation", "codelist_shape_invalid", Some("CT-3")),
    CodelistInvalidValue => ("validation", "codelist_shape_invalid", Some("CT-4")),
    CodelistDuplicateValue => ("validation", "codelist_duplicate_value", Some("CT-4")),
    CodelistPartialItemField => ("validation", "codelist_partial_item_field", Some("CT-5")),
    CodelistExtensionNotAdmitted => ("validation", "codelist_extension_not_admitted", Some("CT-6")),
    CodelistExtendedAlias => ("validation", "codelist_extended_alias", Some("CT-6")),
    CodelistFormatNamePrefix => ("validation", "value_not_permitted", Some("CT-12")),
    UnknownProjectFunction => ("validation", "unknown_project_function", Some("FUNC-4")),
    InvalidFunctionArgument => ("validation", "invalid_function_argument", Some("FUNC-5")),
    FunctionArgumentLeaf => ("validation", "invalid_function_argument", Some("FUNC-6")),
    RuntimeFunctionArgument => ("derivation", "invalid_function_argument", Some("FUNC-5")),
    RuntimeFunctionCallFailed => ("derivation", "function_call_failed", Some("FUNC-8")),
    RuntimeFunctionResult => ("derivation", "invalid_function_result", Some("FUNC-9")),
    FunctionConformanceFailed => ("validation", "function_conformance_failed", Some("FUNC-12")),
    ApplicationUnsupported => ("planning", "unsupported_operation", None),
    ApplicationRejected => ("engine", "engine_rejected", None),
    DatasetMultipleValues => ("derivation", "multiple_values_per_key", Some("COLUMN-2")),
    LookupUnmatchedKey => ("join", "unmatched_key", Some("LOOKUP-24")),
    DatasetMultipleMatches => ("join", "multiple_matches", Some("LOOKUP-10")),
    DatasetBaselineAmbiguity => ("derivation", "ambiguous_baseline", Some("WINDOW-9")),
    DatasetReductionInputType => ("validation", "incompatible_input_type", Some("FORMULA-23")),
    NumericUnknownField => ("validation", "unknown_field", Some("FORMULA-9")),
    NumericInputType => ("validation", "incompatible_input_type", Some("FORMULA-12")),
    ExtremeIncomparable => ("validation", "incomparable_sources", Some("VALUE-19")),
    StrPadWidth => ("validation", "invalid_field_type", Some("TEXT-2")),
    StrSubstrPosition => ("validation", "invalid_field_type", Some("TEXT-13")),
    StrSubstrPiece => ("validation", "invalid_field_type", Some("TEXT-16")),
    StrSubstrStartBesidePiece => ("validation", "unknown_field", Some("TEXT-16")),
    StrSubstrFieldRequired => ("validation", "missing_required_field", Some("TEXT-16")),
    StrDecimalDeclaration => ("validation", "invalid_field_type", Some("TEXT-15")),
    StrDecimalInput => ("validation", "incompatible_input_type", Some("TEXT-15")),
    DeclarativeRoundingInput => ("validation", "incompatible_input_type", Some("FORMULA-16")),
    TemporalOperand => ("validation", "incompatible_input_type", Some("TIME-1")),
    DateShiftInput => ("validation", "incompatible_input_type", Some("TIME-14")),
    DateShiftRange => ("convert", "conversion_failed", Some("TIME-14")),
    DatetimeDifferenceInput => ("validation", "incompatible_input_type", Some("TIME-4")),
    DatetimeShiftInput => ("validation", "incompatible_input_type", Some("TIME-14")),
    DatetimeShiftRange => ("convert", "conversion_failed", Some("TIME-14")),
    DurationValueInput => ("validation", "incompatible_input_type", Some("TIME-17")),
    DurationInvalid => ("impute", "invalid_duration_text", Some("TIME-17")),
    DurationTextInput => ("validation", "incompatible_input_type", Some("TIME-21")),
    DurationTextRange => ("convert", "conversion_failed", Some("TIME-21")),
    DateTextInput => ("validation", "incompatible_input_type", Some("TIME-19")),
    DateTextGap => ("validation", "missing_required_field", Some("TIME-19")),
    DateTextInvalid => ("impute", "invalid_date_text", Some("TIME-19")),
    TimeOfDayInput => ("validation", "incompatible_input_type", Some("TIME-20")),
    TemporalUnit => ("validation", "value_not_permitted", Some("TIME-4")),
    TemporalFieldRequired => ("validation", "missing_required_field", Some("TIME-1")),
    TemporalFieldNotPermitted => ("validation", "unknown_field", Some("TIME-1")),
    ImputationMonthRange => ("validation", "month_out_of_range", Some("TIME-10")),
    ImputationDayRange => ("validation", "value_not_permitted", Some("TIME-10")),
    ImputationDayRule => ("validation", "value_not_permitted", Some("TIME-10")),
    ImputeMonthWithoutDay => ("validation", "missing_required_field", Some("TIME-10")),
    ImputeInvalid => ("impute", "invalid_date_text", Some("TIME-13")),
    ImputeIntervalText => ("impute", "interval_or_duration_text", Some("TIME-13")),
    ImputeCalendar => ("impute", "invalid_calendar_date", Some("TIME-10")),
    ImputeDatetimeInput => ("validation", "incompatible_input_type", Some("TIME-8")),
    PrecisionDatetimeInput => ("validation", "incompatible_input_type", Some("TIME-7")),
    ImputationFlagInput => ("validation", "incompatible_input_type", Some("TIME-18")),
    ToDateInput => ("validation", "incompatible_input_type", Some("TIME-2")),
    ToDateInvalid => ("validation", "invalid_date_text", Some("TIME-2")),
    ToDateIntervalText => ("impute", "interval_or_duration_text", Some("TIME-2")),
    // `date_text`'s source type, its declared forms and the text it reads
    // (TIME-22).
    DateFormsInput => ("validation", "incompatible_input_type", Some("TIME-22")),
    DateFormsList => ("validation", "invalid_field_type", Some("TIME-22")),
    DateFormsInvalid => ("impute", "invalid_date_text", Some("TIME-22")),
    TemplateInvalid => ("validation", "invalid_string_template", Some("TEXT-3")),
    TemplateMissing => ("mapping", "missing_input", Some("COLUMN-10")),
    ExtractGroup => ("validation", "regex_group_out_of_range", Some("TEXT-10")),
    ExtractNoMatch => ("mapping", "unmatched_pattern", Some("COLUMN-10")),
    ExtractUnknown => ("validation", "unknown_field", Some("NAME-3")),
    StrCaseInput => ("validation", "incompatible_input_type", Some("TEXT-1")),
    StrCaseDelimitersNotTaken => ("validation", "unknown_field", Some("TEXT-5")),
    StrCaseDelimiters => ("validation", "invalid_field_type", Some("TEXT-5")),
    MappingInput => ("validation", "incompatible_input_type", Some("EXPR-11")),
    MappingAmbiguousDictionary => ("validation", "ambiguous_dictionary", Some("EXPR-12")),
    MappingUnmapped => ("mapping", "unmapped_value", Some("COLUMN-10")),
    CutInput => ("validation", "incompatible_input_type", Some("EXPR-13")),
    CutDeclaration => ("validation", "invalid_field_type", Some("EXPR-13")),
    DecodeInput => ("validation", "incompatible_input_type", Some("EXPR-15")),
    DecodeUnknownCodelist => ("validation", "unknown_codelist", Some("EXPR-15")),
    DecodeWithoutDecodes => ("validation", "codelist_without_decodes", Some("EXPR-15")),
    TemporalBounds => ("validation", "value_not_permitted", Some("TIME-5")),
    IntegerOverflow => ("derivation", "integer_overflow", Some("VALUE-5")),
    LiteralIntegerOverflow => ("validation", "integer_overflow", Some("VALUE-5")),
    DivisionByZero => ("derivation", "division_by_zero", Some("FORMULA-15")),
    SqrtOfNegative => ("derivation", "sqrt_of_negative", Some("FORMULA-15")),
    LnOfNonpositive => ("derivation", "ln_of_nonpositive", Some("FORMULA-15")),
    InvalidPower => ("derivation", "invalid_power", Some("FORMULA-15")),
    InvalidProbability => ("derivation", "invalid_probability", Some("FORMULA-15")),
    ConversionInput => ("convert", "conversion_failed", Some("VALUE-3")),
    ConversionInteger => ("convert", "conversion_failed", Some("VALUE-7")),
    ConversionTemporal => ("convert", "conversion_failed", Some("VALUE-15")),
    ConversionResultType => ("validation", "conversion_failed", Some("VALUE-24")),
    ResourceMissing => ("validation", "resource_path_missing", Some("FILE-7")),
    ResourceNotRegularFile => ("validation", "resource_path_not_regular_file", Some("FILE-7")),
    ResourceUriScheme => ("validation", "resource_path_uri_scheme", Some("FILE-5")),
    ResourceBackslash => ("validation", "resource_path_not_normalized", Some("FILE-5")),
    ResourceEmptySegment => ("validation", "resource_path_not_normalized", Some("FILE-5")),
    ResourceSymlink => ("validation", "resource_path_symlink", Some("FILE-8")),
    ResourceContentChanged => ("ingest", "resource_path_content_changed", Some("FILE-10")),
    // A save's refusal of a target whose walk stops, decided while publishing
    // (FILE-7, FILE-8; #3030 D189).
    PublicationTargetMissing => ("output", "resource_path_missing", Some("FILE-7")),
    PublicationTargetNotRegularFile => ("output", "resource_path_not_regular_file", Some("FILE-7")),
    PublicationTargetSymlink => ("output", "resource_path_symlink", Some("FILE-8")),
    SchemaInvalidFieldType => ("validation", "invalid_field_type", Some("SCHEMA-12")),
    SchemaMissingRequiredField => ("validation", "missing_required_field", Some("SCHEMA-13")),
    SchemaUnknownField => ("validation", "unknown_field", Some("SCHEMA-13")),
    SchemaInvalidOperationCount => ("validation", "invalid_operation_count", Some("SCHEMA-7")),
    SchemaUnknownOperation => ("validation", "unknown_operation", Some("SCHEMA-7")),
    // The 1.x header (SCHEMA-2, LAYER-4, P-VER-1): a file a call reads, the file a
    // referencing field names, and a parent layer.
    HeaderLanguageVersion => ("validation", "unsupported_language_version", Some("SCHEMA-2")),
    HeaderFileType => ("validation", "unexpected_file_type", Some("SCHEMA-2")),
    ReferenceLanguageVersion => ("validation", "unsupported_language_version", Some("SCHEMA-2")),
    ReferenceFileType => ("validation", "unexpected_file_type", Some("SCHEMA-2")),
    // A file type a call takes as its argument, which names no file: the type
    // whose JSON Schema `schema` gives.
    ArgumentFileType => ("validation", "unexpected_file_type", Some("SCHEMA-2")),
    SchemaUnknownFieldSince => ("validation", "unknown_field", Some("SCHEMA-2")),
    SchemaRedundantLongForm => ("validation", "redundant_long_form", Some("SCHEMA-15")),
    SchemaCaseItems => ("validation", "invalid_field_type", Some("EXPR-7")),
    SchemaLayerInvalidClear => ("validation", "invalid_clear", Some("LAYER-9")),
    SchemaLayerIdentifierRepeated => ("validation", "duplicate_identifier", Some("LAYER-4")),
    CopyDuplicateIdentifier => ("validation", "duplicate_identifier", Some("SPEC-9")),
    CopyUnknownDataset => ("validation", "unknown_field", Some("SPEC-9")),
    CopyUnlinkedInput => ("validation", "prohibited_construct", Some("SPEC-9")),
    CopyUnknownColumn => ("validation", "unknown_field", Some("SPEC-9")),
    CopyProducerCycle => ("validation", "producer_workflow_cycle", Some("SPEC-9")),
    SchemaUnknownWindow => ("validation", "unknown_window", Some("WINDOW-1")),
    InputFrameValue => ("validation", "value_not_permitted", Some("SCHEMA-12")),
    InputOverrideAlias => ("validation", "value_not_permitted", Some("SCHEMA-12")),
    SchemaDescriptorValueNotPermitted => ("validation", "value_not_permitted", Some("SCHEMA-12")),
    SchemaDescriptorPatternMismatch => ("validation", "pattern_mismatch", Some("SCHEMA-12")),
    SchemaDescriptorMinimumLength => ("validation", "minimum_length", Some("SCHEMA-12")),
    SchemaDescriptorInvalidSize => ("validation", "invalid_size", Some("SCHEMA-12")),
    SchemaColumnTypeValueNotPermitted => ("validation", "value_not_permitted", Some("VALUE-1")),
    SchemaColumnTypePatternMismatch => ("validation", "pattern_mismatch", Some("SCHEMA-12")),
    SchemaColumnTypeMinimumLength => ("validation", "minimum_length", Some("SCHEMA-12")),
    SchemaColumnTypeInvalidSize => ("validation", "invalid_size", Some("SCHEMA-12")),
    SchemaDayRuleValueNotPermitted => ("validation", "value_not_permitted", Some("TIME-10")),
    SchemaDayRulePatternMismatch => ("validation", "pattern_mismatch", Some("SCHEMA-12")),
    SchemaDayRuleMinimumLength => ("validation", "minimum_length", Some("SCHEMA-12")),
    SchemaDayRuleInvalidSize => ("validation", "invalid_size", Some("SCHEMA-12")),
    SchemaTimeRuleValueNotPermitted => ("validation", "value_not_permitted", Some("SCHEMA-12")),
    SchemaTimeRulePatternMismatch => ("validation", "pattern_mismatch", Some("SCHEMA-12")),
    SchemaTimeRuleMinimumLength => ("validation", "minimum_length", Some("SCHEMA-12")),
    SchemaTimeRuleInvalidSize => ("validation", "invalid_size", Some("SCHEMA-12")),
    InheritanceInvalidParent => ("validation", "invalid_parent_path", Some("LAYER-2")),
    InheritanceUnknownDependency => ("validation", "unknown_reference", Some("COLUMN-5")),
    InheritanceParentUnavailable => ("validation", "parent_not_found", Some("LAYER-2")),
    InheritanceParentOnAnotherShare => ("validation", "parent_on_another_share", Some("LAYER-10")),
    InheritanceCycle => ("validation", "inheritance_cycle", Some("LAYER-3")),
    InheritanceRepeatedLayer => ("validation", "duplicate_layer", Some("LAYER-3")),
    InheritanceFragmentField => ("validation", "prohibited_construct", Some("LAYER-4")),
    InheritanceNotFragment => ("validation", "value_not_permitted", Some("LAYER-4")),
    InheritanceParentLanguageVersion => ("validation", "unsupported_language_version", Some("LAYER-4")),
    InheritanceParentFileType => ("validation", "unexpected_file_type", Some("LAYER-4")),
    PreflightUndeclaredRowColumn => ("validation", "undeclared_column", Some("SPEC-5")),
    PreflightDuplicateRowDefault => ("validation", "duplicate_derivation", Some("SPEC-6")),
    PreflightMissingRowDerivation => ("validation", "missing_derivation", Some("SPEC-5")),
    PreflightConflictingRowConstruction => ("validation", "conflicting_row_construction", Some("ROW-9")),
    PreflightInvalidGroup => ("validation", "invalid_field_type", Some("ROW-4")),
    PreflightGroupReference => ("validation", "unknown_field", Some("ROW-4")),
    PreflightRowDriverUnavailable => ("validation", "driver_unavailable", Some("ROW-1")),
    PreflightMissingDerivation => ("validation", "missing_derivation", Some("SPEC-4")),
    PreflightUndeclaredKey => ("validation", "undeclared_column", Some("OUTPUT-2")),
    PreflightEmptyKeys => ("validation", "invalid_field_type", Some("OUTPUT-2")),
    PreflightDuplicateKey => ("validation", "duplicate_identifier", Some("OUTPUT-2")),
    PreflightDuplicateColumn => ("validation", "duplicate_identifier", Some("SPEC-2")),
    PreflightDuplicateRow => ("validation", "duplicate_identifier", Some("ROW-1")),
    PreflightDriverUnavailable => ("validation", "driver_unavailable", Some("ROW-8")),
    PreflightDomainInputCollision => ("validation", "duplicate_identifier", Some("NAME-2")),
    PreflightRedundantSourceType => ("validation", "redundant_field_type", Some("INPUT-6")),
    SourceUnknownProfile => ("validation", "source_profile_unknown", Some("INPUT-1")),
    SasSourceEncoding => ("validation", "source_encoding_required", Some("SAS-2")),
    SasInvalid => ("ingest", "source_sas_invalid", Some("SAS-7")),
    // A Dataset-JSON input's refusal with its closed cause (DSJSON-5), and a
    // cell that does not parse as its field's type (DSJSON-3).
    DatasetJsonInvalid => ("ingest", "source_dataset_json_invalid", Some("DSJSON-5")),
    DatasetJsonFieldParse => ("ingest", "field_parse_failed", Some("DSJSON-3")),
    SourceOrdinalCollision => ("validation", "ordinal_field_collision", Some("INPUT-4")),
    SourceEmptyStringPresentUnsupported => ("validation", "empty_string_present_unsupported", Some("INPUT-9")),
    ProducerInvalidContract => ("validation", "invalid_producer_contract", Some("INPUT-10")),
    ProducerRedundantSourceType => ("validation", "redundant_field_type", Some("INPUT-6")),
    ProducerContractMismatch => ("validation", "producer_contract_mismatch", Some("INPUT-11")),
    ProjectDuplicateName => ("validation", "duplicate_specification_name", Some("PROJECT-5")),
    ProjectUnknownName => ("validation", "unknown_specification_name", Some("PROJECT-6")),
    ProjectUnknownSection => ("validation", "unknown_section", Some("PROJECT-14")),
    ProjectUnlinkedProducer => ("validation", "unlinked_producer", Some("PROJECT-10")),
    ProjectProducerNotCompleted => ("validation", "producer_not_completed", Some("PROJECT-11")),
    ProducerWorkflowCycle => ("validation", "producer_workflow_cycle", Some("PROJECT-8")),
    ProjectRunNothingToSave => ("output", "nothing_to_save", Some("PROJECT-12")),
    ProjectSubmissionNothingToSave => ("output", "nothing_to_save", Some("PROJECT-13")),
    // A listed specification's saved artifact or inputs disagree with what
    // its declared run report records (#3030 D151).
    StaleArtifact => ("validation", "stale_artifact", Some("PROJECT-15")),
    OutputUnknownProfile => ("validation", "unknown_artifact_profile", Some("OUTPUT-5")),
    MissingReport => ("validation", "missing_report", Some("CHECK-22")),
    ReportPathCollision => ("validation", "artifact_path_collision", Some("OUTPUT-9")),
    OutputInvalidDecimals => ("validation", "invalid_field_type", Some("CSV-6")),
    OutputDecimalsNotApplicable => ("validation", "decimals_not_applicable", Some("CSV-6")),
    OutputDuplicateColumn => ("validation", "duplicate_identifier", Some("OUTPUT-1")),
    OutputUndeclaredColumn => ("validation", "undeclared_column", Some("OUTPUT-1")),
    OutputInternalKey => ("validation", "internal_column_in_keys", Some("OUTPUT-2")),
    OutputDuplicateOrderTerm => ("validation", "duplicate_order_term", Some("OUTPUT-12")),
    OutputUndeclaredOrderColumn => ("validation", "undeclared_column", Some("OUTPUT-12")),
    OutputPublicationFailed => ("output", "publication_failed", Some("OUTPUT-8")),
    ExpressionInvalid => ("validation", "invalid_expression", Some("FORMULA-1")),
    ExpressionInvalidEscape => ("validation", "invalid_expression", Some("FORMULA-7")),
    ExpressionInvalidRegex => ("validation", "invalid_expression", Some("FORMULA-8")),
    ExpressionPatternNotLiteral => ("validation", "invalid_expression", Some("FORMULA-8")),
    ExpressionProhibitedConstruct => ("validation", "prohibited_construct", Some("FORMULA-27")),
    ExpressionProhibitedFunction => ("validation", "prohibited_function", Some("FORMULA-13")),
    ExpressionNestedReduction => ("validation", "nested_reduction", Some("FORMULA-21")),
    // A `quantile` outside 0 through 1 (FORMULA-18).
    AggregateQuantileRange => ("validation", "invalid_field_type", Some("FORMULA-18")),
    AggregateMultipleRecords => ("derivation", "aggregate_multiple_records", Some("FORMULA-20")),
    AggregateRowMultipleRecords => ("row_construction", "aggregate_multiple_records", Some("FORMULA-20")),
    AggregateIdentifierNotGrouped => ("validation", "aggregate_identifier_not_grouped", Some("FORMULA-22")),
    AggregateIncomparableSources => ("validation", "incomparable_sources", Some("FORMULA-23")),
    AggregateNumericOperand => ("validation", "incompatible_input_type", Some("FORMULA-23")),
    AggregateOutputGroups => ("validation", "invalid_aggregate_context", Some("LOOKUP-19")),
    AggregateOwnGroupKeys => ("validation", "invalid_aggregate_context", Some("LOOKUP-5")),
    AggregateMixedRelations => ("validation", "mixed_relations", Some("LOOKUP-18")),
    AggregateGroupNotOutputKey => ("validation", "invalid_aggregate_context", Some("LOOKUP-19")),
    SourceNonAscii => ("validation", "non_ascii_source", Some("SPEC-8")),
    SourceInvalidText => ("validation", "invalid_text", Some("VALUE-11")),
    SourceInvalidYaml => ("validation", "invalid_yaml", Some("SCHEMA-3")),
    OdmFieldMissing => ("validation", "odm_schema_field_missing", Some("NAME-8")),
    OdmFieldAmbiguous => ("validation", "odm_schema_field_ambiguous", Some("NAME-8")),
    OdmFieldType => ("validation", "odm_schema_field_type", Some("NAME-8")),
    OdmDeclaredOrdinal => ("validation", "odm_schema_field_type", Some("INPUT-4")),
    OdmValueMissing => ("ingest", "odm_schema_value_missing", Some("NAME-9")),
    OdmInvalidContext => ("validation", "invalid_odm_context", Some("NAME-11")),
    OdmNotUnique => ("derivation", "odm_not_unique", Some("NAME-13")),
    OdmRowNotUnique => ("row_construction", "odm_not_unique", Some("NAME-13")),
    // The refusals of an ODM source (ODM-1; #3030 D73, D158): `read_odm`
    // reports each with the source as given.
    OdmSourceArchiveInvalid => ("ingest", "source_odm_archive_invalid", Some("ODM-1")),
    OdmSourceArchiveMember => ("ingest", "source_odm_archive_member", Some("ODM-1")),
    OdmSourceAttributeAmbiguous => ("ingest", "source_odm_attribute_ambiguous", Some("ODM-1")),
    OdmSourceAttributeInvalid => ("ingest", "source_odm_attribute_invalid", Some("ODM-1")),
    OdmSourceAttributeMissing => ("ingest", "source_odm_attribute_missing", Some("ODM-1")),
    OdmSourceCompressionUnsupported => ("ingest", "source_odm_compression_unsupported", Some("ODM-1")),
    OdmSourceDocumentUnsupported => ("ingest", "source_odm_document_unsupported", Some("ODM-1")),
    OdmSourceLayoutUnsupported => ("ingest", "source_odm_layout_unsupported", Some("ODM-1")),
    OdmSourceLimitExceeded => ("ingest", "source_odm_limit_exceeded", Some("ODM-1")),
    OdmSourceMetadataConflict => ("ingest", "source_odm_metadata_conflict", Some("ODM-1")),
    OdmSourceNoClinicalData => ("ingest", "source_odm_no_clinical_data", Some("ODM-1")),
    OdmSourceTransactional => ("ingest", "source_odm_transactional", Some("ODM-1")),
    OdmSourceValueUnsupported => ("ingest", "source_odm_value_unsupported", Some("ODM-1")),
    OdmSourceXmlMalformed => ("ingest", "source_odm_xml_malformed", Some("ODM-1")),
    OdmSourceXmlUnsupported => ("ingest", "source_odm_xml_unsupported", Some("ODM-1")),
    // A `read_odm` option below its minimum, refused before the source is
    // read, as a submission's `created` argument is (DEFINE-3).
    OdmSourceOptionInvalid => ("validation", "value_not_permitted", Some("ODM-1")),
    CsvInvalidText => ("ingest", "invalid_text", Some("CSV-10")),
    CsvByteOrderMark => ("ingest", "source_byte_order_mark", Some("CSV-16")),
    // A byte-order mark declared on an input that another profile reads.
    CsvByteOrderMarkProfile => ("validation", "value_not_permitted", Some("CSV-10")),
    CsvHeaderAbsent => ("ingest", "source_header_absent", Some("CSV-16")),
    CsvFieldNameEmpty => ("ingest", "source_field_name_empty", Some("CSV-16")),
    CsvFieldNameDuplicate => ("ingest", "source_field_name_duplicate", Some("CSV-16")),
    CsvRecordWidth => ("ingest", "source_record_width", Some("CSV-16")),
    CsvQuoteUnterminated => ("ingest", "source_quote_unterminated", Some("CSV-16")),
    CsvCarriageReturn => ("ingest", "source_carriage_return", Some("CSV-16")),
    CsvTextAfterQuote => ("ingest", "source_text_after_quote", Some("CSV-16")),
    CsvQuoteInBareField => ("ingest", "source_quote_in_bare_field", Some("CSV-16")),
    GroupedRowReference => ("validation", "ungrouped_driver_field", Some("ROW-5")),
    GroupedColumnReference => ("validation", "ungrouped_driver_field", Some("NAME-4")),
    AggregateDriverScope => ("validation", "invalid_aggregate_context", Some("LOOKUP-18")),
    AggregateDeriveDeclaration => ("validation", "invalid_derive_step", Some("LOOKUP-22")),
    AggregateDeriveRelation => ("validation", "invalid_derive_step", Some("LOOKUP-22")),
    AggregateDeriveIntermediate => ("validation", "invalid_derive_step", Some("LOOKUP-22")),
    DonorFunctionIntermediate => ("validation", "invalid_derive_step", Some("LOOKUP-22")),
    AggregateDeriveVariable => ("validation", "unknown_derive_variable", Some("LOOKUP-22")),
    RowFilterPhaseBoundary => ("validation", "phase_boundary", Some("ROW-6")),
    RootFilterPhaseBoundary => ("validation", "phase_boundary", Some("ROW-9")),
    IntermediateSelfPhaseBoundary => ("validation", "phase_boundary", Some("LOOKUP-7")),
    IntermediateDerivationReference => ("validation", "unknown_field", Some("LOOKUP-15")),
    IntermediateDerivationDuplicate => ("validation", "duplicate_derivation", Some("LOOKUP-15")),
    IntermediateDerivationWindowReference => ("validation", "unknown_field", Some("LOOKUP-16")),
    IntermediateDerivationDependency => ("validation", "unknown_field", Some("LOOKUP-16")),
    IntermediateDerivationSelfPhase => ("validation", "phase_boundary", Some("LOOKUP-16")),
    IntermediateDerivationCycle => ("validation", "dependency_cycle", Some("LOOKUP-16")),
    IntermediateDriverInvalid => ("validation", "invalid_intermediate_driver", Some("ROW-2")),
    IntermediateDriverType => ("validation", "unknown_intermediate_driver_type", Some("ROW-2")),
    IntermediateUniqueCorrelatedDerivation => ("validation", "correlated_derivation_with_unique_verification", Some("LOOKUP-17")),
    IntermediateDuplicateRecords => ("verification", "duplicate_intermediate_records", Some("LOOKUP-17")),
    IntermediateUniqueField => ("validation", "unknown_field", Some("LOOKUP-17")),
    IntermediateUniqueCorrelatedFilter => ("validation", "correlated_filter_with_unique_verification", Some("LOOKUP-17")),
    // A `warning` unique on an intermediate without `keep` and `order_by`
    // (D210).
    IntermediateUniqueSeverity => ("validation", "value_not_permitted", Some("LOOKUP-17")),
    IntermediateIdentifierCollision => ("validation", "duplicate_identifier", Some("LOOKUP-3")),
    // A 1.0 intermediate read as a lookup without `key` (D23, D63).
    IntermediateKeyRequired => ("validation", "missing_required_field", Some("LOOKUP-3")),
    IntermediateDatasetReference => ("validation", "unknown_field", Some("NAME-1")),
    IntermediateRenameOnly => ("validation", "rename_only_intermediate", Some("LOOKUP-4")),
    IntermediateRepeatedKey => ("validation", "invalid_field_type", Some("LOOKUP-5")),
    IntermediateColumnReference => ("validation", "unknown_field", Some("LOOKUP-11")),
    RowDependencyPhaseBoundary => ("validation", "phase_boundary", Some("COLUMN-4")),
    IntermediateMatchPhaseBoundary => ("validation", "phase_boundary", Some("LOOKUP-2")),
    QualifiedNumericReference => ("validation", "qualified_identifier", Some("FORMULA-9")),
    RowNumericSourceScope => ("validation", "unknown_field", Some("FORMULA-9")),
    GroupedRowNumericSourceScope => ("validation", "unknown_field", Some("FORMULA-9")),
    SourceFilterReference => ("validation", "unknown_field", Some("LOOKUP-8")),
    SourceFilterSingleValue => ("validation", "prohibited_construct", Some("LOOKUP-8")),
    SourceRequireMatchScope => ("validation", "prohibited_construct", Some("LOOKUP-24")),
    SourceRangeScope => ("validation", "prohibited_construct", Some("LOOKUP-9")),
    SourceUnknownReference => ("validation", "unknown_field", Some("NAME-3")),
    VerificationUnknownReference => ("validation", "unknown_field", Some("CHECK-4")),
    OutputMissingKeyFailed => ("output", "missing_key", Some("OUTPUT-3")),
    OutputDuplicateKeyFailed => ("output", "duplicate_key", Some("OUTPUT-3")),
    VerificationUniqueFailed => ("verification", "unique_failed", Some("CHECK-10")),
    VerificationUniqueRedundant => ("validation", "redundant_declaration", Some("CHECK-10")),
    VerificationAllOrNoneFailed => ("verification", "all_or_none_failed", Some("CHECK-11")),
    VerificationAssertFailed => ("verification", "assert_failed", Some("CHECK-12")),
    VerificationRowCountFailed => ("verification", "row_count_failed", Some("CHECK-13")),
    VerificationNotMissingFailed => ("verification", "not_missing_failed", Some("CHECK-5")),
    VerificationNotMissingRedundant => ("validation", "redundant_declaration", Some("CHECK-5")),
    VerificationAllowedValuesFailed => ("verification", "allowed_values_failed", Some("CHECK-6")),
    SubmissionUnknownDocument => ("validation", "unknown_document", Some("DEFINE-8")),
    // A dataset's `submission.standard` naming no guide of its section (D16).
    SubmissionUnknownStandard => ("validation", "unknown_standard", Some("DEFINE-8")),
    SubmissionReservedMetadata => ("validation", "reserved_metadata_key", Some("SUB-2")),
    SubmissionMetadataMissing => ("validation", "submission_metadata_missing", Some("DEFINE-7")),
    SubmissionOriginMissing => ("validation", "origin_missing", Some("SUB-14")),
    // Two origins with one type and source, or a Predecessor or Not
    // Available origin beside another (D69).
    SubmissionConflictingOrigins => ("validation", "conflicting_origins", Some("SUB-14")),
    SubmissionSasName => ("validation", "sas_name_too_long", Some("DEFINE-22")),
    SubmissionCoreMandatory => ("validation", "core_mandatory_conflict", Some("SUB-10")),
    SubmissionKeyMandatory => ("validation", "value_not_permitted", Some("SUB-10")),
    SubmissionClassFamily => ("validation", "value_not_permitted", Some("SUB-10")),
    SubmissionMandatoryFailed => ("verification", "not_missing_failed", Some("SUB-11")),
    SubmissionDataTypeFailed => ("verification", "data_type_failed", Some("SUB-26")),
    SubmissionDatetimeColumn => ("validation", "value_not_permitted", Some("SUB-27")),
    SubmissionMandatoryRedundant => ("validation", "redundant_declaration", Some("SUB-11")),
    SubmissionAdamField => ("validation", "value_not_permitted", Some("SUB-5")),
    SubmissionOriginSource => ("validation", "origin_source_not_admitted", Some("SUB-15")),
    SubmissionOriginDescription => ("validation", "origin_description_missing", Some("SUB-16")),
    SubmissionMethodMissing => ("validation", "method_missing", Some("SUB-18")),
    SubmissionOriginRefuted => ("validation", "origin_contradicts_derivation", Some("SUB-19")),
    SubmissionAnnotatedCrfMissing => ("validation", "origin_document_missing", Some("DEFINE-14")),
    SubmissionOriginDocument => ("validation", "origin_document_missing", Some("SUB-17")),
    SubmissionDataType => ("validation", "submission_data_type_not_admitted", Some("SUB-6")),
    SubmissionLengthMissing => ("validation", "submission_length_missing", Some("SUB-7")),
    SubmissionLengthNotApplicable => ("validation", "submission_length_not_applicable", Some("SUB-7")),
    SubmissionLengthInvalid => ("validation", "submission_length_invalid", Some("SUB-7")),
    SubmissionLengthConflict => ("validation", "declared_length_conflict", Some("SUB-8")),
    SubmissionValueWhereInvalid => ("validation", "value_level_where_invalid", Some("SUB-12")),
    SubmissionValueDuplicateId => ("validation", "duplicate_identifier", Some("SUB-12")),
    SubmissionValueOverlap => ("validation", "value_level_overlap", Some("SUB-12")),
    SubmissionValueOriginRefuted => ("validation", "refuted_value_origin", Some("SUB-13")),
    ValueLevelMultipleMatches => ("verification", "value_level_multiple_matches", Some("SUB-13")),
    ValueLevelUnmatched => ("verification", "value_level_unmatched", Some("SUB-13")),
    SubmissionPredecessorLabel => ("validation", "predecessor_label_mismatch", Some("SUB-20")),
    // A 1.0 specification without the `model` its submission metadata or a
    // section's listing requires (D25).
    SubmissionModelRequired => ("validation", "missing_required_field", Some("SUB-3")),
    // A listed specification's `model` outside the section's model (D58).
    SubmissionModelMismatch => ("validation", "standard_model_mismatch", Some("SUB-3")),
    // A 1.0 section's guide outside the section's model, and a guide name an
    // earlier entry of its `standards` takes (D16).
    SectionStandardModel => ("validation", "standard_model_mismatch", Some("DEFINE-6")),
    SectionStandardDuplicate => ("validation", "duplicate_identifier", Some("DEFINE-6")),
    SubmissionDefineExtension => ("validation", "value_not_permitted", Some("DEFINE-2")),
    SubmissionDefineName => ("validation", "value_not_permitted", Some("DEFINE-2")),
    SubmissionSourceVersion => ("validation", "source_system_incomplete", Some("DEFINE-12")),
    SubmissionDuplicateIdentifier => ("validation", "duplicate_define_identifier", Some("DEFINE-10")),
    SubmissionAnnotatedCrfMultiplicity => ("validation", "value_not_permitted", Some("DEFINE-14")),
    SubmissionDuplicateDataset => ("validation", "duplicate_dataset_entry", Some("DEFINE-5")),
    SubmissionReferenceRepeating => ("validation", "reference_data_repeating_conflict", Some("SUB-4")),
    // A non-standard dataset that also selects a section guide (#4023).
    SubmissionNonStandardBound => ("validation", "value_not_permitted", Some("SUB-4")),
    SubmissionNoDataComment => ("validation", "submission_metadata_missing", Some("DEFINE-17")),
    SubmissionTransportEmpty => ("output", "transport_dataset_empty", Some("JSON-3")),
    SubmissionLabelInvalid => ("validation", "transport_label_invalid", Some("XPT-5")),
    SubmissionDisplayFormatConflict => ("validation", "value_not_permitted", Some("SUB-25")),
    TransportTextInvalid => ("output", "transport_text_invalid", Some("XPT-7")),
    XptValueOutOfRange => ("output", "xpt_value_out_of_range", Some("XPT-8")),
    XptDisplayFormatInvalid => ("validation", "xpt_display_format_invalid", Some("XPT-9")),
    SubmissionTransportDecimals => ("validation", "decimals_not_applicable", Some("JSON-5")),
    XptFileTooLarge => ("output", "xpt_file_too_large", Some("XPT-10")),
    XptLastRowBlank => ("output", "xpt_last_row_blank", Some("XPT-10")),
    SubmissionColumnNotOutput => ("validation", "submission_not_output_column", Some("SUB-1")),
    SubmissionCdiscAliasMissing => ("validation", "missing_required_field", Some("DEFINE-4")),
    // A `Submission` section without the transport a regulator takes (#4050).
    SubmissionTransportRequired => ("validation", "missing_required_field", Some("DEFINE-4")),
    SubmissionXmlWhitespace => ("validation", "untransportable_text", Some("DEFINE-24")),
    SubmissionXmlInvalid => ("validation", "value_not_permitted", Some("DEFINE-25")),
    SubmissionJsonInvalid => ("validation", "value_not_permitted", Some("JSON-4")),
    SubmissionInventoryFailed => ("verification", "allowed_values_failed", Some("CT-11")),
    DefineRequiredDeclaration => ("validation", "missing_required_field", Some("DEFINE-28")),
    DefineArtifactOutsidePackage => ("validation", "artifact_outside_document", Some("DEFINE-16")),
    DefineArtifactCollision => ("validation", "artifact_path_collision", Some("JSON-2")),
    DefineTransportCollision => ("validation", "transport_path_collision", Some("JSON-2")),
    DefineCreatedInvalid => ("validation", "value_not_permitted", Some("DEFINE-3")),
    CodelistUnknownBinding => ("validation", "unknown_codelist", Some("CT-1")),
    ProjectBindingMissing => ("validation", "project_binding_missing", Some("CT-13")),
    CodelistBindingTypeMismatch => ("validation", "codelist_type_mismatch", Some("CT-8")),
    CodelistAllowedValuesRedundant => ("validation", "redundant_declaration", Some("CT-9")),
    VerificationCodelistFailed => ("verification", "allowed_values_failed", Some("CT-7")),
    VerificationRangeFailed => ("verification", "range_failed", Some("CHECK-7")),
    VerificationMaxLengthFailed => ("verification", "length_failed", Some("CHECK-8")),
    OutputUnknownReference => ("validation", "unknown_field", Some("NAME-3")),
    OutputUnresolvableReference => ("validation", "unresolvable_name", Some("NAME-3")),
    ColumnDependencyCycle => ("validation", "dependency_cycle", Some("COLUMN-6")),
    ColumnForwardReference => ("validation", "forward_reference", Some("COLUMN-5")),
    ColumnMissingKeyDerivation => ("validation", "key_dependency", Some("COLUMN-7")),
    ColumnKeyDependency => ("validation", "key_dependency", Some("COLUMN-7")),
    ColumnKeyRecordRead => ("validation", "key_dependency", Some("COLUMN-7")),
    WindowZeroOffset => ("validation", "zero_offset", Some("WINDOW-7")),
    WindowOrderRequired => ("validation", "window_order_by_required", Some("WINDOW-5")),
    WindowBaselineOrder => ("validation", "value_not_permitted", Some("WINDOW-5")),
    WindowRepeatedGroup => ("validation", "invalid_field_type", Some("WINDOW-3")),
    WindowBaselineInputType => ("validation", "incompatible_input_type", Some("VALUE-2")),
    // An order term, a named intermediate's or a window's, over a field whose
    // types do not all compare (VALUE-19), such as `str` beside `int` (#4068).
    OrderTermType => ("validation", "incompatible_input_type", Some("VALUE-22")),
    // A column-level window's read of a driver that some row template does
    // not share names nothing that template's rows hold, as a range value
    // reading such a driver does (LOOKUP-9).
    WindowDriverReference => ("validation", "unknown_field", Some("WINDOW-11")),
    LookupSelectionReference => ("validation", "unknown_field", Some("LOOKUP-7")),
    LookupValueReference => ("validation", "unknown_field", Some("LOOKUP-11")),
    // One cause per lookup fault, whichever kind of lookup raises it
    // (LOOKUP-25): an implicit join, a named intermediate or an aggregate.
    LookupEmptyKey => ("validation", "invalid_field_type", Some("LOOKUP-5")),
    LookupNoApplicableKeys => ("validation", "no_applicable_keys", Some("LOOKUP-5")),
    LookupDonorKeyReference => ("validation", "unknown_field", Some("LOOKUP-5")),
    LookupMatchValueReference => ("validation", "unknown_field", Some("LOOKUP-5")),
    LookupKeyType => ("validation", "incompatible_input_type", Some("LOOKUP-9")),
    LookupRangeDeclaration => ("validation", "invalid_field_type", Some("LOOKUP-9")),
    LookupRangeReference => ("validation", "unknown_field", Some("LOOKUP-9")),
    LookupRangeTypes => ("validation", "incomparable_range_types", Some("LOOKUP-9")),
    LookupSelectionUnpaired => ("validation", "unpaired_fields", Some("LOOKUP-10")),
    LookupAbsenceConflict => ("validation", "conflicting_absence_policy", Some("LOOKUP-24")),
    LookupNearestSelection => ("validation", "prohibited_construct", Some("LOOKUP-26")),
    LookupNearestSourceReference => ("validation", "unknown_field", Some("LOOKUP-26")),
    LookupNearestSourceType => ("validation", "incompatible_input_type", Some("LOOKUP-26")),
    LookupNearestReferenceType => ("validation", "incompatible_input_type", Some("LOOKUP-26")),
    // The pair relation of a named intermediate that writes `inner_join`
    // (LOOKUP-27, D263).
    LookupPairConstruct => ("validation", "prohibited_construct", Some("LOOKUP-27")),
    LookupPairKeyRequired => ("validation", "missing_required_field", Some("LOOKUP-27")),
    LookupPairDerivationsRequired => ("validation", "missing_required_field", Some("LOOKUP-27")),
    LookupPairDerivationsEmpty => ("validation", "invalid_field_type", Some("LOOKUP-27")),
    LookupPairSide => ("validation", "invalid_intermediate_driver", Some("LOOKUP-27")),
    LookupPairSideRepeated => ("validation", "duplicate_identifier", Some("LOOKUP-27")),
    LookupPairKeyReference => ("validation", "unknown_field", Some("LOOKUP-27")),
    LookupPairKeyType => ("validation", "incompatible_input_type", Some("LOOKUP-27")),
    LookupPairReference => ("validation", "unknown_field", Some("LOOKUP-27")),
    LookupPairFieldType => ("validation", "unknown_intermediate_driver_type", Some("LOOKUP-27")),
    FlagMissingValueRequired => ("validation", "missing_value_required", Some("EXPR-8")),
    PredicateUnknownField => ("validation", "unknown_field", Some("FORMULA-9")),
    PredicateUnresolvableName => ("validation", "unresolvable_name", Some("FORMULA-9")),
    PredicateInputType => ("validation", "incompatible_input_type", Some("FORMULA-5")),
    PredicateIncompatiblePair => ("validation", "incompatible_input_type", Some("FORMULA-5")),
    PredicateExpectedText => ("validation", "incompatible_input_type", Some("FORMULA-5")),
    PredicateContainsExpectedText => ("validation", "incompatible_input_type", Some("FORMULA-8")),
    PredicateDanglingEscape => ("validation", "invalid_expression", Some("FORMULA-7")),
    RegexInvalidPattern => ("validation", "invalid_regex", Some("TEXT-6")),
    VerificationMatchesFailed => ("verification", "matches_failed", Some("CHECK-9")),
    VerificationIdentifierType => ("validation", "invalid_declaration", Some("CHECK-3")),
    VerificationDeclarationRequired => ("validation", "invalid_declaration", Some("CHECK-4")),
    VerificationIdentifierRepeated => ("validation", "duplicate_identifier", Some("CHECK-3")),
    VerificationGrouping => ("validation", "invalid_declaration", Some("CHECK-14")),
    VerificationBounds => ("validation", "invalid_declaration", Some("CHECK-4")),
    VerificationLengthBounds => ("validation", "invalid_declaration", Some("CHECK-8")),
    VerificationCardinality => ("validation", "invalid_declaration", Some("CHECK-11")),
    VerificationColumnType => ("validation", "invalid_declaration", Some("CHECK-4")),
    VerificationDeclaredColumnUnknown => ("validation", "unknown_field", Some("CHECK-4")),
    SourceTypeUnknownField => ("validation", "unknown_field", Some("INPUT-6")),
    SourceTypeFieldParse => ("ingest", "field_parse_failed", Some("INPUT-7")),
    ParquetInvalid => ("ingest", "source_parquet_invalid", Some("PARQUET-5")),
    ParquetFieldNameEmpty => ("ingest", "source_field_name_empty", Some("PARQUET-6")),
    ParquetFieldNameDuplicate => ("ingest", "source_field_name_duplicate", Some("PARQUET-6")),
    ParquetFieldTypeUnsupported => ("ingest", "source_field_type_unsupported", Some("PARQUET-7")),
    ParquetFieldValueInvalid => ("ingest", "source_field_value_invalid", Some("PARQUET-9")),
}

/// JSON context retains arbitrary-width integer numbers and ordered sequences.
pub type Context = crate::json::Map<String, crate::json::Value>;

/// The one diagnostic record, from the failure site to the edge: the
/// registered cause, the specification paths in authored order, the
/// structured context and, when recorded, the span and operand route.
///
/// Phase, condition and rule are not stored: each is a pure function
/// of `code` in this registry. Missing geometry means unavailable, never
/// zero: a present `start == end == 0` span and a present empty route are
/// recorded facts. The context is an owned JSON tree whose numbers keep their
/// exact spelling; JSON text is made only at the edge that renders it.
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

    /// The lifecycle phase of the cause.
    pub fn phase(&self) -> &'static str {
        self.definition().phase
    }

    /// The public condition of the cause.
    pub fn condition(&self) -> &'static str {
        self.definition().condition
    }

    /// The rule the cause cites, absent only for implementation refusals.
    pub fn rule(&self) -> Option<&'static str> {
        self.definition().rule
    }

    /// The severity of the cause's condition.
    pub fn severity(&self) -> Severity {
        self.code.severity()
    }
}
