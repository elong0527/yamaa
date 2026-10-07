//! Portable compiler/ingestion diagnostics. This layer only represents findings;
//! it does not select a phase, bind a name, evaluate a formula or read a table.
use crate::{
    csv_source::{self, TextTableError},
    specification_run::Error,
};
use serde_json::{json, Value};
use yamaa_core::{
    column_dependencies,
    numeric_compiler::CompileError,
    numeric_parser::{GrammarFailure, ParseError},
    reference_binding,
};
use yamaa_engine::specification::{
    BindError, BindFinding, PreflightFinding, PrepareError, SourceDeclaration,
};

fn diagnostic(
    phase: &str,
    condition: &str,
    requirement: Option<&str>,
    paths: Vec<String>,
    context: Value,
) -> Value {
    json!({"phase":phase,"condition":condition,"requirement":requirement,"spec_paths":paths,"context":context})
}
fn validation(condition: &str, requirement: Option<&str>, path: String, context: Value) -> Value {
    diagnostic("validation", condition, requirement, vec![path], context)
}
fn numeric(path: &str, expression: &str, error: &CompileError) -> Option<Value> {
    let CompileError::Parse(ParseError::Grammar { failure, .. }) = error else {
        return None;
    };
    let mut context = json!({"expr":expression});
    match failure {
        GrammarFailure::InvalidExpression => {}
        GrammarFailure::ProhibitedConstruct { construct } => {
            context["construct"] = json!(construct)
        }
        GrammarFailure::ProhibitedFunction {
            name,
            argument_count,
        } => {
            context["function"] = json!(&expression[name.start..name.end]);
            if let Some(count) = argument_count {
                context["argument_count"] = json!(count);
            }
        }
    }
    Some(validation(
        failure.condition(),
        Some(failure.requirement()),
        path.into(),
        context,
    ))
}
fn binding(error: &BindError, source: &SourceDeclaration) -> Option<Vec<Value>> {
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
    Some(match error {
        BindFinding::Window(finding) => {
            use yamaa_core::specification::WindowFinding;
            let (path,context) = match finding {
                WindowFinding::ZeroOffset {path} => (path,json!({"offset":0})),
                WindowFinding::Order {path,operation,..} => (path,json!({"operation":operation})),
            };
            let definition = finding.definition();
            vec![diagnostic(definition.phase,definition.condition,Some(definition.requirement),vec![path.clone()],context)]
        },
        BindFinding::Lookup(finding) => {
            let context = finding.context.iter().map(|(name, value)| {
                let value = match value {
                    yamaa_core::value::Value::Str(value) => json!(value),
                    yamaa_core::value::Value::Int(value) => json!(value),
                    _ => return None,
                };
                Some((name.clone(), value))
            }).collect::<Option<serde_json::Map<_, _>>>()?;
            let definition = finding.definition;
            vec![diagnostic(definition.phase, definition.condition, Some(definition.requirement), vec![finding.path.clone()], context.into())]
        },
        BindFinding::QualifiedReference {path,name,row,finding} => {
            use yamaa_core::reference_scope::Finding as F;
            let (condition,requirement,context)=match finding {
                F::RowGroup => ("ungrouped_driver_field",Some("REQ-0067"),json!({"identifier":name,"row":row,"dataset":source.name})),
                F::ColumnGroup => ("ungrouped_driver_field",Some("REQ-0107"),json!({"identifier":name,"dataset":source.name})),
                _ => return None,
            };
            vec![validation(condition,requirement,path.clone(),context)]
        },
        BindFinding::Aggregate{path,expression,error}=> {
            use yamaa_core::aggregate_parser::GrammarFailure as A;
            let mut context=json!({"expr":expression});
            match error {
                A::InvalidExpression => {},
                A::ProhibitedConstruct {construct} => context["construct"]=json!(construct),
                A::ProhibitedFunction {name,argument_count} => {
                    context["function"]=json!(&expression[name.start..name.end]);
                    if let Some(count)=argument_count {context["argument_count"]=json!(count);}
                },
                A::NestedReduction {outer,inner} => {
                    context["outer"]=json!(outer.name());context["inner"]=json!(inner.name());
                },
            }
            vec![validation(error.condition(),Some(error.requirement()),path.clone(),context)]
        },
        BindFinding::AggregateScope{path,expression,relation}=> {
            let reason = match relation {
                None => "a grouped row aggregate reads its row driver".into(),
                Some(relation) => {
                    let mut budget = yamaa_core::schema::ValidationBudget::new(Default::default());
                    let driver = yamaa_core::schema::quoted_diagnostic_text(&source.name, &mut budget).ok()?;
                    let relation = yamaa_core::schema::quoted_diagnostic_text(relation, &mut budget).ok()?;
                    format!("a grouped row aggregate reads {driver}, not {relation}")
                },
            };
            vec![validation("invalid_aggregate_context",Some("REQ-0329"),path.clone(),json!({"expr":expression,"reason":reason}))]
        },
        BindFinding::Numeric{path,expression,error}=>vec![numeric(path,expression,error)?],
        BindFinding::QualifiedNumericReference{path,expression,identifier}=>vec![validation("qualified_identifier",Some("REQ-0442"),path.clone(),json!({"expr":expression,"identifier":identifier}))],
        BindFinding::UnknownReference{path,name}=>vec![validation("unknown_field",Some("REQ-0103"),path.clone(),json!({"identifier":name}))],
        BindFinding::OutputReference{path,name,finding}=>{
            let (condition,context)=match finding {
                reference_binding::Diagnostic::UnknownField=>("unknown_field",json!({"identifier":name})),
                reference_binding::Diagnostic::UnresolvableName{dataset:0}=>("unresolvable_name",json!({"identifier":name,"suggestion":format!("{}.{name}",source.name)})),
                _=>return None,
            };vec![validation(condition,None,path.clone(),context)]
        },
        BindFinding::Dependencies{columns,paths,diagnostics}=>diagnostics.iter().map(|finding|{
            let (site,context)=match finding {
                column_dependencies::Diagnostic::Cycle{columns:members}=>{
                    let site=members[..members.len()-1].iter().map(|&id|paths[id].clone()).collect();
                    (site,json!({"cycle":members.iter().map(|&id|&columns[id]).collect::<Vec<_>>()}))
                },
                column_dependencies::Diagnostic::ForwardReference{column,dependency}=>(vec![paths[*column].clone()],json!({"column":columns[*column],"dependency":columns[*dependency]})),
                column_dependencies::Diagnostic::MissingKeyDerivation{column}=>(vec![format!("columns.{}.derivation",columns[*column])],json!({"column":columns[*column]})),
                column_dependencies::Diagnostic::KeyDependency{column,dependency}=>(vec![paths[*column].rsplit_once('.').expect("compiler operation path").0.into()],json!({"column":columns[*column],"dependency":columns[*dependency]})),
            };diagnostic("validation",finding.condition(),Some(finding.requirement()),site,context)
        }).collect(),
    })
}
fn preparing(error: &PreflightFinding) -> Option<Value> {
    Some(match error {
        PreflightFinding::UndeclaredRowColumn { index, column } => validation(
            "undeclared_column",
            None,
            format!("rows[{index}].derivations.{column}"),
            json!({"column":column}),
        ),
        PreflightFinding::DuplicateRowDefault { column, rows } => validation(
            "duplicate_derivation",
            Some("REQ-1260"),
            format!("columns.{column}.derivation"),
            json!({"column":column,"rows":rows}),
        ),
        PreflightFinding::MissingRowDerivation { column, rows } => validation(
            "missing_derivation",
            Some("REQ-0200"),
            format!("columns.{column}.derivation"),
            json!({"column":column,"rows":rows}),
        ),
        PreflightFinding::ConflictingRowConstruction => diagnostic(
            "validation",
            "conflicting_row_construction",
            Some("REQ-1171"),
            vec!["filter".into(), "rows".into()],
            json!({}),
        ),
        PreflightFinding::InvalidGroup { index, row, groups } => validation(
            "invalid_field_type",
            Some("REQ-0065"),
            format!("rows[{index}].group_by"),
            json!({"row":row,"group_by":groups}),
        ),
        PreflightFinding::GroupReference {
            index,
            row,
            name,
            dataset,
        } => validation(
            "unknown_field",
            Some("REQ-0066"),
            format!("rows[{index}].group_by"),
            json!({"row":row,"identifier":name,"dataset":dataset}),
        ),
        PreflightFinding::RowDriverUnavailable {
            index,
            row,
            dataset,
        } => validation(
            "driver_unavailable",
            None,
            format!("rows[{index}].dataset"),
            json!({"row":row,"dataset":dataset}),
        ),
        PreflightFinding::MissingDerivation { column } => validation(
            "missing_derivation",
            Some("REQ-0198"),
            format!("columns.{column}.derivation"),
            json!({"column":column}),
        ),
        PreflightFinding::UndeclaredKey { position, column } => validation(
            "undeclared_column",
            Some("REQ-0220"),
            format!("keys[{position}]"),
            json!({"column":column}),
        ),
        PreflightFinding::DriverUnavailable { dataset } => validation(
            "driver_unavailable",
            None,
            "base".into(),
            match dataset {
                Some(name) => json!({"dataset":name}),
                None => json!({"row":null}),
            },
        ),
        PreflightFinding::DomainInputCollision { domain } => diagnostic(
            "validation",
            "duplicate_identifier",
            Some("REQ-0080"),
            vec![format!("input.{domain}"), "domain".into()],
            json!({"identifier":domain}),
        ),
        PreflightFinding::RedundantSourceType {
            dataset,
            field,
            kind,
        } => validation(
            "redundant_field_type",
            Some("REQ-0533"),
            format!("input.{dataset}.types.{field}"),
            json!({"dataset":dataset,"field":field,"type":type_name(*kind)}),
        ),
    })
}
/// Return semantic findings only. Policy/transport/internal errors deliberately
/// have no fabricated language condition and must remain host boundary failures.
pub fn findings(error: &Error, source: Option<&SourceDeclaration>) -> Option<Vec<Value>> {
    match error {
        Error::Sources(errors) => {
            let mut result = Vec::new();
            for (source, error) in errors {
                result.extend(findings(error, Some(source))?);
            }
            Some(result)
        }
        Error::Prepare(PrepareError::Invalid(errors)) => errors.iter().map(preparing).collect(),
        Error::Bind(error) => binding(error, source?),
        Error::ParquetSource(error) => {
            use crate::parquet_source::Error as P;
            let source = source?;
            let (condition, requirement, extra) = match error {
                P::Malformed => ("source_parquet_invalid", "REQ-1038", json!({})),
                P::EmptyName { field } => (
                    "source_field_name_empty",
                    "REQ-1039",
                    json!({"field":field}),
                ),
                P::DuplicateName { field } => (
                    "source_field_name_duplicate",
                    "REQ-1039",
                    json!({"field":field}),
                ),
                P::Unsupported { field, stored_type } => (
                    "source_field_type_unsupported",
                    "REQ-1040",
                    json!({"field":field,"stored_type":stored_type}),
                ),
                P::Value { field, row, value } => (
                    "source_field_value_invalid",
                    "REQ-1041",
                    json!({"field":field,"row":row,"value":value}),
                ),
                _ => return None,
            };
            let mut context = json!({"dataset":source.name,"path":source.path});
            context.as_object_mut()?.extend(extra.as_object()?.clone());
            Some(vec![diagnostic(
                "ingest",
                condition,
                Some(requirement),
                vec![format!("input.{}.path", source.name)],
                context,
            )])
        }
        Error::TypedSource(crate::typed_csv::Error::UnknownField { field }) => {
            let source = source?;
            Some(vec![validation(
                "unknown_field",
                Some("REQ-0532"),
                format!("input.{}.types.{field}", source.name),
                json!({"dataset":source.name,"field":field}),
            )])
        }
        Error::TypedSource(crate::typed_csv::Error::FieldParse {
            field,
            target,
            value,
        }) => {
            let source = source?;
            Some(vec![diagnostic(
                "ingest",
                "field_parse_failed",
                Some("REQ-0536"),
                vec![format!("input.{}.types.{field}", source.name)],
                json!({"dataset":source.name,"field":field,"type":type_name(*target),"value":value}),
            )])
        }

        Error::Source(TextTableError::Csv(
            error @ csv_source::Error::Profile {
                condition,
                record,
                field,
            },
        )) => {
            let source = source?;
            let field = match field {
                csv_source::Field::Index(index) => json!(index),
                csv_source::Field::Name(name) => json!(name),
            };
            Some(vec![diagnostic(
                "ingest",
                condition,
                error.requirement(),
                vec![format!("input.{}.path", source.name)],
                json!({"dataset":source.name,"path":source.path,"record":record,"field":field}),
            )])
        }
        _ => None,
    }
}

/// Experimental host boundary reply. Semantic findings stay separate from
/// unsupported vocabulary and resource/internal rejection; no Debug text leaks.
/// This envelope is not the final whole-run conformance report.
pub fn failure(error: &Error, source: Option<&SourceDeclaration>) -> String {
    let outcome = if let Some(diagnostics) = findings(error, source) {
        json!({"status":"invalid","diagnostics":diagnostics})
    } else if let Error::Prepare(PrepareError::Unsupported(features)) = error {
        json!({"status":"unsupported","features":features.iter().map(|f| json!({"operation":f.operation,"spec_path":f.path})).collect::<Vec<_>>()})
    } else {
        let (stage, code) = match error {
            Error::Prepare(PrepareError::Limit(_)) => ("prepare", "compiler_limit"),
            Error::Prepare(PrepareError::Numeric { .. }) => ("prepare", "numeric_policy"),
            Error::Prepare(PrepareError::OutputSchema(_)) => ("prepare", "output_schema"),
            Error::Prepare(_) => ("prepare", "internal"),
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
            Error::Bind(BindError::Catalog(_)) => ("bind", "reference_catalog"),
            Error::Bind(BindError::DependencyPolicy(_)) => ("bind", "dependency_policy"),
            Error::Bind(BindError::SourceCount) => ("bind", "source_count"),
            Error::Bind(_) => ("bind", "internal"),
            Error::Execution(error) if error.is_internal() => ("execute", "internal"),
            Error::Execution(_) => ("execute", "execution_boundary"),
            Error::Sources(_) => ("ingest", "source_collection_boundary"),
        };
        json!({"status":"rejected","stage":stage,"code":code})
    };
    json!({"protocol":"specification/prototype","outcome":outcome}).to_string()
}

fn capture_response(outcome: Value) -> String {
    crate::yaml_transport::encode_outcome("specification/prototype", outcome, 16_777_216)
        .unwrap_or_else(|_| json!({"protocol":"specification/prototype","outcome":{"status":"rejected","stage":"capture","code":"diagnostic_context_limit"}}).to_string())
}
/// Preparation exposes semantic context, never an arbitrary dump of source bytes.
/// Findings resolve against the retained schema/pass input or the shared YAML decoder.
pub fn capture_failure(error: &crate::specification_source::Error) -> String {
    use crate::specification_source::Error;
    if let Error::Decode { identity, error } = error {
        if let Some(mut diagnostics) = crate::yaml_transport::semantic_diagnostics(error) {
            for finding in &mut diagnostics {
                finding["phase"] = json!("validation");
                finding["requirement"] = Value::Null;
                if finding["condition"] == "non_ascii_source" {
                    finding["context"]["path"] = json!(identity);
                }
            }
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

#[path = "specification_inheritance_diagnostics.rs"]
mod inherited;
pub use inherited::failure as inheritance_failure;

#[path = "specification_schema_diagnostics.rs"]
mod captured_schema;
