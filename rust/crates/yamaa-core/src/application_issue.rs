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
    /// The step of an operation in which the engine refused, named in an
    /// engine fault's `step` context key (P-PHASE-1: a phase is a
    /// condition's, never an engine step's).
    Step {
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
        /// A progress callback panicked or raised (#3762).
        HostProgressFailure => "host_progress_failure",
        HostPublicationFailure => "host_publication_failure",
        HostResourceFailure => "host_resource_failure",
        InheritanceRequired => "inheritance_required",
        Internal => "internal",
        InvalidTarget => "invalid_target",
        Inventory => "inventory",
        IssueProjection => "issue_projection",
        Normalization => "normalization",
        NumericPolicy => "numeric_policy",
        OdmProjectionBoundary => "odm_projection_boundary",
        OdmReaderUnavailable => "odm_reader_unavailable",
        OpaqueHost => "opaque_host",
        OutputLimit => "output_limit",
        OutputProjection => "output_projection",
        /// The lint's publication I/O refusal, retaining its public fault code.
        OutputPublicationFailed => "output_publication_failed",
        OutputSchema => "output_schema",
        ParentRequired => "parent_required",
        ParquetBoundary => "parquet_boundary",
        ParquetCodecUnavailable => "parquet_codec_unavailable",
        ParquetLimit => "parquet_limit",
        ParquetReaderUnavailable => "parquet_reader_unavailable",
        /// A partitioned run that needs the whole table at a step and was not
        /// allowed to fall back to it (#3697).
        PartitionGlobal => "partition_global",
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
        ValidationPolicy => "validation_policy",
        YamlAdmission => "yaml_admission",
        YamlDecode => "yaml_decode",
        YamlReaderUnavailable => "yaml_reader_unavailable",
    }
}

/// Format classified installed-package disagreement. Packaging tools supply the
/// facts; this owns the stable normative issue identity without comparing versions.
/// `field` declares the lock: `environments.<host>.lock` in the project file
/// (FUNC-3).
pub fn lock_mismatch(field: &str, context: crate::diagnostic::Context) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::ProjectLockMismatch,
        spec_paths: vec![field.into()],
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

/// An engine fault: the step that refused and its closed code. No rule
/// is fabricated.
pub fn rejected(step: Step, fault: Fault) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::ApplicationRejected,
        spec_paths: Vec::new(),
        context: vec![
            (
                String::from("step"),
                JsonValue::String(step.as_str().into()),
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

/// The refusal of a partitioned run without the whole-table fallback
/// (#3697): the step at `path` needs the whole table. It is the engine's,
/// at the spec path of the step it cannot divide, or at none for a step
/// with no authored path.
pub fn partition_global(path: &str) -> Diagnostic {
    Diagnostic {
        spec_paths: (!path.is_empty())
            .then(|| path.to_owned())
            .into_iter()
            .collect(),
        ..rejected(Step::Execute, Fault::PartitionGlobal)
    }
}

/// A policy refusal names its exhausted capacity without inventing a language
/// rule. The caller supplies only admitted, typed resource facts.
pub fn resource_rejected(
    step: Step,
    fault: Fault,
    resource: &str,
    limit: usize,
    actual: Option<usize>,
    spec_paths: Vec<String>,
) -> Diagnostic {
    let mut finding = rejected(step, fault);
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

    fn snake_case(text: &str) -> bool {
        !text.is_empty()
            && !text.starts_with('_')
            && !text.ends_with('_')
            && !text.contains("__")
            && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
    }

    #[test]
    fn steps_and_faults_are_closed_lists_of_distinct_codes() {
        for spellings in [
            Step::ALL
                .iter()
                .map(|step| step.as_str())
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
    fn an_engine_fault_names_its_step_and_code_and_cites_no_rule() {
        for &step in Step::ALL {
            for &fault in Fault::ALL {
                let finding = rejected(step, fault);
                assert_eq!(finding.code, ConditionCode::ApplicationRejected);
                assert_eq!(finding.rule(), None);
                assert!(finding.spec_paths.is_empty());
                assert_eq!(
                    finding.context,
                    [
                        ("code".into(), JsonValue::String(fault.as_str().into())),
                        ("step".into(), JsonValue::String(step.as_str().into())),
                    ]
                    .into()
                );
            }
        }
    }
}
