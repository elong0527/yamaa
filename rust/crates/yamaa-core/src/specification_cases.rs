//! Bounded ordered case lowering. Unselected branches have no runtime effects.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{CaseBranch, Intermediate, SecondarySource},
    predicate::Plan,
    predicate_compiler, project_call_binding,
};

#[derive(Clone, Debug)]
pub(super) struct Branch {
    when: Option<Result<Plan, crate::diagnostic::Diagnostic>>,
    path: String,
    operation: alloc::boxed::Box<Operation>,
}
fn unsupported(path: &str, operation: &str) -> PrepareError {
    PrepareError::Unsupported(vec![UnsupportedFeature {
        operation: operation.into(),
        path: path.into(),
    }])
}
pub(super) fn prepare(
    d: &Document,
    node: usize,
    path: &str,
    project: Option<&functions::Prepared>,
    intermediates: &intermediates::Declarations,
    record: bool,
    depth: usize,
) -> Result<Vec<Branch>, PrepareError> {
    if depth > 64 || path.len() > 65_536 {
        return Err(PrepareError::Limit("case_depth"));
    }
    let items = sequence(d, node)?;
    if items.is_empty() || items.len() > 1024 {
        return Err(unsupported(path, "case_shape"));
    }
    let mut branches = Vec::new();
    for (index, &item) in items.iter().enumerate() {
        let prefix = format!("{path}[{index}]");
        let (when, value, result_path) = if let Some(value) = d.field(item, "otherwise") {
            if index == 0
                || index + 1 != items.len()
                || d.field(item, "when").is_some()
                || d.field(item, "then").is_some()
            {
                return Err(unsupported(&prefix, "case_shape"));
            }
            (None, value, format!("{prefix}.otherwise"))
        } else {
            let when = text(d, field(d, item, "when")?)?;
            let compiled =
                predicate_compiler::compile(when, &format!("{prefix}.when"), Default::default());
            let compiled = match compiled {
                Ok(plan) => {
                    // Raw predicate reads require a current driver record. A
                    // non-key implicit group has no such record; reject this
                    // unimplemented case scope before granting study authority.
                    if !record && plan.identifiers().iter().any(|name| name.contains('.')) {
                        return Err(unsupported(
                            &format!("{prefix}.when"),
                            "case_source_predicate",
                        ));
                    }
                    Ok(plan)
                }
                Err(predicate_compiler::Error::Parse(
                    crate::predicate_parser::ParseError::Grammar {
                        position, failure, ..
                    },
                )) => Err(failure.diagnostic(&format!("{prefix}.when"), when, position.character)),
                Err(_) => return Err(unsupported(&format!("{prefix}.when"), "case_predicate")),
            };
            (
                Some(compiled),
                field(d, item, "then")?,
                format!("{prefix}.then"),
            )
        };
        let &[(op, payload)] = mapping(d, value)? else {
            return Err(PrepareError::Internal);
        };
        let op = text(d, op)?;
        let result_path = format!("{result_path}.{op}");
        let operation = match op {
            "literal" => Operation::Literal(literal(d, payload, &result_path)?),
            "function" => Operation::ProjectFunction(
                *project
                    .and_then(|p| p.nodes.get(&payload))
                    .ok_or_else(|| unsupported(&result_path, "function_environment"))?,
            ),
            "case" => Operation::Case(prepare(
                d,
                payload,
                &result_path,
                project,
                intermediates,
                record,
                depth + 1,
            )?),
            "compute" => {
                let expression = text(d, field(d, payload, "expr")?)?;
                match compile_numeric_with_policy(
                    expression,
                    &result_path,
                    Default::default(),
                    crate::numeric_compiler::MathPolicy::PortableLibmV1,
                ) {
                    Ok(compiled) => Operation::Compute(compiled),
                    Err(error @ CompileError::Parse(ParseError::Grammar { .. })) => {
                        Operation::InvalidNumeric {
                            expression: expression.into(),
                            error,
                        }
                    }
                    Err(CompileError::Unsupported { .. }) => {
                        return Err(unsupported(&result_path, "numeric_function"))
                    }
                    Err(error) => {
                        return Err(PrepareError::Numeric {
                            path: format!("{result_path}.expr"),
                            error,
                        })
                    }
                }
            }
            "source" => {
                let mut features = Vec::new();
                let source = source_expressions::Declaration::prepare(
                    d,
                    payload,
                    &result_path,
                    &mut features,
                    intermediates,
                )?;
                if !features.is_empty() {
                    return Err(PrepareError::Unsupported(features));
                }
                if source.has_filter() {
                    return Err(unsupported(&result_path, "case_source_filter"));
                }
                Operation::Source(alloc::boxed::Box::new(source))
            }
            _ => return Err(unsupported(&result_path, op)),
        };
        branches.push(Branch {
            when,
            path: result_path,
            operation: alloc::boxed::Box::new(operation),
        });
    }
    Ok(branches)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    spec: &PreparedSpecification,
    branches: &[Branch],
    column: usize,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let start = findings.len();
    let mut result = Vec::new();
    for branch in branches {
        let when = branch
            .when
            .as_ref()
            .map(|compiled| {
                let plan = match compiled {
                    Ok(plan) => plan,
                    Err(diagnostic) => {
                        findings.push(BindFinding::Predicate(diagnostic.clone()));
                        return Ok(None);
                    }
                };
                let mut bindings = Vec::new();
                for name in plan.identifiers() {
                    if let Some(binding) = catalog.bind(name).map_err(BindError::Catalog)? {
                        let read = match binding {
                            reference_binding::Binding::Output { column, .. } => {
                                edges.push(column);
                                Read::Column(column)
                            }
                            reference_binding::Binding::Dataset { field, .. } => {
                                Read::Source(field)
                            }
                        };
                        bindings.push(Binding {
                            name: name.into(),
                            read,
                        });
                    } else {
                        findings.push(BindFinding::UnknownReference {
                            path: plan.spec_path().into(),
                            name: name.into(),
                        });
                    }
                }
                if findings.len() != start {
                    return Ok(None);
                }
                BoundPredicate::new(plan.clone(), bindings)
                    .map(Some)
                    .map_err(BindError::InvalidPredicateBinding)
            })
            .transpose()?
            .flatten();
        let expression = match branch.operation.as_ref() {
            Operation::InvalidNumeric { expression, error } => {
                findings.push(BindFinding::Numeric {
                    path: format!("{}.expr", branch.path),
                    expression: expression.clone(),
                    error: error.clone(),
                });
                None
            }
            Operation::Literal(value) => Some(Expression::Literal(value.clone())),
            Operation::Case(branches) => bind(
                spec,
                branches,
                column,
                catalog,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::ProjectFunction(call) => {
                let calls = spec.project_calls.as_ref().ok_or(BindError::Internal)?;
                let record = spec.keys.contains(&column);
                let context = project_call_binding::Context {
                    source_dataset: 0,
                    source_mode: if record {
                        project_call_binding::SourceMode::Record
                    } else {
                        project_call_binding::SourceMode::Collect
                    },
                    available_outputs: None,
                    scope: crate::reference_scope::Scope {
                        drivers: &[spec.source().name.as_str()],
                        current_driver: true,
                        reach: if record {
                            crate::reference_scope::Reach::Record
                        } else {
                            crate::reference_scope::Reach::Relation
                        },
                        joined: false,
                        phase: crate::reference_scope::Phase::Column { groups: &[] },
                    },
                };
                match project_call_binding::bind(calls, *call, catalog, context) {
                    Ok(bound) => {
                        edges.extend(bound.dependencies);
                        Some(Expression::ProjectFunction(bound.function))
                    }
                    Err(project_call_binding::Error::Findings(errors)) => {
                        findings.extend(functions::bind_findings(calls, *call, errors, None)?);
                        None
                    }
                    Err(project_call_binding::Error::Reference(error)) => {
                        return Err(BindError::Catalog(error))
                    }
                    Err(_) => return Err(BindError::Internal),
                }
            }
            Operation::Source(source) => {
                if source.has_filter() {
                    return Err(BindError::Internal);
                }
                match catalog.bind(&source.variable).map_err(BindError::Catalog)? {
                    Some(reference_binding::Binding::Output { column, .. }) => {
                        edges.push(column);
                        Some(Expression::Column(column))
                    }
                    Some(reference_binding::Binding::Dataset { field, .. }) => {
                        Some(if spec.keys.contains(&column) {
                            Expression::Source(field)
                        } else {
                            Expression::Collect {
                                column: field,
                                identifier: source.variable.clone(),
                                filter: None,
                                selection: None,
                            }
                        })
                    }
                    None => {
                        findings.push(BindFinding::UnknownReference {
                            path: branch.path.clone(),
                            name: source.variable.clone(),
                        });
                        None
                    }
                }
            }
            Operation::Compute(compiled) => {
                let mut bindings = Vec::new();
                for name in compiled.identifiers() {
                    if let Some((index, field)) = spec.intermediates.reference(name) {
                        if let Some(item) = &intermediates[index] {
                            edges.extend(item.keys.iter().map(|key| key.output_column));
                            if let Some(column) = secondary[item.source]
                                .schema
                                .columns()
                                .iter()
                                .position(|candidate| candidate.name == field)
                            {
                                bindings.push(Binding {
                                    name: name.into(),
                                    read: Read::Intermediate { index, column },
                                });
                                continue;
                            }
                        }
                        findings.push(BindFinding::UnknownReference {
                            path: format!("{}.expr", branch.path),
                            name: name.into(),
                        });
                        continue;
                    }
                    match catalog.bind(name).map_err(BindError::Catalog)? {
                        Some(reference_binding::Binding::Output { column, .. }) => {
                            edges.push(column);
                            bindings.push(Binding {
                                name: name.into(),
                                read: Read::Column(column),
                            });
                        }
                        _ => findings.push(BindFinding::UnknownReference {
                            path: format!("{}.expr", branch.path),
                            name: name.into(),
                        }),
                    }
                }
                if findings.len() == start {
                    Some(Expression::Compute(
                        BoundNumeric::new(compiled.clone(), bindings)
                            .map_err(BindError::InvalidNumericBinding)?,
                    ))
                } else {
                    None
                }
            }
            _ => return Err(BindError::Internal),
        };
        if let Some(expression) = expression {
            result.push(CaseBranch {
                when,
                assignment: alloc::boxed::Box::new(Assignment {
                    column,
                    expression,
                    path: branch.path.clone(),
                }),
            });
        }
    }
    Ok((findings.len() == start).then_some(Expression::Case(result)))
}

/// Charge all nested formulas and case predicates before owning any branch plan.
pub(super) fn charge(
    d: &Document,
    value: usize,
    total: &mut usize,
    limit: usize,
    depth: usize,
) -> Result<(), PrepareError> {
    if depth > 64 {
        return Err(PrepareError::Limit("case_depth"));
    }
    let Some(case) = d.field(value, "case") else {
        return Ok(());
    };
    for &branch in sequence(d, case)? {
        let when = d.field(branch, "when");
        let child = d
            .field(branch, "then")
            .or_else(|| d.field(branch, "otherwise"));
        let compute = child
            .and_then(|id| d.field(id, "compute"))
            .and_then(|id| d.field(id, "expr"));
        let filter = child
            .and_then(|id| d.field(id, "source"))
            .and_then(|id| d.field(id, "filter"))
            .filter(|&id| !matches!(d.nodes()[id], N::Null));
        for id in when.into_iter().chain(compute).chain(filter) {
            *total = total
                .checked_add(text(d, id)?.len())
                .filter(|&count| count <= limit)
                .ok_or(PrepareError::Limit("numeric_bytes"))?;
        }
        if let Some(child) = child {
            charge(d, child, total, limit, depth + 1)?;
        }
    }
    Ok(())
}
