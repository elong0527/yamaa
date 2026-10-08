//! Verification admission and deferred declaration diagnostics. The valid prefix
//! executes only after derivation and output keys; unsupported vocabulary is
//! scanned across every declaration before any source request.
use super::*;
use crate::dataset::{Check, Verification};
use crate::{
    bound_expression::BoundPredicate,
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    predicate_compiler,
    value::Value,
};

#[derive(Clone, Debug)]
enum DeclarationFinding {
    Reason {
        path: String,
        condition: &'static str,
        requirement: &'static str,
        reason: String,
    },
    Diagnostic(Diagnostic),
}

fn predicate(
    d: &Document,
    payload: usize,
    field_name: &str,
    path: &str,
    output: &TableSchema,
) -> Result<Result<BoundPredicate, Diagnostic>, PrepareError> {
    let path = format!("{path}.{field_name}");
    let expression = d
        .field(payload, field_name)
        .and_then(|id| match &d.nodes()[id] {
            N::Text(text) => Some(text.as_str()),
            _ => None,
        });
    let Some(expression) = expression else {
        return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
            operation: "assert_predicate_text".into(),
            path,
        }]));
    };
    let plan = match predicate_compiler::compile(expression, &path, Default::default()) {
        Ok(plan) => plan,
        Err(predicate_compiler::Error::Parse(crate::predicate_parser::ParseError::Grammar {
            position,
            failure: crate::predicate_parser::GrammarFailure::InvalidExpression,
            message,
        })) => {
            return Ok(Err(Diagnostic {
                code: ConditionCode::PredicateInvalidExpression,
                spec_paths: vec![path],
                context: [(
                    "reason".into(),
                    ContextValue::Scalar(Value::Str(format!(
                        "{message} at character {}",
                        position.character + 1
                    ))),
                )]
                .into(),
                source_span: None,
                operand_route: None,
            }));
        }
        Err(predicate_compiler::Error::Parse(crate::predicate_parser::ParseError::Grammar {
            ..
        })) => {
            return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "assert_predicate_diagnostic".into(),
                path,
            }]));
        }
        Err(predicate_compiler::Error::UnsupportedLiteral { .. }) => {
            return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "predicate_literal".into(),
                path,
            }]));
        }
        Err(predicate_compiler::Error::Parse(
            crate::predicate_parser::ParseError::UnsupportedRegex { .. },
        )) => {
            return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "predicate_regex".into(),
                path,
            }]));
        }
        Err(predicate_compiler::Error::Internal) => return Err(PrepareError::Internal),
        Err(_) => return Err(PrepareError::Limit("predicate_compilation")),
    };
    if let Some(name) = plan
        .identifiers()
        .into_iter()
        .filter(|name| !output.columns().iter().any(|column| column.name == *name))
        .min()
    {
        return Ok(Err(Diagnostic {
            code: ConditionCode::VerificationUnknownReference,
            spec_paths: vec![path],
            context: [(
                "identifier".into(),
                ContextValue::Scalar(Value::Str(name.into())),
            )]
            .into(),
            source_span: None,
            operand_route: None,
        }));
    }
    let names: alloc::collections::BTreeSet<_> = plan.identifiers().into_iter().collect();
    let bindings = names
        .into_iter()
        .map(|name| Binding {
            name: name.into(),
            read: Read::Column(
                output
                    .columns()
                    .iter()
                    .position(|column| column.name == name)
                    .expect("known output"),
            ),
        })
        .collect();
    BoundPredicate::new(plan, bindings)
        .map(Ok)
        .map_err(|_| PrepareError::Internal)
}
#[derive(Debug, Default)]
pub(super) struct Verifications {
    pub checks: Vec<Verification>,
    pub identities: Vec<Option<String>>,
    deferred: Option<DeclarationFinding>,
}
fn operation(d: &Document, id: usize) -> Result<(&str, usize), PrepareError> {
    let &[(name, payload)] = mapping(d, id)? else {
        return Err(PrepareError::Internal);
    };
    Ok((text(d, name)?, payload))
}
impl Verifications {
    pub(super) fn prepare(d: &Document, output: &TableSchema) -> Result<Self, PrepareError> {
        let mut result = Self::default();
        let Some(id) = d
            .field(d.root(), "verifications")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        else {
            return Ok(result);
        };
        let entries = sequence(d, id)?;
        if entries.len() > 16 {
            return Err(PrepareError::Limit("verifications"));
        }
        let mut extra = Vec::new();
        let mut predicates = BTreeMap::new();
        for (index, &id) in entries.iter().enumerate() {
            let (op, payload) = operation(d, id)?;
            let path = format!("verifications[{index}].{op}");
            let allowed: &[&str] = match op {
                "unique" => &["columns", "id", "severity"],
                "row_count" => &["min", "max", "id", "severity"],
                "assert" => &["when", "require", "id", "severity"],
                _ => {
                    reject(&mut extra, op, path);
                    continue;
                }
            };
            if op == "unique" && matches!(d.nodes()[payload], N::Sequence(_)) {
                continue;
            }
            for &(name, _) in mapping(d, payload)? {
                let name = text(d, name)?;
                if !allowed.contains(&name) {
                    reject(&mut extra, name, format!("{path}.{name}"));
                }
            }
            if let Some(id) = d.field(payload, "severity") {
                if text(d, id)? != "error" {
                    reject(&mut extra, "verification_severity", path.clone());
                }
            }
            if op == "assert" {
                for name in ["when", "require"] {
                    if name == "require" || d.field(payload, name).is_some() {
                        predicates
                            .insert((index, name), predicate(d, payload, name, &path, output)?);
                    }
                }
            }
            // The current dataset engine's count bounds are signed 64-bit. Do
            // not lose arbitrary-width authored bounds through a narrowing cast.
            for name in if op == "row_count" {
                &["min", "max"][..]
            } else {
                &[]
            } {
                if let Some(id) = d.field(payload, name) {
                    if let N::Integer(value) = &d.nodes()[id] {
                        if value.parse::<i64>().is_err() {
                            reject(&mut extra, "verification_bound", format!("{path}.{name}"));
                        }
                    }
                }
            }
        }
        if !extra.is_empty() {
            return Err(PrepareError::Unsupported(extra));
        }
        let mut quote_budget = crate::schema::ValidationBudget::new(Default::default());
        let mut quote = |value: &str| {
            crate::schema::quoted_diagnostic_text(value, &mut quote_budget)
                .map_err(|_| PrepareError::Limit("verification_diagnostic"))
        };
        let mut ids = BTreeMap::<String, String>::new();
        for (index, &id) in entries.iter().enumerate() {
            let (op, payload) = operation(d, id)?;
            let path = format!("verifications[{index}].{op}");
            let invalid = |path: String, condition, requirement, reason: String| {
                Some(DeclarationFinding::Reason {
                    path,
                    condition,
                    requirement,
                    reason,
                })
            };
            let identity = if let Some(id) = d.field(payload, "id") {
                match &d.nodes()[id] {
                    N::Text(value) if !value.is_empty() => {
                        if let Some(previous) = ids.get(value) {
                            result.deferred = invalid(
                                path,
                                "duplicate_identifier",
                                "REQ-0398",
                                format!("verification id {} repeats {previous}", quote(value)?),
                            );
                            break;
                        }
                        ids.insert(value.clone(), path.clone());
                        Some(value.clone())
                    }
                    _ => {
                        result.deferred = invalid(
                            path,
                            "invalid_declaration",
                            "REQ-0374",
                            "a verification id is text".into(),
                        );
                        break;
                    }
                }
            } else {
                None
            };
            let check = if op == "unique" {
                let values = if matches!(d.nodes()[payload], N::Sequence(_)) {
                    sequence(d, payload)?
                } else {
                    sequence(d, field(d, payload, "columns")?)?
                };
                if values.is_empty() {
                    result.deferred = invalid(
                        path,
                        "invalid_declaration",
                        "REQ-0397",
                        "columns names at least one column".into(),
                    );
                    break;
                }
                let mut columns = Vec::new();
                for &id in values {
                    let name = text(d, id)?;
                    if let Some(column) = output.columns().iter().position(|c| c.name == name) {
                        columns.push(column);
                    } else {
                        result.deferred = invalid(
                            format!("{path}.columns"),
                            "unknown_field",
                            "REQ-0405",
                            format!("unknown column {}", quote(name)?),
                        );
                        break;
                    }
                }
                if result.deferred.is_some() {
                    break;
                }
                Check::Unique(columns)
            } else if op == "assert" {
                let mut when = None;
                let mut require = None;
                for name in ["when", "require"] {
                    if let Some(predicate) = predicates.remove(&(index, name)) {
                        match predicate {
                            Ok(predicate) if name == "when" => when = Some(predicate),
                            Ok(predicate) => require = Some(predicate),
                            Err(diagnostic) => {
                                if let Some(when) = when.take() {
                                    result.checks.push(Verification {
                                        path: when.plan().spec_path().into(),
                                        check: Check::PredicateDeclaration(when),
                                    });
                                    result.identities.push(None);
                                }
                                result.deferred = Some(DeclarationFinding::Diagnostic(diagnostic));
                                break;
                            }
                        }
                    }
                }
                if result.deferred.is_some() {
                    break;
                }
                Check::Assert {
                    when,
                    require: require.ok_or(PrepareError::Internal)?,
                }
            } else {
                let bound = |name| {
                    d.field(payload, name)
                        .filter(|&id| !matches!(d.nodes()[id], N::Null))
                        .map(|id| {
                            let N::Integer(value) = &d.nodes()[id] else {
                                return Err(PrepareError::Internal);
                            };
                            value.parse::<i64>().map_err(|_| PrepareError::Internal)
                        })
                        .transpose()
                };
                let min = bound("min")?;
                let max = bound("max")?;
                let reason = if min.is_none() && max.is_none() {
                    Some("row_count requires one bound")
                } else if matches!((min,max),(Some(a),Some(b)) if a>b) {
                    Some("row_count min exceeds max")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    result.deferred =
                        invalid(path, "invalid_declaration", "REQ-0399", reason.into());
                    break;
                }
                Check::RowCount { min, max }
            };
            result.checks.push(Verification { path, check });
            result.identities.push(identity);
        }
        if let Some(finding) = &result.deferred {
            let (path, check) = match finding {
                DeclarationFinding::Reason {
                    path,
                    condition,
                    requirement,
                    reason,
                } => (
                    path.clone(),
                    Check::InvalidDeclaration {
                        condition,
                        requirement,
                        reason: reason.clone(),
                    },
                ),
                DeclarationFinding::Diagnostic(diagnostic) => (
                    diagnostic.spec_paths[0].clone(),
                    Check::InvalidDiagnostic(diagnostic.clone()),
                ),
            };
            result.checks.push(Verification { path, check });
            result.identities.push(None);
        }
        Ok(result)
    }
}
