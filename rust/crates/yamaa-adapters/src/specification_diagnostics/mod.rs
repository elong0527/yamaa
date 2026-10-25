//! Compiler and ingestion refusals as typed diagnostic records. This layer only
//! represents findings; it does not select a phase, bind a name, evaluate a
//! formula or read a table.
use crate::{
    csv_source::{self, TextTableError},
    source_decoding::Error,
};
use yamaa_core::application_issue::{self as issue, Fault, Step};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::json::Value;
use yamaa_core::specification::{BindError, PrepareError, SourceDeclaration};

/// A refused preparation, typed from the failure site to the issues it
/// reports. Callers that keep only some refusals choose by variant, never by
/// reading a reply.
#[derive(Clone, Debug, PartialEq)]
pub enum Rejection {
    /// Language findings, in authored order.
    Invalid(Vec<Diagnostic>),
    /// Authored vocabulary the engine does not support, one issue per use.
    Unsupported(Vec<Diagnostic>),
    /// An engine fault: its step and closed code, with the bound it hit
    /// when it hit one.
    Fault(Diagnostic),
    /// A bound a preparation pass hit; its context names the pass as `phase`.
    Limit(Diagnostic),
}

impl Rejection {
    /// The issues the refusal reports, in order.
    pub fn into_issues(self) -> Vec<Diagnostic> {
        match self {
            Self::Invalid(issues) | Self::Unsupported(issues) => issues,
            Self::Fault(issue) | Self::Limit(issue) => vec![issue],
        }
    }
}

pub(crate) fn fault(step: Step, fault: Fault) -> Rejection {
    Rejection::Fault(issue::rejected(step, fault))
}

/// The bound one preparation pass, named `phase`, hit.
pub(crate) fn bound(
    step: Step,
    phase: &str,
    resource: &str,
    limit: usize,
    actual: Option<usize>,
) -> Rejection {
    let mut finding = issue::resource_rejected(
        step,
        Fault::ResourceLimit,
        resource,
        limit,
        actual,
        Vec::new(),
    );
    finding
        .context
        .insert("phase".into(), Value::String(phase.into()));
    Rejection::Limit(finding)
}

pub fn binding(error: &BindError, source: &SourceDeclaration) -> Option<Vec<Diagnostic>> {
    let BindError::Invalid(findings) = error else {
        return None;
    };
    let mut diagnostics = Vec::new();
    for finding in findings {
        diagnostics.extend(finding.diagnostics(source)?);
    }
    Some(diagnostics)
}

/// Project complete retained compiler and codec findings without repeating source reads.
pub fn findings(
    error: &Error,
    source: Option<&SourceDeclaration>,
) -> Result<Option<Vec<Diagnostic>>, crate::source_diagnostics::SourceProjectionError> {
    match error {
        Error::Sources(_)
        | Error::ParquetSource(_)
        | Error::SasSource(_)
        | Error::DatasetJsonSource(_)
        | Error::TypedSource(_)
        | Error::Source(_)
        | Error::Ordinal(_)
        | Error::OdmFields(_)
        | Error::OdmValues(_)
        | Error::OdmSource(_)
        | Error::ProducerMetadata(_) => crate::source_diagnostics::findings(error, source),
        Error::Prepare(PrepareError::Invalid(errors)) => Ok(Some(
            errors.iter().map(|error| error.diagnostic()).collect(),
        )),
        Error::Bind(error) => Ok(source.and_then(|source| binding(error, source))),
        _ => Ok(None),
    }
}

/// The refusal of one compiler, codec, binding or execution failure. Semantic
/// findings stay separate from unsupported vocabulary and from resource and
/// internal faults; no Debug text leaks.
pub fn failure(error: &Error, source: Option<&SourceDeclaration>) -> Rejection {
    match findings(error, source) {
        Err(crate::source_diagnostics::SourceProjectionError::InvalidContext) => {
            return self::fault(Step::Ingest, Fault::DiagnosticContext)
        }
        Ok(Some(diagnostics)) => return Rejection::Invalid(diagnostics),
        Ok(None) => {}
    }
    let refusal = match error {
        Error::ResourceLimit(refusal) => Some(refusal.clone()),
        _ => source.and_then(|source| crate::resource_diagnostics::source(error, source)),
    };
    if let Some(refusal) = refusal {
        let code = match error {
            Error::Source(TextTableError::Csv(_)) => Fault::CsvLimit,
            Error::Source(TextTableError::Table(_)) => Fault::TableLimit,
            Error::ParquetSource(_) => Fault::ParquetLimit,
            Error::SasSource(_) => Fault::SasLimit,
            Error::DatasetJsonSource(crate::dataset_json_source::Error::Table(_)) => {
                Fault::TableLimit
            }
            _ => Fault::SourceLimit,
        };
        return Rejection::Fault(refusal.diagnostic(Step::Ingest, code));
    }
    if let Error::Prepare(PrepareError::Unsupported(features)) = error {
        return Rejection::Unsupported(
            features
                .iter()
                .map(|feature| issue::unsupported(&feature.operation, Some(&feature.path)))
                .collect(),
        );
    }
    if let Error::Prepare(error) = error {
        return preparation(error);
    }
    let (step, code) = match error {
        Error::Prepare(_) => unreachable!("preparation errors handled above"),
        Error::Source(TextTableError::Csv(csv_source::Error::Limit { .. })) => {
            (Step::Ingest, Fault::CsvLimit)
        }
        Error::Source(TextTableError::Table(crate::column_table::TableError::Limit { .. })) => {
            (Step::Ingest, Fault::TableLimit)
        }
        Error::Source(_) => (Step::Ingest, Fault::Internal),
        Error::TypedSource(_) => (Step::Ingest, Fault::SourceBoundary),
        Error::ParquetSource(crate::parquet_source::Error::ResourceLimit { .. }) => {
            (Step::Ingest, Fault::ParquetLimit)
        }
        Error::ParquetSource(crate::parquet_source::Error::ReaderUnavailable) => {
            (Step::Ingest, Fault::ParquetReaderUnavailable)
        }
        Error::ParquetSource(crate::parquet_source::Error::Unavailable { .. }) => {
            (Step::Ingest, Fault::ParquetCodecUnavailable)
        }
        Error::ParquetSource(_) => (Step::Ingest, Fault::ParquetBoundary),
        Error::SasSource(crate::sas_source::Error::Limit { .. }) => (Step::Ingest, Fault::SasLimit),
        Error::SasSource(crate::sas_source::Error::Unavailable) => {
            (Step::Ingest, Fault::SasReaderUnavailable)
        }
        Error::SasSource(_) => (Step::Ingest, Fault::SasBoundary),
        Error::DatasetJsonSource(crate::dataset_json_source::Error::Limit { .. }) => {
            (Step::Ingest, Fault::SourceLimit)
        }
        Error::DatasetJsonSource(crate::dataset_json_source::Error::Table(
            crate::column_table::TableError::Limit { .. },
        )) => (Step::Ingest, Fault::TableLimit),
        Error::DatasetJsonSource(_) => (Step::Ingest, Fault::SourceBoundary),
        Error::Ordinal(_) => (Step::Ingest, Fault::SourceOrdinalBoundary),
        Error::Bind(BindError::Catalog(_)) => (Step::Bind, Fault::ReferenceCatalog),
        Error::Bind(BindError::DependencyPolicy(_)) => (Step::Bind, Fault::DependencyPolicy),
        Error::Bind(BindError::SourceCount) => (Step::Bind, Fault::SourceCount),
        Error::Bind(BindError::InvalidDiagnostic) => (Step::Bind, Fault::DiagnosticContext),
        Error::Bind(_) => (Step::Bind, Fault::Internal),
        Error::Execution(error) if error.is_internal() => (Step::Execute, Fault::Internal),
        Error::Execution(_) => (Step::Execute, Fault::ExecutionBoundary),
        Error::ResourceLimit(_) => (Step::Ingest, Fault::SourceLimit),
        Error::Sources(_) => (Step::Ingest, Fault::SourceCollectionBoundary),
        Error::OdmFields(_) | Error::OdmValues(_) => (Step::Ingest, Fault::OdmProjectionBoundary),
        Error::OdmSource(crate::odm_xml::Error::Unavailable) => {
            (Step::Ingest, Fault::OdmReaderUnavailable)
        }
        // A refusal of the document is an issue above; an input's held bytes
        // and default options leave only a table the engine cannot hold.
        Error::OdmSource(_) => (Step::Ingest, Fault::Internal),
        Error::ProducerContract(_) => (Step::Ingest, Fault::ProducerContract),
        Error::ProducerMetadata(_) => (Step::Ingest, Fault::ProducerMetadata),
    };
    self::fault(step, code)
}

/// The same compiler failure facts are used by plain and environment routes.
pub fn preparation(error: &PrepareError) -> Rejection {
    if let PrepareError::Limit(limit) = error {
        return Rejection::Fault(issue::resource_rejected(
            Step::Prepare,
            Fault::CompilerLimit,
            limit.resource,
            limit.limit,
            limit.actual,
            limit.spec_paths.clone(),
        ));
    }
    if let PrepareError::Numeric {
        path,
        error:
            yamaa_core::expression::CompileError::Limit {
                resource: yamaa_core::expression::Resource::Depth,
                limit,
                ..
            },
    } = error
    {
        return Rejection::Fault(issue::resource_rejected(
            Step::Prepare,
            Fault::NumericPolicy,
            "semantic_depth",
            *limit,
            None,
            vec![path.clone()],
        ));
    }
    fault(
        Step::Prepare,
        match error {
            PrepareError::Numeric { .. } => Fault::NumericPolicy,
            PrepareError::OutputSchema(_) => Fault::OutputSchema,
            _ => Fault::Internal,
        },
    )
}

pub(super) fn decoder_limit(limit: &yamaa_core::limits::LimitExceeded, step: Step) -> Rejection {
    bound(
        step,
        "yaml_decode",
        limit.resource,
        limit.limit,
        limit.actual,
    )
}

/// Preparation exposes semantic context, never an arbitrary dump of source bytes.
/// Findings resolve against the retained schema/pass input or the shared YAML decoder.
pub fn capture_failure(error: &crate::specification_source::Error) -> Rejection {
    use crate::specification_source::Error;
    use crate::yaml_decode::DecodeFailure;
    match error {
        Error::Limit(limit) => {
            return bound(
                Step::Capture,
                "capture",
                limit.resource,
                limit.limit,
                limit.actual,
            )
        }
        Error::Decode {
            error: DecodeFailure::Limit(limit),
            ..
        } => return decoder_limit(limit, Step::Capture),
        // A dependency refusal keeps the preparation stage it reports.
        Error::Dependencies(error) => {
            if let Ok(rejection) =
                crate::schema_outcomes::inheritance_dependencies(error.clone(), Step::Prepare)
            {
                return rejection;
            }
        }
        Error::Normalize(error) => {
            if let Ok(rejection @ Rejection::Limit(_)) =
                crate::schema_outcomes::normalization(error.clone(), Step::Capture)
            {
                return rejection;
            }
        }
        Error::Validation(error) => {
            if let Ok(rejection) = crate::schema_outcomes::validation(*error, Step::Capture) {
                return rejection;
            }
        }
        Error::Decode { identity, error } => {
            if let Some(diagnostics) = crate::yaml_diagnostics::capture_diagnostics(error, identity)
            {
                return Rejection::Invalid(diagnostics);
            }
        }
        Error::Findings(captured) => {
            return match captured_schema::findings(captured) {
                Ok(diagnostics) => Rejection::Invalid(diagnostics),
                Err(captured_schema::Error::InvalidContext) => {
                    fault(Step::Capture, Fault::DiagnosticContext)
                }
            };
        }
        _ => {}
    }
    fault(
        Step::Capture,
        match error {
            Error::Limit(_) | Error::Findings(_) => unreachable!("handled capture refusal"),
            Error::Decode {
                error: DecodeFailure::Unavailable,
                ..
            } => Fault::YamlReaderUnavailable,
            Error::Decode { .. } => Fault::YamlDecode,
            Error::Bundle(_) => Fault::SchemaBundle,
            Error::Normalize(_) => Fault::Normalization,
            Error::Dependencies(_) => Fault::Dependencies,
            Error::Validation(_) => Fault::ValidationPolicy,
            // Only the reader of the field that names the file can place it.
            Error::Reference(_) => Fault::DiagnosticContext,
        },
    )
}

pub fn type_name(kind: yamaa_core::value::ColumnType) -> &'static str {
    use yamaa_core::value::ColumnType::*;
    match kind {
        Str => "str",
        Int => "int",
        Float => "float",
        Date => "date",
        DateTime => "datetime",
    }
}

mod inherited;
pub use inherited::failure_ref as inheritance_failure_ref;

mod captured_schema;

pub use captured_schema::Error as CapturedProjectionError;
/// Resolve findings against the exact held schema and input arena.
pub fn captured_findings(
    captured: &crate::specification_source::CapturedFindings,
    source_context: &[(&str, &str)],
) -> Result<Vec<Diagnostic>, CapturedProjectionError> {
    captured_schema::schema_findings(
        captured.schema().structure(),
        captured.document(),
        captured.findings(),
        source_context,
    )
}

#[cfg(test)]
mod resource_refusal_tests {
    use super::*;

    fn limit_context(
        step: &str,
        phase: &str,
        resource: &str,
        limit: usize,
        actual: Option<usize>,
    ) -> yamaa_core::diagnostic::Context {
        let mut context: yamaa_core::diagnostic::Context = [
            ("step".into(), Value::String(step.into())),
            ("code".into(), Value::String("resource_limit".into())),
            ("phase".into(), Value::String(phase.into())),
            ("resource".into(), Value::String(resource.into())),
            ("limit".into(), Value::from(limit)),
        ]
        .into();
        if let Some(actual) = actual {
            context.insert("actual".into(), Value::from(actual));
        }
        context
    }

    #[test]
    fn capture_preserves_fixed_decoder_and_normalization_refusals() {
        use crate::specification_source::Error;
        use yamaa_core::{
            limits::Limits,
            schema::{NormalizationError, NormalizationResource},
        };
        for (error, phase, resource, bound) in [
            (
                Error::Limit(yamaa_core::limits::LimitExceeded {
                    resource: "spec_bytes",
                    limit: Limits::SPEC_BYTES,
                    actual: Some(Limits::SPEC_BYTES + 1),
                }),
                "capture",
                "spec_bytes",
                Limits::SPEC_BYTES,
            ),
            (
                Error::Decode {
                    identity: "spec.yaml".into(),
                    error: crate::yaml_decode::DecodeFailure::Limit(
                        yamaa_core::limits::LimitExceeded {
                            resource: "spec_bytes",
                            limit: Limits::SPEC_BYTES,
                            actual: Some(Limits::SPEC_BYTES + 1),
                        },
                    ),
                },
                "yaml_decode",
                "spec_bytes",
                Limits::SPEC_BYTES,
            ),
            (
                Error::Normalize(NormalizationError::Limit {
                    resource: NormalizationResource::Depth,
                    limit: Limits::SEMANTIC_DEPTH,
                }),
                "normalization",
                "semantic_depth",
                Limits::SEMANTIC_DEPTH,
            ),
        ] {
            let Rejection::Limit(finding) = capture_failure(&error) else {
                panic!("a capture bound is a limit refusal");
            };
            assert_eq!(
                finding.code,
                yamaa_core::diagnostic::ConditionCode::ApplicationRejected
            );
            assert!(finding.spec_paths.is_empty());
            let actual = (resource == "spec_bytes").then_some(Limits::SPEC_BYTES + 1);
            assert_eq!(
                finding.context,
                limit_context("capture", phase, resource, bound, actual)
            );
        }
    }

    #[test]
    fn numeric_preparation_keeps_fixed_semantic_depth_and_authored_path() {
        let expression = format!("{}1{}", "(".repeat(129), ")".repeat(129));
        let error = PrepareError::Numeric {
            path: "columns.VALUE.derivation.compute.expr".into(),
            error: yamaa_core::expression::parse(&expression).unwrap_err(),
        };
        assert_eq!(
            preparation(&error),
            Rejection::Fault(issue::resource_rejected(
                Step::Prepare,
                Fault::NumericPolicy,
                "semantic_depth",
                128,
                None,
                vec!["columns.VALUE.derivation.compute.expr".into()],
            ))
        );
    }
}
