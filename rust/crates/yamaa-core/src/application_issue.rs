//! Application admission failures are distinct from normative language findings.
use crate::diagnostic::{ConditionCode, Diagnostic};
use crate::json::Value as JsonValue;
use std::vec;

/// One closed vocabulary: its variants, their spellings and the list of all.
macro_rules! vocabulary {
    ($(#[$meta:meta])* $name:ident { $($(#[$doc:meta])* $variant:ident => $spelling:expr,)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($(#[$doc])* $variant,)*
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $spelling,)*
                }
            }
        }
    };
}

vocabulary! {
    /// Where the engine refused, named in an engine fault's `stage` context key.
    Stage {
        Bind => "bind",
        Build => "build",
        Capture => "capture",
        Check => "check",
        Define => "define",
        Dictionary => "dictionary",
        Environment => "environment",
        Execute => "execute",
        Ingest => "ingest",
        Inheritance => "inheritance",
        Output => "output",
        Prepare => "prepare",
        Project => "project",
        PublicationTargetAdmission => "publication_target_admission",
        Report => "report",
        Resolve => "resolve",
    }
}

vocabulary! {
    /// The closed list of engine fault codes (D73), named in an engine fault's
    /// `code` context key. Each is a classification an adapter or the facade
    /// selects from a typed cause, never text taken from an exception or a panic.
    Fault {
        ActivationBoundary => "activation_boundary",
        ArtifactRepresentation => "artifact_representation",
        CleanupIncomplete => "cleanup_incomplete",
        CompilationBoundary => "compilation_boundary",
        CompilerLimit => "compiler_limit",
        CsvLimit => "csv_limit",
        DeclaringOrigin => "declaring_origin",
        Dependencies => "dependencies",
        DependencyPolicy => "dependency_policy",
        DiagnosticContext => "diagnostic_context",
        Document => "document",
        EnvironmentBoundary => "environment_boundary",
        ExecutionBoundary => "execution_boundary",
        ExecutionLimit => "execution_limit",
        HostPublicationFailure => "host_publication_failure",
        HostResourceFailure => "host_resource_failure",
        InheritanceRequired => "inheritance_required",
        Internal => "internal",
        InvalidPackagingLock => "invalid_packaging_lock",
        InvalidTarget => "invalid_target",
        Inventory => "inventory",
        IssueProjection => "issue_projection",
        Normalization => "normalization",
        NumericPolicy => "numeric_policy",
        OdmProjectionBoundary => "odm_projection_boundary",
        OpaqueHost => "opaque_host",
        OutputLimit => "output_limit",
        OutputProjection => "output_projection",
        OutputSchema => "output_schema",
        ParentRequired => "parent_required",
        ParquetBoundary => "parquet_boundary",
        ParquetCodecUnavailable => "parquet_codec_unavailable",
        ParquetLimit => "parquet_limit",
        ParquetReaderUnavailable => "parquet_reader_unavailable",
        PathMismatch => "path_mismatch",
        ProducerContract => "producer_contract",
        ProducerMetadata => "producer_metadata",
        ProjectSections => "project_sections",
        PublicationCancelled => "publication_cancelled",
        /// A publication input or output error; it spells the condition a
        /// classified publication failure reports.
        PublicationFailed => ConditionCode::OutputPublicationFailed.definition().condition,
        PublicationLimit => "publication_limit",
        PublicationReportLimit => "publication_report_limit",
        PublicationUnwind => "publication_unwind",
        ReferenceCatalog => "reference_catalog",
        ReportRefusal => "report_refusal",
        ReportStack => "report_stack",
        ResourceBase => "resource_base",
        ResourceChanged => "resource_changed",
        ResourceLimit => "resource_limit",
        ResourceLink => "resource_link",
        ResourceMissing => "resource_missing",
        ResourceNotRegular => "resource_not_regular",
        ResourceOutsideRoots => "resource_outside_roots",
        ResourcePath => "resource_path",
        ResourceRoot => "resource_root",
        ResourceUnavailable => "resource_unavailable",
        ResourceUnreadable => "resource_unreadable",
        ResultProjection => "result_projection",
        SasBoundary => "sas_boundary",
        SasLimit => "sas_limit",
        SasReaderUnavailable => "sas_reader_unavailable",
        SchemaBundle => "schema_bundle",
        ShippedSchema => "shipped_schema",
        SourceBoundary => "source_boundary",
        SourceCollectionBoundary => "source_collection_boundary",
        SourceCount => "source_count",
        SourceLimit => "source_limit",
        SourceOrdinalBoundary => "source_ordinal_boundary",
        StagingCollision => "staging_collision",
        TableLimit => "table_limit",
        TargetChanged => "target_changed",
        Transport => "transport",
        UnknownClass => "unknown_class",
        /// A section name the project scope does not define; D73 moves it to a
        /// validation condition of its own.
        UnknownSection => "unknown_section",
        ValidationPolicy => "validation_policy",
        VersionedRootRequired => "versioned_root_required",
        YamlDecode => "yaml_decode",
        YamlReaderUnavailable => "yaml_reader_unavailable",
    }
}

/// Format classified installed-package disagreement. Packaging tools supply the
/// facts; this owns the stable normative issue identity without comparing versions.
pub fn lock_mismatch(context: crate::diagnostic::Context) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::ProjectLockMismatch,
        spec_paths: vec!["lock".into()],
        context,
        source_span: None,
        operand_route: None,
    }
}

pub fn unsupported(operation: &str, path: Option<&str>) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::ApplicationUnsupported,
        spec_paths: path.map(String::from).into_iter().collect(),
        context: [(
            String::from("operation"),
            JsonValue::String(operation.into()),
        )]
        .into(),
        source_span: None,
        operand_route: None,
    }
}

/// An engine fault: the stage that refused and its closed code. No
/// requirement is fabricated.
pub fn rejected(stage: Stage, fault: Fault) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::ApplicationRejected,
        spec_paths: Vec::new(),
        context: vec![
            (
                String::from("stage"),
                JsonValue::String(stage.as_str().into()),
            ),
            (
                String::from("code"),
                JsonValue::String(fault.as_str().into()),
            ),
        ]
        .into_iter()
        .collect(),
        source_span: None,
        operand_route: None,
    }
}

/// A policy refusal names its exhausted capacity without inventing a language
/// requirement. The caller supplies only admitted, typed resource facts.
pub fn resource_rejected(
    stage: Stage,
    fault: Fault,
    resource: &str,
    limit: usize,
    actual: Option<usize>,
    spec_paths: Vec<String>,
) -> Diagnostic {
    let mut finding = rejected(stage, fault);
    finding.spec_paths = spec_paths;
    finding
        .context
        .insert("resource".into(), JsonValue::String(resource.into()));
    finding
        .context
        .insert("limit".into(), JsonValue::from(limit));
    if let Some(actual) = actual {
        finding
            .context
            .insert("actual".into(), JsonValue::from(actual));
    }
    finding
}

/// A classified failure during the actual atomic publication request. Target
/// admission refusals and opaque interrupts do not establish this cause.
pub fn publication_failed(target: &str) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::OutputPublicationFailed,
        spec_paths: vec!["output.path".into()],
        context: [(String::from("path"), JsonValue::String(target.into()))].into(),
        source_span: None,
        operand_route: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    // Frozen public spellings, authored independently of the production mapping.
    // A spelling change must be reviewed as a diagnostic compatibility decision.
    const STAGE_SPELLINGS: &[(Stage, &str)] = &[
        (Stage::Bind, "bind"),
        (Stage::Build, "build"),
        (Stage::Capture, "capture"),
        (Stage::Check, "check"),
        (Stage::Define, "define"),
        (Stage::Dictionary, "dictionary"),
        (Stage::Environment, "environment"),
        (Stage::Execute, "execute"),
        (Stage::Ingest, "ingest"),
        (Stage::Inheritance, "inheritance"),
        (Stage::Output, "output"),
        (Stage::Prepare, "prepare"),
        (Stage::Project, "project"),
        (
            Stage::PublicationTargetAdmission,
            "publication_target_admission",
        ),
        (Stage::Report, "report"),
        (Stage::Resolve, "resolve"),
    ];
    const FAULT_SPELLINGS: &[(Fault, &str)] = &[
        (Fault::ActivationBoundary, "activation_boundary"),
        (Fault::ArtifactRepresentation, "artifact_representation"),
        (Fault::CleanupIncomplete, "cleanup_incomplete"),
        (Fault::CompilationBoundary, "compilation_boundary"),
        (Fault::CompilerLimit, "compiler_limit"),
        (Fault::CsvLimit, "csv_limit"),
        (Fault::DeclaringOrigin, "declaring_origin"),
        (Fault::Dependencies, "dependencies"),
        (Fault::DependencyPolicy, "dependency_policy"),
        (Fault::DiagnosticContext, "diagnostic_context"),
        (Fault::Document, "document"),
        (Fault::EnvironmentBoundary, "environment_boundary"),
        (Fault::ExecutionBoundary, "execution_boundary"),
        (Fault::ExecutionLimit, "execution_limit"),
        (Fault::HostPublicationFailure, "host_publication_failure"),
        (Fault::HostResourceFailure, "host_resource_failure"),
        (Fault::InheritanceRequired, "inheritance_required"),
        (Fault::Internal, "internal"),
        (Fault::InvalidPackagingLock, "invalid_packaging_lock"),
        (Fault::InvalidTarget, "invalid_target"),
        (Fault::Inventory, "inventory"),
        (Fault::IssueProjection, "issue_projection"),
        (Fault::Normalization, "normalization"),
        (Fault::NumericPolicy, "numeric_policy"),
        (Fault::OdmProjectionBoundary, "odm_projection_boundary"),
        (Fault::OpaqueHost, "opaque_host"),
        (Fault::OutputLimit, "output_limit"),
        (Fault::OutputProjection, "output_projection"),
        (Fault::OutputSchema, "output_schema"),
        (Fault::ParentRequired, "parent_required"),
        (Fault::ParquetBoundary, "parquet_boundary"),
        (Fault::ParquetCodecUnavailable, "parquet_codec_unavailable"),
        (Fault::ParquetLimit, "parquet_limit"),
        (
            Fault::ParquetReaderUnavailable,
            "parquet_reader_unavailable",
        ),
        (Fault::PathMismatch, "path_mismatch"),
        (Fault::ProducerContract, "producer_contract"),
        (Fault::ProducerMetadata, "producer_metadata"),
        (Fault::ProjectSections, "project_sections"),
        (Fault::PublicationCancelled, "publication_cancelled"),
        (Fault::PublicationFailed, "publication_failed"),
        (Fault::PublicationLimit, "publication_limit"),
        (Fault::PublicationReportLimit, "publication_report_limit"),
        (Fault::PublicationUnwind, "publication_unwind"),
        (Fault::ReferenceCatalog, "reference_catalog"),
        (Fault::ReportRefusal, "report_refusal"),
        (Fault::ReportStack, "report_stack"),
        (Fault::ResourceBase, "resource_base"),
        (Fault::ResourceChanged, "resource_changed"),
        (Fault::ResourceLimit, "resource_limit"),
        (Fault::ResourceLink, "resource_link"),
        (Fault::ResourceMissing, "resource_missing"),
        (Fault::ResourceNotRegular, "resource_not_regular"),
        (Fault::ResourceOutsideRoots, "resource_outside_roots"),
        (Fault::ResourcePath, "resource_path"),
        (Fault::ResourceRoot, "resource_root"),
        (Fault::ResourceUnavailable, "resource_unavailable"),
        (Fault::ResourceUnreadable, "resource_unreadable"),
        (Fault::ResultProjection, "result_projection"),
        (Fault::SasBoundary, "sas_boundary"),
        (Fault::SasLimit, "sas_limit"),
        (Fault::SasReaderUnavailable, "sas_reader_unavailable"),
        (Fault::SchemaBundle, "schema_bundle"),
        (Fault::ShippedSchema, "shipped_schema"),
        (Fault::SourceBoundary, "source_boundary"),
        (
            Fault::SourceCollectionBoundary,
            "source_collection_boundary",
        ),
        (Fault::SourceCount, "source_count"),
        (Fault::SourceLimit, "source_limit"),
        (Fault::SourceOrdinalBoundary, "source_ordinal_boundary"),
        (Fault::StagingCollision, "staging_collision"),
        (Fault::TableLimit, "table_limit"),
        (Fault::TargetChanged, "target_changed"),
        (Fault::Transport, "transport"),
        (Fault::UnknownClass, "unknown_class"),
        (Fault::UnknownSection, "unknown_section"),
        (Fault::ValidationPolicy, "validation_policy"),
        (Fault::VersionedRootRequired, "versioned_root_required"),
        (Fault::YamlDecode, "yaml_decode"),
        (Fault::YamlReaderUnavailable, "yaml_reader_unavailable"),
    ];

    #[test]
    fn stage_and_fault_spellings_match_the_frozen_public_vocabulary() {
        assert_eq!(STAGE_SPELLINGS.len(), 16);
        assert_eq!(FAULT_SPELLINGS.len(), 78);
        assert_eq!(Stage::ALL.len(), STAGE_SPELLINGS.len());
        assert_eq!(Fault::ALL.len(), FAULT_SPELLINGS.len());
        for &stage in Stage::ALL {
            let expected = STAGE_SPELLINGS
                .iter()
                .find(|(candidate, _)| *candidate == stage)
                .expect("every stage needs an independent literal spelling")
                .1;
            assert_eq!(stage.as_str(), expected, "{stage:?}");
        }
        for &fault in Fault::ALL {
            let expected = FAULT_SPELLINGS
                .iter()
                .find(|(candidate, _)| *candidate == fault)
                .expect("every fault needs an independent literal spelling")
                .1;
            assert_eq!(fault.as_str(), expected, "{fault:?}");
        }
    }

    fn snake_case(text: &str) -> bool {
        !text.is_empty()
            && !text.starts_with('_')
            && !text.ends_with('_')
            && !text.contains("__")
            && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
    }

    #[test]
    fn stages_and_faults_are_closed_lists_of_distinct_codes() {
        for spellings in [
            Stage::ALL
                .iter()
                .map(|stage| stage.as_str())
                .collect::<Vec<_>>(),
            Fault::ALL
                .iter()
                .map(|fault| fault.as_str())
                .collect::<Vec<_>>(),
        ] {
            assert!(spellings.iter().all(|spelling| snake_case(spelling)));
            let distinct = spellings.iter().collect::<BTreeSet<_>>();
            assert_eq!(distinct.len(), spellings.len(), "{spellings:?}");
        }
    }

    #[test]
    fn an_engine_fault_names_its_stage_and_code_and_cites_no_rule() {
        for &(stage, stage_spelling) in STAGE_SPELLINGS {
            for &(fault, fault_spelling) in FAULT_SPELLINGS {
                let finding = rejected(stage, fault);
                assert_eq!(finding.code, ConditionCode::ApplicationRejected);
                assert_eq!(finding.requirement(), None);
                assert!(finding.spec_paths.is_empty());
                assert_eq!(
                    finding.context,
                    [
                        ("code".into(), JsonValue::String(fault_spelling.into())),
                        ("stage".into(), JsonValue::String(stage_spelling.into())),
                    ]
                    .into()
                );
            }
        }
    }
}
