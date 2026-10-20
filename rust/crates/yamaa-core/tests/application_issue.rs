//! Independent compatibility control for public engine fault spellings.
use std::collections::BTreeSet;
use yamaa_core::{
    application_issue::{rejected, Fault, Stage},
    diagnostic::ConditionCode,
    json::Value as JsonValue,
};

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
    (Fault::OutputPublicationFailed, "output_publication_failed"),
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
    (Fault::YamlAdmission, "yaml_admission"),
    (Fault::YamlDecode, "yaml_decode"),
    (Fault::YamlReaderUnavailable, "yaml_reader_unavailable"),
];

#[test]
fn stage_and_fault_spellings_match_the_frozen_public_vocabulary() {
    assert_eq!(STAGE_SPELLINGS.len(), 16);
    assert_eq!(FAULT_SPELLINGS.len(), 80);
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
