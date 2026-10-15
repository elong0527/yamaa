//! Portable compiler/ingestion diagnostics. This layer only represents findings;
//! it does not select a phase, bind a name, evaluate a formula or read a table.
use crate::{
    csv_source::{self, TextTableError},
    specification_run::Error,
};
use serde_json::{json, Value};
use {
    yamaa_core::specification::BindError, yamaa_core::specification::BindFinding,
    yamaa_core::specification::PreflightFinding, yamaa_core::specification::PrepareError,
    yamaa_core::specification::SourceDeclaration,
};

fn diagnostic(
    phase: &str,
    condition: &str,
    requirement: Option<&str>,
    paths: Vec<String>,
    context: Value,
) -> Value {
    crate::issue_rows::report(phase, condition, requirement, paths, context)
}
pub(crate) fn binding(error: &BindError, source: &SourceDeclaration) -> Option<Vec<Value>> {
    let BindError::Invalid(findings) = error else {
        return None;
    };
    let mut diagnostics = Vec::new();
    for finding in findings {
        diagnostics.extend(binding_finding(finding, source)?);
    }
    Some(diagnostics)
}
fn binding_finding(error: &BindFinding, source: &SourceDeclaration) -> Option<Vec<Value>> {
    error
        .diagnostics(source)?
        .into_iter()
        .map(portable_diagnostic)
        .collect()
}
/// Represent the core-owned finding without assigning semantic vocabulary here.
fn preparing(error: &PreflightFinding) -> Option<Value> {
    portable_diagnostic(error.diagnostic())
}

/// The original-document report shape retains ordinary JSON context values.
/// Geometry is retained by the core diagnostic and projected by the report contract.
pub(crate) fn portable_diagnostic(finding: yamaa_core::diagnostic::Diagnostic) -> Option<Value> {
    portable_definition(finding.definition(), finding.spec_paths, finding.context)
}
/// Submission findings retain their core-owned definition until their causes
/// join the closed cause registry; serialization assigns no semantic vocabulary.
pub(crate) fn portable_definition(
    definition: yamaa_core::diagnostic::Definition,
    paths: Vec<String>,
    context: yamaa_core::diagnostic::Context,
) -> Option<Value> {
    fn value(cell: yamaa_core::diagnostic::ContextValue) -> Option<Value> {
        use yamaa_core::{diagnostic::ContextValue as C, value::Value as S};
        Some(match cell {
            C::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, cell)| Some((key, value(cell)?)))
                    .collect::<Option<_>>()?,
            ),
            C::Sequence(values) => {
                Value::Array(values.into_iter().map(value).collect::<Option<_>>()?)
            }
            C::Integer(integer) => Value::Number(integer.parse().ok()?),
            C::Scalar(scalar) => match scalar {
                S::Missing => Value::Null,
                S::Str(v) => json!(v),
                S::Int(v) => json!(v),
                S::Float(v) => json!(v.get()),
                S::Bool(v) => json!(v),
                S::Date(v) => json!(v.to_string()),
                S::DateTime(v) => json!(v.to_string()),
            },
        })
    }
    let context = context
        .into_iter()
        .map(|(name, v)| Some((name, value(v)?)))
        .collect::<Option<serde_json::Map<_, _>>>()?;
    Some(diagnostic(
        definition.phase,
        definition.condition,
        definition.requirement,
        paths,
        context.into(),
    ))
}
/// Complete ingestion reports are admitted in core before copying codec facts.
/// Other existing preparation/binding projections retain their admission policy.
pub fn findings_with_limit(
    error: &Error,
    source: Option<&SourceDeclaration>,
    maximum: usize,
) -> Result<Option<Vec<Value>>, crate::source_diagnostics::SourceProjectionError> {
    match error {
        Error::Sources(_)
        | Error::ParquetSource(_)
        | Error::SasSource(_)
        | Error::TypedSource(_)
        | Error::Source(_)
        | Error::Ordinal(_)
        | Error::OdmFields(_)
        | Error::OdmValues(_) => {
            crate::source_diagnostics::findings_with_limit(error, source, maximum)?
                .map(|findings| {
                    findings
                        .into_iter()
                        .map(|finding| {
                            portable_diagnostic(finding).ok_or_else(|| {
                                yamaa_core::diagnostic::ProjectionError::InvalidContext.into()
                            })
                        })
                        .collect()
                })
                .transpose()
        }
        Error::Prepare(PrepareError::Invalid(errors)) => Ok(errors.iter().map(preparing).collect()),
        Error::Bind(error) => Ok(source.and_then(|source| binding(error, source))),
        _ => Ok(None),
    }
}

/// Experimental host boundary reply. Semantic findings stay separate from
/// unsupported vocabulary and resource/internal rejection; no Debug text leaks.
/// This envelope is not the final whole-run conformance report.
pub fn failure(error: &Error, source: Option<&SourceDeclaration>) -> String {
    let projected =
        findings_with_limit(error, source, yamaa_core::diagnostic::MAX_PROJECTION_BYTES);
    let outcome = if let Err(error) = projected {
        let code = match error {
            crate::source_diagnostics::SourceProjectionError::TraversalLimit => {
                "diagnostic_traversal_limit"
            }
            crate::source_diagnostics::SourceProjectionError::Context(
                yamaa_core::diagnostic::ProjectionError::Limit,
            ) => "diagnostic_context_limit",
            crate::source_diagnostics::SourceProjectionError::Context(
                yamaa_core::diagnostic::ProjectionError::InvalidContext,
            ) => "diagnostic_context",
        };
        json!({"status":"rejected","stage":"ingest","code":code})
    } else if let Ok(Some(diagnostics)) = projected {
        json!({"status":"invalid","diagnostics":diagnostics})
    } else if let Some(refusal) = match error {
        Error::ResourceLimit(refusal) => Some(refusal.clone()),
        _ => source.and_then(|source| crate::resource_diagnostics::source(error, source)),
    } {
        let code = match error {
            Error::Source(TextTableError::Csv(_)) => "csv_limit",
            Error::Source(TextTableError::Table(_)) => "table_limit",
            Error::ParquetSource(_) => "parquet_limit",
            Error::SasSource(_) => "sas_limit",
            _ => "source_limit",
        };
        let mut outcome = json!({"status":"rejected","stage":"ingest","code":code,"resource":refusal.resource,"limit":refusal.limit,"spec_paths":refusal.spec_paths});
        if let Some(actual) = refusal.actual {
            outcome["actual"] = json!(actual);
        }
        outcome
    } else if let Error::Prepare(PrepareError::Unsupported(features)) = error {
        json!({"status":"unsupported","features":features.iter().map(|f| json!({"operation":f.operation,"spec_path":f.path})).collect::<Vec<_>>()})
    } else if let Error::Prepare(error) = error {
        preparation_outcome(error)
    } else {
        let (stage, code) = match error {
            Error::Prepare(_) => unreachable!("preparation errors handled above"),
            Error::Source(TextTableError::Csv(csv_source::Error::Limit { .. })) => {
                ("ingest", "csv_limit")
            }
            Error::Source(TextTableError::Table(crate::arrow_table::TableError::Limit {
                ..
            })) => ("ingest", "table_limit"),
            Error::Source(_) => ("ingest", "internal"),
            Error::TypedSource(_) => ("ingest", "source_boundary"),
            Error::ParquetSource(crate::parquet_source::Error::Limit) => {
                ("ingest", "parquet_limit")
            }
            Error::ParquetSource(crate::parquet_source::Error::Unavailable { .. }) => {
                ("ingest", "parquet_codec_unavailable")
            }
            Error::ParquetSource(_) => ("ingest", "parquet_boundary"),
            Error::SasSource(crate::sas_source::Error::Limit { .. }) => ("ingest", "sas_limit"),
            Error::SasSource(_) => ("ingest", "sas_boundary"),
            Error::Ordinal(_) => ("ingest", "source_ordinal_boundary"),
            Error::Bind(BindError::Catalog(_)) => ("bind", "reference_catalog"),
            Error::Bind(BindError::DependencyPolicy(_)) => ("bind", "dependency_policy"),
            Error::Bind(BindError::SourceCount) => ("bind", "source_count"),
            Error::Bind(BindError::PredicateDiagnosticLimit) => {
                ("bind", "predicate_diagnostic_limit")
            }
            Error::Bind(_) => ("bind", "internal"),
            Error::Execution(error) if error.is_internal() => ("execute", "internal"),
            Error::Execution(_) => ("execute", "execution_boundary"),
            Error::ResourceLimit(_) => ("ingest", "source_limit"),
            Error::Sources(_) => ("ingest", "source_collection_boundary"),
            Error::OdmFields(_) | Error::OdmValues(_) => ("ingest", "odm_projection_boundary"),
        };
        json!({"status":"rejected","stage":stage,"code":code})
    };
    json!({"protocol":"specification/prototype","outcome":outcome}).to_string()
}

/// The same compiler failure facts are used by plain and environment routes.
pub(crate) fn preparation_outcome(error: &PrepareError) -> Value {
    if let PrepareError::Limit(limit) = error {
        let mut outcome = json!({"status":"rejected","stage":"prepare","code":"compiler_limit",
            "resource":limit.resource,"limit":limit.limit,"spec_paths":limit.spec_paths});
        if let Some(actual) = limit.actual {
            outcome["actual"] = json!(actual);
        }
        return outcome;
    }
    if let PrepareError::Numeric { path, error } = error {
        use yamaa_core::{
            numeric_compiler::CompileError,
            numeric_parser::{ParseError, ParseResource},
        };
        let bound = match error {
            CompileError::ResolutionLimit { limit, required } => {
                Some(("resolutions", *limit, Some(*required)))
            }
            CompileError::Parse(ParseError::Limit {
                resource, limit, ..
            }) => {
                let resource = match resource {
                    ParseResource::Bytes => "bytes",
                    ParseResource::Tokens => "tokens",
                    ParseResource::Nodes => "nodes",
                    ParseResource::Depth => "depth",
                };
                Some((resource, *limit, None))
            }
            _ => None,
        };
        if let Some((resource, limit, actual)) = bound {
            let mut outcome = json!({"status":"rejected","stage":"prepare","code":"numeric_policy",
                "resource":resource,"limit":limit,"spec_paths":[path]});
            if let Some(actual) = actual {
                outcome["actual"] = json!(actual);
            }
            return outcome;
        }
    }
    let code = match error {
        PrepareError::Numeric { .. } => "numeric_policy",
        PrepareError::OutputSchema(_) => "output_schema",
        _ => "internal",
    };
    json!({"status":"rejected","stage":"prepare","code":code})
}
pub(crate) fn compiler_failure(error: &PrepareError) -> crate::file_application::Failure {
    crate::file_application::preparation_failure(
        json!({"protocol":"specification/prototype","outcome":preparation_outcome(error)})
            .to_string(),
    )
}

fn capture_response(outcome: Value) -> String {
    crate::yaml_diagnostics::encode_outcome("specification/prototype", outcome, 16_777_216)
        .unwrap_or_else(|_| json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"capture","code":"diagnostic_context_limit"}}).to_string())
}
pub(super) fn decoder_limit(limit: &crate::yaml_decode::ResourceLimit, stage: &str) -> Value {
    let mut outcome = json!({"status":"resource_limit","stage":stage,"phase":"yaml_decode",
        "resource":limit.resource,"limit":limit.limit});
    if let Some(actual) = limit.actual {
        outcome["actual"] = json!(actual);
    }
    outcome
}
/// Preparation exposes semantic context, never an arbitrary dump of source bytes.
/// Findings resolve against the retained schema/pass input or the shared YAML decoder.
pub fn capture_failure(error: &crate::specification_source::Error) -> String {
    use crate::specification_source::Error;
    if let Error::Decode {
        error: crate::yaml_decode::DecodeFailure::Limit(limit),
        ..
    } = error
    {
        return capture_response(decoder_limit(limit, "capture"));
    }
    let resource = match error {
        Error::Normalize(error) => crate::schema_outcomes::normalization(error.clone()).ok(),
        Error::Validation(error) => crate::schema_outcomes::validation(*error).ok(),
        _ => None,
    };
    if let Some(mut outcome) = resource.filter(|outcome| outcome["status"] == "resource_limit") {
        outcome["stage"] = json!("capture");
        return capture_response(outcome);
    }
    if let Error::Decode { identity, error } = error {
        if let Some(diagnostics) = crate::yaml_diagnostics::capture_diagnostics(error, identity) {
            return capture_response(json!({"status":"invalid","diagnostics":diagnostics}));
        }
    }
    if let Error::Findings(captured) = error {
        return match captured_schema::findings(captured) {
            Ok(diagnostics) => {
                capture_response(json!({"status":"invalid","diagnostics":diagnostics}))
            }
            Err(error) => {
                let code = match error {
                    captured_schema::Error::Limit => "diagnostic_context_limit",
                    captured_schema::Error::InvalidContext => "diagnostic_context",
                };
                capture_response(json!({"status":"rejected","stage":"capture","code":code}))
            }
        };
    }
    let code = match error {
        Error::Limit(_) => "capture_limit",
        Error::Decode { .. } => "yaml_decode",
        Error::Bundle(_) => "schema_bundle",
        Error::Normalize(_) => "normalization",
        Error::Validation(_) => "validation_policy",
        Error::Findings(_) => unreachable!("handled captured findings"),
        Error::InheritanceRequired => "inheritance_required",
        Error::VersionedRootRequired => "versioned_root_required",
        Error::UnknownClass(_) => "unknown_class",
    };
    capture_response(json!({"status":"rejected","stage":"capture","code":code}))
}

pub(crate) fn type_name(kind: yamaa_core::value::ColumnType) -> &'static str {
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
pub use inherited::failure as inheritance_failure;
pub use inherited::failure_ref as inheritance_failure_ref;

mod captured_schema;

pub(crate) use captured_schema::{Budget as CapturedBudget, Error as CapturedProjectionError};
/// Reuse the same held schema/input resolver with an aggregate caller-owned budget.
pub(crate) fn captured_findings(
    captured: &crate::specification_source::CapturedFindings,
    source_context: &[(&str, &str)],
    budget: &mut CapturedBudget,
) -> Result<Vec<Value>, CapturedProjectionError> {
    captured_schema::schema_findings_with_budget(
        captured.schema().structure(),
        captured.document(),
        captured.findings(),
        source_context,
        budget,
    )
}

#[cfg(test)]
mod resource_refusal_tests {
    use super::*;
    #[test]
    fn capture_preserves_nondefault_decoder_and_normalization_bounds() {
        use crate::specification_source::Error;
        use yamaa_core::schema::{NormalizationError, NormalizationResource};
        for (error, phase, resource, bound) in [
            (
                Error::Decode {
                    identity: "spec.yaml".into(),
                    error: crate::yaml_decode::DecodeFailure::Limit(
                        crate::yaml_decode::ResourceLimit {
                            resource: "events",
                            limit: 7,
                            actual: Some(8),
                        },
                    ),
                },
                "yaml_decode",
                "events",
                7,
            ),
            (
                Error::Normalize(NormalizationError::Limit {
                    resource: NormalizationResource::Nodes,
                    limit: 3,
                }),
                "normalization",
                "nodes",
                3,
            ),
        ] {
            let value: Value = serde_json::from_str(&capture_failure(&error)).unwrap();
            let outcome = &value["outcome"];
            assert_eq!(outcome["status"], "resource_limit");
            assert_eq!(outcome["stage"], "capture");
            assert_eq!(outcome["phase"], phase);
            assert_eq!(outcome["resource"], resource);
            assert_eq!(outcome["limit"], bound);
        }
    }
}
